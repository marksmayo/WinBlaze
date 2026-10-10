use std::{ffi::c_void, fs};
use winblaze_native::{
    api::{WbCStringView, WbPermissionRow},
    bridge::wb_permission_rows,
};
fn view(text: &str) -> WbCStringView {
    WbCStringView {
        ptr: text.as_ptr().cast(),
        len: text.len(),
    }
}
extern "C" fn collect(row: *const WbPermissionRow, data: *mut c_void) {
    unsafe {
        let row = &*row;
        let bytes = std::slice::from_raw_parts(row.kind.ptr.cast::<u8>(), row.kind.len);
        (&mut *data.cast::<Vec<String>>()).push(String::from_utf8_lossy(bytes).into_owned());
    }
}
#[test]
fn exposes_readable_permission_rows_and_reports_missing_files_without_callbacks() {
    let path = std::env::temp_dir().join(format!("wb-acl-rows-{}.txt", std::process::id()));
    fs::write(&path, b"ACL fixture").unwrap();
    let mut rows = Vec::<String>::new();
    assert_eq!(
        wb_permission_rows(
            view(&path.to_string_lossy()),
            Some(collect),
            (&mut rows as *mut Vec<_>).cast()
        ),
        0
    );
    assert!(rows.iter().any(|kind| kind == "Owner"));
    assert!(rows.iter().any(|kind| kind == "Allow"));
    let count = rows.len();
    fs::remove_file(&path).unwrap();
    assert_eq!(
        wb_permission_rows(
            view(&path.to_string_lossy()),
            Some(collect),
            (&mut rows as *mut Vec<_>).cast()
        ),
        3
    );
    assert_eq!(rows.len(), count);
}
