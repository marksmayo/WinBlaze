use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use winblaze_core::{
    DirectoryId, DirectoryRecord, FileAttributes, FileId, FileRecord, FileSystemKind, MatchMode,
    ScanProgress, ScanSession, ScanState, SearchQuery, SortDirection, SortField, VolumeId,
    VolumeRecord,
};

use crate::{
    schema::{CREATE_FILE_TABLE, SCHEMA_VERSION},
    store::{
        BufferedIndexTransaction, IndexBackend, IndexRepository, IndexTransaction,
        SqliteIndexRepository,
    },
};

#[test]
fn schema_version_is_stable() {
    assert_eq!(SCHEMA_VERSION, 1);
    assert!(CREATE_FILE_TABLE.contains("CREATE TABLE IF NOT EXISTS files"));
}

#[test]
fn buffered_transaction_captures_records() {
    let mut tx = BufferedIndexTransaction::default();
    tx.upsert_volume(&VolumeRecord {
        id: VolumeId(1),
        mount_point: String::from("C:\\"),
        label: Some(String::from("System")),
        file_system: FileSystemKind::Ntfs,
        total_bytes: 1024,
        free_bytes: 512,
        root_directory_id: DirectoryId(10),
    });
    tx.upsert_session(&ScanSession {
        session_id: 7,
        volume_id: VolumeId(1),
        root_path: String::from("C:\\"),
        state: ScanState::Scanning,
        progress: ScanProgress::default(),
    });
    tx.upsert_directory(&DirectoryRecord {
        id: DirectoryId(10),
        parent_directory_id: None,
        name: String::from("root"),
        full_path: String::from("C:\\root"),
        direct_bytes: 100,
        total_bytes: 200,
        direct_entries: 2,
        total_entries: 4,
    });
    tx.upsert_file(&FileRecord {
        id: FileId(99),
        parent_directory_id: DirectoryId(10),
        name: String::from("file.txt"),
        full_path: String::from("C:\\root\\file.txt"),
        size_bytes: 10,
        allocation_bytes: 16,
        attributes: FileAttributes::ARCHIVE,
        created_utc: None,
        modified_utc: None,
        accessed_utc: None,
    });

    assert_eq!(tx.snapshot_volumes().len(), 1);
    assert_eq!(tx.snapshot_sessions().len(), 1);
    assert_eq!(tx.snapshot_directories().len(), 1);
    assert_eq!(tx.snapshot_files().len(), 1);
    assert!(!tx.is_committed());

    tx.commit();
    assert!(tx.is_committed());
}

#[test]
fn repository_snapshot_reflects_configuration() {
    let repo = SqliteIndexRepository::open(Path::new("C:\\WinBlaze\\index"), IndexBackend::Sqlite);
    let snapshot = repo.snapshot();

    assert_eq!(snapshot.backend, IndexBackend::Sqlite);
    assert_eq!(snapshot.schema_version, 1);
    assert_eq!(snapshot.migrations_applied, 5);
}

#[test]
fn incremental_file_changes_replace_and_remove_records() {
    let mut tx = BufferedIndexTransaction::default();
    let previous = vec![FileRecord {
        id: FileId(1),
        parent_directory_id: DirectoryId(10),
        name: String::from("old.txt"),
        full_path: String::from("C:\\root\\old.txt"),
        size_bytes: 10,
        allocation_bytes: 16,
        attributes: FileAttributes::ARCHIVE,
        created_utc: None,
        modified_utc: Some(10),
        accessed_utc: None,
    }];
    let current = vec![FileRecord {
        id: FileId(1),
        parent_directory_id: DirectoryId(10),
        name: String::from("new.txt"),
        full_path: String::from("C:\\root\\new.txt"),
        size_bytes: 12,
        allocation_bytes: 16,
        attributes: FileAttributes::ARCHIVE,
        created_utc: None,
        modified_utc: Some(12),
        accessed_utc: None,
    }];

    let change_set = tx.apply_incremental_files(&previous, &current);
    assert_eq!(change_set.changes.len(), 1);
    assert_eq!(tx.snapshot_files().len(), 1);
    assert_eq!(tx.snapshot_files()[0].name, "new.txt");
    assert_eq!(tx.file_change_sets().len(), 1);
    assert_eq!(tx.lineage_records().len(), 1);
    assert!(tx.lineage_records()[0].renamed);
}

