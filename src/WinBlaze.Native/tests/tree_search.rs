use std::ffi::c_void;
use winblaze_native::{
    api::{WbCStringView, WbSearchFilter, WbTreeNode},
    bridge::{wb_import_csv, wb_tree_search},
};
fn view(text: &str) -> WbCStringView {
    WbCStringView {
        ptr: text.as_ptr().cast(),
        len: text.len(),
    }
}
extern "C" fn collect(node: *const WbTreeNode, data: *mut c_void) {
    unsafe {
        let node = &*node;
        let bytes = std::slice::from_raw_parts(node.name.ptr.cast::<u8>(), node.name.len);
        (&mut *data.cast::<Vec<String>>()).push(String::from_utf8_lossy(bytes).into_owned());
    }
}
#[test]
fn native_search_queries_the_complete_index_and_rejects_invalid_regex() {
    let fixture = std::env::temp_dir().join(format!("wb-search-{}.csv", std::process::id()));
    let mut csv = String::from(
        "Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\n",
    );
    for ordinal in 0..9000 {
        csv.push_str(&format!(
            "Z:\\offline\\file{ordinal:05}.bin,File,7,4096,0,0,1767225600\n"
        ));
    }
    std::fs::write(&fixture, csv).unwrap();
    assert_eq!(wb_import_csv(view(&fixture.to_string_lossy())), 0);
    let mut hits = Vec::<String>::new();
    let options = WbSearchFilter {
        pattern: view(r"file08999\.bin$"),
        mode: 3,
        flags: 64,
        limit: 50,
        ..Default::default()
    };
    let result = wb_tree_search(
        options,
        Some(collect),
        (&mut hits as *mut Vec<String>).cast(),
    );
    assert_eq!(result.status, 0);
    assert_eq!(result.total, 1);
    assert_eq!(result.emitted, 1);
    assert!(hits[0].ends_with("file08999.bin"));
    let result = wb_tree_search(
        WbSearchFilter {
            pattern: view("["),
            ..options
        },
        Some(collect),
        (&mut hits as *mut Vec<String>).cast(),
    );
    assert_eq!(result.status, 1);
    assert_eq!(hits.len(), 1);
    std::fs::remove_file(fixture).unwrap();
}
