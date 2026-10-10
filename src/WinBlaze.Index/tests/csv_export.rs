use winblaze_core::{DirectoryId, DirectoryRecord, FileAttributes, FileId, FileRecord};
use winblaze_index::{BufferedIndexTransaction, IndexTransaction, TreeIndex};

#[test]
fn csv_exports_directory_rollups_with_a_stable_header() {
    let mut transaction = BufferedIndexTransaction::default();
    transaction.upsert_directory(&DirectoryRecord {
        id: DirectoryId(1),
        parent_directory_id: None,
        name: "root".into(),
        full_path: "C:\\root".into(),
        direct_bytes: 0,
        total_bytes: 0,
        direct_entries: 0,
        total_entries: 0,
    });
    let tree = TreeIndex::build(transaction);
    let mut csv = Vec::new();
    winblaze_index::export::write_csv(&tree, &mut csv).unwrap();
    assert_eq!(String::from_utf8(csv).unwrap(),
        "Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\r\n\"C:\\root\",Directory,0,0,0,0,\r\n");
}

#[test]
fn csv_preserves_unicode_quotes_and_exports_more_than_the_ui_limit() {
    let mut transaction = BufferedIndexTransaction::default();
    transaction.upsert_directory(&DirectoryRecord {
        id: DirectoryId(1),
        parent_directory_id: None,
        name: "root".into(),
        full_path: "C:\\root".into(),
        direct_bytes: 0,
        total_bytes: 0,
        direct_entries: 0,
        total_entries: 0,
    });
    for id in 0..8200 {
        transaction.upsert_file(&FileRecord {
            id: FileId(id + 2),
            parent_directory_id: DirectoryId(1),
            name: "日本,\"report\".txt".into(),
            full_path: String::new().into(),
            size_bytes: 7,
            allocation_bytes: 4096,
            attributes: FileAttributes::ARCHIVE,
            timestamps: winblaze_core::FileTimestamps::new(None, Some(116444737230000000), None),
        });
    }
    let mut csv = Vec::new();
    winblaze_index::export::write_csv(&TreeIndex::build(transaction), &mut csv).unwrap();
    let text = String::from_utf8(csv).unwrap();
    assert_eq!(text.lines().count(), 8202);
    assert!(text.contains("\"C:\\root\\日本,\"\"report\"\".txt\",File,7,4096,0,0,123\r\n"));
    assert!(text.contains(",Directory,57400,33587200,8200,0,123\r\n"));
}

#[test]
fn csv_returns_write_errors_instead_of_reporting_success() {
    struct Broken;
    impl std::io::Write for Broken {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "blocked",
            ))
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let tree = TreeIndex::build(BufferedIndexTransaction::default());
    assert_eq!(
        winblaze_index::export::write_csv(&tree, &mut Broken)
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::PermissionDenied
    );
}

#[test]
fn csv_root_totals_come_first_even_when_ids_sort_differently() {
    let mut tx = BufferedIndexTransaction::default();
    for (id, parent, path) in [
        (900, None, "C:\\root"),
        (1, Some(DirectoryId(900)), "C:\\root\\child"),
    ] {
        tx.upsert_directory(&DirectoryRecord {
            id: DirectoryId(id),
            parent_directory_id: parent,
            name: path.into(),
            full_path: path.into(),
            direct_bytes: 0,
            total_bytes: 0,
            direct_entries: 0,
            total_entries: 0,
        });
    }
    let mut csv = Vec::new();
    winblaze_index::export::write_csv(&TreeIndex::build(tx), &mut csv).unwrap();
    assert_eq!(
        String::from_utf8(csv).unwrap().lines().nth(1),
        Some("\"C:\\root\",Directory,0,0,0,1,")
    );
}

#[test]
fn filtered_csv_selects_all_matching_files_case_insensitively_without_directories() {
    let mut tx = BufferedIndexTransaction::default();
    for (id, name) in [(1, "one.BIN"), (2, "two.txt"), (3, "three.bin")] {
        tx.upsert_file(&FileRecord {
            id: FileId(id),
            parent_directory_id: DirectoryId(99),
            name: name.into(),
            full_path: format!("C:\\root\\{name}").into(),
            size_bytes: 7,
            allocation_bytes: 4096,
            attributes: FileAttributes::ARCHIVE,
            timestamps: winblaze_core::FileTimestamps::new(None, None, None),
        });
    }
    let mut csv = Vec::new();
    winblaze_index::export::write_csv_extension(&TreeIndex::build(tx), "*.bIn", &mut csv).unwrap();
    let text = String::from_utf8(csv).unwrap();
    assert_eq!(text.lines().count(), 3);
    assert!(text.contains("one.BIN"));
    assert!(text.contains("three.bin"));
    assert!(!text.contains("two.txt"));
    assert!(!text.contains(",Directory,"));
}