#[test]
fn repository_applies_incremental_transaction_against_persisted_snapshot() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time moved backwards")
        .as_nanos();
    let storage_dir = std::env::temp_dir().join(format!("winblaze-index-incremental-{unique}"));
    fs::create_dir_all(&storage_dir).expect("create temp dir");

    let mut repo = SqliteIndexRepository::open(&storage_dir, IndexBackend::BinaryCache);
    let mut initial = repo.begin_transaction();
    initial.upsert_file(&FileRecord {
        id: FileId(1),
        parent_directory_id: DirectoryId(10),
        name: String::from("old.txt"),
        full_path: String::from("C:\\root\\old.txt"),
        size_bytes: 10,
        allocation_bytes: 16,
        attributes: FileAttributes::ARCHIVE,
        created_utc: None,
        modified_utc: Some(10),
        accessed_utc: None,
    });
    initial.upsert_file(&FileRecord {
        id: FileId(2),
        parent_directory_id: DirectoryId(10),
        name: String::from("removed.txt"),
        full_path: String::from("C:\\root\\removed.txt"),
        size_bytes: 5,
        allocation_bytes: 8,
        attributes: FileAttributes::ARCHIVE,
        created_utc: None,
        modified_utc: Some(10),
        accessed_utc: None,
    });
    repo.apply_transaction(&initial).expect("persist initial");

    let mut current = BufferedIndexTransaction::default();
    current.upsert_file(&FileRecord {
        id: FileId(1),
        parent_directory_id: DirectoryId(10),
        name: String::from("new.txt"),
        full_path: String::from("C:\\root\\new.txt"),
        size_bytes: 12,
        allocation_bytes: 16,
        attributes: FileAttributes::ARCHIVE,
        created_utc: None,
        modified_utc: Some(12),
        accessed_utc: None,
    });
    current.upsert_file(&FileRecord {
        id: FileId(3),
        parent_directory_id: DirectoryId(10),
        name: String::from("added.txt"),
        full_path: String::from("C:\\root\\added.txt"),
        size_bytes: 1,
        allocation_bytes: 1,
        attributes: FileAttributes::ARCHIVE,
        created_utc: None,
        modified_utc: Some(12),
        accessed_utc: None,
    });

    let change_set = repo
        .apply_incremental_transaction(&current)
        .expect("apply incremental");
    assert_eq!(change_set.changes.len(), 3);
    assert_eq!(repo.snapshot_files().len(), 2);
    assert!(repo
        .snapshot_files()
        .iter()
        .any(|file| file.name == "new.txt"));
    assert!(repo
        .snapshot_files()
        .iter()
        .any(|file| file.name == "added.txt"));
    assert!(repo
        .snapshot_files()
        .iter()
        .all(|file| file.name != "removed.txt"));

    let reopened = SqliteIndexRepository::open(&storage_dir, IndexBackend::BinaryCache);
    assert_eq!(reopened.snapshot_files().len(), 2);
    let _ = fs::remove_dir_all(&storage_dir);
}

#[test]
fn repository_path_matched_incremental_ignores_ephemeral_file_id_shifts() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time moved backwards")
        .as_nanos();
    let storage_dir =
        std::env::temp_dir().join(format!("winblaze-index-path-incremental-{unique}"));
    fs::create_dir_all(&storage_dir).expect("create temp dir");

    let mut repo = SqliteIndexRepository::open(&storage_dir, IndexBackend::BinaryCache);
    let mut initial = repo.begin_transaction();
    initial.upsert_file(&FileRecord {
        id: FileId(1),
        parent_directory_id: DirectoryId(10),
        name: String::from("stable.txt"),
        full_path: String::from("C:\\root\\stable.txt"),
        size_bytes: 10,
        allocation_bytes: 16,
        attributes: FileAttributes::ARCHIVE,
        created_utc: None,
        modified_utc: Some(10),
        accessed_utc: None,
    });
    repo.apply_transaction(&initial).expect("persist initial");

    let mut current = BufferedIndexTransaction::default();
    current.upsert_file(&FileRecord {
        id: FileId(2),
        parent_directory_id: DirectoryId(10),
        name: String::from("stable.txt"),
        full_path: String::from("C:\\root\\stable.txt"),
        size_bytes: 10,
        allocation_bytes: 16,
        attributes: FileAttributes::ARCHIVE,
        created_utc: None,
        modified_utc: Some(10),
        accessed_utc: None,
    });
    current.upsert_file(&FileRecord {
        id: FileId(3),
        parent_directory_id: DirectoryId(10),
        name: String::from("added.txt"),
        full_path: String::from("C:\\root\\added.txt"),
        size_bytes: 1,
        allocation_bytes: 1,
        attributes: FileAttributes::ARCHIVE,
        created_utc: None,
        modified_utc: Some(10),
        accessed_utc: None,
    });

    let change_set = repo
        .apply_path_matched_incremental_transaction(&current)
        .expect("apply path-matched incremental");
    assert_eq!(change_set.changes.len(), 1);
    assert_eq!(
        change_set.changes[0].kind,
        winblaze_core::FileChangeKind::Added
    );
    assert_eq!(repo.snapshot_files().len(), 2);
    let _ = fs::remove_dir_all(&storage_dir);
}

