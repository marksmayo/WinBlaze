use std::fmt::{self, Display, Formatter};
use std::fs;
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::{collections::HashMap, iter::FromIterator};

use winblaze_core::{
    diff_file_records, DirectoryId, DirectoryRecord, FileAttributes, FileChangeKind,
    FileChangeRecord, FileChangeSet, FileId, FileLineageRecord, FileRecord, ScanProgress,
    ScanSession, ScanState, SearchQuery, VolumeId, VolumeRecord,
};

use crate::schema::{MIGRATIONS, SCHEMA_VERSION};

const INDEX_MAGIC: &[u8; 4] = b"WBIX";
const INDEX_FORMAT_VERSION: u32 = 1;
const MAX_SNAPSHOT_STRING_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum IndexBackend {
    #[default]
    Sqlite,
    BinaryCache,
}

pub trait IndexTransaction {
    fn upsert_volume(&mut self, volume: &VolumeRecord);
    fn upsert_session(&mut self, session: &ScanSession);
    fn upsert_directory(&mut self, directory: &DirectoryRecord);
    fn upsert_file(&mut self, file: &FileRecord);
    fn commit(&mut self);
}

pub trait IndexRepository {
    type Transaction: IndexTransaction;

    fn open(path: &Path, backend: IndexBackend) -> Self;
    fn begin_transaction(&mut self) -> Self::Transaction;
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IndexSnapshot {
    pub path: PathBuf,
    pub backend: IndexBackend,
    pub schema_version: i64,
    pub migrations_applied: usize,
    pub cache_read_bytes: u64,
    pub cache_read_millis: u64,
    pub cache_decode_millis: u64,
    pub cache_loaded_from_backup: bool,
}

pub struct SqliteIndexRepository {
    snapshot: IndexSnapshot,
    storage_path: PathBuf,
    state: BufferedIndexTransaction,
}

impl IndexRepository for SqliteIndexRepository {
    type Transaction = BufferedIndexTransaction;

    fn open(path: &Path, backend: IndexBackend) -> Self {
        let storage_path = resolve_storage_path(path);
        let loaded = load_index_state(&storage_path, false)
            .or_else(|_| load_index_state(&backup_storage_path(&storage_path), true))
            .unwrap_or_default();

        Self {
            snapshot: IndexSnapshot {
                path: path.to_path_buf(),
                backend,
                schema_version: SCHEMA_VERSION,
                migrations_applied: MIGRATIONS.len(),
                cache_read_bytes: loaded.cache_read_bytes,
                cache_read_millis: loaded.cache_read_millis,
                cache_decode_millis: loaded.cache_decode_millis,
                cache_loaded_from_backup: loaded.cache_loaded_from_backup,
            },
            storage_path,
            state: loaded.state,
        }
    }

    fn begin_transaction(&mut self) -> Self::Transaction {
        self.state.clone()
    }
}

impl SqliteIndexRepository {
    pub fn open_empty(path: &Path, backend: IndexBackend) -> Self {
        Self {
            snapshot: IndexSnapshot {
                path: path.to_path_buf(),
                backend,
                schema_version: SCHEMA_VERSION,
                migrations_applied: MIGRATIONS.len(),
                cache_read_bytes: 0,
                cache_read_millis: 0,
                cache_decode_millis: 0,
                cache_loaded_from_backup: false,
            },
            storage_path: resolve_storage_path(path),
            state: BufferedIndexTransaction::default(),
        }
    }

    pub fn snapshot(&self) -> &IndexSnapshot {
        &self.snapshot
    }

    pub fn migration_count(&self) -> usize {
        MIGRATIONS.len()
    }

    pub fn snapshot_volumes(&self) -> Vec<VolumeRecord> {
        self.state.snapshot_volumes()
    }

    pub fn snapshot_sessions(&self) -> Vec<ScanSession> {
        self.state.snapshot_sessions()
    }

    pub fn snapshot_directories(&self) -> Vec<DirectoryRecord> {
        self.state.snapshot_directories()
    }

    pub fn snapshot_files(&self) -> Vec<FileRecord> {
        self.state.snapshot_files()
    }

    pub fn apply_transaction(
        &mut self,
        transaction: &BufferedIndexTransaction,
    ) -> Result<(), IndexStorageError> {
        self.state = transaction.clone();
        persist_index_state(&self.storage_path, &self.state)
    }

