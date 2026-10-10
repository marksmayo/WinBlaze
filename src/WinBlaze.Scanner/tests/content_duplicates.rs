use std::fs;
use winblaze_scanner::content::find_duplicates;

#[test]
fn groups_only_exact_contents_and_reports_unreadable_candidates() {
    let root = std::env::temp_dir().join(format!("winblaze-dupes-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let mut paths = Vec::new();
    for (name, bytes) in [
        ("a.bin", b"same body"),
        ("b.txt", b"same body"),
        ("c.bin", b"different"),
    ] {
        let path = root.join(name);
        fs::write(&path, bytes).unwrap();
        paths.push(path);
    }
    paths.push(root.join("missing.bin"));
    let result = find_duplicates(paths).unwrap();
    assert_eq!(result.groups.len(), 1);
    assert_eq!(result.groups[0].len(), 2);
    assert_eq!(result.errors.len(), 1);
    assert_eq!(result.reclaimable_bytes, 9);
    for entry in fs::read_dir(&root).unwrap() {
        fs::remove_file(entry.unwrap().path()).unwrap();
    }
    fs::remove_dir(root).unwrap();
}

#[test]
fn hardlink_aliases_do_not_claim_reclaimable_space_and_late_differences_are_not_duplicates() {
    let root = std::env::temp_dir().join(format!("winblaze-dupes-links-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let mut bytes = vec![1u8; 131072];
    fs::write(root.join("a"), &bytes).unwrap();
    fs::hard_link(root.join("a"), root.join("alias")).unwrap();
    bytes[100000] = 2;
    fs::write(root.join("different"), bytes).unwrap();
    let result =
        find_duplicates([root.join("a"), root.join("alias"), root.join("different")]).unwrap();
    assert!(result.groups.is_empty());
    assert_eq!(result.reclaimable_bytes, 0);
    for entry in fs::read_dir(&root).unwrap() {
        fs::remove_file(entry.unwrap().path()).unwrap();
    }
    fs::remove_dir(root).unwrap();
}

#[cfg(windows)]
#[test]
fn default_duplicate_detection_skips_offline_content_without_reading_it() {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn SetFileAttributesW(path: *const u16, attributes: u32) -> i32;
    }
    let root = std::env::temp_dir().join(format!("wb-dupes-offline-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let local = root.join("local");
    let offline = root.join("offline");
    fs::write(&local, b"same bytes").unwrap();
    fs::write(&offline, b"same bytes").unwrap();
    let wide: Vec<_> = offline.as_os_str().encode_wide().chain(Some(0)).collect();
    assert_ne!(unsafe { SetFileAttributesW(wide.as_ptr(), 0x1000) }, 0);
    let report = find_duplicates([local, offline.clone()]).unwrap();
    assert!(report.groups.is_empty());
    assert_eq!(report.errors.len(), 1);
    assert!(report.errors[0].1.contains("offline"));
    assert_ne!(unsafe { SetFileAttributesW(wide.as_ptr(), 0x80) }, 0);
    fs::remove_file(offline).unwrap();
    fs::remove_file(root.join("local")).unwrap();
    fs::remove_dir(root).unwrap();
}

#[test]
fn exposes_selected_content_hashes_and_still_verifies_duplicate_bytes() {
    use winblaze_scanner::content::{find_duplicates_with_algorithm, HashAlgorithm};
    let root = std::env::temp_dir().join(format!("wb-dupes-hashes-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("a"), b"abc").unwrap();
    fs::write(root.join("b"), b"abc").unwrap();
    fs::write(root.join("c"), b"abd").unwrap();
    let report = find_duplicates_with_algorithm(
        [root.join("a"), root.join("b"), root.join("c")],
        HashAlgorithm::Sha256,
    )
    .unwrap();
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].len(), 2);
    assert_eq!(
        report.hashes[&root.join("a")],
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_ne!(
        report.hashes[&root.join("a")],
        report.hashes[&root.join("c")]
    );
    assert_eq!(report.reclaimable_bytes, 3);
    for name in ["a", "b", "c"] {
        fs::remove_file(root.join(name)).unwrap();
    }
    fs::remove_dir(root).unwrap();
}

#[test]
fn selected_algorithms_produce_their_standard_digest_lengths_and_values() {
    use winblaze_scanner::content::{find_duplicates_with_algorithm, HashAlgorithm};
    let root = std::env::temp_dir().join(format!("wb-hash-algorithms-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("a"), b"abc").unwrap();
    fs::write(root.join("b"), b"abc").unwrap();
    for (algorithm, expected) in [
        (HashAlgorithm::Md5,"900150983cd24fb0d6963f7d28e17f72"),
        (HashAlgorithm::Sha1,"a9993e364706816aba3e25717850c26c9cd0d89d"),
        (HashAlgorithm::Sha512,"ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f"),
    ] {
        let report = find_duplicates_with_algorithm([root.join("a"),root.join("b")],algorithm).unwrap();
        assert_eq!(report.groups.len(),1);
        assert_eq!(report.hashes[&root.join("a")],expected);
    }
    fs::remove_file(root.join("a")).unwrap();
    fs::remove_file(root.join("b")).unwrap();
    fs::remove_dir(root).unwrap();
}

#[cfg(windows)]
#[test]
fn guarded_content_reader_refuses_offline_files_before_returning_a_read_handle() {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn SetFileAttributesW(path: *const u16, attributes: u32) -> i32;
    }
    let path = std::env::temp_dir().join(format!("wb-content-guard-{}", std::process::id()));
    fs::write(&path, b"local fixture bytes").unwrap();
    let wide: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    assert_ne!(unsafe { SetFileAttributesW(wide.as_ptr(), 0x1000) }, 0);
    let refused = winblaze_scanner::content::open_local_file(&path).is_err();
    assert_ne!(unsafe { SetFileAttributesW(wide.as_ptr(), 0x80) }, 0);
    fs::remove_file(path).unwrap();
    assert!(refused);
}
