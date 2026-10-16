use std::{ffi::c_void, sync::Mutex, time::Duration};
use winblaze_native::{
    api::{WbCStringView, WbEvent, WbEventKind},
    bridge::{wb_scan_session_destroy, wb_scan_session_start, wb_scan_session_start_incremental},
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

/// Runs one session to completion; destruction joins its deferred work.
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
        std::thread::sleep(Duration::from_millis(5));
    }
    wb_scan_session_destroy(handle);
    observed
}

#[test]
fn incremental_rescan_publishes_before_merging_and_persists_the_merge() {
    const CHILD: &str = "WINBLAZE_INCREMENTAL_RESCAN_ROOT";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let data = root.join("data");
        let data_path = data.to_str().unwrap();
        scan(data_path, false);
        std::fs::write(data.join("nested/delta.txt"), b"delta").unwrap();

        let observed = scan(data_path, true);
        let kinds = observed.kinds.lock().unwrap().clone();
        let completed = kinds
            .iter()
            .position(|kind| *kind == WbEventKind::Completed);
        let changes = kinds
            .iter()
            .position(|kind| *kind == WbEventKind::IncrementalChanges);
        assert!(
            matches!((completed, changes), (Some(c), Some(i)) if c < i),
            "the change summary follows Completed: {kinds:?}"
        );
        assert_eq!(*observed.added.lock().unwrap(), Some(1));

        use winblaze_index::{IndexBackend, IndexRepository, SqliteIndexRepository, TreeIndex};
        let index = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap())
            .join("WinBlaze")
            .join("index");
        let repository = SqliteIndexRepository::open(&index, IndexBackend::BinaryCache);
        let tree = TreeIndex::build(repository.into_transaction());
        let mut names: Vec<_> = tree
            .files()
            .iter()
            .map(|file| file.name.to_string())
            .collect();
        names.sort_unstable();
        assert_eq!(names, ["alpha.txt", "beta.bin", "delta.txt", "gamma.txt"]);

        // An unchanged rescan reports no additions against the merged snapshot.
        let unchanged = scan(data_path, true);
        assert_eq!(*unchanged.added.lock().unwrap(), Some(0));
        return;
    }
    let root = std::env::temp_dir().join(format!("winblaze-incremental-{}", std::process::id()));
    std::fs::create_dir_all(root.join("data/nested")).unwrap();
    std::fs::write(root.join("data/alpha.txt"), b"alpha").unwrap();
    std::fs::write(root.join("data/beta.bin"), b"beta").unwrap();
    std::fs::write(root.join("data/nested/gamma.txt"), b"gamma").unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "incremental_rescan_publishes_before_merging_and_persists_the_merge",
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