    pub fn apply_incremental_transaction(
        &mut self,
        transaction: &BufferedIndexTransaction,
    ) -> Result<FileChangeSet, IndexStorageError> {
        let previous_files = self.state.snapshot_files();
        let current_files = transaction.snapshot_files();
        let mut merged = transaction.clone();
        merged.files = self.state.files.clone();
        merged.lineages = self.state.lineages.clone();
        merged.file_changes = self.state.file_changes.clone();
        let change_set = merged.apply_incremental_files(&previous_files, &current_files);
        self.state = merged;
        persist_index_state(&self.storage_path, &self.state)?;
        Ok(change_set)
    }

    pub fn apply_path_matched_incremental_transaction(
        &mut self,
        transaction: &BufferedIndexTransaction,
    ) -> Result<FileChangeSet, IndexStorageError> {
        let previous_files = self.state.snapshot_files();
        let current_files =
            remap_current_files_by_path(&previous_files, &transaction.snapshot_files());
        let mut current_transaction = transaction.clone();
        current_transaction.files = current_files
            .iter()
            .map(|file| (file.id, file.clone()))
            .collect();
        self.apply_incremental_transaction(&current_transaction)
    }

    pub fn invalidate_cache(&mut self) -> Result<(), IndexStorageError> {
        self.state = BufferedIndexTransaction::default();
        remove_snapshot_files(&self.storage_path)?;
        Ok(())
    }

    pub fn compact_cache(&mut self) -> Result<(), IndexStorageError> {
        remove_auxiliary_snapshot_files(&self.storage_path)?;
        persist_index_state(&self.storage_path, &self.state)
    }

