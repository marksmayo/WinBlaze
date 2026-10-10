use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::mpsc;
use winblaze_core::ScanEvent;
use winblaze_scanner::performance::ScanPipelineConfig;
use winblaze_scanner::pipeline::ScanEventPipeline;

thread_local! {
    static WATCH: Cell<bool> = const { Cell::new(false) };
    static BYTES: Cell<usize> = const { Cell::new(0) };
}

struct MeasuredAllocator;
// SAFETY: allocation pointers and layouts pass unchanged to System; counters do not allocate.
unsafe impl GlobalAlloc for MeasuredAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        WATCH.with(|watch| {
            if watch.get() {
                BYTES.with(|bytes| bytes.set(bytes.get() + layout.size()));
            }
        });
        // SAFETY: the caller's valid layout is forwarded unchanged.
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: pointer and layout are forwarded to their originating allocator.
        unsafe { System.dealloc(pointer, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: MeasuredAllocator = MeasuredAllocator;

#[test]
fn standalone_progress_is_delivered_promptly_without_full_batch_allocation() {
    let (sender, receiver) = mpsc::channel();
    let mut pipeline = ScanEventPipeline::new(sender, ScanPipelineConfig::default());
    // Initialize channel storage outside the measured interval.
    pipeline.emit_progress(0, 64, 1024);
    drop(receiver.try_recv().unwrap());
    BYTES.with(|bytes| bytes.set(0));
    WATCH.with(|watch| watch.set(true));
    for completed in 1..=16 {
        pipeline.emit_progress(completed, 64, 1024);
        let batch = receiver
            .try_recv()
            .expect("progress must arrive without explicit flush");
        assert_eq!(batch.len(), 1);
        match &batch[0] {
            ScanEvent::Progress(progress) => {
                assert_eq!(progress.completed_items, completed);
                assert_eq!(progress.total_items, 64);
                assert_eq!(progress.total_bytes, 1024);
            }
            event => panic!("unexpected event: {event:?}"),
        }
    }
    WATCH.with(|watch| watch.set(false));
    let bytes = BYTES.with(Cell::get);
    assert!(
        bytes <= 16_384,
        "sixteen standalone progress events allocated {bytes} bytes"
    );
    pipeline.emit_completed();
    assert!(matches!(
        receiver.try_recv().unwrap().as_slice(),
        [ScanEvent::Completed]
    ));
    assert!(receiver.try_recv().is_err());
}
