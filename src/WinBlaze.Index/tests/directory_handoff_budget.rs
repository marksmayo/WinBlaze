use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use winblaze_core::{DirectoryId, DirectoryRecord};
use winblaze_index::BufferedIndexTransaction;

const DIRECTORY_COUNT: usize = 16_384;

#[test]
fn publishing_unestimated_directories_does_not_retain_large_unused_capacity() {
    const COUNT: usize = 20_000;
    let mut transaction = BufferedIndexTransaction::default();
    for id in 0..COUNT as u64 {
        transaction.insert_directory(DirectoryRecord {
            id: DirectoryId(id),
            direct_bytes: id * 3,
            ..DirectoryRecord::default()
        });
    }
    let (_, directories, files) = transaction.into_record_vecs();
    assert!(files.is_empty());
    assert_eq!(directories.len(), COUNT);
    for (id, directory) in directories.iter().enumerate() {
        assert_eq!(directory.id.0, id as u64);
        assert_eq!(directory.direct_bytes, id as u64 * 3);
    }
    assert!(
        directories.capacity() <= COUNT + COUNT / 8,
        "publication retained {} slots for {COUNT} directories",
        directories.capacity()
    );
}
thread_local! {
    static WATCH: Cell<bool> = const { Cell::new(false) };
    static LARGE_RECORD_BYTES: Cell<usize> = const { Cell::new(0) };
}
struct MeasuredAllocator;
// SAFETY: all memory operations delegate unchanged to System. The observer
// uses allocation-free counters local to the calling test thread.
unsafe impl GlobalAlloc for MeasuredAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        WATCH.with(|watch| {
            if watch.get()
                && layout.size() >= DIRECTORY_COUNT * std::mem::size_of::<DirectoryRecord>()
            {
                LARGE_RECORD_BYTES.with(|bytes| bytes.set(bytes.get() + layout.size()));
            }
        });
        // SAFETY: forward the caller's valid layout unchanged.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: preserve the pointer and layout of its original allocator.
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: MeasuredAllocator = MeasuredAllocator;

#[test]
fn publishing_directory_records_transfers_storage_without_a_second_record_array() {
    let mut transaction = BufferedIndexTransaction::default();
    transaction.reserve(0, DIRECTORY_COUNT);
    for id in (1..=DIRECTORY_COUNT as u64).rev() {
        transaction.insert_directory(DirectoryRecord {
            id: DirectoryId(id),
            direct_bytes: id * 3,
            ..DirectoryRecord::default()
        });
    }
    WATCH.with(|watch| watch.set(true));
    let (_, mut directories, files) = transaction.into_record_vecs();
    WATCH.with(|watch| watch.set(false));
    assert!(files.is_empty());
    assert_eq!(directories.len(), DIRECTORY_COUNT);
    directories.sort_unstable_by_key(|directory| directory.id.0);
    for (index, directory) in directories.iter().enumerate() {
        assert_eq!(directory.id.0, index as u64 + 1);
        assert_eq!(directory.direct_bytes, directory.id.0 * 3);
    }
    assert_eq!(
        LARGE_RECORD_BYTES.with(Cell::get),
        0,
        "directory publication must transfer its record storage instead of duplicating it"
    );
}

#[test]
fn directory_replacement_preserves_sparse_ids_and_insertion_independent_equality() {
    let records: Vec<_> = [0, 5, u64::MAX]
        .into_iter()
        .map(|id| DirectoryRecord {
            id: DirectoryId(id),
            direct_bytes: 7,
            full_path: format!("C:\\fixture\\{id}"),
            ..DirectoryRecord::default()
        })
        .collect();
    let mut transaction = BufferedIndexTransaction::default();
    let mut reversed = BufferedIndexTransaction::default();
    for record in &records {
        transaction.insert_directory(record.clone());
    }
    for record in records.iter().rev() {
        reversed.insert_directory(record.clone());
    }
    assert_eq!(transaction, reversed);
    let replacement = DirectoryRecord {
        direct_bytes: 123,
        ..records[2].clone()
    };
    transaction.insert_directory(replacement.clone());
    reversed.insert_directory(replacement.clone());
    assert_eq!(transaction, reversed);
    assert_eq!(transaction, transaction.clone());
    assert_eq!(
        transaction.snapshot_directories(),
        vec![records[0].clone(), records[1].clone(), replacement]
    );
    let (_, directories, _) = transaction.into_record_vecs();
    assert_eq!(directories.len(), 3);
    assert!(directories
        .iter()
        .any(|record| record.id.0 == u64::MAX && record.direct_bytes == 123));
}