    pub fn search(&self, query: &SearchQuery) -> Vec<crate::query::IndexSearchHit> {
        crate::query::IndexCatalog::from_transaction(&self.state).search(query)
    }
}

#[derive(Debug, Default)]
struct LoadedIndexState {
    state: BufferedIndexTransaction,
    cache_read_bytes: u64,
    cache_read_millis: u64,
    cache_decode_millis: u64,
    cache_loaded_from_backup: bool,
}

fn remap_current_files_by_path(previous: &[FileRecord], current: &[FileRecord]) -> Vec<FileRecord> {
    let previous_by_path: HashMap<&str, &FileRecord> = previous
        .iter()
        .map(|record| (record.full_path.as_str(), record))
        .collect();
    let mut next_id = previous
        .iter()
        .chain(current.iter())
        .map(|record| record.id.0)
        .max()
        .unwrap_or(0)
        .saturating_add(1);

    current
        .iter()
        .map(|record| {
            let mut mapped = record.clone();
            if let Some(previous_record) = previous_by_path.get(record.full_path.as_str()) {
                mapped.id = previous_record.id;
            } else {
                mapped.id = FileId(next_id);
                next_id = next_id.saturating_add(1);
            }
            mapped
        })
        .collect()
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BufferedIndexTransaction {
    volumes: HashMap<VolumeId, VolumeRecord>,
    sessions: HashMap<u64, ScanSession>,
    directories: HashMap<DirectoryId, DirectoryRecord>,
    files: HashMap<FileId, FileRecord>,
    lineages: Vec<FileLineageRecord>,
    file_changes: Vec<FileChangeSet>,
    committed: bool,
}

impl IndexTransaction for BufferedIndexTransaction {
    fn upsert_volume(&mut self, volume: &VolumeRecord) {
        self.volumes.insert(volume.id, volume.clone());
    }

    fn upsert_session(&mut self, session: &ScanSession) {
        self.sessions.insert(session.session_id, session.clone());
    }

    fn upsert_directory(&mut self, directory: &DirectoryRecord) {
        self.directories.insert(directory.id, directory.clone());
    }

    fn upsert_file(&mut self, file: &FileRecord) {
        self.files.insert(file.id, file.clone());
    }

    fn commit(&mut self) {
        self.committed = true;
    }
}

impl BufferedIndexTransaction {
    pub fn snapshot_volumes(&self) -> Vec<VolumeRecord> {
        let mut volumes = Vec::from_iter(self.volumes.values().cloned());
        volumes.sort_by_key(|volume| volume.id.0);
        volumes
    }

    pub fn snapshot_sessions(&self) -> Vec<ScanSession> {
        let mut sessions = Vec::from_iter(self.sessions.values().cloned());
        sessions.sort_by_key(|session| session.session_id);
        sessions
    }

    pub fn snapshot_directories(&self) -> Vec<DirectoryRecord> {
        let mut directories = Vec::from_iter(self.directories.values().cloned());
        directories.sort_by_key(|directory| directory.id.0);
        directories
    }

    pub fn snapshot_files(&self) -> Vec<FileRecord> {
        let mut files = Vec::from_iter(self.files.values().cloned());
        files.sort_by_key(|file| file.id.0);
        files
    }

    pub fn lineage_records(&self) -> &[FileLineageRecord] {
        &self.lineages
    }

    pub fn file_change_sets(&self) -> &[FileChangeSet] {
        &self.file_changes
    }

    pub fn apply_incremental_files(
        &mut self,
        previous: &[FileRecord],
        current: &[FileRecord],
    ) -> FileChangeSet {
        let change_set = diff_file_records(previous, current);
        let current_by_id: HashMap<FileId, &FileRecord> =
            current.iter().map(|record| (record.id, record)).collect();
        let previous_by_id: HashMap<FileId, &FileRecord> =
            previous.iter().map(|record| (record.id, record)).collect();

        for change in &change_set.changes {
            match change.kind {
                winblaze_core::FileChangeKind::Added
                | winblaze_core::FileChangeKind::Modified
                | winblaze_core::FileChangeKind::Renamed
                | winblaze_core::FileChangeKind::Moved => {
                    if let Some(record) = current_by_id.get(&change.file_id).copied() {
                        self.files.insert(record.id, record.clone());
                    }

                    if let Some(previous_record) = previous_by_id.get(&change.file_id).copied() {
                        if let Some(current_record) = current_by_id.get(&change.file_id).copied() {
                            if let Some(lineage) = winblaze_core::detect_file_lineage_change(
                                previous_record,
                                current_record,
                            ) {
                                self.lineages.push(lineage);
                            }
                        }
                    }
                }
                winblaze_core::FileChangeKind::Removed => {
                    self.files.remove(&change.file_id);
                }
            }
        }

        self.file_changes.push(change_set.clone());
        change_set
    }

    pub fn is_committed(&self) -> bool {
        self.committed
    }
}

#[derive(Debug)]
pub enum IndexStorageError {
    Io(io::Error),
    CorruptSnapshot(String),
}

impl Display for IndexStorageError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::CorruptSnapshot(message) => write!(f, "corrupt index snapshot: {message}"),
        }
    }
}

impl std::error::Error for IndexStorageError {}

impl From<io::Error> for IndexStorageError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

fn resolve_storage_path(path: &Path) -> PathBuf {
    if path.extension().is_some() {
        path.to_path_buf()
    } else {
        path.join("winblaze.index.bin")
    }
}

fn backup_storage_path(path: &Path) -> PathBuf {
    path.with_extension("bak")
}

fn temp_storage_path(path: &Path) -> PathBuf {
    path.with_extension("tmp")
}

fn load_index_state(
    path: &Path,
    loaded_from_backup: bool,
) -> Result<LoadedIndexState, IndexStorageError> {
    let read_started = Instant::now();
    let bytes = fs::read(path)?;
    let cache_read_millis = millis_u64(read_started.elapsed().as_millis());
    let decode_started = Instant::now();
    let state = deserialize_state(&bytes)?;
    let cache_decode_millis = millis_u64(decode_started.elapsed().as_millis());
    Ok(LoadedIndexState {
        state,
        cache_read_bytes: bytes.len() as u64,
        cache_read_millis,
        cache_decode_millis,
        cache_loaded_from_backup: loaded_from_backup,
    })
}

fn millis_u64(value: u128) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn persist_index_state(
    storage_path: &Path,
    state: &BufferedIndexTransaction,
) -> Result<(), IndexStorageError> {
    if let Some(parent) = storage_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let temp_path = temp_storage_path(storage_path);
    let backup_path = backup_storage_path(storage_path);
    let bytes = serialize_state(state)?;

    {
        let mut file = fs::File::create(&temp_path)?;
        file.write_all(&bytes)?;
        file.flush()?;
    }

    if storage_path.exists() {
        if backup_path.exists() {
            let _ = fs::remove_file(&backup_path);
        }
        fs::rename(storage_path, &backup_path)?;
    }

    fs::rename(&temp_path, storage_path)?;

    if backup_path.exists() {
        let _ = fs::remove_file(backup_path);
    }

    Ok(())
}

