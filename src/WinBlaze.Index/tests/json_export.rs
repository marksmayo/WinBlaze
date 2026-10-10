use winblaze_core::{DirectoryId, DirectoryRecord};
use winblaze_index::{BufferedIndexTransaction, IndexTransaction, TreeIndex};

#[test]
fn json_exports_a_versioned_complete_catalog_with_numeric_totals() {
    let mut tx = BufferedIndexTransaction::default();
    tx.upsert_directory(&DirectoryRecord {
        id: DirectoryId(1),
        parent_directory_id: None,
        name: "root".into(),
        full_path: "C:\\root".into(),
        direct_bytes: 0,
        total_bytes: 0,
        direct_entries: 0,
        total_entries: 0,
    });
    let mut output = Vec::new();
    winblaze_index::export::write_json(&TreeIndex::build(tx), &mut output).unwrap();
    assert_eq!(String::from_utf8(output).unwrap(), "{\"schema_version\":1,\"entries\":[{\"Path\":\"C:\\\\root\",\"Kind\":\"Directory\",\"LogicalBytes\":0,\"AllocatedBytes\":0,\"Files\":0,\"Directories\":0,\"ModifiedUnixSeconds\":null}]}\n");
}

#[test]
fn json_escapes_all_controls_and_preserves_unicode() {
    let mut tx = BufferedIndexTransaction::default();
    tx.upsert_directory(&DirectoryRecord {
        id: DirectoryId(1),
        parent_directory_id: None,
        name: "root".into(),
        full_path: "日本\"\\\n\r\t\u{0}\u{8}\u{c}\u{1f}".into(),
        direct_bytes: 0,
        total_bytes: 0,
        direct_entries: 0,
        total_entries: 0,
    });
    let mut output = Vec::new();
    winblaze_index::export::write_json(&TreeIndex::build(tx), &mut output).unwrap();
    assert!(String::from_utf8(output)
        .unwrap()
        .contains("日本\\\"\\\\\\n\\r\\t\\u0000\\u0008\\u000c\\u001f"));
}

#[test]
fn json_exports_every_file_with_derived_paths_and_large_integer_bytes() {
    use winblaze_core::{FileAttributes, FileId, FileRecord};
    let mut tx = BufferedIndexTransaction::default();
    tx.upsert_directory(&DirectoryRecord {
        id: DirectoryId(90),
        parent_directory_id: None,
        name: "root".into(),
        full_path: "C:\\root\\".into(),
        direct_bytes: 0,
        total_bytes: 0,
        direct_entries: 0,
        total_entries: 0,
    });
    for id in 0..8200 {
        tx.upsert_file(&FileRecord {
            id: FileId(id),
            parent_directory_id: DirectoryId(90),
            name: "日本.txt".into(),
            full_path: String::new().into(),
            size_bytes: 9007199254740993,
            allocation_bytes: 4096,
            attributes: FileAttributes::ARCHIVE,
            timestamps: winblaze_core::FileTimestamps::new(None, Some(116444734770000000), None),
        });
    }
    let mut out = Vec::new();
    winblaze_index::export::write_json(&TreeIndex::build(tx), &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text.matches("\"Kind\":\"File\"").count(), 8200);
    assert_eq!(text.matches("C:\\\\root\\\\日本.txt").count(), 8200);
    assert_eq!(
        text.matches("\"LogicalBytes\":9007199254740993,").count(),
        8200
    );
    assert_eq!(text.matches("\"ModifiedUnixSeconds\":-123").count(), 8201);
}

#[test]
fn json_propagates_destination_errors() {
    let tree = TreeIndex::build(BufferedIndexTransaction::default());
    let mut destination = std::io::Cursor::new([0u8; 2]);
    assert_eq!(
        winblaze_index::export::write_json(&tree, &mut destination)
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::WriteZero
    );
}

