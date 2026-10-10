//! Public directory iteration allocation budgets, independent of timing.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
    fs,
    time::{SystemTime, UNIX_EPOCH},
};
struct MeasuredAllocator;
thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static BYTES: Cell<usize> = const { Cell::new(0) };
    static LIVE: Cell<isize> = const { Cell::new(0) };
}
// SAFETY: every request is forwarded unchanged to System. The thread-local
// counters never allocate; measurement excludes fixture construction.
unsafe impl GlobalAlloc for MeasuredAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ENABLED.try_with(Cell::get).unwrap_or(false) {
            let _ = BYTES.try_with(|bytes| bytes.set(bytes.get().saturating_add(layout.size())));
            let _ = LIVE.try_with(|bytes| bytes.set(bytes.get() + layout.size() as isize));
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if ENABLED.try_with(Cell::get).unwrap_or(false) {
            let _ = LIVE.try_with(|bytes| bytes.set(bytes.get() - layout.size() as isize));
        }
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if ENABLED.try_with(Cell::get).unwrap_or(false) {
            let _ = BYTES.try_with(|bytes| bytes.set(bytes.get().saturating_add(size)));
            let _ = LIVE
                .try_with(|bytes| bytes.set(bytes.get() + size as isize - layout.size() as isize));
        }
        unsafe { System.realloc(pointer, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: MeasuredAllocator = MeasuredAllocator;

fn measured<T>(operation: impl FnOnce() -> T) -> (T, usize) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ENABLED.set(false);
        }
    }
    BYTES.set(0);
    LIVE.set(0);
    ENABLED.set(true);
    let reset = Reset;
    let result = operation();
    drop(reset);
    (result, BYTES.get())
}

