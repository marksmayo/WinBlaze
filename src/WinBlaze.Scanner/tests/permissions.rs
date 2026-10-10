#[test]
fn exports_owner_group_and_dacl_and_surfaces_missing_file_errors() {
    let path = std::env::temp_dir().join(format!("winblaze-acl-{}.txt", std::process::id()));
    std::fs::write(&path, b"ACL fixture").unwrap();
    let descriptor = winblaze_scanner::permissions::security_descriptor(&path).unwrap();
    assert!(descriptor.contains("O:"));
    assert!(descriptor.contains("G:"));
    assert!(descriptor.contains("D:"));
    std::fs::remove_file(&path).unwrap();
    assert!(winblaze_scanner::permissions::security_descriptor(&path).is_err());
}

#[test]
fn explores_resolved_accounts_and_readable_rights_without_changing_the_acl() {
    let path = std::env::temp_dir().join(format!("wb-readable-acl-{}.txt", std::process::id()));
    std::fs::write(&path, b"ACL fixture").unwrap();
    let before = winblaze_scanner::permissions::security_descriptor(&path).unwrap();
    let rows = winblaze_scanner::permissions::permission_rows(&path).unwrap();
    let owner = rows.iter().find(|row| row.kind == "Owner").unwrap();
    assert!(owner.sid.starts_with("S-1-"));
    assert!(!owner.account.is_empty());
    assert_ne!(owner.account, owner.sid);
    assert!(rows.iter().any(|row| row.kind == "Group"));
    assert!(rows
        .iter()
        .any(|row| (row.kind == "Allow" || row.kind == "Deny") && !row.rights.is_empty()));
    assert_eq!(
        winblaze_scanner::permissions::security_descriptor(&path).unwrap(),
        before
    );
    std::fs::remove_file(&path).unwrap();
    assert!(winblaze_scanner::permissions::permission_rows(&path).is_err());
}

#[test]
fn readable_acl_distinguishes_denies_inheritance_empty_and_unrestricted_descriptors() {
    use winblaze_scanner::permissions::permission_rows_from_sddl;
    let rows =
        permission_rows_from_sddl("O:SYG:SYD:(A;;0x000f01ff;;;WD)(D;ID;0xe0100000;;;BU)").unwrap();
    let allow = rows.iter().find(|row| row.kind == "Allow").unwrap();
    assert!(allow.rights.contains("Read data"));
    assert!(allow.rights.contains("Take ownership"));
    assert!(!allow.inherited);
    let deny = rows.iter().find(|row| row.kind == "Deny").unwrap();
    assert!(deny.inherited);
    assert!(deny.rights.contains("Generic write"));
    assert!(deny.rights.contains("Synchronize"));
    assert!(permission_rows_from_sddl("O:SYG:SYD:")
        .unwrap()
        .iter()
        .any(|row| row.rights.contains("Empty DACL")));
    assert!(permission_rows_from_sddl("O:SYG:SY")
        .unwrap()
        .iter()
        .any(|row| row.rights.contains("no discretionary ACL")));
    assert!(permission_rows_from_sddl("not a descriptor").is_err());
}
