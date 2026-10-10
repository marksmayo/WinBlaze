#![cfg(windows)]

use std::{
    alloc::{GlobalAlloc, Layout, System},
    fs,
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};
use winblaze_core::ScanEvent;
use winblaze_scanner::{
    performance::ScanPipelineConfig, ScanBackend, ScanController, ScanRequest, ScanRuntimeConfig,
};

static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
struct MeasuredAllocator;

// SAFETY: allocation and deallocation delegate unchanged to System. Counters
// allocate no storage and track successful allocations across all scan threads.
unsafe impl GlobalAlloc for MeasuredAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: forward the caller's valid layout unchanged.
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            let live = LIVE.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK.fetch_max(live, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: forward the original allocation pointer and layout unchanged.
        unsafe { System.dealloc(pointer, layout) };
    }
}

#[global_allocator]
static ALLOCATOR: MeasuredAllocator = MeasuredAllocator;

#[test]
fn wide_nested_scan_completes_with_bounded_transient_memory() {
    let root = std::env::temp_dir().join(format!("wb-traversal-budget-{}", std::process::id()));
    for branch in 0..100 {
        for leaf in 0..100 {
            fs::create_dir_all(root.join(format!("branch-{branch:03}/leaf-{leaf:03}"))).unwrap();
        }
    }
    let (controller, receiver) = ScanController::bounded_channel(1);
    // Allocate consumer verification storage before measuring producer memory.
    let mut leaf_parents = Vec::with_capacity(10_000);
    let mut branch_ids = [None; 100];
    let mut leaf_seen = [false; 10_000];
    let mut root_id = None;
    let baseline = LIVE.load(Ordering::Relaxed);
    PEAK.store(baseline, Ordering::Relaxed);
    let handle = controller.start_scan(ScanRequest {
        root_path: root.clone(),
        config: ScanRuntimeConfig {
            root_path: root.clone(),
            backend: ScanBackend::DirectoryWalk,
            max_parallelism: 4,
            directory_walk_max_parallelism: 4,
            pipeline: ScanPipelineConfig {
                batch_size: 32,
                ..Default::default()
            },
            ..Default::default()
        },
    });
    let mut directories = 0;
    let mut completed = false;
    while !completed {
        for event in receiver.recv_timeout(Duration::from_secs(30)).unwrap() {
            match event {
                ScanEvent::DirectoryFound(directory) => {
                    directories += 1;
                    let relative = std::path::Path::new(&directory.full_path)
                        .strip_prefix(&root)
                        .unwrap();
                    let mut components = relative.components();
                    if let Some(branch) = components.next() {
                        let branch = branch
                            .as_os_str()
                            .to_str()
                            .unwrap()
                            .strip_prefix("branch-")
                            .unwrap()
                            .parse::<usize>()
                            .unwrap();
                        assert!(branch < 100);
                        if let Some(leaf) = components.next() {
                            let leaf = leaf
                                .as_os_str()
                                .to_str()
                                .unwrap()
                                .strip_prefix("leaf-")
                                .unwrap()
                                .parse::<usize>()
                                .unwrap();
                            assert!(leaf < 100);
                            assert!(!leaf_seen[branch * 100 + leaf]);
                            leaf_seen[branch * 100 + leaf] = true;
                            leaf_parents.push((directory.parent_directory_id.unwrap(), branch));
                        } else {
                            assert!(branch_ids[branch].replace(directory.id).is_none());
                            assert_eq!(directory.parent_directory_id, root_id);
                        }
                        assert!(components.next().is_none());
                    } else {
                        assert!(root_id.replace(directory.id).is_none());
                    }
                }
                ScanEvent::FileFound(_) => panic!("owned fixture contains no files"),
                ScanEvent::Completed => completed = true,
                ScanEvent::Failed(error) => panic!("scan failed: {error}"),
                ScanEvent::Cancelled | ScanEvent::Issue(_) => {
                    panic!("owned empty-folder fixture must complete without issues")
                }
                _ => {}
            }
        }
    }
    handle.join();
    let transient = PEAK.load(Ordering::Relaxed).saturating_sub(baseline);
    fs::remove_dir_all(root).unwrap();
    eprintln!("Complete traversal transient allocation: {transient} bytes");
    assert_eq!(directories, 10_101);
    assert!(leaf_seen.into_iter().all(|seen| seen));
    for (parent, branch) in leaf_parents {
        assert_eq!(Some(parent), branch_ids[branch]);
    }
    assert!(
        transient <= 262_144,
        "complete empty-folder traversal used {transient} transient bytes; budget is 256 KiB"
    );
}