#[test]
fn json_reopens_offline_without_losing_large_integer_sizes_or_utc_dates() {
    let csv = "Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\nZ:\\offline,Directory,9007199254740993,4096,1,0,\nZ:\\offline\\日本.bin,File,9007199254740993,4096,0,0,-123\n";
    let tree = winblaze_index::import::read_csv(csv.as_bytes()).unwrap();
    let mut json = Vec::new();
    winblaze_index::export::write_json(&tree, &mut json).unwrap();
    let reopened = winblaze_index::import::read_json(json.as_slice()).unwrap();
    assert_eq!(reopened.files()[0].name.as_ref(), "日本.bin");
    assert_eq!(reopened.files()[0].size_bytes, 9007199254740993);
    assert_eq!(reopened.files()[0].modified_utc(), Some(116444734770000000));
}

#[test]
fn json_rejects_ambiguous_duplicate_keys_instead_of_silently_replacing_metadata() {
    let scan = r#"{"schema_version":1,"schema_version":2,"entries":[]}"#;
    assert!(winblaze_index::import::read_json(scan.as_bytes()).is_err());
    let scan = r#"{"schema_version":2,"schema_version":1,"entries":[{"Path":"Z:\\root","Kind":"Directory","LogicalBytes":0,"AllocatedBytes":0,"Files":0,"Directories":0,"ModifiedUnixSeconds":null}]}"#;
    assert!(winblaze_index::import::read_json(scan.as_bytes()).is_err());
}

#[test]
fn json_reopens_windirstat_scan_with_owner_and_utc_metadata() {
    let scan = r#"[
        {"Name":"Z:\\offline","Files":1,"Folders":0,"Logical Size":7,"Physical Size":4096,"Attributes":"D","Last Change":"2026-01-01T00:00:00Z","WinDirStat Attributes":"0x00000004","Index":"0x00000001","Owner":"DOMAIN\\user"},
        {"Name":"Z:\\offline\\日本.bin","Files":0,"Folders":0,"Logical Size":7,"Physical Size":4096,"Attributes":"A","Last Change":"2026-01-01T00:00:00Z","WinDirStat Attributes":"0x00000008","Index":"0x00000002","Owner":"DOMAIN\\user"}
    ]"#;
    let tree = winblaze_index::import::read_json(scan.as_bytes()).unwrap();
    assert_eq!(tree.files()[0].name.as_ref(), "日本.bin");
    assert_eq!(tree.files()[0].size_bytes, 7);
    assert_eq!(tree.files()[0].allocation_bytes, 4096);
    assert_eq!(tree.files()[0].modified_utc(), Some(134116992000000000));
}

#[test]
fn json_roundtrip_retains_attributes_creation_and_access_dates() {
    use winblaze_core::{FileAttributes, FileId, FileRecord};
    let mut tx = BufferedIndexTransaction::default();
    tx.upsert_directory(&DirectoryRecord {
        id: DirectoryId(1),
        parent_directory_id: None,
        name: "offline".into(),
        full_path: "Z:\\offline".into(),
        direct_bytes: 0,
        total_bytes: 0,
        direct_entries: 0,
        total_entries: 0,
    });
    tx.upsert_file(&FileRecord {
        id: FileId(2),
        parent_directory_id: DirectoryId(1),
        name: "a.bin".into(),
        full_path: String::new().into(),
        size_bytes: 7,
        allocation_bytes: 4096,
        attributes: FileAttributes(0x822),
        timestamps: winblaze_core::FileTimestamps::new(
            Some(116444736100000000),
            Some(116444736200000000),
            Some(116444736300000000),
        ),
    });
    let mut output = Vec::new();
    winblaze_index::export::write_json(&TreeIndex::build(tx), &mut output).unwrap();
    let reopened = winblaze_index::import::read_json(output.as_slice()).unwrap();
    let file = &reopened.files()[0];
    assert_eq!(file.attributes, FileAttributes(0x822));
    assert_eq!(file.created_utc(), Some(116444736100000000));
    assert_eq!(file.accessed_utc(), Some(116444736300000000));
}