#[test]
fn repository_persists_and_recovers_state_on_disk() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time moved backwards")
        .as_nanos();
    let storage_dir = std::env::temp_dir().join(format!("winblaze-index-{unique}"));
    fs::create_dir_all(&storage_dir).expect("create temp dir");

    let mut repo = SqliteIndexRepository::open(&storage_dir, IndexBackend::BinaryCache);
    let mut tx = repo.begin_transaction();
    tx.upsert_volume(&VolumeRecord {
        id: VolumeId(1),
        mount_point: String::from("C:\\"),
        label: Some(String::from("System")),
        file_system: FileSystemKind::Ntfs,
        total_bytes: 1024,
        free_bytes: 512,
        root_directory_id: DirectoryId(10),
    });
    tx.upsert_directory(&DirectoryRecord {
        id: DirectoryId(10),
        parent_directory_id: None,
        name: String::from("root"),
        full_path: String::from("C:\\root"),
        direct_bytes: 0,
        total_bytes: 0,
        direct_entries: 0,
        total_entries: 0,
    });
    tx.upsert_file(&FileRecord {
        id: FileId(1),
        parent_directory_id: DirectoryId(10),
        name: String::from("persisted.txt"),
        full_path: String::from("C:\\root\\persisted.txt"),
        size_bytes: 42,
        allocation_bytes: 64,
        attributes: FileAttributes::ARCHIVE,
        created_utc: None,
        modified_utc: Some(42),
        accessed_utc: None,
    });

    repo.apply_transaction(&tx).expect("persist state");

    let reopened = SqliteIndexRepository::open(&storage_dir, IndexBackend::BinaryCache);
    assert_eq!(reopened.snapshot_files().len(), 1);
    assert_eq!(reopened.snapshot_files()[0].name, "persisted.txt");
    assert_eq!(reopened.snapshot_directories().len(), 1);
    assert_eq!(reopened.snapshot_volumes().len(), 1);
    assert!(reopened.snapshot().cache_read_bytes > 0);
    assert!(!reopened.snapshot().cache_loaded_from_backup);

    let hits = reopened.search(&SearchQuery {
        include_files: true,
        pattern: Some(String::from("persisted")),
        match_mode: MatchMode::Substring,
        sort_field: SortField::Name,
        sort_direction: SortDirection::Ascending,
        ..SearchQuery::default()
    });
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].name, "persisted.txt");

    fs::remove_file(storage_dir.join("winblaze.index.bin")).ok();
    fs::remove_file(storage_dir.join("winblaze.index.bak")).ok();
    fs::remove_file(storage_dir.join("winblaze.index.tmp")).ok();
    fs::remove_dir_all(&storage_dir).ok();
}

#[test]
fn repository_recovers_from_corrupt_primary_snapshot() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time moved backwards")
        .as_nanos();
    let storage_dir = std::env::temp_dir().join(format!("winblaze-index-recovery-{unique}"));
    fs::create_dir_all(&storage_dir).expect("create temp dir");

    let mut repo = SqliteIndexRepository::open(&storage_dir, IndexBackend::BinaryCache);
    let mut tx = repo.begin_transaction();
    tx.upsert_file(&FileRecord {
        id: FileId(9),
        parent_directory_id: DirectoryId(1),
        name: String::from("recover.txt"),
        full_path: String::from("C:\\root\\recover.txt"),
        size_bytes: 8,
        allocation_bytes: 16,
        attributes: FileAttributes::ARCHIVE,
        created_utc: None,
        modified_utc: Some(8),
        accessed_utc: None,
    });
    repo.apply_transaction(&tx).expect("persist state");

    let primary = storage_dir.join("winblaze.index.bin");
    let backup = storage_dir.join("winblaze.index.bak");
    fs::copy(&primary, &backup).expect("create backup snapshot");
    fs::write(&primary, b"broken snapshot").expect("corrupt primary snapshot");

    let reopened = SqliteIndexRepository::open(&storage_dir, IndexBackend::BinaryCache);
    assert_eq!(reopened.snapshot_files().len(), 1);
    assert_eq!(reopened.snapshot_files()[0].name, "recover.txt");
    assert!(reopened.snapshot().cache_read_bytes > 0);
    assert!(reopened.snapshot().cache_loaded_from_backup);

    fs::remove_file(primary).ok();
    fs::remove_file(backup).ok();
    fs::remove_file(storage_dir.join("winblaze.index.tmp")).ok();
    fs::remove_dir_all(&storage_dir).ok();
}