fn remove_snapshot_files(storage_path: &Path) -> Result<(), IndexStorageError> {
    for candidate in [
        storage_path.to_path_buf(),
        backup_storage_path(storage_path),
        temp_storage_path(storage_path),
    ] {
        match fs::remove_file(&candidate) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(IndexStorageError::Io(error)),
        }
    }

    Ok(())
}

fn remove_auxiliary_snapshot_files(storage_path: &Path) -> Result<(), IndexStorageError> {
    for candidate in [
        backup_storage_path(storage_path),
        temp_storage_path(storage_path),
    ] {
        match fs::remove_file(&candidate) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(IndexStorageError::Io(error)),
        }
    }

    Ok(())
}

fn serialize_state(state: &BufferedIndexTransaction) -> Result<Vec<u8>, IndexStorageError> {
    // Pre-size the byte buffer with a heuristic estimate to avoid repeated reallocation.
    // Each file record is roughly 8+8+2*path_avg bytes; 128 bytes/entry is conservative
    // and still avoids most reallocation events for typical catalogs.
    let estimated_bytes =
        state.files.len().saturating_mul(128) + state.directories.len().saturating_mul(64) + 64; // header
    let mut bytes = Vec::with_capacity(estimated_bytes);
    bytes.extend_from_slice(INDEX_MAGIC);
    write_u32(&mut bytes, INDEX_FORMAT_VERSION);
    write_volume_records(&mut bytes, &state.snapshot_volumes())?;
    write_session_records(&mut bytes, &state.snapshot_sessions())?;
    write_directory_records(&mut bytes, &state.snapshot_directories())?;
    write_file_records(&mut bytes, &state.snapshot_files())?;
    write_lineage_records(&mut bytes, state.lineages.as_slice())?;
    write_change_sets(&mut bytes, state.file_changes.as_slice())?;
    Ok(bytes)
}

fn deserialize_state(bytes: &[u8]) -> Result<BufferedIndexTransaction, IndexStorageError> {
    let mut cursor = Cursor::new(bytes);
    let mut magic = [0u8; 4];
    cursor.read_exact(&mut magic)?;
    if &magic != INDEX_MAGIC {
        return Err(IndexStorageError::CorruptSnapshot(String::from(
            "invalid magic",
        )));
    }

    let version = read_u32(&mut cursor)?;
    if version != INDEX_FORMAT_VERSION {
        return Err(IndexStorageError::CorruptSnapshot(String::from(
            "unsupported format version",
        )));
    }

    let volumes = read_volume_records(&mut cursor)?;
    let sessions = read_session_records(&mut cursor)?;
    let directories = read_directory_records(&mut cursor)?;
    let files = read_file_records(&mut cursor)?;
    let lineages = read_lineage_records(&mut cursor)?;
    let file_changes = read_change_sets(&mut cursor)?;

    // Pre-size each HashMap to the exact entry count read from the binary header.
    // HashMap::from_iter without a size hint causes repeated rehashing (O(log N) events)
    // as the map grows; with a known count we pay for exactly one bucket allocation per
    // map regardless of catalog size.  For a 500k-file index this removes ~76 total
    // reallocation events spread across all four maps.
    let mut volumes_map = HashMap::with_capacity(volumes.len());
    volumes_map.extend(volumes.into_iter().map(|v| (v.id, v)));

    let mut sessions_map = HashMap::with_capacity(sessions.len());
    sessions_map.extend(sessions.into_iter().map(|s| (s.session_id, s)));

    let mut directories_map = HashMap::with_capacity(directories.len());
    directories_map.extend(directories.into_iter().map(|d| (d.id, d)));

    let mut files_map = HashMap::with_capacity(files.len());
    files_map.extend(files.into_iter().map(|f| (f.id, f)));

    Ok(BufferedIndexTransaction {
        volumes: volumes_map,
        sessions: sessions_map,
        directories: directories_map,
        files: files_map,
        lineages,
        file_changes,
        committed: false,
    })
}

