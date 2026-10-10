use winblaze_native::{api::WbCStringView, bridge::wb_consolidate_duplicates};
fn view(value: &str) -> WbCStringView {
    WbCStringView {
        ptr: value.as_ptr().cast(),
        len: value.len(),
    }
}
#[test]
fn explicit_native_action_consolidates_verified_copies() {
    let root = std::env::temp_dir().join(format!("wb-native-hardlinks-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let master = root.join("a");
    let duplicate = root.join("b");
    std::fs::write(&master, b"identical").unwrap();
    std::fs::write(&duplicate, b"identical").unwrap();
    assert_eq!(
        wb_consolidate_duplicates(
            view(&master.to_string_lossy()),
            view(&duplicate.to_string_lossy())
        ),
        0
    );
    assert_eq!(
        winblaze_scanner::file_details::query_file_details(&master)
            .unwrap()
            .links,
        2
    );
    std::fs::remove_file(master).unwrap();
    std::fs::remove_file(duplicate).unwrap();
    std::fs::remove_dir(root).unwrap();
}
