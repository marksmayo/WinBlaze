use winblaze_core::{
    DirectoryId, DirectoryRecord, FileChangeKind, FileId, FileRecord, VolumeId, VolumeRecord,
};
use winblaze_index::{
    BufferedIndexTransaction, IndexBackend, IndexRepository, SqliteIndexRepository,
};

fn snapshot(swapped: bool, changed: bool) -> BufferedIndexTransaction {
    let mut tx = BufferedIndexTransaction::default();
    let ids = if swapped { [20, 10] } else { [10, 20] };
    let root_id = if swapped { 99 } else { 1 };
    tx.insert_volume(VolumeRecord {
        id: VolumeId(1),
        root_directory_id: DirectoryId(root_id),
        ..Default::default()
    });
    tx.insert_directory(DirectoryRecord {
        id: DirectoryId(root_id),
        name: "root".into(),
        full_path: r"C:\root".into(),
        ..Default::default()
    });
    for (i, name) in ["a", "b"].into_iter().enumerate() {
        tx.insert_directory(DirectoryRecord {
            id: DirectoryId(ids[i]),
            parent_directory_id: Some(DirectoryId(root_id)),
            name: name.into(),
            full_path: format!(r"C:\root\{name}"),
            ..Default::default()
        });
        tx.insert_file(FileRecord {
            id: FileId(100 + i as u64),
            parent_directory_id: DirectoryId(ids[i]),
            name: "file.txt".into(),
            size_bytes: if changed && i == 0 { 2 } else { 1 },
            ..Default::default()
        });
    }
    tx
}

#[test]
fn path_matched_rescan_preserves_directory_identity_and_real_modifications() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../artifacts")
        .join(format!(
            "incremental-directory-tdd-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    let mut repo = SqliteIndexRepository::open(&root, IndexBackend::BinaryCache);
    repo.apply_transaction(&snapshot(false, false)).unwrap();
    let changes = repo
        .apply_path_matched_incremental_transaction(&snapshot(true, true))
        .unwrap();
    assert_eq!(
        changes.changes.len(),
        1,
        "directory enumeration order must not create false changes"
    );
    assert_eq!(changes.changes[0].kind, FileChangeKind::Modified);
    let files = repo.snapshot_files();
    assert_eq!(
        files
            .iter()
            .find(|file| file.id == FileId(100))
            .unwrap()
            .parent_directory_id,
        DirectoryId(10)
    );
    assert_eq!(
        files
            .iter()
            .find(|file| file.id == FileId(101))
            .unwrap()
            .parent_directory_id,
        DirectoryId(20)
    );
    let no_changes = repo
        .apply_path_matched_incremental_transaction(&snapshot(false, true))
        .unwrap();
    assert!(no_changes.changes.is_empty());
    let state = repo.into_transaction();
    assert_eq!(
        state.snapshot_volumes()[0].root_directory_id,
        DirectoryId(1)
    );
    let tree = winblaze_index::TreeIndex::build(state);
    assert_eq!(tree.root().unwrap().1.file_count, 2);
    assert_eq!(tree.root().unwrap().1.logical_bytes, 3);
}

#[test]
fn newly_named_directory_gets_fresh_identity_and_linked_file_parent() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../artifacts")
        .join(format!(
            "incremental-new-dir-tdd-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    let mut repo = SqliteIndexRepository::open(&root, IndexBackend::BinaryCache);
    repo.apply_transaction(&snapshot(false, false)).unwrap();
    let mut current = BufferedIndexTransaction::default();
    current.insert_directory(DirectoryRecord {
        id: DirectoryId(1),
        name: "root".into(),
        full_path: r"C:\root".into(),
        ..Default::default()
    });
    current.insert_directory(DirectoryRecord {
        id: DirectoryId(10),
        parent_directory_id: Some(DirectoryId(1)),
        name: "new".into(),
        full_path: r"C:\root\new".into(),
        ..Default::default()
    });
    current.insert_file(FileRecord {
        id: FileId(100),
        parent_directory_id: DirectoryId(10),
        name: "file.txt".into(),
        size_bytes: 4,
        ..Default::default()
    });
    let changes = repo
        .apply_path_matched_incremental_transaction(&current)
        .unwrap();
    assert_eq!(
        changes
            .changes
            .iter()
            .filter(|change| change.kind == FileChangeKind::Added)
            .count(),
        1
    );
    assert_eq!(
        changes
            .changes
            .iter()
            .filter(|change| change.kind == FileChangeKind::Removed)
            .count(),
        2
    );
    let state = repo.into_transaction();
    let dirs = state.snapshot_directories();
    let new_dir = dirs.iter().find(|dir| dir.name == "new").unwrap();
    assert_ne!(new_dir.id, DirectoryId(10));
    assert_eq!(state.snapshot_files()[0].parent_directory_id, new_dir.id);
    let tree = winblaze_index::TreeIndex::build(state);
    assert_eq!(tree.root().unwrap().1.logical_bytes, 4);
}

#[test]
fn dangling_parent_does_not_alias_a_remapped_root() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../artifacts")
        .join(format!(
            "incremental-orphan-tdd-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
    let mut repo = SqliteIndexRepository::open(&root, IndexBackend::BinaryCache);
    repo.apply_transaction(&snapshot(false, false)).unwrap();
    let mut current = snapshot(true, false);
    current.insert_file(FileRecord {
        id: FileId(101),
        parent_directory_id: DirectoryId(1),
        name: "file.txt".into(),
        full_path: r"C:\root\b\file.txt".into(),
        size_bytes: 1,
        ..Default::default()
    });
    repo.apply_path_matched_incremental_transaction(&current)
        .unwrap();
    let state = repo.into_transaction();
    let files = state.snapshot_files();
    let orphan = files.iter().find(|file| file.id == FileId(101)).unwrap();
    assert!(!state
        .snapshot_directories()
        .iter()
        .any(|dir| dir.id == orphan.parent_directory_id));
    let tree = winblaze_index::TreeIndex::build(state);
    assert_eq!(tree.files().len(), 2);
    // TreeIndex deliberately attaches orphan files to its display root.
    assert_eq!(tree.root().unwrap().1.file_count, 2);
}