#[test]
fn json_reopen_retains_owner_and_lossless_physical_identity() {
    let input=br#"{"schema_version":1,"entries":[{"Path":"C:\\root\\a","Kind":"File","LogicalBytes":3,"AllocatedBytes":4096,"Files":0,"Directories":0,"ModifiedUnixSeconds":null,"Owner":"DOMAIN\\Example","VolumeIdentity":"18446744073709551615","FileIdentity":"340282366920938463463374607431768211455","LinkCount":2}]}"#;
    let tree = winblaze_index::import::read_json(input.as_slice()).unwrap();
    let mut output = Vec::new();
    winblaze_index::export::write_json(&tree, &mut output).unwrap();
    let json: serde_json::Value = serde_json::from_slice(&output).unwrap();
    let file = json["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["Kind"] == "File")
        .unwrap();
    assert_eq!(file["Owner"], "DOMAIN\\Example");
    assert_eq!(file["VolumeIdentity"], "18446744073709551615");
    assert_eq!(
        file["FileIdentity"],
        "340282366920938463463374607431768211455"
    );
    assert_eq!(file["LinkCount"], 2);
}

#[test]
fn reopens_localized_windirstat_json_and_retains_optional_owner() {
    let input = r#"[{"Name":"Z:\\offline\\a.bin","Dateien":0,"Ordner":0,"Logische Größe":7,"Physische Größe":4096,"Attribute":"A","Letzte Änderung":"2026-01-01T00:00:00Z","WinDirStat-Attribute":"0x20000008","Index":"1","Besitzer":"DOMAIN\\user"}]"#;
    let tree = winblaze_index::import::read_json(input.as_bytes()).unwrap();
    assert_eq!(tree.files()[0].size_bytes, 7);
    assert_eq!(
        tree.file_export_metadata(tree.files()[0].id.0)
            .unwrap()
            .owner
            .as_deref(),
        Some("DOMAIN\\user")
    );
}

#[test]
fn rejects_malformed_physical_identity_metadata() {
    let input=br#"{"schema_version":1,"entries":[{"Path":"C:\\root\\a","Kind":"File","LogicalBytes":3,"AllocatedBytes":4096,"Files":0,"Directories":0,"ModifiedUnixSeconds":null,"FileIdentity":"not-a-number"}]}"#;
    assert!(winblaze_index::import::read_json(input.as_slice()).is_err());
}

#[test]
fn refuses_unsupported_or_inconsistent_localized_json_columns() {
    let valid = r#"{"Name":"Z:\\offline\\a.bin","Dateien":0,"Ordner":0,"Logische Größe":7,"Physische Größe":4096,"Attribute":"A","Letzte Änderung":"2026-01-01T00:00:00Z","WinDirStat-Attribute":"0x20000008","Index":"1","Besitzer":"DOMAIN\\user"}"#;
    for invalid in [
        valid.replace("WinDirStat-Attribute", "UnknownFlags"),
        valid.replace("\"Dateien\":0", "\"Dateien\":\"invalid\""),
        valid.replace("\"Attribute\":\"A\"", "\"Attribute\":32"),
        valid.replace("\"Physische Größe\":4096", "\"Physische Größe\":-1"),
    ] {
        assert!(winblaze_index::import::read_json(format!("[{invalid}]").as_bytes()).is_err());
    }
    let second = valid.replace("a.bin", "b.bin").replace("Besitzer", "Owner");
    assert!(winblaze_index::import::read_json(format!("[{valid},{second}]").as_bytes()).is_err());
}

#[test]
fn localized_json_without_optional_owner_reopens() {
    let input = r#"[{"Name":"Z:\\offline\\a.bin","Dateien":0,"Ordner":0,"Logische Größe":7,"Physische Größe":4096,"Attribute":"A","Letzte Änderung":"2026-01-01T00:00:00Z","WinDirStat-Attribute":"0x20000008","Index":"1"}]"#;
    let tree = winblaze_index::import::read_json(input.as_bytes()).unwrap();
    assert_eq!(tree.files()[0].size_bytes, 7);
    assert_eq!(tree.files()[0].allocation_bytes, 4096);
}
