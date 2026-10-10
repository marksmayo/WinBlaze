//! Benchmarks only newly generated owned fixtures, never an existing user path.
use std::{fs, path::PathBuf};
use winblaze_native::{
    api::WbCStringView,
    bridge::{wb_consolidate_duplicates, wb_storage_maintenance},
};
fn view(value: &str) -> WbCStringView {
    WbCStringView {
        ptr: value.as_ptr().cast(),
        len: value.len(),
    }
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 || !["hardlinks", "compression", "sparsify"].contains(&args[1].as_str()) {
        eprintln!("usage: feature_maintenance <hardlinks|compression|sparsify> <new-owned-fixture-directory>");
        std::process::exit(2);
    }
    let root = PathBuf::from(&args[2]);
    fs::create_dir(&root).expect("fixture must be a new directory");
    if args[1] != "hardlinks" {
        let file = root.join("compressible.bin");
        let mut bytes = vec![0u8; 2 * 1024 * 1024];
        if args[1] == "sparsify" {
            bytes[..65536].fill(7);
            bytes[2 * 1024 * 1024 - 65536..].fill(9);
        }
        fs::write(&file, &bytes).unwrap();
        let before = winblaze_scanner::file_details::query_file_details(&file)
            .unwrap()
            .allocation_bytes;
        assert_eq!(
            wb_storage_maintenance(
                view(&file.to_string_lossy()),
                if args[1] == "compression" { 0 } else { 2 }
            ),
            0
        );
        assert_eq!(fs::read(&file).unwrap(), bytes);
        let after = winblaze_scanner::file_details::query_file_details(&file)
            .unwrap()
            .allocation_bytes;
        assert!(after < before);
        if args[1] == "compression" {
            assert_eq!(wb_storage_maintenance(view(&file.to_string_lossy()), 1), 0);
        }
        assert_eq!(fs::read(&file).unwrap(), bytes);
        fs::remove_file(file).unwrap();
        fs::remove_dir(root).unwrap();
        println!("{{\"action\":\"{}\",\"verified\":true,\"bytes_preserved\":true,\"reversible\":{},\"logical_bytes\":2097152,\"before_allocated\":{before},\"after_allocated\":{after}}}", args[1], args[1] == "compression");
        return;
    }
    let master = root.join("master.bin");
    let duplicate = root.join("duplicate.bin");
    let bytes = vec![7u8; 2 * 1024 * 1024];
    fs::write(&master, &bytes).expect("create owned master");
    fs::write(&duplicate, &bytes).expect("create owned duplicate");
    assert_eq!(
        wb_consolidate_duplicates(
            view(&master.to_string_lossy()),
            view(&duplicate.to_string_lossy())
        ),
        0
    );
    assert_eq!(fs::read(&master).unwrap(), bytes);
    assert_eq!(fs::read(&duplicate).unwrap(), bytes);
    let first = winblaze_scanner::file_details::query_file_details(&master).unwrap();
    let second = winblaze_scanner::file_details::query_file_details(&duplicate).unwrap();
    assert_eq!(first.identity, second.identity);
    assert_eq!(first.links, 2);
    fs::remove_file(master).unwrap();
    fs::remove_file(duplicate).unwrap();
    fs::remove_dir(root).unwrap();
    println!("{{\"action\":\"hardlinks\",\"verified\":true,\"bytes_preserved\":true,\"shared_identity\":true,\"logical_bytes\":2097152}}");
}
