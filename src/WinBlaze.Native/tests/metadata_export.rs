use winblaze_native::{
    api::WbCStringView,
    bridge::{wb_export_json_metadata, wb_import_csv},
};
fn view(value: &str) -> WbCStringView {
    WbCStringView {
        ptr: value.as_ptr().cast(),
        len: value.len(),
    }
}
#[test]
fn detailed_json_exports_owner_and_real_physical_identity() {
    let root = std::env::temp_dir().join(format!("wb-detailed-json-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("a.bin");
    std::fs::write(&file, b"abc").unwrap();
    let input = root.join("input.csv");
    let output = root.join("output.json");
    std::fs::write(&input,format!("Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\n{},File,3,4096,0,0,\n",file.display())).unwrap();
    assert_eq!(wb_import_csv(view(&input.to_string_lossy())), 0);
    assert_eq!(wb_export_json_metadata(view(&output.to_string_lossy())), 0);
    let tree = winblaze_index::import::read_json(std::fs::File::open(&output).unwrap()).unwrap();
    let metadata = tree.file_export_metadata(tree.files()[0].id.0).unwrap();
    assert!(!metadata.owner.as_ref().unwrap().is_empty());
    assert_ne!(
        metadata
            .file_identity
            .as_ref()
            .unwrap()
            .parse::<u128>()
            .unwrap(),
        0
    );
    assert!(metadata
        .volume_identity
        .as_ref()
        .unwrap()
        .parse::<u64>()
        .is_ok());
    assert_eq!(metadata.link_count, Some(1));
    std::fs::remove_file(root.join("a.bin")).unwrap();
    std::fs::remove_file(input).unwrap();
    std::fs::remove_file(output).unwrap();
    std::fs::remove_dir(root).unwrap();
}
