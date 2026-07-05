use std::ffi::c_void;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::thread;
use std::time::{Duration, Instant};

use winblaze_native::api::{WbCStringView, WbEvent, WbEventKind, WbTreeNode};
use winblaze_native::bridge::{
    wb_scan_session_destroy, wb_scan_session_start, wb_tree_children, wb_tree_root,
};

static SCAN_STARTED: OnceLock<Instant> = OnceLock::new();

fn elapsed_ms() -> u64 {
    SCAN_STARTED
        .get()
        .map(|started| started.elapsed().as_millis() as u64)
        .unwrap_or(0)
}

#[derive(Default)]
struct Counters {
    files: AtomicU64,
    directories: AtomicU64,
    progress_events: AtomicU64,
    completed: AtomicU64,
    last_items_done: AtomicU64,
    // Phase timing (ms from scan start): Summary marks producer+persist-at-
    // summary done; Completed marks the full native drain done.
    summary_ms: AtomicU64,
    completed_ms: AtomicU64,
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
        WbEventKind::Summary => {
            counters.summary_ms.store(elapsed_ms(), Ordering::Relaxed);
        }
        WbEventKind::Completed => {
            counters.completed_ms.store(elapsed_ms(), Ordering::Relaxed);
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
    let _ = SCAN_STARTED.set(started);
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
        "{{\"elapsed_ms\":{},\"summary_ms\":{},\"completed_ms\":{},\"files\":{},\"directories\":{},\"progress_events\":{},\"completed\":{}}}",
        started.elapsed().as_millis(),
        counters.summary_ms.load(Ordering::Relaxed),
        counters.completed_ms.load(Ordering::Relaxed),
        counters.files.load(Ordering::Relaxed),
        counters.directories.load(Ordering::Relaxed),
        counters.progress_events.load(Ordering::Relaxed),
        counters.completed.load(Ordering::Relaxed)
    );

    wb_scan_session_destroy(handle);
    walk_tree_api();
}

#[derive(Default)]
struct CapturedNode {
    id: u64,
    is_directory: bool,
    name: String,
    logical_bytes: u64,
    physical_bytes: u64,
    file_count: u64,
    item_count: u64,
}

extern "C" fn capture_node(node: *const WbTreeNode, user_data: *mut c_void) {
    if node.is_null() || user_data.is_null() {
        return;
    }
    let nodes = unsafe { &mut *(user_data as *mut Vec<CapturedNode>) };
    let node = unsafe { &*node };
    let name = if node.name.ptr.is_null() {
        String::new()
    } else {
        let bytes = unsafe { std::slice::from_raw_parts(node.name.ptr.cast::<u8>(), node.name.len) };
        String::from_utf8_lossy(bytes).to_string()
    };
    nodes.push(CapturedNode {
        id: node.id,
        is_directory: node.is_directory != 0,
        name,
        logical_bytes: node.logical_bytes,
        physical_bytes: node.physical_bytes,
        file_count: node.file_count,
        item_count: node.item_count,
    });
}

/// Exercises wb_tree_root + wb_tree_children against the freshly scanned
/// index: prints the root, its top children, and asserts the root physical
/// total equals the sum of its direct children (rollup consistency).
fn walk_tree_api() {
    let mut roots: Vec<CapturedNode> = Vec::new();
    let has_root = wb_tree_root(Some(capture_node), (&mut roots as *mut Vec<CapturedNode>).cast());
    if has_root == 0 || roots.is_empty() {
        println!("tree: no root (empty index)");
        return;
    }
    let root = &roots[0];
    println!(
        "tree root: \"{}\" physical={} logical={} files={} items={}",
        root.name, root.physical_bytes, root.logical_bytes, root.file_count, root.item_count
    );

    let mut children: Vec<CapturedNode> = Vec::new();
    let result = wb_tree_children(
        root.id,
        0,
        Some(capture_node),
        (&mut children as *mut Vec<CapturedNode>).cast(),
    );
    println!("tree children: emitted={} total={}", result.emitted, result.total);
    for child in children.iter().take(10) {
        println!(
            "  {} \"{}\" physical={} files={}",
            if child.is_directory { "dir " } else { "file" },
            child.name,
            child.physical_bytes,
            child.file_count
        );
    }

    if result.emitted == result.total {
        let child_sum: u64 = children.iter().map(|child| child.physical_bytes).sum();
        assert_eq!(
            child_sum, root.physical_bytes,
            "root physical bytes must equal the sum of its direct children"
        );
        println!("tree rollup check: OK (children sum == root physical)");
    }
}