fn write_volume_records(
    bytes: &mut Vec<u8>,
    volumes: &[VolumeRecord],
) -> Result<(), IndexStorageError> {
    write_len(bytes, volumes.len())?;
    for volume in volumes {
        write_u64(bytes, volume.id.0);
        write_string(bytes, &volume.mount_point)?;
        write_option_string(bytes, volume.label.as_deref())?;
        write_u8(bytes, encode_file_system_kind(volume.file_system));
        write_u64(bytes, volume.total_bytes);
        write_u64(bytes, volume.free_bytes);
        write_u64(bytes, volume.root_directory_id.0);
    }
    Ok(())
}

fn write_session_records(
    bytes: &mut Vec<u8>,
    sessions: &[ScanSession],
) -> Result<(), IndexStorageError> {
    write_len(bytes, sessions.len())?;
    for session in sessions {
        write_u64(bytes, session.session_id);
        write_u64(bytes, session.volume_id.0);
        write_string(bytes, &session.root_path)?;
        write_u8(bytes, encode_scan_state(session.state));
        write_u64(bytes, session.progress.completed_items);
        write_u64(bytes, session.progress.total_items);
        write_u64(bytes, session.progress.completed_bytes);
        write_u64(bytes, session.progress.total_bytes);
    }
    Ok(())
}

fn write_directory_records(
    bytes: &mut Vec<u8>,
    directories: &[DirectoryRecord],
) -> Result<(), IndexStorageError> {
    write_len(bytes, directories.len())?;
    for directory in directories {
        write_u64(bytes, directory.id.0);
        match directory.parent_directory_id {
            Some(parent) => {
                write_u8(bytes, 1);
                write_u64(bytes, parent.0);
            }
            None => write_u8(bytes, 0),
        }
        write_string(bytes, &directory.name)?;
        write_string(bytes, &directory.full_path)?;
        write_u64(bytes, directory.direct_bytes);
        write_u64(bytes, directory.total_bytes);
        write_u64(bytes, directory.direct_entries);
        write_u64(bytes, directory.total_entries);
    }
    Ok(())
}

fn write_file_records(bytes: &mut Vec<u8>, files: &[FileRecord]) -> Result<(), IndexStorageError> {
    write_len(bytes, files.len())?;
    for file in files {
        write_u64(bytes, file.id.0);
        write_u64(bytes, file.parent_directory_id.0);
        write_string(bytes, &file.name)?;
        write_string(bytes, &file.full_path)?;
        write_u64(bytes, file.size_bytes);
        write_u64(bytes, file.allocation_bytes);
        write_u32(bytes, file.attributes.0);
        write_option_i64(bytes, file.created_utc)?;
        write_option_i64(bytes, file.modified_utc)?;
        write_option_i64(bytes, file.accessed_utc)?;
    }
    Ok(())
}

fn write_lineage_records(
    bytes: &mut Vec<u8>,
    lineages: &[FileLineageRecord],
) -> Result<(), IndexStorageError> {
    write_len(bytes, lineages.len())?;
    for lineage in lineages {
        write_u64(bytes, lineage.file_id.0);
        write_u64(bytes, lineage.previous_parent_directory_id.0);
        write_u64(bytes, lineage.current_parent_directory_id.0);
        write_string(bytes, &lineage.previous_full_path)?;
        write_string(bytes, &lineage.current_full_path)?;
        write_u8(bytes, u8::from(lineage.renamed));
        write_u8(bytes, u8::from(lineage.moved));
    }
    Ok(())
}

fn write_change_sets(
    bytes: &mut Vec<u8>,
    change_sets: &[FileChangeSet],
) -> Result<(), IndexStorageError> {
    write_len(bytes, change_sets.len())?;
    for change_set in change_sets {
        write_len(bytes, change_set.changes.len())?;
        for change in &change_set.changes {
            write_u64(bytes, change.file_id.0);
            write_u8(bytes, encode_file_change_kind(change.kind));
            write_option_string(bytes, change.previous_full_path.as_deref())?;
            write_option_string(bytes, change.current_full_path.as_deref())?;
        }
    }
    Ok(())
}

