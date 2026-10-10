use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use winblaze_core::{DirectoryId, DirectoryRecord, FileId, FileRecord};
use winblaze_index::{BufferedIndexTransaction, TreeIndex};

const FILE_COUNT: usize = 16_384;
thread_local! {
    static WATCH: Cell<bool> = const { Cell::new(false) };
    static LARGE_BYTES: Cell<usize> = const { Cell::new(0) };
    static TOTAL_BYTES: Cell<usize> = const { Cell::new(0) };
}

struct MeasuredAllocator;
// SAFETY: all allocations delegate unchanged to System. Measurement uses
// allocation-free thread-local counters on the caller's thread.
unsafe impl GlobalAlloc for MeasuredAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        WATCH.with(|watch| {
            if watch.get() {
                TOTAL_BYTES.with(|bytes| bytes.set(bytes.get().saturating_add(layout.size())));
            }
            if watch.get() && layout.size() >= FILE_COUNT * 16 {
                LARGE_BYTES.with(|bytes| bytes.set(bytes.get() + layout.size()));
            }
        });
        // SAFETY: forward the caller's valid layout unchanged.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: forward the pointer and original layout to its allocator.
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: MeasuredAllocator = MeasuredAllocator;

#[test]
fn small_catalog_persistence_round_trips_without_megabyte_scratch_allocations() {
    use winblaze_index::{IndexBackend, IndexRepository, SqliteIndexRepository};
    let root = std::env::temp_dir().join(format!(
        "winblaze-small-persist-{}-{:?}",
        std::process::id(),
        std::thread::current().id()
    ));
    let repository = SqliteIndexRepository::open_empty(&root, IndexBackend::BinaryCache);
    let files: Vec<_> = (0..72)
        .map(|id| FileRecord {
            id: FileId(id),
            name: format!("file-{id}.bin").into(),
            size_bytes: 1024,
            ..FileRecord::default()
        })
        .collect();
    LARGE_BYTES.with(|bytes| bytes.set(0));
    WATCH.with(|watch| watch.set(true));
    let result = repository.persist_sorted_records(&[], &[], &[], &files, &[], &[]);
    WATCH.with(|watch| watch.set(false));
    result.unwrap();
    let scratch = LARGE_BYTES.with(Cell::get);
    let reopened = SqliteIndexRepository::open(&root, IndexBackend::BinaryCache);
    assert_eq!(reopened.snapshot_files(), files);
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        scratch, 0,
        "small catalog requested {scratch} large scratch bytes"
    );
}

#[test]
fn dense_directory_tree_preserves_every_lookup_with_bounded_bookkeeping() {
    let mut transaction = BufferedIndexTransaction::default();
    transaction.reserve(0, FILE_COUNT);
    for id in 0..FILE_COUNT as u64 {
        transaction.insert_directory(DirectoryRecord {
            id: DirectoryId(id),
            parent_directory_id: (id != 0).then_some(DirectoryId(0)),
            ..DirectoryRecord::default()
        });
    }
    TOTAL_BYTES.with(|bytes| bytes.set(0));
    WATCH.with(|watch| watch.set(true));
    let tree = TreeIndex::build(transaction);
    WATCH.with(|watch| watch.set(false));
    for id in 0..FILE_COUNT as u64 {
        assert!(tree.rollup_for(id).is_some(), "lost directory {id}");
    }
    assert!(tree.rollup_for(FILE_COUNT as u64).is_none());
    assert!(tree.rollup_for(u64::MAX).is_none());
    assert_eq!(
        tree.rollup_for(0).unwrap().item_count,
        FILE_COUNT as u64 - 1
    );
    let bytes = TOTAL_BYTES.with(Cell::get);
    assert!(
        bytes <= FILE_COUNT * 104,
        "dense directory-tree bookkeeping allocated {bytes} bytes for {FILE_COUNT} directories"
    );
}

#[test]
fn building_an_ordered_catalog_preserves_every_file_without_large_temporary_arrays() {
    verify_catalog_budget(false);
}

