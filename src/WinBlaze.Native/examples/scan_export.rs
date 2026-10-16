//! Scan and export through the same complete native catalog used by the UI.
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use winblaze_native::api::{WbCStringView, WbEvent, WbEventKind};
use winblaze_native::bridge::{wb_export_csv, wb_scan_session_destroy, wb_scan_session_start};

struct State {
    completed: AtomicBool,
    failed: AtomicBool,
}
extern "C" fn event(event: *const WbEvent, context: *mut c_void) {
    // SAFETY: the session invokes this callback with live pointers supplied below.
    let (event, state) = unsafe { (&*event, &*(context.cast::<State>())) };
    match event.kind {
        WbEventKind::Completed => state.completed.store(true, Ordering::Release),
        WbEventKind::Failed | WbEventKind::Cancelled => state.failed.store(true, Ordering::Release),
        _ => {}
    }
}
fn view(text: &str) -> WbCStringView {
    WbCStringView {
        ptr: text.as_ptr().cast(),
        len: text.len(),
    }
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 3 {
        eprintln!("usage: scan_export <scan-root> <output.csv>");
        std::process::exit(2);
    }
    let state = State {
        completed: AtomicBool::new(false),
        failed: AtomicBool::new(false),
    };
    let started = Instant::now();
    let session = wb_scan_session_start(
        view(&args[1]),
        Some(event),
        (&state as *const State).cast_mut().cast(),
    );
    while !state.completed.load(Ordering::Acquire)
        && !state.failed.load(Ordering::Acquire)
        && started.elapsed() < Duration::from_secs(300)
    {
        std::thread::sleep(Duration::from_millis(5));
    }
    let ok = state.completed.load(Ordering::Acquire);
    wb_scan_session_destroy(session);
    if !ok {
        eprintln!("scan failed or timed out");
        std::process::exit(1);
    }
    if wb_export_csv(view(&args[2])) != 0 {
        eprintln!("CSV export failed");
        std::process::exit(1);
    }
    println!("{{\"scan_export_ms\":{}}}", started.elapsed().as_millis());
}