#[test]
fn repository_rejects_corrupt_snapshot_lengths_without_allocating() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time moved backwards")
        .as_nanos();
    let storage_dir = std::env::temp_dir().join(format!("winblaze-index-lengths-{unique}"));
    fs::create_dir_all(&storage_dir).expect("create temp dir");

    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"WBIX");
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes.extend_from_slice(&u64::MAX.to_le_bytes());
    fs::write(storage_dir.join("winblaze.index.bin"), bytes).expect("write corrupt snapshot");

    let reopened = SqliteIndexRepository::open(&storage_dir, IndexBackend::BinaryCache);
    assert!(reopened.snapshot_files().is_empty());
    assert!(reopened.snapshot_directories().is_empty());
    assert!(!reopened.snapshot().cache_loaded_from_backup);

    fs::remove_file(storage_dir.join("winblaze.index.bin")).ok();
    fs::remove_dir_all(&storage_dir).ok();
}

#[test]
fn repository_rejects_oversized_snapshot_string_without_allocating() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time moved backwards")
        .as_nanos();
    let storage_dir =
        std::env::temp_dir().join(format!("winblaze-index-oversized-string-{unique}"));
    fs::create_dir_all(&storage_dir).expect("create temp dir");

    let mut bytes = snapshot_header();
    bytes.extend_from_slice(&1_u64.to_le_bytes()); // volume count
    bytes.extend_from_slice(&1_u64.to_le_bytes()); // volume id
    bytes.extend_from_slice(&(1024_u64 * 1024 + 1).to_le_bytes()); // oversized mount path
    bytes.extend(std::iter::repeat(0).take(64)); // enough bytes to pass collection preflight
    fs::write(storage_dir.join("winblaze.index.bin"), bytes).expect("write corrupt snapshot");

    let reopened = SqliteIndexRepository::open(&storage_dir, IndexBackend::BinaryCache);
    assert!(reopened.snapshot_volumes().is_empty());
    assert!(reopened.snapshot_files().is_empty());

    fs::remove_file(storage_dir.join("winblaze.index.bin")).ok();
    fs::remove_dir_all(&storage_dir).ok();
}

#[test]
fn repository_rejects_invalid_snapshot_enum_values() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time moved backwards")
        .as_nanos();
    let storage_dir = std::env::temp_dir().join(format!("winblaze-index-invalid-enum-{unique}"));
    fs::create_dir_all(&storage_dir).expect("create temp dir");

    let mut bytes = snapshot_header();
    bytes.extend_from_slice(&1_u64.to_le_bytes()); // volume count
    bytes.extend_from_slice(&1_u64.to_le_bytes()); // volume id
    push_snapshot_string(&mut bytes, "C:\\");
    bytes.push(0); // no label
    bytes.push(255); // invalid filesystem kind
    bytes.extend_from_slice(&1024_u64.to_le_bytes());
    bytes.extend_from_slice(&512_u64.to_le_bytes());
    bytes.extend_from_slice(&10_u64.to_le_bytes());
    bytes.extend_from_slice(&0_u64.to_le_bytes()); // sessions
    bytes.extend_from_slice(&0_u64.to_le_bytes()); // directories
    bytes.extend_from_slice(&0_u64.to_le_bytes()); // files
    bytes.extend_from_slice(&0_u64.to_le_bytes()); // lineages
    bytes.extend_from_slice(&0_u64.to_le_bytes()); // change sets
    fs::write(storage_dir.join("winblaze.index.bin"), bytes).expect("write corrupt snapshot");

    let reopened = SqliteIndexRepository::open(&storage_dir, IndexBackend::BinaryCache);
    assert!(reopened.snapshot_volumes().is_empty());
    assert!(reopened.snapshot_files().is_empty());

    fs::remove_file(storage_dir.join("winblaze.index.bin")).ok();
    fs::remove_dir_all(&storage_dir).ok();
}

