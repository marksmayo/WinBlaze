//! Long-run stability soak: repeatedly scans (and periodically incrementally
//! rescans + reloads the snapshot) in a single process, printing the working
//! set and handle count each cycle so a leak shows up as an upward trend. A
//! representative stand-in for the multi-hour release soak.
//!
//! Usage: soak_repro [root] [cycles]

use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use winblaze_native::api::{WbCStringView, WbEvent, WbEventKind};
use winblaze_native::bridge::{
    wb_index_snapshot_stats, wb_scan_session_destroy, wb_scan_session_start,
    wb_scan_session_start_incremental,
};

static COMPLETED: AtomicU64 = AtomicU64::new(0);
static FILES: AtomicU64 = AtomicU64::new(0);
static ROOT: OnceLock<String> = OnceLock::new();

extern "C" fn on_event(event: *const WbEvent, _user: *mut c_void) {
    if event.is_null() {
        return;
    }
    let event = unsafe { &*event };
    match event.kind {
        WbEventKind::Completed | WbEventKind::Cancelled | WbEventKind::Failed => {
            COMPLETED.store(1, Ordering::SeqCst);
        }
        WbEventKind::Summary => {
            FILES.store(event.summary.files_seen, Ordering::Relaxed);
        }
        _ => {}
    }
}

fn run_scan(incremental: bool) -> bool {
    COMPLETED.store(0, Ordering::SeqCst);
    let root = ROOT.get().unwrap();
    let view = WbCStringView {
        ptr: root.as_ptr().cast::<i8>(),
        len: root.len(),
    };
    let handle = if incremental {
        wb_scan_session_start_incremental(view, Some(on_event), std::ptr::null_mut())
    } else {
        wb_scan_session_start(view, Some(on_event), std::ptr::null_mut())
    };
    let started = Instant::now();
    while COMPLETED.load(Ordering::SeqCst) == 0 {
        if started.elapsed() > Duration::from_secs(120) {
            eprintln!("  timeout waiting for completion");
            break;
        }
        thread::sleep(Duration::from_millis(25));
    }
    // Destroy joins the worker + finalizes the snapshot write.
    wb_scan_session_destroy(handle);
    true
}

fn main() {
    let root = std::env::args()
        .nth(1)
        .unwrap_or_else(|| r"C:\".to_string());
    let cycles: usize = std::env::args()
        .nth(2)
        .and_then(|value| value.parse().ok())
        .unwrap_or(12);
    ROOT.set(root.clone()).ok();

    println!("soak: root={root} cycles={cycles}");
    let mut samples: Vec<(u64, u32)> = Vec::new();
    for cycle in 1..=cycles {
        // Cache-load + incremental every 4th cycle to exercise the merge and
        // snapshot read paths, full replace-scan otherwise.
        let incremental = cycle % 4 == 0;
        let started = Instant::now();
        run_scan(incremental);
        let _ = wb_index_snapshot_stats(); // exercise the read-model path
        let elapsed = started.elapsed().as_millis();

        let working_set_mb = working_set_bytes() / (1024 * 1024);
        let handles = handle_count();
        samples.push((working_set_mb, handles));
        println!(
            "cycle {cycle:>2}/{cycles} {:<12} {elapsed:>6} ms  ws={working_set_mb:>4} MB  handles={handles:>5}  files={}",
            if incremental { "incremental" } else { "full" },
            FILES.load(Ordering::Relaxed),
        );
    }

    // Leak verdict: compare the mean of the first third against the last third.
    if samples.len() >= 6 {
        let third = samples.len() / 3;
        let mean = |slice: &[(u64, u32)]| {
            let (ws, h) = slice
                .iter()
                .fold((0u64, 0u64), |(a, b), &(w, c)| (a + w, b + c as u64));
            (
                ws as f64 / slice.len() as f64,
                h as f64 / slice.len() as f64,
            )
        };
        let (first_ws, first_h) = mean(&samples[..third]);
        let (last_ws, last_h) = mean(&samples[samples.len() - third..]);
        println!(
            "\nworking set: first-third mean {first_ws:.0} MB -> last-third mean {last_ws:.0} MB ({:+.1}%)",
            (last_ws - first_ws) / first_ws.max(1.0) * 100.0
        );
        println!("handles:     first-third mean {first_h:.0} -> last-third mean {last_h:.0}");
        let ws_growth = (last_ws - first_ws) / first_ws.max(1.0);
        let handle_growth = last_h - first_h;
        if ws_growth > 0.15 || handle_growth > 64.0 {
            println!(
                "VERDICT: possible leak (ws +{:.0}%, handles +{:.0})",
                ws_growth * 100.0,
                handle_growth
            );
        } else {
            println!("VERDICT: stable (no significant working-set or handle growth)");
        }
    }
}

// --- Win32 process metrics -------------------------------------------------

#[repr(C)]
#[derive(Default)]
struct ProcessMemoryCounters {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
}

fn working_set_bytes() -> u64 {
    let mut counters = ProcessMemoryCounters {
        cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
        ..Default::default()
    };
    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<ProcessMemoryCounters>() as u32,
        )
    };
    if ok != 0 {
        counters.working_set_size as u64
    } else {
        0
    }
}

fn handle_count() -> u32 {
    let mut count = 0u32;
    let ok = unsafe { GetProcessHandleCount(GetCurrentProcess(), &mut count) };
    if ok != 0 {
        count
    } else {
        0
    }
}

#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> *mut c_void;
    fn GetProcessHandleCount(process: *mut c_void, count: *mut u32) -> i32;
}

#[link(name = "psapi")]
extern "system" {
    fn GetProcessMemoryInfo(
        process: *mut c_void,
        counters: *mut ProcessMemoryCounters,
        cb: u32,
    ) -> i32;
}
