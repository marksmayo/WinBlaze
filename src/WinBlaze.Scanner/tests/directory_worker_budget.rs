use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use winblaze_core::ScanEvent;
use winblaze_scanner::{
    ScanBackend, ScanController, ScanPipelineConfig, ScanRequest, ScanRuntimeConfig,
};

static WATCH: AtomicBool = AtomicBool::new(false);
static SCRATCH_BYTES: AtomicUsize = AtomicUsize::new(0);
struct MeasuredAllocator;
// SAFETY: all allocations delegate unchanged to System. Atomic counters do not allocate.
unsafe impl GlobalAlloc for MeasuredAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if WATCH.load(Ordering::Relaxed)
            && (16_384..=65_536).contains(&layout.size())
            && layout.align() == 8
        {
            SCRATCH_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        }
        // SAFETY: the caller's valid layout is forwarded unchanged.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: allocation pointer and layout are forwarded to their originating allocator.
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: MeasuredAllocator = MeasuredAllocator;

#[test]
fn default_directory_walk_bounds_worker_scratch_and_preserves_complete_totals() {
    let root = std::env::temp_dir().join(format!("wb-worker-budget-{}", std::process::id()));
    for child in 0..256 {
        let directory = root.join(format!("child-{child}"));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("file.txt"), b"abc").unwrap();
    }
    let (controller, rx) = ScanController::channel();
    let request = ScanRequest {
        root_path: root.clone(),
        config: ScanRuntimeConfig {
            root_path: root.clone(),
            backend: ScanBackend::DirectoryWalk,
            max_parallelism: 20,
            // Measure the default directory worker budget separately from
            // event arenas. Compact records can otherwise make a 512-event
            // arena exactly 64 KiB and misclassify it as enumeration scratch.
            // Keep arenas above the measured range rather than excluding a
            // legitimate scratch size or raising the scratch budget.
            pipeline: ScanPipelineConfig {
                batch_size: 65_536 / std::mem::size_of::<ScanEvent>() + 1,
                ..Default::default()
            },
            ..ScanRuntimeConfig::default()
        },
    };
    WATCH.store(true, Ordering::Relaxed);
    let handle = controller.start_scan(request);
    let mut files = 0;
    let mut directories = 0;
    let mut bytes = 0;
    let mut completed = false;
    for event in rx.into_iter().flatten() {
        match event {
            ScanEvent::FileFound(file) => {
                files += 1;
                bytes += file.size_bytes;
            }
            ScanEvent::DirectoryFound(_) => directories += 1,
            ScanEvent::Summary(summary) => {
                assert_eq!(summary.files_seen, 256);
                assert_eq!(summary.directories_seen, 257);
                assert_eq!(summary.total_size_bytes, 768);
            }
            ScanEvent::Completed => {
                completed = true;
                break;
            }
            ScanEvent::Failed(message) => panic!("scan failed: {message}"),
            ScanEvent::Cancelled | ScanEvent::Issue(_) => {
                panic!("owned fixture must complete without issues")
            }
            _ => {}
        }
    }
    handle.join();
    WATCH.store(false, Ordering::Relaxed);
    std::fs::remove_dir_all(root).unwrap();
    assert!(completed);
    assert_eq!((files, directories, bytes), (256, 257, 768));
    let scratch = SCRATCH_BYTES.load(Ordering::Relaxed);
    assert!(
        (1..=262_144).contains(&scratch),
        "default directory traversal allocated {scratch} scratch bytes; budget is four 64 KiB buffers"
    );
}