#[test]
fn filtered_csv_rejects_invalid_filters_without_writing_output() {
    let tree = TreeIndex::build(BufferedIndexTransaction::default());
    for invalid in [
        "", "*", "*.*", "b?n", "bin;txt", "foo/bar", ".", "bin\\txt", "bin.txt",
    ] {
        let mut output = Vec::new();
        assert_eq!(
            winblaze_index::export::write_csv_extension(&tree, invalid, &mut output)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::InvalidInput
        );
        assert!(output.is_empty());
    }
}

#[test]
fn detailed_csv_preserves_all_metadata_and_lossless_identities() {
    let json = br#"{"schema_version":1,"entries":[{"Path":"C:\\root","Kind":"Directory","LogicalBytes":7,"AllocatedBytes":4096,"Files":1,"Directories":0,"ModifiedUnixSeconds":123},{"Path":"C:\\root\\file.txt","Kind":"File","LogicalBytes":7,"AllocatedBytes":4096,"Files":0,"Directories":0,"ModifiedUnixSeconds":123,"Attributes":32,"CreatedUnixSeconds":100,"AccessedUnixSeconds":120,"Owner":"DOMAIN,owner","VolumeIdentity":"18446744073709551615","FileIdentity":"340282366920938463463374607431768211455","LinkCount":2}]}"#;
    let tree = winblaze_index::import::read_json(json.as_slice()).unwrap();
    let mut output = Vec::new();
    winblaze_index::export::write_csv_with_metadata(&tree, &mut output, |file| {
        Ok(tree.file_export_metadata(file.id.0).cloned())
    })
    .unwrap();
    let text = String::from_utf8(output.clone()).unwrap();
    assert!(text.contains(
        "CreatedUnixSeconds,AccessedUnixSeconds,Owner,VolumeIdentity,FileIdentity,LinkCount"
    ));
    let reopened = winblaze_index::import::read_csv(output.as_slice()).unwrap();
    assert_eq!(reopened.files()[0].attributes.0, 32);
    assert_eq!(
        reopened.files()[0].created_utc(),
        tree.files()[0].created_utc()
    );
    assert_eq!(
        reopened.files()[0].accessed_utc(),
        tree.files()[0].accessed_utc()
    );
    let actual = reopened
        .file_export_metadata(reopened.files()[0].id.0)
        .unwrap();
    let expected = tree.file_export_metadata(tree.files()[0].id.0).unwrap();
    assert_eq!(actual.owner, expected.owner);
    assert_eq!(actual.volume_identity, expected.volume_identity);
    assert_eq!(actual.file_identity, expected.file_identity);
    assert_eq!(actual.link_count, expected.link_count);
}

#[test]
fn detailed_csv_missing_optional_values_remain_missing_and_provider_errors_propagate() {
    let json=br#"{"schema_version":1,"entries":[{"Path":"C:\\root","Kind":"Directory","LogicalBytes":0,"AllocatedBytes":0,"Files":1,"Directories":0,"ModifiedUnixSeconds":null},{"Path":"C:\\root\\file.txt","Kind":"File","LogicalBytes":0,"AllocatedBytes":0,"Files":0,"Directories":0,"ModifiedUnixSeconds":null}]}"#;
    let tree = winblaze_index::import::read_json(json.as_slice()).unwrap();
    let mut output = Vec::new();
    winblaze_index::export::write_csv_with_metadata(&tree, &mut output, |_| Ok(None)).unwrap();
    let reopened = winblaze_index::import::read_csv(output.as_slice()).unwrap();
    assert_eq!(reopened.files()[0].created_utc(), None);
    assert_eq!(reopened.files()[0].accessed_utc(), None);
    assert!(reopened
        .file_export_metadata(reopened.files()[0].id.0)
        .is_none());
    let error = winblaze_index::export::write_csv_with_metadata(&tree, &mut Vec::new(), |_| {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "denied",
        ))
    })
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
}
#[test]
fn detailed_csv_refuses_invalid_metadata_instead_of_publishing_a_partial_scan() {
    let header="Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds,Attributes,CreatedUnixSeconds,AccessedUnixSeconds,Owner,VolumeIdentity,FileIdentity,LinkCount\r\n";
    for (column, value) in [
        (7, "not-attributes"),
        (8, "bad-time"),
        (8, "9223372036854775807"),
        (9, "-9223372036854775808"),
        (9, "bad-time"),
        (11, "18446744073709551616"),
        (12, "340282366920938463463374607431768211456"),
        (13, "-1"),
    ] {
        let mut fields = vec![
            "C:\\root\\file.txt",
            "File",
            "7",
            "4096",
            "0",
            "0",
            "123",
            "32",
            "100",
            "120",
            "Alice",
            "5",
            "7",
            "2",
        ];
        fields[column] = value;
        let csv = format!("{header}{}\r\n", fields.join(","));
        assert!(
            winblaze_index::import::read_csv(csv.as_bytes()).is_err(),
            "column {column}"
        );
    }
    assert!(winblaze_index::import::read_csv(header.as_bytes()).is_err());
    assert!(winblaze_index::import::read_csv(
        format!("{header}C:\\root\\file.txt,File,7,4096,0,0,123\r\n").as_bytes()
    )
    .is_err());
}
