//! Allocation budgets exercise public operations without wall-clock flakiness.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
use winblaze_core::{DirectoryId, DirectoryRecord, FileId, FileRecord};
use winblaze_index::{BufferedIndexTransaction, TreeIndex};

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

fn fixture(count: u64) -> BufferedIndexTransaction {
    let mut records = BufferedIndexTransaction::default();
    records.insert_directory(DirectoryRecord {
        id: DirectoryId(1),
        name: "root".into(),
        full_path: "C:\\root".into(),
        ..Default::default()
    });
    for id in (1..=count).rev() {
        records.insert_file(FileRecord {
            id: FileId(id),
            parent_directory_id: DirectoryId(1),
            name: format!("file-{id}.txt").into(),
            size_bytes: id,
            allocation_bytes: id,
            ..Default::default()
        });
    }
    records
}

#[test]
fn invalid_analysis_does_not_materialize_the_catalog_paths() {
    let tree = TreeIndex::build(fixture(20_000));
    let (result, bytes) =
        measured(|| winblaze_native::reports::write_analysis(&tree, 99, &mut std::io::sink()));
    assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::InvalidInput);
    assert!(bytes < 4096, "invalid analysis allocated {bytes} bytes");
}
#[test]
fn permission_csv_streams_quotes_and_error_prefixes_without_extra_strings() {
    let tree = TreeIndex::build(fixture(1000));
    let (result, bytes) =
        measured(|| winblaze_native::reports::write_analysis(&tree, 2, &mut std::io::sink()));
    result.unwrap();
    assert!(bytes < 250_000, "permission CSV allocated {bytes} bytes");
}
#[test]
fn analysis_csv_preserves_quoted_paths_and_propagates_writer_errors() {
    let mut records = fixture(1);
    let mut file = records.snapshot_files().remove(0);
    file.name = "quoted-\"Ω\".txt".into();
    records.insert_file(file);
    let tree = TreeIndex::build(records);
    let mut output = Vec::new();
    winblaze_native::reports::write_analysis(&tree, 2, &mut output).unwrap();
    let text = String::from_utf8(output).unwrap();
    assert!(text.contains("quoted-\"\"Ω\"\".txt\""));
    struct FailedWriter;
    impl std::io::Write for FailedWriter {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("destination failure"))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    assert!(winblaze_native::reports::write_analysis(&tree, 2, &mut FailedWriter).is_err());
}
