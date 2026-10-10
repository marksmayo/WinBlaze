use std::{ffi::c_void, path::Path, sync::mpsc::SyncSender, time::Duration};
use winblaze_native::{
    api::{WbCStringView, WbEvent, WbEventKind},
    bridge::{wb_export_csv, wb_scan_session_destroy, wb_scan_session_start_terminal},
    capacity_hint::{load_hint, scope_key},
};

fn view(text: &str) -> WbCStringView {
    WbCStringView {
        ptr: text.as_ptr().cast(),
        len: text.len(),
    }
}
extern "C" fn observe(event: *const WbEvent, context: *mut c_void) {
    // SAFETY: both borrowed pointers remain live until session destruction joins it.
    let (event, sender) = unsafe { (&*event, &*context.cast::<SyncSender<bool>>()) };
    if matches!(
        event.kind,
        WbEventKind::Completed | WbEventKind::Failed | WbEventKind::Cancelled
    ) {
        let _ = sender.try_send(event.kind == WbEventKind::Completed);
    }
}
fn scan_and_export(data: &Path, csv: &Path, expected_files: u64, expected_bytes: u64) {
    let (sender, receiver) = std::sync::mpsc::sync_channel::<bool>(1);
    let session = wb_scan_session_start_terminal(
        view(data.to_str().unwrap()),
        Some(observe),
        (&sender as *const SyncSender<bool>).cast_mut().cast(),
    );
    let completed = receiver.recv_timeout(Duration::from_secs(30));
    wb_scan_session_destroy(session);
    assert!(completed.unwrap());
    assert_eq!(wb_export_csv(view(csv.to_str().unwrap())), 0);
    let tree = winblaze_index::import::read_csv(std::fs::File::open(csv).unwrap()).unwrap();
    assert_eq!(tree.files().len() as u64, expected_files);
    assert_eq!(tree.root().unwrap().1.logical_bytes, expected_bytes);
}

#[test]
fn allocation_hints_preserve_complete_rescans_and_export_when_hint_writes_fail() {
    const CHILD: &str = "WINBLAZE_CAPACITY_TEST_ROOT";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = std::path::PathBuf::from(root);
        let data = root.join("data");
        let hint = root.join("cache/WinBlaze/index/capacity-hint.bin");
        let scope = scope_key(
            data.to_str().unwrap(),
            &winblaze_scanner::ScanRules::default(),
        );
        scan_and_export(&data, &root.join("first.csv"), 3, 14);
        assert_eq!(load_hint(&hint, &scope).unwrap().files, 3);
        let other = root.join("other-data");
        std::fs::create_dir_all(&other).unwrap();
        std::fs::write(other.join("other.txt"), b"isolated").unwrap();
        let other_scope = scope_key(
            other.to_str().unwrap(),
            &winblaze_scanner::ScanRules::default(),
        );
        scan_and_export(&other, &root.join("other.csv"), 1, 8);
        assert_eq!(load_hint(&hint, &scope).unwrap().files, 3);
        assert_eq!(load_hint(&hint, &other_scope).unwrap().files, 1);
        std::fs::write(data.join("delta.txt"), b"delta!").unwrap();
        scan_and_export(&data, &root.join("second.csv"), 4, 20);
        assert_eq!(load_hint(&hint, &scope).unwrap().files, 4);
        assert_eq!(load_hint(&hint, &other_scope).unwrap().files, 1);
        std::fs::remove_file(&hint).unwrap();
        std::fs::create_dir(&hint).unwrap();
        std::fs::write(data.join("epsilon.txt"), b"epsilon!").unwrap();
        scan_and_export(&data, &root.join("third.csv"), 5, 28);
        return;
    }
    let root = std::env::temp_dir().join(format!("wb-capacity-scan-{}", std::process::id()));
    let data = root.join("data");
    std::fs::create_dir_all(data.join("nested")).unwrap();
    std::fs::write(data.join("alpha.txt"), b"alpha").unwrap();
    std::fs::write(data.join("beta.bin"), b"beta").unwrap();
    std::fs::write(data.join("nested/gamma.txt"), b"gamma").unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "allocation_hints_preserve_complete_rescans_and_export_when_hint_writes_fail",
            "--nocapture",
        ])
        .env(CHILD, &root)
        .env("LOCALAPPDATA", root.join("cache"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(root).unwrap();
}
