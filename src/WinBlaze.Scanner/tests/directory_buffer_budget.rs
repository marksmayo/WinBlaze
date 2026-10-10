use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::fs;
use winblaze_scanner::directory_metadata::read_directory;

thread_local! {
    static WATCH: Cell<bool> = const { Cell::new(false) };
    static LARGE_ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
    static ALL_ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

struct MeasuredAllocator;
// SAFETY: every allocation operation delegates to System unchanged. Thread-local
// counters contain no allocations and only measure the invoking test thread.
unsafe impl GlobalAlloc for MeasuredAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        WATCH.with(|watch| {
            if watch.get() {
                ALL_ALLOCATIONS.with(|count| count.set(count.get() + 1));
                if layout.size() >= 65_536 {
                    LARGE_ALLOCATIONS.with(|count| count.set(count.get() + 1));
                }
            }
        });
        // SAFETY: layout is forwarded unchanged to the system allocator.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: ptr/layout are forwarded to the allocator that created them.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[test]
fn empty_directory_iteration_allocates_no_discarded_entry_names() {
    let root = std::env::temp_dir().join(format!("wb-empty-name-budget-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let mut entries = read_directory(&root).unwrap();
    ALL_ALLOCATIONS.with(|count| count.set(0));
    WATCH.with(|watch| watch.set(true));
    let next = entries.next();
    WATCH.with(|watch| watch.set(false));
    assert!(
        next.is_none(),
        "owned empty fixture must contain no visible entries"
    );
    let allocations = ALL_ALLOCATIONS.with(Cell::get);
    drop(entries);
    fs::remove_dir_all(root).unwrap();
    assert_eq!(
        allocations, 0,
        "empty traversal must not allocate discarded names"
    );
}

#[global_allocator]
static ALLOCATOR: MeasuredAllocator = MeasuredAllocator;

#[test]
fn sequential_directory_walk_reuses_bounded_scratch_without_stale_entries() {
    let root = std::env::temp_dir().join(format!("wb-buffer-budget-{}", std::process::id()));
    fs::create_dir_all(root.join("empty")).unwrap();
    fs::create_dir_all(root.join("populated")).unwrap();
    fs::write(root.join("populated/first.txt"), b"first").unwrap();
    WATCH.with(|watch| watch.set(true));
    for _ in 0..12 {
        let entries = read_directory(&root.join("populated"))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "first.txt");
        assert_eq!(entries[0].size_bytes, 5);
        assert_eq!(read_directory(&root.join("empty")).unwrap().count(), 0);
    }
    WATCH.with(|watch| watch.set(false));
    let allocations = LARGE_ALLOCATIONS.with(Cell::get);
    fs::remove_dir_all(root).unwrap();
    assert!(
        allocations <= 1,
        "directory scratch allocated {allocations} times"
    );
}
