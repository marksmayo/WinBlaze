use winblaze_native::capacity_hint::{load_hint, save_hint, scope_key, CapacityHint};

#[test]
fn returning_to_a_recent_root_preserves_its_own_allocation_estimate() {
    let root = std::env::temp_dir().join(format!("wb-capacity-alternating-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("capacity.bin");
    let rules = winblaze_scanner::ScanRules::default();
    let first = scope_key(r"C:\first", &rules);
    let second = scope_key(r"C:\second", &rules);
    save_hint(&path, &first, 100_000, 4_000).unwrap();
    save_hint(&path, &second, 200, 20).unwrap();
    assert_eq!(
        load_hint(&path, &first),
        Some(CapacityHint {
            files: 105_000,
            directories: 4_200,
        }),
        "scanning another root must not discard a recent root's allocation advice"
    );
    assert_eq!(
        load_hint(&path, &second),
        Some(CapacityHint {
            files: 210,
            directories: 21,
        })
    );
    assert_eq!(load_hint(&path, "an unscanned root"), None);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn advice_retention_is_bounded_and_updates_do_not_replace_other_scopes() {
    let root = std::env::temp_dir().join(format!("wb-capacity-retention-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("capacity.bin");
    let scopes: Vec<_> = (0..33)
        .map(|index| format!("{index:02}{}", "x".repeat(4094)))
        .collect();
    for (index, scope) in scopes.iter().take(32).enumerate() {
        save_hint(&path, scope, index + 10, 1).unwrap();
    }
    assert!(scopes
        .iter()
        .take(32)
        .all(|scope| load_hint(&path, scope).is_some()));
    save_hint(&path, &scopes[0], 500, 2).unwrap();
    save_hint(&path, &scopes[32], 100, 1).unwrap();
    assert_eq!(
        load_hint(&path, &scopes[1]),
        None,
        "oldest update is evicted"
    );
    assert_eq!(load_hint(&path, &scopes[0]).unwrap().files, 525);
    assert_eq!(load_hint(&path, &scopes[32]).unwrap().files, 105);
    assert_eq!(
        scopes
            .iter()
            .filter(|scope| load_hint(&path, scope).is_some())
            .count(),
        32
    );
    let before = std::fs::read(&path).unwrap();
    assert!(before.len() <= 12 + 32 * (20 + 4096));
    assert!(save_hint(&path, "invalid", usize::MAX, 1).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before);
    let mut oversized = before;
    oversized.push(0);
    std::fs::write(&path, oversized).unwrap();
    assert_eq!(load_hint(&path, &scopes[0]), None);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn malformed_multi_scope_advice_is_a_cache_miss() {
    let root =
        std::env::temp_dir().join(format!("wb-capacity-multi-corrupt-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("capacity.bin");
    save_hint(&path, "a", 100, 10).unwrap();
    save_hint(&path, "b", 200, 20).unwrap();
    let valid = std::fs::read(&path).unwrap();
    for size in 0..valid.len() {
        std::fs::write(&path, &valid[..size]).unwrap();
        assert_eq!(load_hint(&path, "a"), None);
    }
    let mut corruptions = Vec::new();
    for count in [0_u32, 33] {
        let mut bytes = valid.clone();
        bytes[8..12].copy_from_slice(&count.to_le_bytes());
        corruptions.push(bytes);
    }
    let mut excessive = valid.clone();
    excessive[12..20].copy_from_slice(&u64::MAX.to_le_bytes());
    corruptions.push(excessive);
    let mut scope_length = valid.clone();
    scope_length[28..32].copy_from_slice(&4097_u32.to_le_bytes());
    corruptions.push(scope_length);
    let mut empty_scope = valid.clone();
    empty_scope[28..32].copy_from_slice(&0_u32.to_le_bytes());
    corruptions.push(empty_scope);
    let mut duplicate = valid.clone();
    *duplicate.last_mut().unwrap() = b'a';
    corruptions.push(duplicate);
    let mut invalid_utf8 = valid.clone();
    *invalid_utf8.last_mut().unwrap() = 255;
    corruptions.push(invalid_utf8);
    let mut trailing = valid;
    trailing.push(0);
    corruptions.push(trailing);
    for bytes in corruptions {
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(load_hint(&path, "a"), None);
    }
    save_hint(&path, "recovered", 100, 10).unwrap();
    assert_eq!(load_hint(&path, "recovered").unwrap().files, 105);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn successful_catalog_counts_reopen_as_a_bounded_allocation_estimate() {
    let root = std::env::temp_dir().join(format!("wb-capacity-roundtrip-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("capacity.bin");
    save_hint(&path, r"C:\", 100_000, 4_000).unwrap();
    assert_eq!(
        load_hint(&path, r"C:\"),
        Some(CapacityHint {
            files: 105_000,
            directories: 4_200,
        })
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn allocation_estimates_never_cross_root_or_relative_drive_scopes() {
    let root = std::env::temp_dir().join(format!("wb-capacity-scope-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("capacity.bin");
    save_hint(&path, r"C:\", 100_000, 4_000).unwrap();
    for scope in [r"D:\", r"C:\folder", "C:"] {
        assert_eq!(load_hint(&path, scope), None, "different scope: {scope}");
    }
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn excessive_estimates_are_rejected_without_replacing_a_valid_hint() {
    let root = std::env::temp_dir().join(format!("wb-capacity-budget-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("capacity.bin");
    save_hint(&path, r"C:\", 100, 10).unwrap();
    let previous = std::fs::read(&path).unwrap();
    for (files, directories) in [(usize::MAX, 1), (1, usize::MAX), (100_000_000, 1)] {
        assert!(save_hint(&path, r"C:\", files, directories).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), previous);
    }
    assert!(save_hint(&path, &"x".repeat(4097), 1, 1).is_err());
    assert!(save_hint(&path, "", 1, 1).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn same_root_with_different_scan_rules_cannot_reuse_counts() {
    let root = std::env::temp_dir().join(format!("wb-capacity-rules-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("capacity.bin");
    let rules = winblaze_scanner::ScanRules::default();
    let scope = scope_key(r"C:\", &rules);
    save_hint(&path, &scope, 100, 10).unwrap();
    let filtered = winblaze_scanner::ScanRules {
        exclude_paths: vec!["Windows".into()],
        include_files: vec!["*.txt".into()],
    };
    assert_eq!(load_hint(&path, &scope_key(r"C:\", &filtered)), None);
    assert!(load_hint(&path, &scope_key(r"C:\", &rules.clone())).is_some());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn missing_corrupt_truncated_or_oversized_hints_are_ignored() {
    let root = std::env::temp_dir().join(format!("wb-capacity-corrupt-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("capacity.bin");
    assert_eq!(load_hint(&path, r"C:\"), None);
    save_hint(&path, r"C:\", 100, 10).unwrap();
    let valid = std::fs::read(&path).unwrap();
    for size in 0..valid.len() {
        std::fs::write(&path, &valid[..size]).unwrap();
        assert_eq!(load_hint(&path, r"C:\"), None);
    }
    for bytes in [vec![0; 8192], b"garbage".to_vec()] {
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(load_hint(&path, r"C:\"), None);
    }
    let mut excessive = valid.clone();
    excessive[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
    std::fs::write(&path, excessive).unwrap();
    assert_eq!(load_hint(&path, r"C:\"), None);
    let mut invalid_utf8 = valid;
    *invalid_utf8.last_mut().unwrap() = 255;
    std::fs::write(&path, invalid_utf8).unwrap();
    assert_eq!(load_hint(&path, r"C:\"), None);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn relative_root_keys_include_the_actual_working_directory() {
    let current = std::env::current_dir().unwrap();
    let rules = winblaze_scanner::ScanRules::default();
    let expected = scope_key(current.to_str().unwrap(), &rules);
    assert_eq!(scope_key(".", &rules), expected);
    assert_ne!(scope_key("another-relative-folder", &rules), expected);
}

#[test]
#[cfg(windows)]
fn unchanged_catalog_hint_does_not_need_write_access_or_replace_the_file() {
    use std::os::windows::fs::OpenOptionsExt;
    let root = std::env::temp_dir().join(format!("wb-capacity-idempotent-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("capacity.bin");
    let scope = scope_key(r"C:\owned-fixture", &winblaze_scanner::ScanRules::default());
    save_hint(&path, &scope, 72, 8).unwrap();
    // A reader permits other readers but forbids writes/deletion. Identical
    // advisory counts should be reusable without replacing a durable file.
    let reader = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    let before = std::fs::read(&path).unwrap();
    save_hint(&path, &scope, 72, 8).expect("unchanged counts must not require a write");
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(
        load_hint(&path, &scope),
        Some(CapacityHint {
            files: 75,
            directories: 8
        })
    );
    assert!(
        save_hint(&path, &scope, 73, 8).is_err(),
        "changed counts still need atomic replacement"
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
    drop(reader);
    save_hint(&path, &scope, 73, 8).unwrap();
    assert_eq!(
        load_hint(&path, &scope),
        Some(CapacityHint {
            files: 76,
            directories: 8
        })
    );
    assert_ne!(std::fs::read(&path).unwrap(), before);
    std::fs::remove_dir_all(root).unwrap();
}
