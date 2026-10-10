use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use winblaze_core::{DirectoryId, DirectoryRecord, FileId, FileRecord};
use winblaze_index::BufferedIndexTransaction;

thread_local! {
    static WATCH: Cell<bool> = const { Cell::new(false) };
    static RECORD_ARRAY_BYTES: Cell<usize> = const { Cell::new(0) };
    static CATALOG_CAPACITY_BYTES: Cell<usize> = const { Cell::new(0) };
}

struct MeasuredAllocator;
// SAFETY: allocation and deallocation delegate unchanged to System; the
// thread-local measurement performs no allocation itself.
unsafe impl GlobalAlloc for MeasuredAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        WATCH.with(|watch| {
            if watch.get() && layout.size() >= 16_384 * std::mem::size_of::<u32>() {
                CATALOG_CAPACITY_BYTES.with(|bytes| bytes.set(bytes.get() + layout.size()));
            }
            if watch.get() && layout.size() >= 16_384 * std::mem::size_of::<FileRecord>() {
                RECORD_ARRAY_BYTES.with(|bytes| bytes.set(bytes.get() + layout.size()));
            }
        });
        // SAFETY: delegate the caller's valid layout to the system allocator.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: pointer and layout are forwarded to the originating allocator.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: MeasuredAllocator = MeasuredAllocator;

#[test]
fn publishing_unestimated_files_does_not_retain_large_unused_record_capacity() {
    const COUNT: usize = 20_000;
    let mut transaction = BufferedIndexTransaction::default();
    for id in 1..=COUNT as u64 {
        transaction.insert_file(FileRecord {
            id: FileId(id),
            size_bytes: id * 3,
            ..FileRecord::default()
        });
    }
    let (_, directories, files) = transaction.into_record_vecs();
    assert!(directories.is_empty());
    assert_eq!(files.len(), COUNT);
    for (position, file) in files.iter().enumerate() {
        assert_eq!(file.id.0, position as u64 + 1);
        assert_eq!(file.size_bytes, file.id.0 * 3);
    }
    assert!(
        files.capacity() <= COUNT + COUNT / 8,
        "published catalog retains {} record slots for {COUNT} files",
        files.capacity()
    );
}

#[test]
fn reserving_and_filling_dense_file_ids_uses_compact_lookup_capacity() {
    const COUNT: usize = 16_384;
    let mut transaction = BufferedIndexTransaction::default();
    CATALOG_CAPACITY_BYTES.with(|bytes| bytes.set(0));
    WATCH.with(|watch| watch.set(true));
    transaction.reserve(COUNT, 0);
    for position in 0..COUNT as u64 {
        let id = (position * 7_919) % COUNT as u64 + 1;
        transaction.insert_file(FileRecord {
            id: FileId(id),
            size_bytes: id * 3,
            ..FileRecord::default()
        });
    }
    WATCH.with(|watch| watch.set(false));
    let snapshot = transaction.snapshot_files();
    assert_eq!(snapshot.len(), COUNT);
    for (position, file) in snapshot.iter().enumerate() {
        assert_eq!(file.id.0, position as u64 + 1);
        assert_eq!(file.size_bytes, file.id.0 * 3);
    }
    let budget =
        COUNT * std::mem::size_of::<FileRecord>() + (COUNT + 1) * std::mem::size_of::<u32>();
    let observed = CATALOG_CAPACITY_BYTES.with(Cell::get);
    assert!(
        observed <= budget,
        "dense catalog large allocations {observed} exceed record-plus-lookup budget {budget}"
    );
}

#[test]
fn publishing_large_unestimated_catalog_avoids_a_replacement_record_arena() {
    const COUNT: usize = 100_000;
    let mut transaction = BufferedIndexTransaction::default();
    for id in 1..=COUNT as u64 {
        transaction.insert_file(FileRecord {
            id: FileId(id),
            size_bytes: id * 3,
            ..FileRecord::default()
        });
    }
    RECORD_ARRAY_BYTES.with(|bytes| bytes.set(0));
    WATCH.with(|watch| watch.set(true));
    let (_, directories, files) = transaction.into_record_vecs();
    WATCH.with(|watch| watch.set(false));
    assert!(directories.is_empty());
    assert_eq!(files.len(), COUNT);
    assert!(files.capacity() <= COUNT + COUNT / 8);
    for (position, file) in files.iter().enumerate() {
        assert_eq!(file.id.0, position as u64 + 1);
        assert_eq!(file.size_bytes, file.id.0 * 3);
    }
    assert_eq!(
        RECORD_ARRAY_BYTES.with(Cell::get),
        0,
        "publication must transfer the bounded arena without allocating a replacement"
    );
}

