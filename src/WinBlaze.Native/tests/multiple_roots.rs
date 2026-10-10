use winblaze_native::{
    api::WbCStringView,
    bridge::{wb_export_csv, wb_scan_roots},
};
fn view(s: &str) -> WbCStringView {
    WbCStringView {
        ptr: s.as_ptr().cast(),
        len: s.len(),
    }
}
#[test]
fn scans_multiple_folders_atomically_and_preserves_original_export_paths() {
    let root = std::env::temp_dir().join(format!("wb-multi-{}", std::process::id()));
    std::fs::create_dir_all(root.join("one")).unwrap();
    std::fs::create_dir_all(root.join("two")).unwrap();
    std::fs::write(root.join("one/a.bin"), b"one").unwrap();
    std::fs::write(root.join("two/b.bin"), b"second").unwrap();
    let paths = format!(
        "{}\r{}",
        root.join("one").display(),
        root.join("two").display()
    );
    let result = wb_scan_roots(view(&paths));
    assert_eq!(result.status, 0);
    assert_eq!(result.roots, 2);
    assert_eq!(result.files, 2);
    assert_eq!(result.logical_bytes, 9);
    let output = root.join("combined.csv");
    assert_eq!(wb_export_csv(view(&output.to_string_lossy())), 0);
    let csv = std::fs::read_to_string(&output).unwrap();
    assert!(csv.contains(&root.join("one").join("a.bin").to_string_lossy().to_string()));
    assert!(csv.contains(&root.join("two").join("b.bin").to_string_lossy().to_string()));
    assert_eq!(
        wb_scan_roots(view(&format!(
            "{}\n{}",
            root.display(),
            root.join("one").display()
        )))
        .status,
        1
    );
    assert_eq!(wb_export_csv(view(&output.to_string_lossy())), 0);
    assert_eq!(std::fs::read_to_string(&output).unwrap(), csv);
    std::fs::hard_link(root.join("one/a.bin"), root.join("two/alias.bin")).unwrap();
    let result = wb_scan_roots(view(&paths));
    let expected = winblaze_scanner::file_details::query_file_details(&root.join("one/a.bin"))
        .unwrap()
        .allocation_bytes
        + winblaze_scanner::file_details::query_file_details(&root.join("two/b.bin"))
            .unwrap()
            .allocation_bytes;
    assert_eq!(result.status, 0);
    assert_eq!(result.files, 3);
    assert_eq!(result.logical_bytes, 12);
    assert_eq!(result.allocated_bytes, expected);
    std::fs::remove_dir_all(root).unwrap();
}
