use std::{
    ffi::c_void,
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use winblaze_native::{
    api::{WbCStringView, WbEvent, WbEventKind},
    bridge::{wb_export_csv, wb_scan_session_destroy, wb_scan_session_start_rules},
};
fn view(s: &str) -> WbCStringView {
    WbCStringView {
        ptr: s.as_ptr().cast(),
        len: s.len(),
    }
}
extern "C" fn collect(event: *const WbEvent, data: *mut c_void) {
    unsafe {
        if (*event).kind == WbEventKind::Completed {
            (&*data.cast::<AtomicBool>()).store(true, Ordering::SeqCst);
        }
    }
}
#[test]
fn native_scan_rules_exclude_subtree_without_leaking_into_the_export() {
    let root = std::env::temp_dir().join(format!("wb-native-rules-{}", std::process::id()));
    std::fs::create_dir_all(root.join("skip")).unwrap();
    std::fs::write(root.join("skip/hidden.txt"), b"excluded").unwrap();
    std::fs::write(root.join("chosen.bin"), b"yes").unwrap();
    std::env::set_var("LOCALAPPDATA", root.join("appdata"));
    let done = AtomicBool::new(false);
    let session = wb_scan_session_start_rules(
        view(&root.to_string_lossy()),
        view("skip\rappdata"),
        view("*.bin"),
        Some(collect),
        (&done as *const AtomicBool).cast_mut().cast(),
    );
    let deadline = Instant::now() + Duration::from_secs(15);
    while !done.load(Ordering::SeqCst) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(done.load(Ordering::SeqCst));
    wb_scan_session_destroy(session);
    let output = root.join("output.csv");
    assert_eq!(wb_export_csv(view(&output.to_string_lossy())), 0);
    let csv = std::fs::read_to_string(&output).unwrap();
    assert!(csv.contains("chosen.bin"));
    assert!(!csv.contains("hidden.txt"));
    assert!(!csv.contains("\\skip"));
    std::fs::remove_dir_all(root).unwrap();
}