#[test]
fn dense_removals_and_sparse_transition_preserve_every_surviving_file() {
    let records: Vec<_> = (0..100)
        .map(|id| FileRecord {
            id: FileId(id),
            size_bytes: id * 3,
            name: format!("file-{id}").into(),
            ..FileRecord::default()
        })
        .collect();
    let mut transaction = BufferedIndexTransaction::default();
    transaction.reserve(records.len(), 0);
    for record in &records {
        transaction.insert_file(record.clone());
    }
    let mut kept: Vec<_> = records
        .iter()
        .filter(|record| record.id.0 % 3 != 0)
        .cloned()
        .collect();
    transaction.apply_incremental_files(&records, &kept);
    transaction = transaction.clone();
    let replacement = FileRecord {
        id: FileId(u64::MAX),
        size_bytes: 123,
        ..FileRecord::default()
    };
    transaction.insert_file(replacement.clone());
    kept.push(replacement);
    transaction.insert_file(records[0].clone());
    kept.push(records[0].clone());
    kept.sort_unstable_by_key(|record| record.id.0);
    assert_eq!(transaction.snapshot_files(), kept);
    let mut reversed = BufferedIndexTransaction::default();
    for record in kept.iter().rev() {
        reversed.insert_file(record.clone());
    }
    // Incremental history differs from a newly built transaction. Compare the
    // complete file values, and check value equality with matching history.
    assert_eq!(transaction.snapshot_files(), reversed.snapshot_files());
    assert_eq!(transaction.clone(), transaction);
    let mut rebuilt = BufferedIndexTransaction::default();
    for record in &kept {
        rebuilt.insert_file(record.clone());
    }
    assert_eq!(rebuilt, reversed);
}

#[test]
fn consuming_catalog_records_does_not_allocate_a_second_record_array() {
    let mut transaction = BufferedIndexTransaction::default();
    transaction.reserve(16_384, 0);
    for id in (1..=16_384).rev() {
        transaction.insert_file(FileRecord {
            id: FileId(id),
            size_bytes: id * 3,
            ..FileRecord::default()
        });
    }
    WATCH.with(|watch| watch.set(true));
    let (_, _, mut files) = transaction.into_record_vecs();
    WATCH.with(|watch| watch.set(false));
    assert_eq!(files.len(), 16_384);
    files.sort_unstable_by_key(|file| file.id.0);
    for (index, file) in files.iter().enumerate() {
        assert_eq!(file.id.0, index as u64 + 1);
        assert_eq!(file.size_bytes, file.id.0 * 3);
    }
    assert_eq!(
        RECORD_ARRAY_BYTES.with(Cell::get),
        0,
        "catalog publication must transfer its record allocation rather than duplicate it"
    );
}

#[test]
fn late_total_record_estimates_do_not_expand_existing_record_storage() {
    let mut transaction = BufferedIndexTransaction::default();
    transaction.reserve(16_384, 16_384);
    for id in 1..=16_384 {
        transaction.insert_file(FileRecord {
            id: FileId(id),
            ..FileRecord::default()
        });
        transaction.insert_directory(DirectoryRecord {
            id: DirectoryId(id),
            ..DirectoryRecord::default()
        });
    }
    RECORD_ARRAY_BYTES.with(|bytes| bytes.set(0));
    WATCH.with(|watch| watch.set(true));
    transaction.reserve(16_384, 16_384);
    transaction.reserve(8_192, 8_192);
    WATCH.with(|watch| watch.set(false));
    assert_eq!(
        RECORD_ARRAY_BYTES.with(Cell::get),
        0,
        "a total estimate must not be added to records already ingested"
    );
    assert_eq!(transaction.snapshot_files().len(), 16_384);
    assert_eq!(transaction.snapshot_directories().len(), 16_384);
}

#[test]
fn removals_and_replacements_preserve_sparse_ids_and_transaction_value_equality() {
    let records: Vec<_> = [1, 2, u64::MAX]
        .into_iter()
        .map(|id| FileRecord {
            id: FileId(id),
            size_bytes: 7,
            ..FileRecord::default()
        })
        .collect();
    let mut transaction = BufferedIndexTransaction::default();
    let mut reversed = BufferedIndexTransaction::default();
    for record in &records {
        transaction.insert_file(record.clone());
    }
    for record in records.iter().rev() {
        reversed.insert_file(record.clone());
    }
    assert_eq!(transaction, reversed);
    let kept = records[1..].to_vec();
    transaction.apply_incremental_files(&records, &kept);
    transaction.apply_incremental_files(&records, &kept);
    let replacement = FileRecord {
        size_bytes: 123,
        ..records[2].clone()
    };
    transaction.insert_file(replacement.clone());
    assert_eq!(
        transaction.snapshot_files(),
        vec![records[1].clone(), replacement]
    );
    transaction.insert_file(records[0].clone());
    let snapshot = transaction.snapshot_files();
    assert_eq!(snapshot.len(), 3);
    assert_eq!(snapshot[2].id, FileId(u64::MAX));
    assert_eq!(snapshot[2].size_bytes, 123);
}
