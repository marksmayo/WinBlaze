//! An incremental refresh patches the published model from recorded changes
//! when it can, and must then show exactly what a full walk shows.
use std::{ffi::c_void, path::Path, sync::Mutex, time::Duration};
use winblaze_native::{
    api::{WbCStringView, WbEvent, WbEventKind},
    bridge::{
        wb_export_csv, wb_last_refresh_touched, wb_scan_session_destroy, wb_scan_session_start,
        wb_scan_session_start_incremental,
    },
};

#[derive(Default)]
struct Observed {
    kinds: Mutex<Vec<WbEventKind>>,
    added: Mutex<Option<u64>>,
}

extern "C" fn observe(event: *const WbEvent, context: *mut c_void) {
    // SAFETY: the session borrows these pointers until destruction joins it.
    let (event, observed) = unsafe { (&*event, &*context.cast::<Observed>()) };
    observed.kinds.lock().unwrap().push(event.kind);
    if event.kind == WbEventKind::IncrementalChanges {
        *observed.added.lock().unwrap() = Some(event.incremental_changes.added);
    }
}

fn view(text: &str) -> WbCStringView {
    WbCStringView {
        ptr: text.as_ptr().cast(),
        len: text.len(),
    }
}

fn scan(root: &str, incremental: bool) -> Observed {
    let observed = Observed::default();
    let context = (&observed as *const Observed).cast_mut().cast();
    let handle = if incremental {
        wb_scan_session_start_incremental(view(root), Some(observe), context)
    } else {
        wb_scan_session_start(view(root), Some(observe), context)
    };
    let deadline = std::time::Instant::now() + Duration::from_secs(30);
    while !observed
        .kinds
        .lock()
        .unwrap()
        .contains(&WbEventKind::Completed)
    {
        assert!(
            std::time::Instant::now() < deadline,
            "scan did not complete"
        );
        std::thread::sleep(Duration::from_millis(2));
    }
    wb_scan_session_destroy(handle);
    observed
}

/// The published catalog as sorted CSV rows (row order is not part of it).
fn catalog(export: &Path) -> Vec<String> {
    let path = export.to_str().unwrap();
    assert_eq!(wb_export_csv(view(path)), 0, "export succeeds");
    let text = std::fs::read_to_string(export).unwrap();
    let mut rows: Vec<String> = text.lines().map(str::to_owned).collect();
    rows.sort_unstable();
    rows
}

fn touched() -> i64 {
    // SAFETY: a null buffer asks for the count only.
    unsafe { wb_last_refresh_touched(std::ptr::null_mut(), 0) }
}

/// Refreshes, then checks the result against a full walk of the same tree.
/// Returns whether the refresh patched the model (rather than walking).
fn refresh_matches_walk(data: &str, export: &Path, label: &str) -> bool {
    let refreshed = scan(data, true);
    let kinds = refreshed.kinds.lock().unwrap().clone();
    let completed = kinds
        .iter()
        .position(|kind| *kind == WbEventKind::Completed);
    let summary = kinds
        .iter()
        .position(|kind| *kind == WbEventKind::IncrementalChanges);
    assert!(
        matches!((completed, summary), (Some(c), Some(s)) if c < s),
        "{label}: the change summary follows Completed: {kinds:?}"
    );
    let patched = touched() >= 0;
    let after_refresh = catalog(export);
    scan(data, false);
    let after_walk = catalog(export);
    assert_eq!(
        after_refresh, after_walk,
        "{label}: refresh shows what a walk shows"
    );
    patched
}

#[test]
fn a_refresh_patches_new_and_changed_files_and_walks_for_anything_else() {
    const CHILD: &str = "WINBLAZE_FAST_REFRESH_ROOT";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let data = root.join("data");
        let data_path = data.to_str().unwrap();
        let export = root.join("catalog.csv");
        scan(data_path, false);
        assert_eq!(touched(), -1, "a walk patches nothing");

        // New and grown files in existing folders: patched.
        std::fs::write(data.join("nested/delta.txt"), b"delta").unwrap();
        std::fs::write(data.join("alpha.txt"), b"alpha grown much longer").unwrap();
        std::fs::write(data.join("top.bin"), vec![7u8; 9000]).unwrap();
        let log = root.join("cache/WinBlaze/logs/events.jsonl");
        let completions = || {
            std::fs::read_to_string(&log)
                .unwrap_or_default()
                .matches("scanner.completed")
                .count()
        };
        let before = completions();
        assert!(refresh_matches_walk(
            data_path,
            &export,
            "added and modified"
        ));
        // The patched refresh and the comparison walk each log a completion.
        assert_eq!(
            completions(),
            before + 2,
            "a patched refresh is logged like a walk"
        );

        // Nothing changed: patched (and still correct).
        scan(data_path, false);
        let unchanged = scan(data_path, true);
        assert_eq!(touched(), 0, "an unchanged refresh touches no folder");
        assert_eq!(*unchanged.added.lock().unwrap(), Some(0));

        // A removal, a new folder, a hard link: walked.
        std::fs::remove_file(data.join("beta.bin")).unwrap();
        assert!(!refresh_matches_walk(data_path, &export, "removed"));
        std::fs::create_dir(data.join("fresh")).unwrap();
        std::fs::write(data.join("fresh/inner.txt"), b"inner").unwrap();
        assert!(!refresh_matches_walk(data_path, &export, "new folder"));
        // Both names in one folder: parallel walkers meet names in different
        // folders in either order, so which one is charged would vary.
        std::fs::hard_link(data.join("alpha.txt"), data.join("alpha-link.txt")).unwrap();
        assert!(!refresh_matches_walk(data_path, &export, "hard link"));

        // After a walk the next change is patched again (in a folder with no
        // hard link: a linked file's walk allocation depends on which name
        // the walk met first, so its folder always walks).
        std::fs::write(data.join("fresh/later.txt"), b"later").unwrap();
        assert!(refresh_matches_walk(data_path, &export, "after a walk"));

        // A folder holding a junction: a name match cannot prove the folder
        // is the one the model has, so it walks.
        let junction = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(data.join("fresh").join("junction"))
            .arg(data.join("nested"))
            .output()
            .unwrap();
        assert!(junction.status.success(), "{junction:?}");
        assert!(!refresh_matches_walk(data_path, &export, "new junction"));
        std::fs::write(data.join("fresh/beside-junction.txt"), b"beside").unwrap();
        assert!(!refresh_matches_walk(
            data_path,
            &export,
            "beside a junction"
        ));
        return;
    }
    let root = std::env::temp_dir().join(format!("winblaze-fast-refresh-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("data/nested")).unwrap();
    std::fs::write(root.join("data/alpha.txt"), b"alpha").unwrap();
    std::fs::write(root.join("data/beta.bin"), b"beta").unwrap();
    std::fs::write(root.join("data/nested/gamma.txt"), b"gamma").unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "a_refresh_patches_new_and_changed_files_and_walks_for_anything_else",
            "--nocapture",
        ])
        .env(CHILD, &root)
        .env("LOCALAPPDATA", root.join("cache"))
        .output()
        .unwrap();
    let _ = std::fs::remove_dir_all(&root);
    assert!(
        result.status.success(),
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
