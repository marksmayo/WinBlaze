use winblaze_native::{
    api::WbCStringView,
    bridge::{wb_export_analysis, wb_import_csv},
};
fn view(s: &str) -> WbCStringView {
    WbCStringView {
        ptr: s.as_ptr().cast(),
        len: s.len(),
    }
}
#[test]
fn reopened_catalog_exports_verified_duplicates_and_real_acl_rows() {
    let root = std::env::temp_dir().join(format!("winblaze-analysis-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    for name in ["a", "b"] {
        std::fs::write(root.join(name), b"duplicate").unwrap();
    }
    let csv=format!("Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\n{},Directory,18,18,2,0,\n{},File,9,9,0,0,\n{},File,9,9,0,0,\n",root.display(),root.join("a").display(),root.join("b").display());
    let input = root.join("input.csv");
    std::fs::write(&input, csv).unwrap();
    assert_eq!(wb_import_csv(view(&input.to_string_lossy())), 0);
    let output = root.join("dupes.csv");
    assert_eq!(wb_export_analysis(view(&output.to_string_lossy()), 1), 0);
    let rows = std::fs::read_to_string(&output).unwrap();
    assert!(rows.contains("Group,Path,LogicalBytes,Status"));
    assert!(rows.contains("verified"));
    assert_eq!(rows.lines().count(), 3);
    let acl = root.join("acl.csv");
    assert_eq!(wb_export_analysis(view(&acl.to_string_lossy()), 2), 0);
    let rows = std::fs::read_to_string(&acl).unwrap();
    assert!(rows.contains("SDDL"));
    assert!(rows.contains("D:"));
    assert_eq!(rows.lines().count(), 4);
    assert_eq!(
        wb_export_analysis(view(&root.join("a").to_string_lossy()), 99),
        1
    );
    assert_eq!(std::fs::read(root.join("a")).unwrap(), b"duplicate");
    assert_eq!(
        wb_export_analysis(view(&root.join("a").to_string_lossy()), 1),
        1
    );
    assert_eq!(std::fs::read(root.join("a")).unwrap(), b"duplicate");
    for e in std::fs::read_dir(&root).unwrap() {
        std::fs::remove_file(e.unwrap().path()).unwrap();
    }
    std::fs::remove_dir(root).unwrap();
}
