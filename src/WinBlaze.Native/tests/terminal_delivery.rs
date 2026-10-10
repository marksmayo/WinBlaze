use std::{ffi::c_void, sync::Mutex, time::Duration};
use winblaze_native::{
    api::{WbCStringView, WbEvent, WbEventKind},
    bridge::{wb_export_csv, wb_scan_session_destroy, wb_scan_session_start_terminal},
};

struct Observed {
    kinds: Mutex<Vec<WbEventKind>>,
    terminal: std::sync::mpsc::SyncSender<()>,
}

#[test]
fn synchronous_terminal_scan_indexes_on_the_calling_thread_and_preserves_exports() {
    const CHILD: &str = "WINBLAZE_SYNC_TERMINAL_ROOT";
    if let Some(root) = std::env::var_os(CHILD) {
        struct ThreadObserved {
            caller: std::thread::ThreadId,
            correct: std::sync::atomic::AtomicBool,
        }
        extern "C" fn callback(event: *const WbEvent, context: *mut c_void) {
            // SAFETY: the synchronous call borrows this event/context for the callback.
            let (event, observed) = unsafe { (&*event, &*context.cast::<ThreadObserved>()) };
            if event.kind == WbEventKind::Completed {
                observed.correct.store(
                    std::thread::current().id() == observed.caller,
                    std::sync::atomic::Ordering::Relaxed,
                );
            }
        }
        let root = std::path::PathBuf::from(root);
        let observed = ThreadObserved {
            caller: std::thread::current().id(),
            correct: std::sync::atomic::AtomicBool::new(false),
        };
        let outcome = winblaze_native::bridge::run_terminal_scan(
            root.join("data").to_str().unwrap(),
            Duration::from_secs(30),
            Some(callback),
            (&observed as *const ThreadObserved).cast_mut().cast(),
        );
        assert!(outcome.completed);
        assert!(observed.correct.load(std::sync::atomic::Ordering::Relaxed));
        let csv = root.join("complete.csv");
        assert_eq!(wb_export_csv(view(csv.to_str().unwrap())), 0);
        assert_complete_catalog(&csv);
        let timed_out = winblaze_native::bridge::run_terminal_scan(
            root.join("data").to_str().unwrap(),
            Duration::ZERO,
            None,
            std::ptr::null_mut(),
        );
        assert!(
            !timed_out.completed,
            "an expired deadline must cancel and join its producer"
        );
        return;
    }
    let root = std::env::temp_dir().join(format!("winblaze-sync-terminal-{}", std::process::id()));
    std::fs::create_dir_all(root.join("data/nested")).unwrap();
    std::fs::write(root.join("data/alpha.txt"), b"alpha").unwrap();
    std::fs::write(root.join("data/beta.bin"), b"beta").unwrap();
    std::fs::write(root.join("data/nested/gamma.txt"), b"gamma").unwrap();
    let result = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "synchronous_terminal_scan_indexes_on_the_calling_thread_and_preserves_exports",
            "--nocapture",
        ])
        .env(CHILD, &root)
        .env("LOCALAPPDATA", root.join("cache"))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

extern "C" fn observe(event: *const WbEvent, context: *mut c_void) {
    // SAFETY: the scan borrows these pointers until session destruction joins it.
    let (event, observed) = unsafe { (&*event, &*context.cast::<Observed>()) };
    observed.kinds.lock().unwrap().push(event.kind);
    if matches!(
        event.kind,
        WbEventKind::Completed | WbEventKind::Failed | WbEventKind::Cancelled
    ) {
        let _ = observed.terminal.try_send(());
    }
}

fn view(text: &str) -> WbCStringView {
    WbCStringView {
        ptr: text.as_ptr().cast(),
        len: text.len(),
    }
}

fn assert_complete_catalog(csv: &std::path::Path) {
    let tree = winblaze_index::import::read_csv(std::fs::File::open(csv).unwrap()).unwrap();
    let mut files: Vec<_> = tree
        .files()
        .iter()
        .map(|file| (file.name.as_ref(), file.size_bytes))
        .collect();
    files.sort_unstable();
    assert_eq!(
        files,
        vec![("alpha.txt", 5), ("beta.bin", 4), ("gamma.txt", 5)]
    );
}

#[test]
fn unattended_scan_delivers_only_terminal_events_and_exports_the_complete_catalog() {
    const CHILD_ROOT: &str = "WINBLAZE_TERMINAL_TEST_ROOT";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let root = std::path::PathBuf::from(root);
        if std::env::var_os("WINBLAZE_TERMINAL_REOPEN").is_some() {
            assert!(!root.join("data").exists(), "source fixture must be gone");
            let csv = root.join("reopened.csv");
            assert_eq!(wb_export_csv(view(csv.to_str().unwrap())), 0);
            assert_complete_catalog(&csv);
            return;
        }
        let (sender, receiver) = std::sync::mpsc::sync_channel(1);
        let mut observed = Observed {
            kinds: Mutex::new(Vec::new()),
            terminal: sender,
        };
        let data = root.join("data");
        let session = wb_scan_session_start_terminal(
            view(data.to_str().unwrap()),
            Some(observe),
            (&mut observed as *mut Observed).cast(),
        );
        let completed = receiver.recv_timeout(Duration::from_secs(30));
        wb_scan_session_destroy(session);
        completed.expect("scan must deliver a terminal notification");
        let csv = root.join("complete.csv");
        assert_eq!(wb_export_csv(view(csv.to_str().unwrap())), 0);
        assert_complete_catalog(&csv);
        assert_eq!(
            *observed.kinds.lock().unwrap(),
            vec![WbEventKind::Completed],
            "unattended scans must not deliver unused live preview events"
        );
        return;
    }
    let root = std::env::temp_dir().join(format!("winblaze-terminal-{}", std::process::id()));
    let data = root.join("data");
    std::fs::create_dir_all(data.join("nested")).unwrap();
    std::fs::write(data.join("alpha.txt"), b"alpha").unwrap();
    std::fs::write(data.join("beta.bin"), b"beta").unwrap();
    std::fs::write(data.join("nested/gamma.txt"), b"gamma").unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "unattended_scan_delivers_only_terminal_events_and_exports_the_complete_catalog",
            "--nocapture",
        ])
        .env(CHILD_ROOT, &root)
        .env("LOCALAPPDATA", root.join("cache"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(&data).unwrap();
    let reopen = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "unattended_scan_delivers_only_terminal_events_and_exports_the_complete_catalog",
            "--nocapture",
        ])
        .env(CHILD_ROOT, &root)
        .env("WINBLAZE_TERMINAL_REOPEN", "1")
        .env("LOCALAPPDATA", root.join("cache"))
        .output()
        .unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    assert!(
        reopen.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&reopen.stdout),
        String::from_utf8_lossy(&reopen.stderr)
    );
}