#[test]
fn repository_rejects_truncated_snapshot_records() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time moved backwards")
        .as_nanos();
    let storage_dir = std::env::temp_dir().join(format!("winblaze-index-truncated-{unique}"));
    fs::create_dir_all(&storage_dir).expect("create temp dir");

    let mut bytes = snapshot_header();
    bytes.extend_from_slice(&1_u64.to_le_bytes()); // volume count
    bytes.extend_from_slice(&1_u64.to_le_bytes()); // partial volume id only
    fs::write(storage_dir.join("winblaze.index.bin"), bytes).expect("write corrupt snapshot");

    let reopened = SqliteIndexRepository::open(&storage_dir, IndexBackend::BinaryCache);
    assert!(reopened.snapshot_volumes().is_empty());
    assert!(reopened.snapshot_files().is_empty());

    fs::remove_file(storage_dir.join("winblaze.index.bin")).ok();
    fs::remove_dir_all(&storage_dir).ok();
}

#[test]
fn repository_invalidate_cache_clears_files_and_state() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time moved backwards")
        .as_nanos();
    let storage_dir = std::env::temp_dir().join(format!("winblaze-index-invalidate-{unique}"));
    fs::create_dir_all(&storage_dir).expect("create temp dir");

    let mut repo = SqliteIndexRepository::open(&storage_dir, IndexBackend::BinaryCache);
    let mut tx = repo.begin_transaction();
    tx.upsert_file(&FileRecord {
        id: FileId(21),
        parent_directory_id: DirectoryId(1),
        name: String::from("invalidate.txt"),
        full_path: String::from("C:\\root\\invalidate.txt"),
        size_bytes: 1,
        allocation_bytes: 1,
        attributes: FileAttributes::ARCHIVE,
        created_utc: None,
        modified_utc: None,
        accessed_utc: None,
    });
    repo.apply_transaction(&tx).expect("persist state");

    repo.invalidate_cache().expect("invalidate cache");
    assert!(repo.snapshot_files().is_empty());
    assert!(!storage_dir.join("winblaze.index.bin").exists());
    assert!(!storage_dir.join("winblaze.index.bak").exists());
    assert!(!storage_dir.join("winblaze.index.tmp").exists());

    fs::remove_dir_all(&storage_dir).ok();
}

#[test]
fn repository_compact_cache_removes_auxiliary_files() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time moved backwards")
        .as_nanos();
    let storage_dir = std::env::temp_dir().join(format!("winblaze-index-compact-{unique}"));
    fs::create_dir_all(&storage_dir).expect("create temp dir");

    let mut repo = SqliteIndexRepository::open(&storage_dir, IndexBackend::BinaryCache);
    let mut tx = repo.begin_transaction();
    tx.upsert_file(&FileRecord {
        id: FileId(31),
        parent_directory_id: DirectoryId(1),
        name: String::from("compact.txt"),
        full_path: String::from("C:\\root\\compact.txt"),
        size_bytes: 2,
        allocation_bytes: 4,
        attributes: FileAttributes::ARCHIVE,
        created_utc: None,
        modified_utc: None,
        accessed_utc: None,
    });
    repo.apply_transaction(&tx).expect("persist state");

    let backup = storage_dir.join("winblaze.index.bak");
    let temp = storage_dir.join("winblaze.index.tmp");
    fs::copy(storage_dir.join("winblaze.index.bin"), &backup).expect("seed backup");
    fs::write(&temp, b"stale temp").expect("seed temp");

    repo.compact_cache().expect("compact cache");
    assert!(storage_dir.join("winblaze.index.bin").exists());
    assert!(!backup.exists());
    assert!(!temp.exists());

    fs::remove_file(storage_dir.join("winblaze.index.bin")).ok();
    fs::remove_dir_all(&storage_dir).ok();
}

fn snapshot_header() -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(b"WBIX");
    bytes.extend_from_slice(&1_u32.to_le_bytes());
    bytes
}

fn push_snapshot_string(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend_from_slice(&(value.len() as u64).to_le_bytes());
    bytes.extend_from_slice(value.as_bytes());
}
