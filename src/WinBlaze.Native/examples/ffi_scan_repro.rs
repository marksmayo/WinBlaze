use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use winblaze_native::api::{WbCStringView, WbEvent, WbEventKind};
use winblaze_native::bridge::{wb_scan_session_destroy, wb_scan_session_start};

#[derive(Default)]
struct Counters {
    files: AtomicU64,
    directories: AtomicU64,
    progress_events: AtomicU64,
    completed: AtomicU64,
    last_items_done: AtomicU64,
}

extern "C" fn on_event(event: *const WbEvent, user_data: *mut c_void) {
    if event.is_null() || user_data.is_null() {
        return;
    }
    let counters = unsafe { &*(user_data as *const Counters) };
    let event = unsafe { &*event };
    match event.kind {
        WbEventKind::FileFound => {
            counters.files.fetch_add(1, Ordering::Relaxed);
        }
        WbEventKind::DirectoryFound => {
            counters.directories.fetch_add(1, Ordering::Relaxed);
        }
        WbEventKind::Progress => {
            counters.progress_events.fetch_add(1, Ordering::Relaxed);
            counters
                .last_items_done
                .store(event.progress_items_done, Ordering::Relaxed);
        }
        WbEventKind::Completed => {
            counters.completed.store(1, Ordering::Relaxed);
        }
        _ => {}
    }
}

fn main() {
    let root = std::env::args().nth(1).unwrap_or_else(|| r"C:\".to_string());
    let counters = Box::new(Counters::default());
    let counters_ptr = Box::into_raw(counters);

    let view = WbCStringView {
        ptr: root.as_ptr().cast::<i8>(),
        len: root.len(),
    };

    let started = Instant::now();
    let handle = wb_scan_session_start(view, Some(on_event), counters_ptr as *mut c_void);

    loop {
        let counters = unsafe { &*counters_ptr };
        if counters.completed.load(Ordering::Relaxed) == 1 {
            break;
        }
        if started.elapsed() > Duration::from_secs(300) {
            eprintln!("timeout waiting for completion");
            break;
        }
        eprintln!(
            "[{:>7} ms] files={} directories={} progress_events={} last_items_done={}",
            started.elapsed().as_millis(),
            counters.files.load(Ordering::Relaxed),
            counters.directories.load(Ordering::Relaxed),
            counters.progress_events.load(Ordering::Relaxed),
            counters.last_items_done.load(Ordering::Relaxed)
        );
        thread::sleep(Duration::from_millis(2000));
    }

    let counters = unsafe { &*counters_ptr };
    println!(
        "{{\"elapsed_ms\":{},\"files\":{},\"directories\":{},\"progress_events\":{},\"completed\":{}}}",
        started.elapsed().as_millis(),
        counters.files.load(Ordering::Relaxed),
        counters.directories.load(Ordering::Relaxed),
        counters.progress_events.load(Ordering::Relaxed),
        counters.completed.load(Ordering::Relaxed)
    );

    wb_scan_session_destroy(handle);
}
