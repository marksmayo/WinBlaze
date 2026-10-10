use winblaze_core::{FileId, FileRecord, FileTimestamps};
use winblaze_index::{IndexBackend, IndexRepository, IndexTransaction, SqliteIndexRepository};

#[test]
fn persisted_file_timestamps_preserve_missing_zero_and_signed_extremes() {
    let path = std::env::temp_dir().join(format!(
        "winblaze-timestamp-fidelity-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&path).unwrap();
    let times = [None, Some(i64::MIN), Some(0), Some(i64::MAX)];
    let mut repository = SqliteIndexRepository::open(&path, IndexBackend::BinaryCache);
    let mut transaction = repository.begin_transaction();
    let mut expected = Vec::new();
    for created in times {
        for modified in times {
            for accessed in times {
                let id = expected.len() as u64 + 1;
                let file = FileRecord {
                    id: FileId(id),
                    name: format!("日本-{id}.txt").into(),
                    full_path: format!("C:\\fixture\\日本-{id}.txt").into(),
                    timestamps: FileTimestamps::new(created, modified, accessed),
                    ..Default::default()
                };
                transaction.upsert_file(&file);
                expected.push(file);
            }
        }
    }
    repository.apply_transaction(&transaction).unwrap();
    drop(repository);
    let reopened = SqliteIndexRepository::open(&path, IndexBackend::BinaryCache);
    assert!(reopened.snapshot().cache_read_bytes > 0);
    assert_eq!(reopened.snapshot_files(), expected);
    drop(reopened);
    std::fs::remove_dir_all(path).unwrap();
}
