#![cfg(windows)]

use std::{
    alloc::{GlobalAlloc, Layout, System},
    collections::HashSet,
    fs,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};
use winblaze_core::ScanEvent;
use winblaze_scanner::{
    ScanBackend, ScanController, ScanPipelineConfig, ScanRequest, ScanRuntimeConfig,
};

const BATCH_SIZE: usize = 32;
static WATCH: AtomicBool = AtomicBool::new(false);
static BATCH_ALLOCATIONS: AtomicUsize = AtomicUsize::new(0);
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
struct MeasuredAllocator;

// SAFETY: pointer/layout requests are forwarded unchanged to System. Atomic
// accounting allocates no memory and counts successful batch-sized allocations
// across producer and consumer threads in this single-test executable.
unsafe impl GlobalAlloc for MeasuredAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let pointer = unsafe { System.alloc(layout) };
        if !pointer.is_null() {
            LIVE_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        if !pointer.is_null()
            && WATCH.load(Ordering::Relaxed)
            && layout.size() == BATCH_SIZE * size_of::<ScanEvent>()
        {
            BATCH_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        }
        pointer
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
        unsafe { System.dealloc(pointer, layout) };
    }
}

#[global_allocator]
static ALLOCATOR: MeasuredAllocator = MeasuredAllocator;

#[test]
fn consumed_scan_batches_reuse_storage_without_losing_files_or_final_totals() {
    let root = std::env::temp_dir().join(format!("wb-recycling-delivery-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    for file in 0..2048 {
        fs::write(root.join(format!("file-{file:04}.bin")), b"abc").unwrap();
    }
    let mut identities = HashSet::with_capacity(2048);
    let mut files = 0;
    let mut bytes = 0;
    let mut summary = None;
    let mut completed = false;
    let mut root_id = None;
    let (controller, receiver) = ScanController::recycling_bounded_channel(1);
    BATCH_ALLOCATIONS.store(0, Ordering::Relaxed);
    WATCH.store(true, Ordering::Relaxed);
    let handle = controller.start_scan(ScanRequest {
        root_path: root.clone(),
        config: ScanRuntimeConfig {
            root_path: root.clone(),
            backend: ScanBackend::DirectoryWalk,
            pipeline: ScanPipelineConfig {
                batch_size: BATCH_SIZE,
                ..Default::default()
            },
            ..Default::default()
        },
    });
    drop(controller);
    receiver.for_each_event(|event| {
        assert!(!completed, "events followed terminal completion");
        match event {
            ScanEvent::DirectoryFound(directory) => root_id = Some(directory.id),
            ScanEvent::FileFound(file) => {
                assert_eq!(file.parent_directory_id, root_id.unwrap());
                assert!(identities.insert(file.id));
                files += 1;
                bytes += file.size_bytes;
            }
            ScanEvent::Summary(value) => summary = Some(value),
            ScanEvent::Completed => completed = true,
            ScanEvent::Failed(message) => panic!("fixture scan failed: {message}"),
            ScanEvent::Cancelled | ScanEvent::Issue(_) => panic!("owned fixture must complete"),
            _ => {}
        }
    });
    handle.join();
    WATCH.store(false, Ordering::Relaxed);
    let allocations = BATCH_ALLOCATIONS.load(Ordering::Relaxed);
    assert!(completed);
    assert_eq!((files, bytes), (2048, 6144));
    let summary = summary.unwrap();
    assert_eq!(summary.files_seen, files);
    assert_eq!(summary.total_size_bytes, bytes);
    eprintln!(
        "batch-sized allocations={allocations}; batch bytes={}",
        BATCH_SIZE * size_of::<ScanEvent>()
    );
    assert!(
        allocations <= 32,
        "streaming 2048 files allocated {allocations} batch-sized arenas"
    );

    // Caller-selected oversized batches must not remain in the recycling pool.
    // Measure while the receiver is still alive at terminal delivery, rather
    // than after its destructor has released any accidentally retained arenas.
    let baseline = LIVE_BYTES.load(Ordering::Relaxed);
    let (controller, receiver) = ScanController::recycling_bounded_channel(1);
    let handle = controller.start_scan(ScanRequest {
        root_path: root.clone(),
        config: ScanRuntimeConfig {
            root_path: root.clone(),
            backend: ScanBackend::DirectoryWalk,
            pipeline: ScanPipelineConfig {
                batch_size: 1024,
                ..Default::default()
            },
            ..Default::default()
        },
    });
    drop(controller);
    let mut oversized_files = 0;
    let mut retained_at_completion = None;
    receiver.for_each_event(|event| match event {
        ScanEvent::FileFound(_) => oversized_files += 1,
        ScanEvent::Completed => {
            retained_at_completion =
                Some(LIVE_BYTES.load(Ordering::Relaxed).saturating_sub(baseline))
        }
        ScanEvent::Failed(message) => panic!("oversized fixture scan failed: {message}"),
        ScanEvent::Cancelled | ScanEvent::Issue(_) => panic!("owned fixture must complete"),
        _ => {}
    });
    handle.join();
    fs::remove_dir_all(&root).unwrap();
    assert_eq!(oversized_files, 2048);
    let retained = retained_at_completion.unwrap();
    eprintln!("retained bytes at oversized-batch completion={retained}");
    assert!(
        retained < 64 * 1024,
        "oversized batch retained {retained} bytes at completion"
    );
}