#[test]
fn directory_lookup_preserves_boundary_ids_paths_and_file_parentage() {
    for ids in [
        vec![40, 41, 42],
        vec![u64::MAX - 2, u64::MAX - 1, u64::MAX],
        vec![0, 5, u64::MAX],
        vec![40, 42, 44],
    ] {
        let root = ids[0];
        let mut transaction = BufferedIndexTransaction::default();
        for (position, &id) in ids.iter().enumerate().rev() {
            transaction.insert_directory(DirectoryRecord {
                id: DirectoryId(id),
                parent_directory_id: (id != root).then_some(DirectoryId(root)),
                full_path: format!("C:\\fixture\\{position}"),
                ..DirectoryRecord::default()
            });
            transaction.insert_file(FileRecord {
                id: FileId(position as u64),
                parent_directory_id: DirectoryId(id),
                size_bytes: position as u64 + 1,
                allocation_bytes: (position as u64 + 1) * 2,
                ..FileRecord::default()
            });
        }
        let tree = TreeIndex::build(transaction);
        assert_eq!(tree.root().unwrap().0.id.0, root);
        let rollup = tree.rollup_for(root).unwrap();
        assert_eq!(
            (
                rollup.file_count,
                rollup.logical_bytes,
                rollup.physical_bytes
            ),
            (3, 6, 12)
        );
        for (position, &id) in ids.iter().enumerate() {
            assert_eq!(
                tree.directory_full_path(id),
                Some(format!("C:\\fixture\\{position}").as_str())
            );
            let mut files = Vec::new();
            assert!(tree
                .for_each_child(id, 0, 10, |entry| {
                    if let winblaze_index::TreeEntry::File(file) = entry {
                        files.push(file.id.0);
                    }
                })
                .is_some());
            assert_eq!(files, vec![position as u64]);
        }
        for absent in [1, 39, 43, u64::MAX - 3] {
            assert!(tree.rollup_for(absent).is_none());
            assert!(tree.directory_full_path(absent).is_none());
            assert!(tree.child_count(absent).is_none());
            assert!(tree.for_each_child(absent, 0, 10, |_| {}).is_none());
        }
    }
    let empty = TreeIndex::build(BufferedIndexTransaction::default());
    assert!(empty.root().is_none());
    assert!(empty.rollup_for(0).is_none());
    assert!(empty.rollup_for(u64::MAX).is_none());
}

#[test]
fn building_a_shuffled_dense_catalog_preserves_every_file_without_large_temporary_arrays() {
    verify_catalog_budget(true);
}

#[test]
fn catalog_ordering_preserves_sparse_and_boundary_ids_with_latest_replacement_values() {
    for ids in [
        vec![2, 0, 1],
        vec![u64::MAX, u64::MAX - 2, u64::MAX - 1],
        vec![u64::MAX, 0, 5],
        vec![8, 2, 8, 4],
        vec![],
    ] {
        let mut transaction = BufferedIndexTransaction::default();
        transaction.insert_directory(DirectoryRecord {
            id: DirectoryId(5),
            ..DirectoryRecord::default()
        });
        let mut expected = std::collections::BTreeMap::new();
        for (position, id) in ids.into_iter().enumerate() {
            let size = position as u64 + 1;
            expected.insert(id, size);
            transaction.insert_file(FileRecord {
                id: FileId(id),
                parent_directory_id: DirectoryId(5),
                size_bytes: size,
                allocation_bytes: size * 2,
                ..FileRecord::default()
            });
        }
        let tree = TreeIndex::build(transaction);
        let observed: Vec<_> = tree
            .files()
            .iter()
            .map(|file| (file.id.0, file.size_bytes))
            .collect();
        assert_eq!(
            observed,
            expected
                .iter()
                .map(|(&id, &size)| (id, size))
                .collect::<Vec<_>>()
        );
        let total: u64 = expected.values().sum();
        let rollup = tree.rollup_for(5).unwrap();
        assert_eq!(rollup.file_count, expected.len() as u64);
        assert_eq!(rollup.logical_bytes, total);
        assert_eq!(rollup.physical_bytes, total * 2);
    }
}

fn verify_catalog_budget(shuffled: bool) {
    let mut transaction = BufferedIndexTransaction::default();
    transaction.reserve(FILE_COUNT, 1);
    transaction.insert_directory(DirectoryRecord {
        id: DirectoryId(5),
        ..DirectoryRecord::default()
    });
    for position in 0..FILE_COUNT as u64 {
        // Multiplication by an odd number permutes this power-of-two fixture.
        let id = if shuffled {
            (position * 7_919) % FILE_COUNT as u64 + 1
        } else {
            position + 1
        };
        transaction.insert_file(FileRecord {
            id: FileId(id),
            parent_directory_id: DirectoryId(5),
            size_bytes: id,
            allocation_bytes: id * 2,
            ..FileRecord::default()
        });
    }
    LARGE_BYTES.with(|bytes| bytes.set(0));
    WATCH.with(|watch| watch.set(true));
    let tree = TreeIndex::build(transaction);
    WATCH.with(|watch| watch.set(false));
    assert_eq!(tree.files().len(), FILE_COUNT);
    for (index, file) in tree.files().iter().enumerate() {
        assert_eq!(file.id.0, index as u64 + 1);
        assert_eq!(file.size_bytes, file.id.0);
        assert_eq!(file.allocation_bytes, file.id.0 * 2);
    }
    let total = FILE_COUNT as u64 * (FILE_COUNT as u64 + 1) / 2;
    let rollup = tree.rollup_for(5).expect("root directory is retained");
    assert_eq!(rollup.file_count, FILE_COUNT as u64);
    assert_eq!(rollup.logical_bytes, total);
    assert_eq!(rollup.physical_bytes, total * 2);
    assert_eq!(
        LARGE_BYTES.with(Cell::get),
        0,
        "dense catalog construction must not allocate a volume-sized temporary array"
    );
}
