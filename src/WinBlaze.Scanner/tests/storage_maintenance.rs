use std::{fs, os::windows::fs::MetadataExt};
use winblaze_scanner::storage_maintenance::set_compression;
#[test]
fn compression_is_reversible_and_preserves_every_file_byte() {
    let root = std::env::temp_dir().join(format!("wb-compression-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let file = root.join("data.bin");
    let content = vec![7u8; 1024 * 1024];
    fs::write(&file, &content).unwrap();
    set_compression(&file, true).unwrap();
    assert_ne!(fs::metadata(&file).unwrap().file_attributes() & 0x800, 0);
    assert_eq!(fs::read(&file).unwrap(), content);
    set_compression(&file, false).unwrap();
    assert_eq!(fs::metadata(&file).unwrap().file_attributes() & 0x800, 0);
    assert_eq!(fs::read(&file).unwrap(), content);
    assert!(set_compression(&root, true).is_err());
    assert!(set_compression(&root.join("missing"), true).is_err());
    fs::remove_file(file).unwrap();
    fs::remove_dir(root).unwrap();
}

#[test]
fn sparsification_punches_only_verified_zero_ranges_and_preserves_nonzero_bytes() {
    let file = std::env::temp_dir().join(format!("wb-sparsify-{}.bin", std::process::id()));
    let mut content = vec![0u8; 2 * 1024 * 1024 + 123];
    content[..4096].fill(7);
    content[1024 * 1024..1024 * 1024 + 4096].fill(9);
    content[2 * 1024 * 1024..].fill(11);
    fs::write(&file, &content).unwrap();
    let before = winblaze_scanner::file_details::query_file_details(&file)
        .unwrap()
        .allocation_bytes;
    winblaze_scanner::storage_maintenance::sparsify_zeros(&file).unwrap();
    assert_eq!(fs::read(&file).unwrap(), content);
    assert_ne!(fs::metadata(&file).unwrap().file_attributes() & 0x200, 0);
    assert!(
        winblaze_scanner::file_details::query_file_details(&file)
            .unwrap()
            .allocation_bytes
            < before
    );
    fs::remove_file(file).unwrap();
}
