use std::os::windows::fs::MetadataExt;
use winblaze_native::{api::WbCStringView, bridge::wb_storage_maintenance};
fn view(s: &str) -> WbCStringView {
    WbCStringView {
        ptr: s.as_ptr().cast(),
        len: s.len(),
    }
}
#[test]
fn maintenance_boundary_validates_operations_and_preserves_file_content() {
    let file =
        std::env::temp_dir().join(format!("wb-native-maintenance-{}.bin", std::process::id()));
    let bytes = vec![7u8; 1024 * 1024];
    std::fs::write(&file, &bytes).unwrap();
    let path = file.to_string_lossy();
    assert_eq!(wb_storage_maintenance(view(&path), 99), 1);
    assert_eq!(wb_storage_maintenance(view(&path), 0), 0);
    assert_ne!(
        std::fs::metadata(&file).unwrap().file_attributes() & 0x800,
        0
    );
    assert_eq!(std::fs::read(&file).unwrap(), bytes);
    assert_eq!(wb_storage_maintenance(view(&path), 1), 0);
    assert_eq!(
        std::fs::metadata(&file).unwrap().file_attributes() & 0x800,
        0
    );
    std::fs::remove_file(file).unwrap();
}