fn read_volume_records(cursor: &mut Cursor<&[u8]>) -> Result<Vec<VolumeRecord>, IndexStorageError> {
    let len = read_len(cursor)?;
    validate_collection_len(cursor, len, 34, "volume records")?;
    let mut volumes = Vec::with_capacity(len);
    for _ in 0..len {
        volumes.push(VolumeRecord {
            id: VolumeId(read_u64(cursor)?),
            mount_point: read_string(cursor)?,
            label: read_option_string(cursor)?,
            file_system: decode_file_system_kind(read_u8(cursor)?)?,
            total_bytes: read_u64(cursor)?,
            free_bytes: read_u64(cursor)?,
            root_directory_id: DirectoryId(read_u64(cursor)?),
        });
    }
    Ok(volumes)
}

fn read_session_records(cursor: &mut Cursor<&[u8]>) -> Result<Vec<ScanSession>, IndexStorageError> {
    let len = read_len(cursor)?;
    validate_collection_len(cursor, len, 41, "session records")?;
    let mut sessions = Vec::with_capacity(len);
    for _ in 0..len {
        sessions.push(ScanSession {
            session_id: read_u64(cursor)?,
            volume_id: VolumeId(read_u64(cursor)?),
            root_path: read_string(cursor)?,
            state: decode_scan_state(read_u8(cursor)?)?,
            progress: ScanProgress {
                completed_items: read_u64(cursor)?,
                total_items: read_u64(cursor)?,
                completed_bytes: read_u64(cursor)?,
                total_bytes: read_u64(cursor)?,
            },
        });
    }
    Ok(sessions)
}

fn read_directory_records(
    cursor: &mut Cursor<&[u8]>,
) -> Result<Vec<DirectoryRecord>, IndexStorageError> {
    let len = read_len(cursor)?;
    validate_collection_len(cursor, len, 41, "directory records")?;
    let mut directories = Vec::with_capacity(len);
    for _ in 0..len {
        let id = DirectoryId(read_u64(cursor)?);
        let parent_directory_id = match read_u8(cursor)? {
            0 => None,
            1 => Some(DirectoryId(read_u64(cursor)?)),
            _ => {
                return Err(IndexStorageError::CorruptSnapshot(String::from(
                    "invalid directory parent flag",
                )))
            }
        };

        directories.push(DirectoryRecord {
            id,
            parent_directory_id,
            name: read_string(cursor)?,
            full_path: read_string(cursor)?,
            direct_bytes: read_u64(cursor)?,
            total_bytes: read_u64(cursor)?,
            direct_entries: read_u64(cursor)?,
            total_entries: read_u64(cursor)?,
        });
    }
    Ok(directories)
}

fn read_file_records(cursor: &mut Cursor<&[u8]>) -> Result<Vec<FileRecord>, IndexStorageError> {
    let len = read_len(cursor)?;
    validate_collection_len(cursor, len, 57, "file records")?;
    let mut files = Vec::with_capacity(len);
    for _ in 0..len {
        files.push(FileRecord {
            id: FileId(read_u64(cursor)?),
            parent_directory_id: DirectoryId(read_u64(cursor)?),
            name: read_string(cursor)?,
            full_path: read_string(cursor)?,
            size_bytes: read_u64(cursor)?,
            allocation_bytes: read_u64(cursor)?,
            attributes: FileAttributes(read_u32(cursor)?),
            created_utc: read_option_i64(cursor)?,
            modified_utc: read_option_i64(cursor)?,
            accessed_utc: read_option_i64(cursor)?,
        });
    }
    Ok(files)
}

fn read_lineage_records(
    cursor: &mut Cursor<&[u8]>,
) -> Result<Vec<FileLineageRecord>, IndexStorageError> {
    let len = read_len(cursor)?;
    validate_collection_len(cursor, len, 34, "lineage records")?;
    let mut lineages = Vec::with_capacity(len);
    for _ in 0..len {
        lineages.push(FileLineageRecord {
            file_id: FileId(read_u64(cursor)?),
            previous_parent_directory_id: DirectoryId(read_u64(cursor)?),
            current_parent_directory_id: DirectoryId(read_u64(cursor)?),
            previous_full_path: read_string(cursor)?,
            current_full_path: read_string(cursor)?,
            renamed: read_u8(cursor)? != 0,
            moved: read_u8(cursor)? != 0,
        });
    }
    Ok(lineages)
}