#[test]
fn dense_directory_enumeration_batches_metadata_requests() {
    let root = std::env::temp_dir().join(format!(
        "winblaze-dense-requests-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    for id in 0..4096 {
        fs::write(root.join(format!("file-000-000-{id:04}.bin")), []).unwrap();
    }
    let mut iterator = winblaze_scanner::directory_metadata::read_directory(&root).unwrap();
    let mut count = 0;
    for entry in iterator.by_ref() {
        entry.unwrap();
        count += 1;
    }
    let requests = iterator.metadata_reads();
    drop(iterator);
    for id in 0..4096 {
        fs::remove_file(root.join(format!("file-000-000-{id:04}.bin"))).unwrap();
    }
    fs::remove_dir(root).unwrap();
    assert_eq!(count, 4096);
    assert!(
        requests <= 12,
        "dense enumeration used {requests} metadata requests"
    );
}

#[test]
fn short_directory_names_decode_without_per_entry_heap_storage() {
    let root = std::env::temp_dir().join(format!(
        "winblaze-inline-names-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    for id in 0..1024 {
        fs::write(root.join(format!("file-000-000-{id:04}.bin")), []).unwrap();
    }
    let mut iterator = winblaze_scanner::directory_metadata::read_directory(&root).unwrap();
    // Warm active enumeration scratch, which is deliberately released at EOF.
    // Measure the remaining names separately from the bounded kernel buffer.
    let mut warmed = 0;
    while iterator.metadata_reads() < 2 {
        iterator.next().unwrap().unwrap();
        warmed += 1;
    }
    let (count, bytes) = measured(|| {
        let mut count = 0;
        for entry in iterator {
            let entry = entry.unwrap();
            assert!(entry.name.to_string_lossy().starts_with("file-000-000-"));
            count += 1;
        }
        count
    });
    for id in 0..1024 {
        fs::remove_file(root.join(format!("file-000-000-{id:04}.bin"))).unwrap();
    }
    fs::remove_dir(root).unwrap();
    assert_eq!(count + warmed, 1024);
    assert!(count > 512, "measure most names after scratch growth");
    assert!(
        bytes <= 1024,
        "short directory names allocated {bytes} bytes"
    );
}

#[test]
fn unused_event_pipeline_allocates_no_batch_storage() {
    let (tx, _rx) = std::sync::mpsc::channel();
    let (mut pipeline, bytes) = measured(|| {
        winblaze_scanner::pipeline::ScanEventPipeline::new(
            tx,
            winblaze_scanner::performance::ScanPipelineConfig::default(),
        )
    });
    pipeline.flush();
    assert_eq!(bytes, 0, "unused event pipeline allocated {bytes} bytes");
}

#[test]
fn flushed_event_batch_preserves_records_without_reserving_an_unused_successor() {
    use winblaze_core::{FileId, FileRecord, ScanEvent};
    use winblaze_scanner::{ScanEventPipeline, ScanPipelineConfig};
    let (tx, rx) = std::sync::mpsc::channel();
    let config = ScanPipelineConfig {
        batch_size: 1024,
        max_in_flight_events: 2048,
        max_in_flight_bytes: 1 << 20,
    };
    let mut pipeline = ScanEventPipeline::new(tx, config);
    pipeline.emit_file(FileRecord {
        id: FileId(7),
        size_bytes: 23,
        ..Default::default()
    });
    let (_, bytes) = measured(|| pipeline.flush());
    let batch = rx.try_recv().unwrap();
    assert!(
        matches!(batch.as_slice(), [ScanEvent::FileFound(file)] if file.id == FileId(7) && file.size_bytes == 23)
    );
    pipeline.emit_completed();
    assert!(matches!(
        rx.try_recv().unwrap().as_slice(),
        [ScanEvent::Completed]
    ));
    assert!(rx.try_recv().is_err());
    assert!(
        bytes <= 4096,
        "flushing allocated {bytes} bytes for an unused successor"
    );
}

#[test]
fn first_event_batch_preserves_every_record_with_bounded_growth() {
    use winblaze_core::{FileId, FileRecord, ScanEvent};
    use winblaze_scanner::{performance::ScanPipelineConfig, pipeline::ScanEventPipeline};
    let config = ScanPipelineConfig::default();
    let records: Vec<_> = (0..config.batch_size)
        .map(|id| FileRecord {
            id: FileId(id as u64),
            size_bytes: 256,
            ..Default::default()
        })
        .collect();
    let (tx, rx) = std::sync::mpsc::channel();
    let mut pipeline = ScanEventPipeline::new(tx, config);
    let (_, bytes) = measured(|| {
        for record in records {
            pipeline.emit_file(record);
        }
    });
    let batch = rx.try_recv().unwrap();
    assert_eq!(batch.len(), config.batch_size);
    for (id, event) in batch.into_iter().enumerate() {
        match event {
            ScanEvent::FileFound(file) => {
                assert_eq!(file.id, FileId(id as u64));
                assert_eq!(file.size_bytes, 256);
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }
    pipeline.emit_completed();
    assert!(matches!(
        rx.try_recv().unwrap().as_slice(),
        [ScanEvent::Completed]
    ));
    assert!(rx.try_recv().is_err());
    let budget = 2 * config.batch_size * std::mem::size_of::<ScanEvent>() + 4096;
    assert!(
        bytes <= budget,
        "first batch allocated {bytes} bytes, budget {budget}"
    );
}

#[test]
fn batched_names_use_one_owned_utf16_copy_per_entry() {
    let root = std::env::temp_dir().join(format!(
        "winblaze-enum-budget-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    for id in 0..3000 {
        fs::write(root.join(format!("budget-unicode-Ω-{id:08}.dat")), b"x").unwrap();
    }
    let iterator = winblaze_scanner::directory_metadata::read_directory(&root).unwrap();
    let (count, bytes) = measured(|| {
        iterator
            .map(|entry| {
                let entry = entry.unwrap();
                assert_eq!(entry.size_bytes, 1);
                1
            })
            .sum::<usize>()
    });
    assert_eq!(count, 3000);
    assert!(
        bytes < 300_000,
        "directory iteration allocated {bytes} bytes for 3000 names"
    );
    for entry in fs::read_dir(&root).unwrap() {
        fs::remove_file(entry.unwrap().path()).unwrap();
    }
    fs::remove_dir(root).unwrap();
}

#[test]
fn physical_identity_budget_stores_volume_once_instead_of_per_file() {
    let (bytes_claimed, bytes) = measured(|| {
        let mut tracker = winblaze_scanner::allocation::AllocationTracker::default();
        (1..=100_000u128)
            .map(|id| tracker.claim((123, id), 4096))
            .sum::<u64>()
    });
    assert_eq!(bytes_claimed, 4096 * 100_000);
    assert!(
        bytes < 5_000_000,
        "identity tracking allocated {bytes} bytes cumulatively"
    );
}
#[test]
fn common_64_bit_file_identities_fit_a_compact_budget_without_truncating_wide_ids() {
    let (claimed, bytes) = measured(|| {
        let mut tracker = winblaze_scanner::allocation::AllocationTracker::default();
        let mut total = (1..=100_000u128)
            .map(|id| tracker.claim((123, id), 4096))
            .sum::<u64>();
        total += tracker.claim((123, 1 + (1u128 << 64)), 8192);
        assert_eq!(tracker.claim((123, 1 + (1u128 << 64)), 8192), 0);
        total
    });
    assert_eq!(claimed, 4096 * 100_000 + 8192);
    assert!(
        bytes < 2_600_000,
        "compact identities allocated {bytes} bytes"
    );
}

#[test]
fn single_volume_hardlink_tracking_uses_bounded_storage_across_all_shards() {
    let (tracker, _) = measured(|| {
        let tracker = winblaze_scanner::allocation::ConcurrentAllocationTracker::default();
        for id in 0..64 {
            assert_eq!(tracker.claim((123, id), 4096), 4096);
        }
        tracker
    });
    let retained = LIVE.get() as usize + std::mem::size_of_val(&tracker);
    eprintln!("single-volume identity tracker retained {retained} bytes for 64 files");
    // Include inline state: moving heap allocations into the struct alone
    // cannot satisfy this complete retained-storage budget.
    assert!(
        retained <= 16 * 1024,
        "single-volume identity tracking retained {retained} bytes for 64 files"
    );
    for id in 0..64 {
        assert_eq!(tracker.claim((123, id), 4096), 0);
        assert_eq!(tracker.claim((124, id), 8192), 8192);
        assert_eq!(tracker.claim((124, id), 8192), 0);
    }
    let wide = (1u128 << 64) + 7;
    assert_eq!(tracker.claim((123, wide), 16384), 16384);
    assert_eq!(tracker.claim((123, wide), 16384), 0);
    assert_eq!(tracker.claim((123, 7), 4096), 0);
}
#[test]
fn duplicate_detection_reuses_comparison_buffers_across_candidates() {
    let root = std::env::temp_dir().join(format!(
        "winblaze-duplicate-budget-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let paths: Vec<_> = (0..20)
        .map(|id| root.join(format!("{id:02}.bin")))
        .collect();
    for path in &paths {
        fs::write(path, [7u8; 1024]).unwrap();
    }
    let (result, bytes) =
        measured(|| winblaze_scanner::content::find_duplicates(paths.iter().cloned()));
    for path in paths {
        fs::remove_file(path).unwrap();
    }
    fs::remove_dir(root).unwrap();
    let result = result.unwrap();
    assert_eq!(result.groups.len(), 1);
    assert_eq!(result.groups[0].len(), 20);
    assert_eq!(result.reclaimable_bytes, 19 * 1024);
    assert!(
        bytes < 50_000,
        "duplicate detection allocated {bytes} bytes"
    );
}

#[test]
fn large_clustered_hardlink_scans_retain_bounded_exact_identity_storage() {
    let (tracker, _) = measured(|| {
        let tracker = winblaze_scanner::allocation::ConcurrentAllocationTracker::default();
        for id in 0..1_000_000u128 {
            assert_eq!(tracker.claim((123, (7 << 48) + id), 4096), 4096);
        }
        tracker
    });
    let retained = LIVE.get() as usize + std::mem::size_of_val(&tracker);
    eprintln!("million-file identity tracker retained {retained} bytes");
    assert!(retained < 12 * 1024 * 1024, "retained {retained} bytes");
    for id in (0..1_000_000u128).rev() {
        assert_eq!(tracker.claim((123, (7 << 48) + id), 4096), 0);
    }
    for id in [0, u64::MAX as u128, 1u128 << 64, u128::MAX] {
        assert_eq!(tracker.claim((123, id), 8192), 8192);
        assert_eq!(tracker.claim((123, id), 8192), 0);
        assert_eq!(tracker.claim((124, id), 16384), 16384);
        assert_eq!(tracker.claim((124, id), 16384), 0);
    }
}

#[test]
fn scattered_file_identities_keep_bounded_storage_without_false_duplicates() {
    let (tracker, _) = measured(|| {
        let tracker = winblaze_scanner::allocation::ConcurrentAllocationTracker::default();
        for id in 0..1_000_000u64 {
            let file = id.wrapping_mul(0x9e37_79b9_7f4a_7c15);
            assert_eq!(tracker.claim((123, u128::from(file)), 4096), 4096);
        }
        tracker
    });
    let retained = LIVE.get() as usize + std::mem::size_of_val(&tracker);
    eprintln!("scattered million-file identity tracker retained {retained} bytes");
    assert!(retained < 20 * 1024 * 1024, "retained {retained} bytes");
    for id in (0..1_000_000u64).rev() {
        let file = id.wrapping_mul(0x9e37_79b9_7f4a_7c15);
        assert_eq!(tracker.claim((123, u128::from(file)), 4096), 0);
    }
}

#[test]
fn rare_distant_file_references_do_not_defeat_compact_exact_storage() {
    let (tracker, _) = measured(|| {
        let tracker = winblaze_scanner::allocation::ConcurrentAllocationTracker::default();
        for id in 0..1_000_000u64 {
            assert_eq!(
                tracker.claim((123, (7u128 << 48) + u128::from(id)), 4096),
                4096
            );
            if id % 100 == 0 {
                let distant = (id / 100).wrapping_mul(0x9e37_79b9_7f4a_7c15);
                assert_eq!(tracker.claim((123, u128::from(distant)), 8192), 8192);
            }
        }
        tracker
    });
    let retained = LIVE.get() as usize + std::mem::size_of_val(&tracker);
    eprintln!("clustered identities with distant outliers retained {retained} bytes");
    assert!(retained < 12 * 1024 * 1024, "retained {retained} bytes");
    for id in (0..1_000_000u64).rev() {
        assert_eq!(
            tracker.claim((123, (7u128 << 48) + u128::from(id)), 4096),
            0
        );
        if id % 100 == 0 {
            let distant = (id / 100).wrapping_mul(0x9e37_79b9_7f4a_7c15);
            assert_eq!(tracker.claim((123, u128::from(distant)), 8192), 0);
        }
    }
}
#[test]
fn terminal_pipeline_flush_does_not_allocate_an_unused_full_batch() {
    let (tx, rx) = std::sync::mpsc::channel();
    let mut pipeline = winblaze_scanner::pipeline::ScanEventPipeline::new(tx, Default::default());
    let (_, bytes) = measured(|| pipeline.emit_completed());
    assert!(matches!(
        rx.recv().unwrap().as_slice(),
        [winblaze_core::ScanEvent::Completed]
    ));
    assert!(bytes < 4096, "terminal flush allocated {bytes} bytes");
    pipeline.emit_cancelled();
    assert!(matches!(
        rx.recv().unwrap().as_slice(),
        [winblaze_core::ScanEvent::Cancelled]
    ));
}
#[test]
fn empty_duplicates_still_validate_metadata_and_allocate_no_content_buffers() {
    let root = std::env::temp_dir().join(format!(
        "winblaze-empty-duplicate-budget-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let paths: Vec<_> = (0..2).map(|id| root.join(format!("{id}.bin"))).collect();
    for path in &paths {
        fs::write(path, []).unwrap();
    }
    let (result, bytes) =
        measured(|| winblaze_scanner::content::find_duplicates(paths.iter().cloned()));
    for path in paths {
        fs::remove_file(path).unwrap();
    }
    fs::remove_dir(root).unwrap();
    let result = result.unwrap();
    assert_eq!(result.groups.len(), 1);
    assert_eq!(result.reclaimable_bytes, 0);
    assert!(result.errors.is_empty());
    assert!(
        bytes < 10_000,
        "empty duplicate analysis allocated {bytes} bytes"
    );
}
