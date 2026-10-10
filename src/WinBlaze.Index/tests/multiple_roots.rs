use winblaze_index::{import::read_csv, multiple_roots::combine};
#[test]
fn combines_unrelated_roots_and_reopens_the_complete_csv_snapshot() {
    let first=read_csv(b"Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\nC:\\one\\a.bin,File,7,4096,0,0,\n".as_slice()).unwrap();
    let second=read_csv(b"Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\nD:\\two\\b.bin,File,9,8192,0,0,\n".as_slice()).unwrap();
    let combined = combine(vec![first, second]).unwrap();
    let (root, totals) = combined.root().unwrap();
    assert_eq!(root.full_path, "WinBlaze:\\Multiple roots");
    assert_eq!(
        (
            totals.file_count,
            totals.logical_bytes,
            totals.physical_bytes
        ),
        (2, 16, 12288)
    );
    let mut csv = Vec::new();
    winblaze_index::export::write_csv(&combined, &mut csv).unwrap();
    let reopened = read_csv(csv.as_slice()).unwrap();
    assert_eq!(reopened.files().len(), 2);
    let paths: Vec<_> = reopened
        .files()
        .iter()
        .map(|file| reopened.file_display_path(file))
        .collect();
    assert!(paths.iter().any(|path| path == "C:\\one\\a.bin"));
    assert!(paths.iter().any(|path| path == "D:\\two\\b.bin"));
}

#[test]
fn volume_without_a_directory_id_uses_its_source_catalog_root() {
    use winblaze_core::{DirectoryId, DirectoryRecord, VolumeRecord};
    use winblaze_index::{BufferedIndexTransaction, TreeIndex};
    let mut transaction = BufferedIndexTransaction::default();
    transaction.insert_directory(DirectoryRecord {
        id: DirectoryId(55),
        full_path: "C:\\source".into(),
        name: "source".into(),
        ..Default::default()
    });
    transaction.insert_volume(VolumeRecord::default());
    let combined = combine(vec![TreeIndex::build(transaction)]).unwrap();
    let volume = &combined.volumes()[0];
    let source = combined
        .directories()
        .iter()
        .find(|directory| directory.full_path == "C:\\source")
        .unwrap();
    assert_eq!(volume.root_directory_id, source.id);
    assert_ne!(volume.root_directory_id, combined.root().unwrap().0.id);
}

#[test]
fn combines_optional_owner_metadata_without_changing_original_paths() {
    let csv="Name,Files,Folders,Logical Size,Physical Size,Attributes,Last Change,WinDirStat Attributes,Index,Owner\nC:\\source\\a.bin,0,0,7,4096,A,2026-01-01T00:00:00Z,0x20000008,1,DOMAIN\\user\n";
    let tree = winblaze_index::import::read_csv(csv.as_bytes()).unwrap();
    let combined = combine(vec![tree]).unwrap();
    assert_eq!(
        combined
            .file_export_metadata(combined.files()[0].id.0)
            .unwrap()
            .owner
            .as_deref(),
        Some("DOMAIN\\user")
    );
}
