use std::{ffi::c_void, fs};
use winblaze_native::{
    api::{WbCStringView, WbDuplicateRow},
    bridge::{wb_duplicate_results, wb_import_csv},
};
fn view(text: &str) -> WbCStringView {
    WbCStringView {
        ptr: text.as_ptr().cast(),
        len: text.len(),
    }
}
extern "C" fn collect(row: *const WbDuplicateRow, data: *mut c_void) {
    unsafe {
        let row = &*row;
        let bytes = std::slice::from_raw_parts(row.digest.ptr.cast::<u8>(), row.digest.len);
        (&mut *data.cast::<Vec<(u64, u64, String)>>()).push((
            row.group,
            row.logical_bytes,
            String::from_utf8_lossy(bytes).into_owned(),
        ));
    }
}
#[test]
fn exposes_verified_group_rows_with_selected_hash_and_reclaimable_bytes() {
    let root = std::env::temp_dir().join(format!("wb-dupe-results-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let mut csv = String::from(
        "Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\n",
    );
    for (name, bytes) in [("a", b"abc"), ("b", b"abc"), ("c", b"abd")] {
        let path = root.join(name);
        fs::write(&path, bytes).unwrap();
        csv.push_str(&format!("{},File,3,4096,0,0,\n", path.display()));
    }
    let input = root.join("scan.csv");
    fs::write(&input, csv).unwrap();
    assert_eq!(wb_import_csv(view(&input.to_string_lossy())), 0);
    let mut rows = Vec::<(u64, u64, String)>::new();
    let result = wb_duplicate_results(1, Some(collect), (&mut rows as *mut Vec<_>).cast());
    assert_eq!(result.status, 0);
    assert_eq!(result.groups, 1);
    assert_eq!(result.errors, 0);
    assert_eq!(result.reclaimable_bytes, 3);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row.0 == 1
        && row.1 == 3
        && row.2 == "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"));
    assert_eq!(
        wb_duplicate_results(9, None, std::ptr::null_mut()).status,
        1
    );
    for name in ["a", "b", "c", "scan.csv"] {
        fs::remove_file(root.join(name)).unwrap();
    }
    fs::remove_dir(root).unwrap();
}
