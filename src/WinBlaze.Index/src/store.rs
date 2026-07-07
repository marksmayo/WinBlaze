use std::fmt::{self, Display, Formatter};
use std::fs;
#[cfg(test)]
use std::io::Cursor;
use std::io::{self, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::{collections::HashMap, iter::FromIterator};
use winblaze_core::IdHashMap;

use winblaze_core::{
    diff_file_records, DirectoryId, DirectoryRecord, FileAttributes, FileChangeKind,
    FileChangeRecord, FileChangeSet, FileId, FileLineageRecord, FileRecord, ScanProgress,
    ScanSession, ScanState, SearchQuery, VolumeId, VolumeRecord,
};

use crate::schema::{MIGRATIONS, SCHEMA_VERSION};

const INDEX_MAGIC: &[u8; 4] = b"WBIX";
const INDEX_FORMAT_VERSION: u32 = 2;
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

    /// Persists `transaction` to disk without adopting it as this
    /// repository's in-memory state. Scan-session flushes only ever write —
    /// nothing reads back through the same repository instance — and
    /// `apply_transaction`'s clone doubles a multi-GB working set just to
    /// populate state that is thrown away when the session ends.
    pub fn persist_transaction(
        &self,
        transaction: &BufferedIndexTransaction,
    ) -> Result<(), IndexStorageError> {
        persist_index_state(&self.storage_path, transaction)
    }

    /// Persists a snapshot from record slices already sorted by id (the
    /// shape `TreeIndex` holds after a scan). Lets the caller publish the
    /// in-memory model first and write the snapshot off the critical path
    /// without cloning millions of records back into a transaction.
    #[allow(clippy::too_many_arguments)]
    pub fn persist_sorted_records(
        &self,
        volumes: &[VolumeRecord],
        sessions: &[ScanSession],
        directories: &[DirectoryRecord],
        files: &[FileRecord],
        lineages: &[FileLineageRecord],
        file_changes: &[FileChangeSet],
    ) -> Result<(), IndexStorageError> {
        persist_snapshot_with(&self.storage_path, |writer| {
            write_sorted_records(
                writer,
                volumes,
                sessions,
                directories,
                files,
                lineages,
                file_changes,
            )
        })
    }

    pub fn apply_incremental_transaction(
        &mut self,
        transaction: &BufferedIndexTransaction,
    ) -> Result<FileChangeSet, IndexStorageError> {
        // Path-materialized on both sides: lineage/move detection compares
        // full paths, and scanners no longer store them per file.
        let previous_files = self.state.snapshot_files_with_paths();
        let current_files = transaction.snapshot_files_with_paths();
        // Keep the new scan's volumes/sessions/directories but the prior state's
        // files/lineages/file_changes. Building the struct directly avoids
        // `transaction.clone()` deep-copying millions of new-scan file records
        // that the next lines would only overwrite.
        let mut merged = BufferedIndexTransaction {
            volumes: transaction.volumes.clone(),
            sessions: transaction.sessions.clone(),
            directories: transaction.directories.clone(),
            files: self.state.files.clone(),
            lineages: self.state.lineages.clone(),
            file_changes: self.state.file_changes.clone(),
            committed: transaction.committed,
        };
        let change_set = merged.apply_incremental_files(&previous_files, &current_files);
        self.state = merged;
        persist_index_state(&self.storage_path, &self.state)?;
        Ok(change_set)
    }

    pub fn apply_path_matched_incremental_transaction(
        &mut self,
        transaction: &BufferedIndexTransaction,
    ) -> Result<FileChangeSet, IndexStorageError> {
        // Path-materialized on both sides: the remap joins files across
        // scans by full path, and scanners no longer store them per file.
        let previous_files = self.state.snapshot_files_with_paths();
        let current_files =
            remap_current_files_by_path(&previous_files, transaction.snapshot_files_with_paths());
        // Set `files` to the remapped set directly rather than cloning the whole
        // transaction (millions of file records) only to overwrite `.files`.
        let current_transaction = BufferedIndexTransaction {
            volumes: transaction.volumes.clone(),
            sessions: transaction.sessions.clone(),
            directories: transaction.directories.clone(),
            files: current_files
                .into_iter()
                .map(|file| (file.id, file))
                .collect(),
            lineages: transaction.lineages.clone(),
            file_changes: transaction.file_changes.clone(),
            committed: transaction.committed,
        };
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

    /// Consumes the repository, yielding its in-memory state. Used to build
    /// derived read models (e.g. the display tree) without cloning millions
    /// of records.
    pub fn into_transaction(self) -> BufferedIndexTransaction {
        self.state
    }

    /// Takes the in-memory state, leaving the repository empty. Used when
    /// the repository must stay alive but its state moves into a read model.
    pub fn take_state(&mut self) -> BufferedIndexTransaction {
        std::mem::take(&mut self.state)
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

/// Reassigns each current file's id to the previous scan's id when their full
/// paths match (so lineage/diff joins across scans), minting fresh ids for
/// genuinely new paths. Consumes `current` and rewrites ids in place — the
/// records are already owned clones from `snapshot_files_with_paths`, so this
/// avoids a second full-record clone of every file on the volume.
fn remap_current_files_by_path(
    previous: &[FileRecord],
    mut current: Vec<FileRecord>,
) -> Vec<FileRecord> {
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

    for record in &mut current {
        if let Some(previous_record) = previous_by_path.get(record.full_path.as_str()) {
            record.id = previous_record.id;
        } else {
            record.id = FileId(next_id);
            next_id = next_id.saturating_add(1);
        }
    }
    current
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BufferedIndexTransaction {
    volumes: HashMap<VolumeId, VolumeRecord>,
    sessions: HashMap<u64, ScanSession>,
    directories: IdHashMap<DirectoryId, DirectoryRecord>,
    files: IdHashMap<FileId, FileRecord>,
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
    /// Clones the small non-record parts (sessions, lineage, change sets):
    /// callers that feed the records to `TreeIndex::build` still need these
    /// for a full snapshot write.
    pub fn auxiliary_parts(
        &self,
    ) -> (Vec<ScanSession>, Vec<FileLineageRecord>, Vec<FileChangeSet>) {
        let mut sessions: Vec<ScanSession> = self.sessions.values().cloned().collect();
        sessions.sort_unstable_by_key(|session| session.session_id);
        (sessions, self.lineages.clone(), self.file_changes.clone())
    }

    /// Consumes the transaction into plain record vectors (unsorted), so
    /// derived read models can take ownership without cloning.
    pub fn into_record_vecs(self) -> (Vec<VolumeRecord>, Vec<DirectoryRecord>, Vec<FileRecord>) {
        (
            self.volumes.into_values().collect(),
            self.directories.into_values().collect(),
            self.files.into_values().collect(),
        )
    }

    /// By-value counterparts to the trait's `upsert_*` methods, for callers
    /// that own the record: skips the per-record clone, which matters when a
    /// scan session inserts millions of records.
    pub fn insert_volume(&mut self, volume: VolumeRecord) {
        self.volumes.insert(volume.id, volume);
    }

    pub fn insert_directory(&mut self, directory: DirectoryRecord) {
        self.directories.insert(directory.id, directory);
    }

    pub fn insert_file(&mut self, file: FileRecord) {
        self.files.insert(file.id, file);
    }

    /// Pre-allocates bucket capacity for the file and directory maps. A full
    /// scan inserts millions of records into maps that start empty, so without
    /// this the maps rehash ~20 times as they grow. Callers pass an estimate
    /// (the scanner's total record count); over-estimating only wastes a
    /// bounded amount of transient capacity.
    pub fn reserve(&mut self, files: usize, directories: usize) {
        self.files.reserve(files);
        self.directories.reserve(directories);
    }

    pub fn snapshot_volumes(&self) -> Vec<VolumeRecord> {
        let mut volumes = Vec::from_iter(self.volumes.values().cloned());
        // Keys are the map keys (unique), so unstable sort yields identical
        // order without the stable sort's O(n) scratch allocation.
        volumes.sort_unstable_by_key(|volume| volume.id.0);
        volumes
    }

    pub fn snapshot_sessions(&self) -> Vec<ScanSession> {
        let mut sessions = Vec::from_iter(self.sessions.values().cloned());
        sessions.sort_unstable_by_key(|session| session.session_id);
        sessions
    }

    pub fn snapshot_directories(&self) -> Vec<DirectoryRecord> {
        let mut directories = Vec::from_iter(self.directories.values().cloned());
        directories.sort_unstable_by_key(|directory| directory.id.0);
        directories
    }

    pub fn snapshot_files(&self) -> Vec<FileRecord> {
        let mut files = Vec::from_iter(self.files.values().cloned());
        files.sort_unstable_by_key(|file| file.id.0);
        files
    }

    /// Like `snapshot_files`, but with `full_path` materialized from the
    /// directory table (scanners emit files without one). Incremental
    /// rescans join records across scans by path, so both sides of the
    /// comparison need real paths.
    pub fn snapshot_files_with_paths(&self) -> Vec<FileRecord> {
        let mut files = self.snapshot_files();
        for file in &mut files {
            if file.full_path.is_empty() {
                file.full_path = winblaze_core::derive_file_path(&self.directories, file);
            }
        }
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
        let current_by_id: IdHashMap<FileId, &FileRecord> =
            current.iter().map(|record| (record.id, record)).collect();
        let previous_by_id: IdHashMap<FileId, &FileRecord> =
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
    let file = fs::File::open(path)?;
    let cache_read_bytes = file.metadata()?.len();
    let cache_read_millis = millis_u64(read_started.elapsed().as_millis());
    let decode_started = Instant::now();
    let state = deserialize_state_from_reader(BufReader::new(file), cache_read_bytes as usize)?;
    let cache_decode_millis = millis_u64(decode_started.elapsed().as_millis());
    Ok(LoadedIndexState {
        state,
        cache_read_bytes,
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
    persist_snapshot_with(storage_path, |writer| write_state(writer, state))
}

fn persist_snapshot_with<F>(storage_path: &Path, write: F) -> Result<(), IndexStorageError>
where
    F: FnOnce(&mut io::BufWriter<fs::File>) -> Result<(), IndexStorageError>,
{
    if let Some(parent) = storage_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let temp_path = temp_storage_path(storage_path);
    let backup_path = backup_storage_path(storage_path);

    {
        let file = fs::File::create(&temp_path)?;
        let mut writer = io::BufWriter::with_capacity(1 << 20, file);
        write(&mut writer)?;
        writer.flush()?;
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

/// Serializes `state` directly into `writer`, borrowing every record instead
/// of cloning it. The old approach cloned all records twice per flush (once
/// into sorted `Vec`s, once transitively via string copies) and built the
/// whole snapshot in one contiguous in-memory buffer — several hundred MB and
/// multiple seconds of pure copying for a full-drive index.
fn write_state<W: Write>(
    writer: &mut W,
    state: &BufferedIndexTransaction,
) -> Result<(), IndexStorageError> {
    writer.write_all(INDEX_MAGIC)?;
    write_u32(writer, INDEX_FORMAT_VERSION)?;

    let mut volumes: Vec<&VolumeRecord> = state.volumes.values().collect();
    volumes.sort_unstable_by_key(|volume| volume.id.0);
    write_volume_records(writer, &volumes)?;

    let mut sessions: Vec<&ScanSession> = state.sessions.values().collect();
    sessions.sort_unstable_by_key(|session| session.session_id);
    write_session_records(writer, &sessions)?;

    let mut directories: Vec<&DirectoryRecord> = state.directories.values().collect();
    directories.sort_unstable_by_key(|directory| directory.id.0);
    write_directory_records(writer, &directories)?;

    let mut files: Vec<&FileRecord> = state.files.values().collect();
    files.sort_unstable_by_key(|file| file.id.0);
    write_file_records(writer, &files)?;

    write_lineage_records(writer, state.lineages.as_slice())?;
    write_change_sets(writer, state.file_changes.as_slice())?;
    Ok(())
}

/// `write_state` twin over slices that are already sorted by record id.
fn write_sorted_records<W: Write>(
    writer: &mut W,
    volumes: &[VolumeRecord],
    sessions: &[ScanSession],
    directories: &[DirectoryRecord],
    files: &[FileRecord],
    lineages: &[FileLineageRecord],
    file_changes: &[FileChangeSet],
) -> Result<(), IndexStorageError> {
    writer.write_all(INDEX_MAGIC)?;
    write_u32(writer, INDEX_FORMAT_VERSION)?;
    write_volume_records(writer, volumes)?;
    write_session_records(writer, sessions)?;
    write_directory_records(writer, directories)?;
    write_file_records(writer, files)?;
    write_lineage_records(writer, lineages)?;
    write_change_sets(writer, file_changes)?;
    Ok(())
}

#[cfg(test)]
fn deserialize_state(bytes: &[u8]) -> Result<BufferedIndexTransaction, IndexStorageError> {
    deserialize_state_from_reader(Cursor::new(bytes), bytes.len())
}

struct SnapshotReader<R> {
    inner: R,
    remaining: usize,
}

impl<R: Read> SnapshotReader<R> {
    fn new(inner: R, len: usize) -> Self {
        Self {
            inner,
            remaining: len,
        }
    }

    fn read_exact(&mut self, buffer: &mut [u8]) -> Result<(), IndexStorageError> {
        if buffer.len() > self.remaining {
            return Err(IndexStorageError::Io(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "snapshot ended before field was complete",
            )));
        }
        self.inner.read_exact(buffer)?;
        self.remaining -= buffer.len();
        Ok(())
    }

    fn remaining_bytes(&self) -> usize {
        self.remaining
    }
}

fn deserialize_state_from_reader<R: Read>(
    reader: R,
    len: usize,
) -> Result<BufferedIndexTransaction, IndexStorageError> {
    let mut cursor = SnapshotReader::new(reader, len);
    let mut magic = [0u8; 4];
    cursor.read_exact(&mut magic)?;
    if &magic != INDEX_MAGIC {
        return Err(IndexStorageError::CorruptSnapshot(String::from(
            "invalid magic",
        )));
    }

    let version = read_u32(&mut cursor)?;
    if !(1..=INDEX_FORMAT_VERSION).contains(&version) {
        return Err(IndexStorageError::CorruptSnapshot(String::from(
            "unsupported format version",
        )));
    }

    let volumes = read_volume_records(&mut cursor)?;
    let sessions = read_session_records(&mut cursor)?;
    // Directories and files are read straight into their pre-sized maps.
    let directories = read_directory_records(&mut cursor)?;
    let files = read_file_records(&mut cursor, version)?;
    let lineages = read_lineage_records(&mut cursor)?;
    let file_changes = read_change_sets(&mut cursor)?;

    // Volumes/sessions are small; pre-size their maps to the header count so
    // they allocate their buckets exactly once instead of rehashing as they grow.
    let mut volumes_map = HashMap::with_capacity(volumes.len());
    volumes_map.extend(volumes.into_iter().map(|v| (v.id, v)));

    let mut sessions_map = HashMap::with_capacity(sessions.len());
    sessions_map.extend(sessions.into_iter().map(|s| (s.session_id, s)));

    Ok(BufferedIndexTransaction {
        volumes: volumes_map,
        sessions: sessions_map,
        directories,
        files,
        lineages,
        file_changes,
        committed: false,
    })
}

fn write_volume_records<W: Write, R: std::borrow::Borrow<VolumeRecord>>(
    writer: &mut W,
    volumes: &[R],
) -> Result<(), IndexStorageError> {
    write_len(writer, volumes.len())?;
    for volume in volumes {
        let volume = volume.borrow();
        write_u64(writer, volume.id.0)?;
        write_string(writer, &volume.mount_point)?;
        write_option_string(writer, volume.label.as_deref())?;
        write_u8(writer, encode_file_system_kind(volume.file_system))?;
        write_u64(writer, volume.total_bytes)?;
        write_u64(writer, volume.free_bytes)?;
        write_u64(writer, volume.root_directory_id.0)?;
    }
    Ok(())
}

fn write_session_records<W: Write, R: std::borrow::Borrow<ScanSession>>(
    writer: &mut W,
    sessions: &[R],
) -> Result<(), IndexStorageError> {
    write_len(writer, sessions.len())?;
    for session in sessions {
        let session = session.borrow();
        write_u64(writer, session.session_id)?;
        write_u64(writer, session.volume_id.0)?;
        write_string(writer, &session.root_path)?;
        write_u8(writer, encode_scan_state(session.state))?;
        write_u64(writer, session.progress.completed_items)?;
        write_u64(writer, session.progress.total_items)?;
        write_u64(writer, session.progress.completed_bytes)?;
        write_u64(writer, session.progress.total_bytes)?;
    }
    Ok(())
}

fn write_directory_records<W: Write, R: std::borrow::Borrow<DirectoryRecord>>(
    writer: &mut W,
    directories: &[R],
) -> Result<(), IndexStorageError> {
    write_len(writer, directories.len())?;
    // Encode each record into a reused scratch buffer and emit it with a
    // single `write_all`, rather than ~8 tiny writes per record. On a full
    // drive that turns tens of millions of writer calls into a few million.
    let mut buffer = Vec::with_capacity(128);
    for directory in directories {
        let directory = directory.borrow();
        buffer.clear();
        push_u64(&mut buffer, directory.id.0);
        match directory.parent_directory_id {
            Some(parent) => {
                buffer.push(1);
                push_u64(&mut buffer, parent.0);
            }
            None => buffer.push(0),
        }
        push_string(&mut buffer, &directory.name);
        push_string(&mut buffer, &directory.full_path);
        push_u64(&mut buffer, directory.direct_bytes);
        push_u64(&mut buffer, directory.total_bytes);
        push_u64(&mut buffer, directory.direct_entries);
        push_u64(&mut buffer, directory.total_entries);
        writer.write_all(&buffer)?;
    }
    Ok(())
}

fn write_file_records<W: Write, R: std::borrow::Borrow<FileRecord>>(
    writer: &mut W,
    files: &[R],
) -> Result<(), IndexStorageError> {
    write_len(writer, files.len())?;
    // See write_directory_records: one buffered `write_all` per record.
    let mut buffer = Vec::with_capacity(128);
    for file in files {
        let file = file.borrow();
        buffer.clear();
        push_u64(&mut buffer, file.id.0);
        push_u64(&mut buffer, file.parent_directory_id.0);
        push_string(&mut buffer, &file.name);
        push_snapshot_path(&mut buffer, &file.full_path);
        push_u64(&mut buffer, file.size_bytes);
        push_u64(&mut buffer, file.allocation_bytes);
        push_u32(&mut buffer, file.attributes.0);
        push_option_i64(&mut buffer, file.created_utc);
        push_option_i64(&mut buffer, file.modified_utc);
        push_option_i64(&mut buffer, file.accessed_utc);
        writer.write_all(&buffer)?;
    }
    Ok(())
}

fn push_u32(buffer: &mut Vec<u8>, value: u32) {
    buffer.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(buffer: &mut Vec<u8>, value: u64) {
    buffer.extend_from_slice(&value.to_le_bytes());
}

fn push_string(buffer: &mut Vec<u8>, value: &str) {
    push_u64(buffer, value.len() as u64);
    buffer.extend_from_slice(value.as_bytes());
}

fn push_snapshot_path(buffer: &mut Vec<u8>, value: &str) {
    if value.is_empty() {
        buffer.push(0);
    } else {
        buffer.push(1);
        push_string(buffer, value);
    }
}

fn push_option_i64(buffer: &mut Vec<u8>, value: Option<i64>) {
    match value {
        Some(value) => {
            buffer.push(1);
            buffer.extend_from_slice(&value.to_le_bytes());
        }
        None => buffer.push(0),
    }
}

fn write_lineage_records<W: Write>(
    writer: &mut W,
    lineages: &[FileLineageRecord],
) -> Result<(), IndexStorageError> {
    write_len(writer, lineages.len())?;
    for lineage in lineages {
        write_u64(writer, lineage.file_id.0)?;
        write_u64(writer, lineage.previous_parent_directory_id.0)?;
        write_u64(writer, lineage.current_parent_directory_id.0)?;
        write_string(writer, &lineage.previous_full_path)?;
        write_string(writer, &lineage.current_full_path)?;
        write_u8(writer, u8::from(lineage.renamed))?;
        write_u8(writer, u8::from(lineage.moved))?;
    }
    Ok(())
}

fn write_change_sets<W: Write>(
    writer: &mut W,
    change_sets: &[FileChangeSet],
) -> Result<(), IndexStorageError> {
    write_len(writer, change_sets.len())?;
    for change_set in change_sets {
        write_len(writer, change_set.changes.len())?;
        for change in &change_set.changes {
            write_u64(writer, change.file_id.0)?;
            write_u8(writer, encode_file_change_kind(change.kind))?;
            write_option_string(writer, change.previous_full_path.as_deref())?;
            write_option_string(writer, change.current_full_path.as_deref())?;
        }
    }
    Ok(())
}

fn read_volume_records<R: Read>(
    cursor: &mut SnapshotReader<R>,
) -> Result<Vec<VolumeRecord>, IndexStorageError> {
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

fn read_session_records<R: Read>(
    cursor: &mut SnapshotReader<R>,
) -> Result<Vec<ScanSession>, IndexStorageError> {
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

fn read_directory_records<R: Read>(
    cursor: &mut SnapshotReader<R>,
) -> Result<IdHashMap<DirectoryId, DirectoryRecord>, IndexStorageError> {
    let len = read_len(cursor)?;
    validate_collection_len(cursor, len, 41, "directory records")?;
    // Insert straight into the pre-sized map instead of a transient Vec that
    // would then be moved element-by-element into the map.
    let mut directories = IdHashMap::with_capacity_and_hasher(len, Default::default());
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

        let record = DirectoryRecord {
            id,
            parent_directory_id,
            name: read_string(cursor)?,
            full_path: read_string(cursor)?,
            direct_bytes: read_u64(cursor)?,
            total_bytes: read_u64(cursor)?,
            direct_entries: read_u64(cursor)?,
            total_entries: read_u64(cursor)?,
        };
        directories.insert(id, record);
    }
    Ok(directories)
}

fn read_file_records<R: Read>(
    cursor: &mut SnapshotReader<R>,
    version: u32,
) -> Result<IdHashMap<FileId, FileRecord>, IndexStorageError> {
    let len = read_len(cursor)?;
    let min_bytes_per_item = if version >= 2 { 48 } else { 57 };
    validate_collection_len(cursor, len, min_bytes_per_item, "file records")?;
    let mut files = IdHashMap::with_capacity_and_hasher(len, Default::default());
    for _ in 0..len {
        let id = FileId(read_u64(cursor)?);
        let record = FileRecord {
            id,
            parent_directory_id: DirectoryId(read_u64(cursor)?),
            name: read_string(cursor)?,
            full_path: read_snapshot_path(cursor, version)?,
            size_bytes: read_u64(cursor)?,
            allocation_bytes: read_u64(cursor)?,
            attributes: FileAttributes(read_u32(cursor)?),
            created_utc: read_option_i64(cursor)?,
            modified_utc: read_option_i64(cursor)?,
            accessed_utc: read_option_i64(cursor)?,
        };
        files.insert(id, record);
    }
    Ok(files)
}

fn read_lineage_records<R: Read>(
    cursor: &mut SnapshotReader<R>,
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

fn read_change_sets<R: Read>(
    cursor: &mut SnapshotReader<R>,
) -> Result<Vec<FileChangeSet>, IndexStorageError> {
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

fn write_u8<W: Write>(writer: &mut W, value: u8) -> Result<(), IndexStorageError> {
    writer.write_all(&[value])?;
    Ok(())
}

fn write_u32<W: Write>(writer: &mut W, value: u32) -> Result<(), IndexStorageError> {
    writer.write_all(&value.to_le_bytes())?;
    Ok(())
}

fn write_u64<W: Write>(writer: &mut W, value: u64) -> Result<(), IndexStorageError> {
    writer.write_all(&value.to_le_bytes())?;
    Ok(())
}

fn write_len<W: Write>(writer: &mut W, value: usize) -> Result<(), IndexStorageError> {
    let value = u64::try_from(value)
        .map_err(|_| IndexStorageError::CorruptSnapshot(String::from("collection too large")))?;
    write_u64(writer, value)
}

fn write_string<W: Write>(writer: &mut W, value: &str) -> Result<(), IndexStorageError> {
    let value_bytes = value.as_bytes();
    write_len(writer, value_bytes.len())?;
    writer.write_all(value_bytes)?;
    Ok(())
}

fn write_option_string<W: Write>(
    writer: &mut W,
    value: Option<&str>,
) -> Result<(), IndexStorageError> {
    match value {
        Some(value) => {
            write_u8(writer, 1)?;
            write_string(writer, value)?;
        }
        None => write_u8(writer, 0)?,
    }
    Ok(())
}

fn read_u8<R: Read>(cursor: &mut SnapshotReader<R>) -> Result<u8, IndexStorageError> {
    let mut value = [0u8; 1];
    cursor.read_exact(&mut value)?;
    Ok(value[0])
}

fn read_u32<R: Read>(cursor: &mut SnapshotReader<R>) -> Result<u32, IndexStorageError> {
    let mut value = [0u8; 4];
    cursor.read_exact(&mut value)?;
    Ok(u32::from_le_bytes(value))
}

fn read_u64<R: Read>(cursor: &mut SnapshotReader<R>) -> Result<u64, IndexStorageError> {
    let mut value = [0u8; 8];
    cursor.read_exact(&mut value)?;
    Ok(u64::from_le_bytes(value))
}

fn read_i64<R: Read>(cursor: &mut SnapshotReader<R>) -> Result<i64, IndexStorageError> {
    let mut value = [0u8; 8];
    cursor.read_exact(&mut value)?;
    Ok(i64::from_le_bytes(value))
}

fn read_len<R: Read>(cursor: &mut SnapshotReader<R>) -> Result<usize, IndexStorageError> {
    let value = read_u64(cursor)?;
    usize::try_from(value).map_err(|_| {
        IndexStorageError::CorruptSnapshot(String::from("collection length too large"))
    })
}

fn remaining_bytes<R: Read>(cursor: &SnapshotReader<R>) -> usize {
    cursor.remaining_bytes()
}

fn validate_collection_len<R: Read>(
    cursor: &SnapshotReader<R>,
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

fn read_string<R: Read>(cursor: &mut SnapshotReader<R>) -> Result<String, IndexStorageError> {
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

fn read_snapshot_path<R: Read>(
    cursor: &mut SnapshotReader<R>,
    version: u32,
) -> Result<String, IndexStorageError> {
    if version < 2 {
        return read_string(cursor);
    }

    match read_u8(cursor)? {
        0 => Ok(String::new()),
        1 => read_string(cursor),
        _ => Err(IndexStorageError::CorruptSnapshot(String::from(
            "invalid path-present flag",
        ))),
    }
}

fn read_option_string<R: Read>(
    cursor: &mut SnapshotReader<R>,
) -> Result<Option<String>, IndexStorageError> {
    match read_u8(cursor)? {
        0 => Ok(None),
        1 => Ok(Some(read_string(cursor)?)),
        _ => Err(IndexStorageError::CorruptSnapshot(String::from(
            "invalid optional string flag",
        ))),
    }
}

fn read_option_i64<R: Read>(
    cursor: &mut SnapshotReader<R>,
) -> Result<Option<i64>, IndexStorageError> {
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

/// Fuzz/corpus coverage for the binary-cache decoder. The decoder reads an
/// untrusted file (`%LOCALAPPDATA%\WinBlaze\index`); a corrupt or hostile
/// snapshot must always yield `Ok`/`Err`, never panic or over-allocate. These
/// exercise every truncation offset, random garbage, valid-header + garbage
/// bodies, and single-byte flips of a valid snapshot.
#[cfg(test)]
mod fuzz_tests {
    use super::*;
    use winblaze_core::FileSystemKind;

    /// A small but structurally complete snapshot: every section is populated
    /// so truncations and flips land in real fields, not just empty tails.
    fn valid_snapshot() -> Vec<u8> {
        let mut txn = BufferedIndexTransaction::default();
        txn.insert_volume(VolumeRecord {
            id: VolumeId(0),
            mount_point: String::from(r"C:\"),
            label: Some(String::from("OS")),
            file_system: FileSystemKind::Ntfs,
            total_bytes: 1_000,
            free_bytes: 400,
            root_directory_id: DirectoryId(5),
        });
        txn.insert_directory(DirectoryRecord {
            id: DirectoryId(5),
            parent_directory_id: None,
            name: String::from(r"C:\"),
            full_path: String::from(r"C:\"),
            ..Default::default()
        });
        txn.insert_directory(DirectoryRecord {
            id: DirectoryId(10),
            parent_directory_id: Some(DirectoryId(5)),
            name: String::from("Users"),
            full_path: String::from(r"C:\Users"),
            ..Default::default()
        });
        txn.insert_file(FileRecord {
            id: FileId(11),
            parent_directory_id: DirectoryId(10),
            name: String::from("report.txt"),
            full_path: String::new(),
            size_bytes: 1234,
            allocation_bytes: 4096,
            attributes: FileAttributes::ARCHIVE,
            created_utc: Some(1),
            modified_utc: Some(2),
            accessed_utc: None,
        });

        let mut buffer = Vec::new();
        write_state(&mut buffer, &txn).expect("serialize valid snapshot");
        buffer
    }

    /// Deterministic xorshift so the corpus is reproducible across runs.
    struct Rng(u64);
    impl Rng {
        fn next_u32(&mut self) -> u32 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            (self.0 >> 32) as u32
        }
        fn byte(&mut self) -> u8 {
            (self.next_u32() & 0xff) as u8
        }
    }

    #[test]
    fn valid_snapshot_round_trips() {
        let snapshot = valid_snapshot();
        let state = deserialize_state(&snapshot).expect("valid snapshot decodes");
        assert_eq!(state.snapshot_volumes().len(), 1);
        assert_eq!(state.snapshot_directories().len(), 2);
        assert_eq!(state.snapshot_files().len(), 1);
    }

    #[test]
    fn decoder_survives_every_truncation() {
        let snapshot = valid_snapshot();
        // Every prefix of a valid snapshot must decode or error, never panic.
        for len in 0..=snapshot.len() {
            let _ = deserialize_state(&snapshot[..len]);
        }
    }

    #[test]
    fn decoder_survives_random_garbage() {
        let mut rng = Rng(0x9E37_79B9_7F4A_7C15);
        for _ in 0..4000 {
            let len = (rng.next_u32() % 4096) as usize;
            let bytes: Vec<u8> = (0..len).map(|_| rng.byte()).collect();
            let _ = deserialize_state(&bytes);
        }
        // Valid magic + version, then garbage: exercises the section readers
        // rather than bouncing off the header check.
        for _ in 0..4000 {
            let len = (rng.next_u32() % 4096) as usize;
            let mut bytes = Vec::with_capacity(len + 8);
            bytes.extend_from_slice(INDEX_MAGIC);
            bytes.extend_from_slice(&INDEX_FORMAT_VERSION.to_le_bytes());
            bytes.extend((0..len).map(|_| rng.byte()));
            let _ = deserialize_state(&bytes);
        }
    }

    #[test]
    fn decoder_survives_single_byte_flips() {
        let snapshot = valid_snapshot();
        for index in 0..snapshot.len() {
            for delta in [0x01u8, 0x7f, 0x80, 0xff] {
                let mut mutated = snapshot.clone();
                mutated[index] ^= delta;
                let _ = deserialize_state(&mutated);
            }
        }
    }
}