fn read_change_sets(cursor: &mut Cursor<&[u8]>) -> Result<Vec<FileChangeSet>, IndexStorageError> {
    let len = read_len(cursor)?;
    validate_collection_len(cursor, len, 8, "change sets")?;
    let mut change_sets = Vec::with_capacity(len);
    for _ in 0..len {
        let change_len = read_len(cursor)?;
        validate_collection_len(cursor, change_len, 10, "file changes")?;
        let mut changes = Vec::with_capacity(change_len);
        for _ in 0..change_len {
            changes.push(FileChangeRecord {
                file_id: FileId(read_u64(cursor)?),
                kind: decode_file_change_kind(read_u8(cursor)?)?,
                previous_full_path: read_option_string(cursor)?,
                current_full_path: read_option_string(cursor)?,
            });
        }
        change_sets.push(FileChangeSet { changes });
    }
    Ok(change_sets)
}

fn write_u8(bytes: &mut Vec<u8>, value: u8) {
    bytes.push(value);
}

fn write_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_i64(bytes: &mut Vec<u8>, value: i64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn write_len(bytes: &mut Vec<u8>, value: usize) -> Result<(), IndexStorageError> {
    let value = u64::try_from(value)
        .map_err(|_| IndexStorageError::CorruptSnapshot(String::from("collection too large")))?;
    write_u64(bytes, value);
    Ok(())
}

fn write_string(bytes: &mut Vec<u8>, value: &str) -> Result<(), IndexStorageError> {
    let value_bytes = value.as_bytes();
    write_len(bytes, value_bytes.len())?;
    bytes.extend_from_slice(value_bytes);
    Ok(())
}

fn write_option_string(bytes: &mut Vec<u8>, value: Option<&str>) -> Result<(), IndexStorageError> {
    match value {
        Some(value) => {
            write_u8(bytes, 1);
            write_string(bytes, value)?;
        }
        None => write_u8(bytes, 0),
    }
    Ok(())
}

fn write_option_i64(bytes: &mut Vec<u8>, value: Option<i64>) -> Result<(), IndexStorageError> {
    match value {
        Some(value) => {
            write_u8(bytes, 1);
            write_i64(bytes, value);
        }
        None => write_u8(bytes, 0),
    }
    Ok(())
}

fn read_u8(cursor: &mut Cursor<&[u8]>) -> Result<u8, IndexStorageError> {
    let mut value = [0u8; 1];
    cursor.read_exact(&mut value)?;
    Ok(value[0])
}

fn read_u32(cursor: &mut Cursor<&[u8]>) -> Result<u32, IndexStorageError> {
    let mut value = [0u8; 4];
    cursor.read_exact(&mut value)?;
    Ok(u32::from_le_bytes(value))
}

fn read_u64(cursor: &mut Cursor<&[u8]>) -> Result<u64, IndexStorageError> {
    let mut value = [0u8; 8];
    cursor.read_exact(&mut value)?;
    Ok(u64::from_le_bytes(value))
}

fn read_i64(cursor: &mut Cursor<&[u8]>) -> Result<i64, IndexStorageError> {
    let mut value = [0u8; 8];
    cursor.read_exact(&mut value)?;
    Ok(i64::from_le_bytes(value))
}

fn read_len(cursor: &mut Cursor<&[u8]>) -> Result<usize, IndexStorageError> {
    let value = read_u64(cursor)?;
    usize::try_from(value).map_err(|_| {
        IndexStorageError::CorruptSnapshot(String::from("collection length too large"))
    })
}

fn remaining_bytes(cursor: &Cursor<&[u8]>) -> usize {
    cursor
        .get_ref()
        .len()
        .saturating_sub(usize::try_from(cursor.position()).unwrap_or(usize::MAX))
}

fn validate_collection_len(
    cursor: &Cursor<&[u8]>,
    len: usize,
    min_bytes_per_item: usize,
    label: &str,
) -> Result<(), IndexStorageError> {
    if min_bytes_per_item == 0 {
        return Ok(());
    }
    let remaining = remaining_bytes(cursor);
    let max_possible = remaining / min_bytes_per_item;
    if len > max_possible {
        return Err(IndexStorageError::CorruptSnapshot(format!(
            "{label} length exceeds remaining snapshot data"
        )));
    }
    Ok(())
}

fn read_string(cursor: &mut Cursor<&[u8]>) -> Result<String, IndexStorageError> {
    let len = read_len(cursor)?;
    if len > MAX_SNAPSHOT_STRING_BYTES {
        return Err(IndexStorageError::CorruptSnapshot(String::from(
            "string length exceeds maximum",
        )));
    }
    if len > remaining_bytes(cursor) {
        return Err(IndexStorageError::CorruptSnapshot(String::from(
            "string length exceeds remaining snapshot data",
        )));
    }
    let mut value = vec![0u8; len];
    cursor.read_exact(&mut value)?;
    String::from_utf8(value)
        .map_err(|_| IndexStorageError::CorruptSnapshot(String::from("invalid utf-8 string")))
}

fn read_option_string(cursor: &mut Cursor<&[u8]>) -> Result<Option<String>, IndexStorageError> {
    match read_u8(cursor)? {
        0 => Ok(None),
        1 => Ok(Some(read_string(cursor)?)),
        _ => Err(IndexStorageError::CorruptSnapshot(String::from(
            "invalid optional string flag",
        ))),
    }
}

fn read_option_i64(cursor: &mut Cursor<&[u8]>) -> Result<Option<i64>, IndexStorageError> {
    match read_u8(cursor)? {
        0 => Ok(None),
        1 => Ok(Some(read_i64(cursor)?)),
        _ => Err(IndexStorageError::CorruptSnapshot(String::from(
            "invalid optional integer flag",
        ))),
    }
}

fn encode_file_system_kind(kind: winblaze_core::FileSystemKind) -> u8 {
    match kind {
        winblaze_core::FileSystemKind::Ntfs => 1,
        winblaze_core::FileSystemKind::Refs => 2,
        winblaze_core::FileSystemKind::Fat32 => 3,
        winblaze_core::FileSystemKind::ExFat => 4,
        winblaze_core::FileSystemKind::Unknown => 0,
    }
}

fn decode_file_system_kind(value: u8) -> Result<winblaze_core::FileSystemKind, IndexStorageError> {
    match value {
        0 => Ok(winblaze_core::FileSystemKind::Unknown),
        1 => Ok(winblaze_core::FileSystemKind::Ntfs),
        2 => Ok(winblaze_core::FileSystemKind::Refs),
        3 => Ok(winblaze_core::FileSystemKind::Fat32),
        4 => Ok(winblaze_core::FileSystemKind::ExFat),
        _ => Err(IndexStorageError::CorruptSnapshot(String::from(
            "invalid filesystem kind",
        ))),
    }
}

fn encode_scan_state(state: ScanState) -> u8 {
    match state {
        ScanState::Idle => 0,
        ScanState::Initializing => 1,
        ScanState::Scanning => 2,
        ScanState::Indexing => 3,
        ScanState::Completed => 4,
        ScanState::Failed => 5,
        ScanState::Cancelled => 6,
    }
}

fn decode_scan_state(value: u8) -> Result<ScanState, IndexStorageError> {
    match value {
        0 => Ok(ScanState::Idle),
        1 => Ok(ScanState::Initializing),
        2 => Ok(ScanState::Scanning),
        3 => Ok(ScanState::Indexing),
        4 => Ok(ScanState::Completed),
        5 => Ok(ScanState::Failed),
        6 => Ok(ScanState::Cancelled),
        _ => Err(IndexStorageError::CorruptSnapshot(String::from(
            "invalid scan state",
        ))),
    }
}

fn encode_file_change_kind(kind: FileChangeKind) -> u8 {
    match kind {
        FileChangeKind::Added => 0,
        FileChangeKind::Removed => 1,
        FileChangeKind::Modified => 2,
        FileChangeKind::Renamed => 3,
        FileChangeKind::Moved => 4,
    }
}

fn decode_file_change_kind(value: u8) -> Result<FileChangeKind, IndexStorageError> {
    match value {
        0 => Ok(FileChangeKind::Added),
        1 => Ok(FileChangeKind::Removed),
        2 => Ok(FileChangeKind::Modified),
        3 => Ok(FileChangeKind::Renamed),
        4 => Ok(FileChangeKind::Moved),
        _ => Err(IndexStorageError::CorruptSnapshot(String::from(
            "invalid file change kind",
        ))),
    }
}
