use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use winblaze_native::{
    api::WbCStringView,
    bridge::{wb_export_csv, wb_import_csv},
};

thread_local! {
    static WATCH: Cell<bool> = const { Cell::new(false) };
    static LARGE: Cell<usize> = const { Cell::new(0) };
}
struct Allocator;
// SAFETY: delegate layouts unchanged; allocation-free thread-local observation.
unsafe impl GlobalAlloc for Allocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        WATCH.with(|watch| {
            if watch.get() && layout.size() >= 262_144 {
                LARGE.with(|bytes| bytes.set(bytes.get() + layout.size()));
            }
        });
        // SAFETY: forward the caller's valid layout unchanged.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        // SAFETY: pointer and originating allocation layout are unchanged.
        unsafe { System.dealloc(pointer, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: Allocator = Allocator;
fn view(text: &str) -> WbCStringView {
    WbCStringView {
        ptr: text.as_ptr().cast(),
        len: text.len(),
    }
}
#[test]
fn small_csv_export_preserves_all_records_without_megabyte_scratch() {
    let root = std::env::temp_dir().join(format!("wb-small-export-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let input = root.join("input.csv");
    let output = root.join("output.csv");
    let mut csv = String::from(
        "Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\n",
    );
    for id in 0..72 {
        csv.push_str(&format!("C:\\fixture\\file-{id}.bin,File,1024,4096,0,0,\n"));
    }
    std::fs::write(&input, csv).unwrap();
    assert_eq!(wb_import_csv(view(&input.to_string_lossy())), 0);
    LARGE.with(|bytes| bytes.set(0));
    WATCH.with(|watch| watch.set(true));
    let result = wb_export_csv(view(&output.to_string_lossy()));
    WATCH.with(|watch| watch.set(false));
    assert_eq!(result, 0);
    let allocated = LARGE.with(Cell::get);
    // Re-import the actual output through the supported public interface.
    assert_eq!(wb_import_csv(view(&output.to_string_lossy())), 0);
    let text = std::fs::read_to_string(&output).unwrap();
    for id in 0..72 {
        assert!(text.contains(&format!("file-{id}.bin\",File,1024,4096,")));
    }
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        allocated, 0,
        "small export requested {allocated} large scratch bytes"
    );
}
