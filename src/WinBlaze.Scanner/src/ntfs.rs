use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{FromRawHandle, RawHandle};
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::{
    ffi::c_void,
    ptr::{null, null_mut},
};
use winblaze_core::{IdHashMap, IdHashSet};

use winblaze_core::{
    aggregate_directory_records, DirectoryId, DirectoryRecord, FileAttributes, FileId, FileRecord,
    FileSystemKind, ScanEvent, ScanProgress, ScanSummary, VolumeId, VolumeRecord,
};

const FILE_RECORD_SIGNATURE: &[u8; 4] = b"FILE";
const NTFS_RECORD_SIZE: usize = 1024;
const ATTRIBUTE_LIST: u32 = 0x20;
const ATTRIBUTE_FILE_NAME: u32 = 0x30;
const ATTRIBUTE_DATA: u32 = 0x80;
const ATTRIBUTE_BITMAP: u32 = 0xB0;
const FILE_RECORD_FLAG_IN_USE: u16 = 0x0001;
const FILE_RECORD_FLAG_DIRECTORY: u16 = 0x0002;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NtfsEnumeration {
    pub volume: VolumeRecord,
    pub directories: Vec<DirectoryRecord>,
    pub files: Vec<FileRecord>,
    pub summary: ScanSummary,
}

#[derive(Debug)]
pub enum NtfsEnumerationError {
    Io(io::Error),
    InvalidRecord(String),
}

impl From<io::Error> for NtfsEnumerationError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

pub fn enumerate_ntfs_volume(root: &Path) -> Result<NtfsEnumeration, NtfsEnumerationError> {
    let mut stream = open_mft_stream(root)?;
    let mut mft_bytes = Vec::new();
    stream.read_to_end(&mut mft_bytes)?;
    apply_mft_fixups(&mut mft_bytes, stream.bytes_per_sector as usize);
    parse_mft_records(root, &mft_bytes)
}

pub fn enumerate_ntfs_volume_parallel(
    root: &Path,
    worker_count: usize,
) -> Result<NtfsEnumeration, NtfsEnumerationError> {
    let mut stream = open_mft_stream(root)?;
    let mut mft_bytes = Vec::new();
    stream.read_to_end(&mut mft_bytes)?;
    apply_mft_fixups(&mut mft_bytes, stream.bytes_per_sector as usize);
    parse_mft_records_parallel(root, &mft_bytes, worker_count)
}

/// Streaming enumeration that finishes with just the scan summary instead of
/// the fully materialized `NtfsEnumeration`. The scan controller only reads
/// the summary from the result, and building the rest re-resolves every path
/// and clones every record purely to throw them away — a significant share
/// of the fast-path wall clock on a multi-million-record volume.
pub fn enumerate_ntfs_volume_parallel_streaming_summary<F>(
    root: &Path,
    worker_count: usize,
    mut on_event: F,
) -> Result<ScanSummary, NtfsEnumerationError>
where
    F: FnMut(ScanEvent),
{
    stream_ntfs_entries(root, worker_count, &mut on_event)
}

/// Streams MFT records straight to `on_event` as they parse, returning the
/// running summary.
///
/// Files are emitted the moment they parse: their records carry a parent id
/// but no materialized path (paths derive on demand downstream), so nothing
/// gates them and nothing retains them. Directories bake their full path
/// into the emitted record, so a directory whose parent has not resolved yet
/// waits in a bucket keyed by that parent id and cascades out the moment the
/// parent resolves - each record is touched a constant number of times,
/// where the previous implementation rescanned every still-pending record on
/// every read batch.
fn stream_ntfs_entries(
    root: &Path,
    worker_count: usize,
    on_event: &mut dyn FnMut(ScanEvent),
) -> Result<ScanSummary, NtfsEnumerationError> {
    let mut file = open_mft_stream(root)?;
    let bytes_per_sector = file.bytes_per_sector as usize;
    let total_records = (file.total_bytes / NTFS_RECORD_SIZE as u64).max(1);
    let workers = worker_count.max(1);
    let records_per_read = workers.saturating_mul(1024).clamp(1024, 65_536);
    let block_len = NTFS_RECORD_SIZE * records_per_read;
    let mut state = NtfsStreamState::new(root.display().to_string());

    state.emit_root(on_event);

    // Three-stage pipeline so the single-threaded path-resolution emit overlaps
    // the parse and the read instead of running after them:
    //   reader thread  -> fills pooled buffers from the volume
    //   parser thread  -> fixup + parse fan-out, recycles buffers
    //   this thread    -> ingest/emit (keeps on_event + state single-threaded)
    // Four pooled buffers cover one in each stage plus one in flight.
    let (block_tx, block_rx) = mpsc::sync_channel::<io::Result<(Vec<u8>, usize)>>(2);
    let (pool_tx, pool_rx) = mpsc::channel::<Vec<u8>>();
    for _ in 0..4 {
        let _ = pool_tx.send(vec![0u8; block_len]);
    }
    let reader = thread::spawn(move || {
        while let Ok(mut buffer) = pool_rx.recv() {
            let mut filled = 0usize;
            // Fill the block completely (reads can return one extent tail
            // at a time) so parse batches stay large.
            let result = loop {
                match file.read(&mut buffer[filled..]) {
                    Ok(0) => break Ok((buffer, filled)),
                    Ok(read) => {
                        filled += read;
                        if filled == block_len {
                            break Ok((buffer, filled));
                        }
                    }
                    Err(error) => break Err(error),
                }
            };
            let is_error = result.is_err();
            let is_final = matches!(&result, Ok((_, filled)) if *filled < block_len);
            if block_tx.send(result).is_err() || is_error || is_final {
                return;
            }
        }
    });

    // Parser thread: owns the carry, applies fixups + parses each block across
    // the worker fan-out, recycles the buffer, and forwards parsed entries (in
    // MFT order) to the emit loop below.
    type ParsedBatch = (Vec<ParsedNtfsEntry>, Vec<ExtensionSizes>, u64);
    let (parsed_tx, parsed_rx) = mpsc::sync_channel::<Result<ParsedBatch, NtfsEnumerationError>>(2);
    let parser = thread::spawn(move || {
        let mut carry: Vec<u8> = Vec::new();
        let mut processed_records = 0u64;
        while let Ok(block) = block_rx.recv() {
            let (block, filled) = match block {
                Ok(value) => value,
                Err(error) => {
                    let _ = parsed_tx.send(Err(NtfsEnumerationError::Io(error)));
                    return;
                }
            };
            if filled == 0 {
                break;
            }

            // Records split across block boundaries are rare (blocks are
            // record multiples except across odd extent tails); stitch them
            // through the carry path.
            let (batch_ptr, full_records) = if carry.is_empty() && filled % NTFS_RECORD_SIZE == 0 {
                (None, filled / NTFS_RECORD_SIZE)
            } else {
                carry.extend_from_slice(&block[..filled]);
                let full_records = carry.len() / NTFS_RECORD_SIZE;
                (Some(full_records * NTFS_RECORD_SIZE), full_records)
            };

            let mut owned_block = block;
            let batch: &mut [u8] = match batch_ptr {
                None => &mut owned_block[..filled],
                Some(full_bytes) => &mut carry[..full_bytes],
            };

            let result = fixup_and_parse_batch(batch, full_records, workers, bytes_per_sector);
            if batch_ptr.is_some() {
                let full_bytes = full_records * NTFS_RECORD_SIZE;
                carry.drain(..full_bytes);
            }
            let _ = pool_tx.send(owned_block);

            match result {
                Ok((parsed, extensions)) => {
                    processed_records = processed_records.saturating_add(full_records as u64);
                    if parsed_tx
                        .send(Ok((parsed, extensions, processed_records)))
                        .is_err()
                    {
                        return;
                    }
                }
                Err(error) => {
                    let _ = parsed_tx.send(Err(error));
                    return;
                }
            }
        }
    });

    // Env-gated profiling: split this thread's wall into wait (blocked on the
    // parser) vs emit (path resolution). Zero cost unless the var is set.
    let profile = std::env::var_os("WINBLAZE_PROFILE_STREAM").is_some();
    let mut wait_ns = 0u128;
    let mut emit_ns = 0u128;

    let stream_result = (|| -> Result<(), NtfsEnumerationError> {
        loop {
            let message = if profile {
                let started = std::time::Instant::now();
                let message = parsed_rx.recv();
                wait_ns += started.elapsed().as_nanos();
                message
            } else {
                parsed_rx.recv()
            };
            let Ok(message) = message else { break };
            let (parsed, extensions, processed_records) = message?;

            let emit_started = profile.then(std::time::Instant::now);
            for entry in parsed {
                state.ingest_entry(entry, on_event);
            }
            for extension in extensions {
                state.ingest_extension(extension, on_event);
            }
            if let Some(started) = emit_started {
                emit_ns += started.elapsed().as_nanos();
            }

            let progress = ScanProgress {
                completed_items: processed_records,
                total_items: total_records,
                completed_bytes: processed_records.saturating_mul(NTFS_RECORD_SIZE as u64),
                total_bytes: total_records.saturating_mul(NTFS_RECORD_SIZE as u64),
            };
            on_event(ScanEvent::Progress(progress));
        }
        Ok(())
    })();

    if profile {
        eprintln!(
            "[stream-profile] wait_ms={} emit_ms={}",
            wait_ns / 1_000_000,
            emit_ns / 1_000_000,
        );
    }

    drop(parsed_rx);
    let _ = parser.join();
    let _ = reader.join();
    stream_result?;

    state.finish(on_event);
    Ok(state.into_summary())
}

const NTFS_ROOT_RECORD: u64 = 5;

/// Streaming emit state: resolved directory paths, orphaned directories
/// bucketed by the parent id they wait on, and the running summary.
struct NtfsStreamState {
    root_text: String,
    /// Directory id -> full path, for every directory emitted so far.
    resolved_paths: IdHashMap<u64, String>,
    /// Directories whose parent has not resolved yet, keyed by that parent.
    orphan_dirs: IdHashMap<u64, Vec<ParsedNtfsEntry>>,
    /// Emitted files that carry $ATTRIBUTE_LIST: the only records whose
    /// sizes a later extension record can still correct (with a re-emit).
    retained_files: IdHashMap<u64, ParsedNtfsEntry>,
    /// Extension sizes seen before their base record parsed.
    pending_extensions: IdHashMap<u64, ExtensionSizes>,
    /// Reusable worklist for cascading newly resolved directories.
    cascade: Vec<ParsedNtfsEntry>,
    files_seen: u64,
    directories_seen: u64,
    total_size_bytes: u64,
    total_allocation_bytes: u64,
}

impl NtfsStreamState {
    fn new(root_text: String) -> Self {
        Self {
            root_text,
            resolved_paths: IdHashMap::default(),
            orphan_dirs: IdHashMap::default(),
            retained_files: IdHashMap::default(),
            pending_extensions: IdHashMap::default(),
            cascade: Vec::new(),
            files_seen: 0,
            directories_seen: 0,
            total_size_bytes: 0,
            total_allocation_bytes: 0,
        }
    }

    /// Emits the scan root (record 5) up front: children can then resolve in
    /// a single hop, and downstream root selection needs record 5 in the
    /// persisted model (its absence made the tree mis-root at whichever
    /// top-level directory happened to sort first).
    fn emit_root(&mut self, on_event: &mut dyn FnMut(ScanEvent)) {
        self.resolved_paths
            .insert(NTFS_ROOT_RECORD, self.root_text.clone());
        self.directories_seen = self.directories_seen.saturating_add(1);
        on_event(ScanEvent::DirectoryFound(DirectoryRecord {
            id: DirectoryId(NTFS_ROOT_RECORD),
            parent_directory_id: None,
            name: self.root_text.clone(),
            full_path: self.root_text.clone(),
            direct_bytes: 0,
            total_bytes: 0,
            direct_entries: 0,
            total_entries: 0,
        }));
    }

    fn ingest_entry(&mut self, mut entry: ParsedNtfsEntry, on_event: &mut dyn FnMut(ScanEvent)) {
        let file_id = entry.file_id.0;
        // Extension records are rare; the map is empty for the vast majority of
        // records, so skip the per-entry hash probe unless something is pending.
        if !self.pending_extensions.is_empty() {
            if let Some(sizes) = self.pending_extensions.remove(&file_id) {
                entry.size_bytes = entry.size_bytes.max(sizes.size_bytes);
                entry.allocation_bytes = entry.allocation_bytes.max(sizes.allocation_bytes);
            }
        }

        if entry.is_directory {
            if file_id == NTFS_ROOT_RECORD {
                return; // synthetic root already emitted
            }
            let parent = normalized_parent(&entry);
            if self.resolved_paths.contains_key(&parent) {
                self.cascade.push(entry);
                self.drain_cascade(on_event);
            } else {
                self.orphan_dirs.entry(parent).or_default().push(entry);
            }
        } else {
            self.emit_file(entry, on_event);
        }
    }

    fn emit_file(&mut self, mut entry: ParsedNtfsEntry, on_event: &mut dyn FnMut(ScanEvent)) {
        self.files_seen = self.files_seen.saturating_add(1);
        self.total_size_bytes = self.total_size_bytes.saturating_add(entry.size_bytes);
        self.total_allocation_bytes = self
            .total_allocation_bytes
            .saturating_add(entry.allocation_bytes);

        let retain = entry.has_attribute_list;
        let name = if retain {
            entry.name.clone()
        } else {
            std::mem::take(&mut entry.name)
        };
        on_event(ScanEvent::FileFound(FileRecord {
            id: entry.file_id,
            parent_directory_id: DirectoryId(entry.parent_directory_id.unwrap_or(NTFS_ROOT_RECORD)),
            name,
            full_path: String::new(),
            size_bytes: entry.size_bytes,
            allocation_bytes: entry.allocation_bytes,
            attributes: entry.attributes,
            created_utc: entry.created_utc,
            modified_utc: entry.modified_utc,
            accessed_utc: entry.accessed_utc,
        }));
        if retain {
            self.retained_files.insert(entry.file_id.0, entry);
        }
    }

    /// Extension records carry attributes (and so sizes) for a base record
    /// that overflowed. NTFS only moves attributes out of a record that has
    /// $ATTRIBUTE_LIST, so a file base is always in `retained_files` when its
    /// extension arrives after it; before it, the sizes park in
    /// `pending_extensions`. Directory bases ignore sizes entirely.
    fn ingest_extension(&mut self, extension: ExtensionSizes, on_event: &mut dyn FnMut(ScanEvent)) {
        if let Some(entry) = self.retained_files.get_mut(&extension.base_record) {
            let size = entry.size_bytes.max(extension.size_bytes);
            let allocation = entry.allocation_bytes.max(extension.allocation_bytes);
            if size == entry.size_bytes && allocation == entry.allocation_bytes {
                return;
            }
            self.total_size_bytes = self
                .total_size_bytes
                .saturating_add(size - entry.size_bytes);
            self.total_allocation_bytes = self
                .total_allocation_bytes
                .saturating_add(allocation - entry.allocation_bytes);
            entry.size_bytes = size;
            entry.allocation_bytes = allocation;
            // Re-emit so downstream consumers upsert the corrected sizes.
            on_event(ScanEvent::FileFound(FileRecord {
                id: entry.file_id,
                parent_directory_id: DirectoryId(
                    entry.parent_directory_id.unwrap_or(NTFS_ROOT_RECORD),
                ),
                name: entry.name.clone(),
                full_path: String::new(),
                size_bytes: entry.size_bytes,
                allocation_bytes: entry.allocation_bytes,
                attributes: entry.attributes,
                created_utc: entry.created_utc,
                modified_utc: entry.modified_utc,
                accessed_utc: entry.accessed_utc,
            }));
            return;
        }

        if self.resolved_paths.contains_key(&extension.base_record) {
            return; // directory base: sizes unused
        }

        self.pending_extensions
            .entry(extension.base_record)
            .and_modify(|sizes| {
                sizes.size_bytes = sizes.size_bytes.max(extension.size_bytes);
                sizes.allocation_bytes = sizes.allocation_bytes.max(extension.allocation_bytes);
            })
            .or_insert(extension);
    }

    /// Emits every directory on the worklist, then any orphans that were
    /// waiting on one of them, transitively. Entries reaching this point
    /// have a resolved parent except during `finish`, where a missing parent
    /// deliberately falls back to the root path.
    fn drain_cascade(&mut self, on_event: &mut dyn FnMut(ScanEvent)) {
        while let Some(mut entry) = self.cascade.pop() {
            let file_id = entry.file_id.0;
            let parent = normalized_parent(&entry);
            let full_path = {
                let parent_path = self
                    .resolved_paths
                    .get(&parent)
                    .map(String::as_str)
                    .unwrap_or(self.root_text.as_str());
                join_path(parent_path, &entry.name)
            };
            let name = std::mem::take(&mut entry.name);
            self.directories_seen = self.directories_seen.saturating_add(1);
            on_event(ScanEvent::DirectoryFound(DirectoryRecord {
                id: DirectoryId(file_id),
                parent_directory_id: entry.parent_directory_id.map(DirectoryId),
                name,
                full_path: full_path.clone(),
                direct_bytes: 0,
                total_bytes: 0,
                direct_entries: 0,
                total_entries: 0,
            }));
            self.resolved_paths.insert(file_id, full_path);
            if let Some(children) = self.orphan_dirs.remove(&file_id) {
                self.cascade.extend(children);
            }
        }
    }

    /// Final pass: directories whose parent never parsed (deleted, corrupt,
    /// or cyclic references) resolve under the root. Buckets whose key is
    /// itself waiting in another bucket are left for their parent's cascade
    /// so they keep their real path; pure cycles break at the smallest key.
    fn finish(&mut self, on_event: &mut dyn FnMut(ScanEvent)) {
        while !self.orphan_dirs.is_empty() {
            let mut waiting: IdHashSet<u64> = IdHashSet::default();
            for children in self.orphan_dirs.values() {
                for child in children {
                    waiting.insert(child.file_id.0);
                }
            }
            let mut stuck: Vec<u64> = self
                .orphan_dirs
                .keys()
                .filter(|key| !waiting.contains(key))
                .copied()
                .collect();
            if stuck.is_empty() {
                if let Some(any) = self.orphan_dirs.keys().copied().min() {
                    stuck.push(any);
                }
            }
            stuck.sort_unstable();
            for parent in stuck {
                if let Some(children) = self.orphan_dirs.remove(&parent) {
                    self.cascade.extend(children);
                    self.drain_cascade(on_event);
                }
            }
        }
    }

    fn into_summary(self) -> ScanSummary {
        ScanSummary {
            files_seen: self.files_seen,
            directories_seen: self.directories_seen,
            total_size_bytes: self.total_size_bytes,
            total_allocation_bytes: self.total_allocation_bytes,
        }
    }
}

/// The parent a record resolves under: self-referential or absent parent
/// links resolve under the root record.
fn normalized_parent(entry: &ParsedNtfsEntry) -> u64 {
    match entry.parent_directory_id {
        Some(parent) if parent != entry.file_id.0 => parent,
        _ => NTFS_ROOT_RECORD,
    }
}

/// Parses one read batch of MFT records, fanning the work across `workers`
/// threads. The read loop was previously single-threaded regardless of
/// `worker_count` (the count only sized the read buffer), which left the
/// elevated fast path slower than the directory-walk fallback it exists to
/// beat.
fn parse_batch(
    batch: &[u8],
    record_count: usize,
    workers: usize,
) -> Result<(Vec<ParsedNtfsEntry>, Vec<ExtensionSizes>), NtfsEnumerationError> {
    if workers <= 1 || record_count < 2048 {
        return parse_record_range(batch, 0, record_count);
    }

    let chunk_size = record_count.div_ceil(workers);
    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        for worker_index in 0..workers {
            let start_record = worker_index * chunk_size;
            if start_record >= record_count {
                break;
            }
            let end_record = ((worker_index + 1) * chunk_size).min(record_count);
            handles.push(scope.spawn(move || parse_record_range(batch, start_record, end_record)));
        }

        // Most records parse into an entry, so size the combined buffer to the
        // record count up front rather than growing it across worker joins.
        let mut parsed = Vec::with_capacity(record_count);
        let mut extensions = Vec::new();
        for handle in handles {
            match handle.join() {
                Ok(Ok((chunk_entries, chunk_extensions))) => {
                    parsed.extend(chunk_entries);
                    extensions.extend(chunk_extensions);
                }
                Ok(Err(error)) => return Err(error),
                Err(_) => {
                    return Err(NtfsEnumerationError::InvalidRecord(String::from(
                        "parallel parser worker panicked",
                    )))
                }
            }
        }
        Ok((parsed, extensions))
    })
}

/// Streaming-path counterpart to [`parse_batch`] that also applies the sector
/// fixups, with each worker fixing up and parsing its own disjoint,
/// record-aligned slice of the buffer. Folding the fixup into the parse fan-out
/// removes the serial full-batch fixup pass that used to run on the read thread
/// before the workers started. `batch.len()` must be `record_count *
/// NTFS_RECORD_SIZE` (the streaming reader guarantees this).
fn fixup_and_parse_batch(
    batch: &mut [u8],
    record_count: usize,
    workers: usize,
    bytes_per_sector: usize,
) -> Result<(Vec<ParsedNtfsEntry>, Vec<ExtensionSizes>), NtfsEnumerationError> {
    if workers <= 1 || record_count < 2048 {
        apply_mft_fixups(batch, bytes_per_sector);
        return parse_record_range(batch, 0, record_count);
    }

    let chunk_size = record_count.div_ceil(workers);
    thread::scope(|scope| {
        let mut handles = Vec::with_capacity(workers);
        let mut remaining: &mut [u8] = batch;
        for worker_index in 0..workers {
            let start_record = worker_index * chunk_size;
            if start_record >= record_count {
                break;
            }
            let end_record = ((worker_index + 1) * chunk_size).min(record_count);
            let chunk_records = end_record - start_record;
            // split_at_mut on a record boundary hands each worker a disjoint,
            // exclusively-borrowed slice it can fix up in place.
            let (chunk, rest) = remaining.split_at_mut(chunk_records * NTFS_RECORD_SIZE);
            remaining = rest;
            handles.push(scope.spawn(move || {
                apply_mft_fixups(chunk, bytes_per_sector);
                parse_record_range(chunk, 0, chunk_records)
            }));
        }

        let mut parsed = Vec::with_capacity(record_count);
        let mut extensions = Vec::new();
        for handle in handles {
            match handle.join() {
                Ok(Ok((chunk_entries, chunk_extensions))) => {
                    parsed.extend(chunk_entries);
                    extensions.extend(chunk_extensions);
                }
                Ok(Err(error)) => return Err(error),
                Err(_) => {
                    return Err(NtfsEnumerationError::InvalidRecord(String::from(
                        "parallel parser worker panicked",
                    )))
                }
            }
        }
        Ok((parsed, extensions))
    })
}

// ---------------------------------------------------------------------------
// Raw-volume MFT access.
//
// NTFS refuses to open metadata files like $MFT through the filesystem
// namespace — for every caller, elevated or not, backup privilege or not —
// so the only way to read the MFT is the volume itself: open \\.\C:, read
// the boot sector for geometry, read the MFT's own record (record 0) to get
// its $DATA runlist, then read those extents directly. Requires Administrator
// (volume handles are ACL'd), which is exactly the "fast scan" contract.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
struct MftGeometry {
    bytes_per_sector: u16,
    bytes_per_cluster: u64,
    mft_start_offset: u64,
    record_size: u32,
}

/// Reader over the MFT's extents on a raw volume handle. Buffers internally
/// in sector-aligned chunks because direct volume access requires aligned
/// offsets and lengths, while callers read in arbitrary sizes.
pub struct VolumeMftReader {
    volume: File,
    /// (byte offset on the volume, byte length), cluster-aligned by
    /// construction.
    extents: Vec<(u64, u64)>,
    extent_index: usize,
    consumed_in_extent: u64,
    bytes_per_sector: usize,
    chunk: Vec<u8>,
    chunk_pos: usize,
    chunk_len: usize,
}

const VOLUME_READ_CHUNK: usize = 4 * 1024 * 1024;

impl VolumeMftReader {
    fn total_bytes(&self) -> u64 {
        self.extents.iter().map(|extent| extent.1).sum()
    }

    fn fill_chunk(&mut self) -> io::Result<()> {
        while self.extent_index < self.extents.len() {
            let (start, length) = self.extents[self.extent_index];
            let remaining = length - self.consumed_in_extent;
            if remaining == 0 {
                self.extent_index += 1;
                self.consumed_in_extent = 0;
                continue;
            }

            // Chunk size and consumed offset are both sector multiples, so
            // the volume read below stays aligned.
            let to_read = remaining.min(VOLUME_READ_CHUNK as u64) as usize;
            self.volume
                .seek(SeekFrom::Start(start + self.consumed_in_extent))?;
            self.chunk.resize(to_read, 0);
            self.volume.read_exact(&mut self.chunk[..to_read])?;
            self.consumed_in_extent += to_read as u64;
            self.chunk_pos = 0;
            self.chunk_len = to_read;
            return Ok(());
        }
        self.chunk_pos = 0;
        self.chunk_len = 0;
        Ok(())
    }
}

impl Read for VolumeMftReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // Serve any buffered remainder first so bytes stay in order.
        if self.chunk_pos < self.chunk_len {
            let available = self.chunk_len - self.chunk_pos;
            let count = available.min(buf.len());
            buf[..count].copy_from_slice(&self.chunk[self.chunk_pos..self.chunk_pos + count]);
            self.chunk_pos += count;
            return Ok(count);
        }

        // Large sector-aligned requests read straight from the volume into
        // the caller's buffer: the bounce-chunk copy cost ~0.7s per full
        // C:\ MFT (4.8 GB memcpy'd twice).
        while self.extent_index < self.extents.len() {
            let (start, length) = self.extents[self.extent_index];
            let remaining = length - self.consumed_in_extent;
            if remaining == 0 {
                self.extent_index += 1;
                self.consumed_in_extent = 0;
                continue;
            }

            let sector = self.bytes_per_sector.max(512);
            let direct = (buf.len().min(remaining as usize) / sector) * sector;
            if direct >= VOLUME_READ_CHUNK {
                self.volume
                    .seek(SeekFrom::Start(start + self.consumed_in_extent))?;
                self.volume.read_exact(&mut buf[..direct])?;
                self.consumed_in_extent += direct as u64;
                return Ok(direct);
            }

            // Small or unaligned tail: use the aligned bounce chunk.
            self.fill_chunk()?;
            if self.chunk_len == 0 {
                return Ok(0);
            }
            let available = self.chunk_len - self.chunk_pos;
            let count = available.min(buf.len());
            buf[..count].copy_from_slice(&self.chunk[self.chunk_pos..self.chunk_pos + count]);
            self.chunk_pos += count;
            return Ok(count);
        }
        Ok(0)
    }
}

/// The MFT byte stream plus the metadata batch-processing needs.
pub struct MftStream {
    reader: Box<dyn Read + Send>,
    pub bytes_per_sector: u16,
    pub total_bytes: u64,
}

impl Read for MftStream {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.reader.read(buf)
    }
}

fn open_volume_handle(root: &Path) -> io::Result<File> {
    let root_text = root.display().to_string();
    let drive = root_text
        .chars()
        .next()
        .filter(|ch| ch.is_ascii_alphabetic())
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("scan root {root_text} is not a drive root"),
            )
        })?;
    if root_text.len() > 3 || root_text.chars().nth(1) != Some(':') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("MFT fast path requires a drive root, got {root_text}"),
        ));
    }

    let volume_path = format!(r"\\.\{drive}:");
    let wide_path: Vec<u16> = volume_path
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    let handle = unsafe {
        CreateFileW(
            wide_path.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            null_mut(),
            OPEN_EXISTING,
            FILE_FLAG_SEQUENTIAL_SCAN,
            null_mut(),
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(unsafe { File::from_raw_handle(handle as RawHandle) })
}

fn read_volume_geometry(volume: &mut File) -> io::Result<MftGeometry> {
    let mut boot = [0u8; 512];
    volume.seek(SeekFrom::Start(0))?;
    volume.read_exact(&mut boot)?;

    if &boot[3..7] != b"NTFS" {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "volume is not NTFS",
        ));
    }

    let bytes_per_sector = u16::from_le_bytes([boot[11], boot[12]]);
    let sectors_per_cluster = boot[13] as u64;
    let mft_lcn = u64::from_le_bytes(boot[48..56].try_into().unwrap());
    let clusters_per_record = boot[64] as i8;

    if bytes_per_sector == 0 || sectors_per_cluster == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid NTFS boot sector geometry",
        ));
    }

    let bytes_per_cluster = bytes_per_sector as u64 * sectors_per_cluster;
    let record_size = if clusters_per_record > 0 {
        clusters_per_record as u32 * bytes_per_cluster as u32
    } else {
        1u32 << (-(clusters_per_record as i32))
    };

    Ok(MftGeometry {
        bytes_per_sector,
        bytes_per_cluster,
        mft_start_offset: mft_lcn * bytes_per_cluster,
        record_size,
    })
}

/// Decodes an NTFS data-run list into (volume byte offset, byte length)
/// extents. Sparse runs (offset size 0) are invalid for the MFT.
fn decode_data_runs(runs: &[u8], bytes_per_cluster: u64) -> io::Result<Vec<(u64, u64)>> {
    let mut extents = Vec::new();
    let mut cursor = 0usize;
    let mut current_lcn: i64 = 0;

    while cursor < runs.len() {
        let header = runs[cursor];
        if header == 0 {
            break;
        }
        cursor += 1;
        let length_size = (header & 0x0F) as usize;
        let offset_size = (header >> 4) as usize;
        if length_size == 0 || length_size > 8 || offset_size > 8 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "invalid data run header",
            ));
        }
        if cursor + length_size + offset_size > runs.len() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "truncated data run",
            ));
        }

        let mut run_length = 0u64;
        for i in 0..length_size {
            run_length |= (runs[cursor + i] as u64) << (8 * i);
        }
        cursor += length_size;

        if offset_size == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "sparse run in MFT data attribute",
            ));
        }
        let mut offset_delta = 0i64;
        for i in 0..offset_size {
            offset_delta |= (runs[cursor + i] as i64) << (8 * i);
        }
        // Sign-extend the offset delta.
        let shift = 64 - 8 * offset_size;
        offset_delta = (offset_delta << shift) >> shift;
        cursor += offset_size;

        // Checked arithmetic throughout: a corrupt/hostile MFT can carry run
        // values that overflow these products (which would panic in debug and
        // wrap to a bogus offset in release). A real volume never overflows,
        // so overflow means the data is bad — reject it and fall back.
        current_lcn = match current_lcn.checked_add(offset_delta) {
            Some(value) => value,
            None => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "data run offset overflow",
                ))
            }
        };
        if current_lcn < 0 || run_length == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "data run points before volume start",
            ));
        }
        let start_bytes = (current_lcn as u64).checked_mul(bytes_per_cluster);
        let length_bytes = run_length.checked_mul(bytes_per_cluster);
        match (start_bytes, length_bytes) {
            (Some(start), Some(length)) => extents.push((start, length)),
            _ => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "data run size overflow",
                ))
            }
        }
    }

    if extents.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "MFT data attribute has no runs",
        ));
    }
    Ok(extents)
}

/// Drops (and partially trims) trailing MFT data-run extents that lie beyond
/// the $DATA valid-data-length: those clusters are uninitialized and hold no
/// FILE records. Extents are in data-stream order, so this walks them
/// accumulating length until the cluster-rounded valid length is covered.
fn trim_extents_to_valid_length(extents: &mut Vec<(u64, u64)>, valid_len: u64, cluster: u64) {
    if cluster == 0 || valid_len == 0 || valid_len == u64::MAX {
        return;
    }
    let cap = valid_len.div_ceil(cluster).saturating_mul(cluster);
    let mut cumulative = 0u64;
    let mut keep = 0usize;
    for (index, &(_, length)) in extents.iter().enumerate() {
        let remaining = cap.saturating_sub(cumulative);
        if remaining == 0 {
            break;
        }
        if length <= remaining {
            cumulative += length;
            keep = index + 1;
        } else {
            extents[index].1 = remaining;
            keep = index + 1;
            break;
        }
    }
    // Never trim to empty: a bogus valid length must not defeat the scan.
    if keep > 0 {
        extents.truncate(keep);
    }
}

/// Reads the MFT's own file record (record 0) and extracts the $DATA
/// runlist. A heavily fragmented MFT whose runs spill into an attribute
/// list is not supported and falls back to the directory walk.
fn read_mft_extents(volume: &mut File, geometry: &MftGeometry) -> io::Result<Vec<(u64, u64)>> {
    // Read a full cluster (aligned) which contains record 0 at its start.
    let read_len = (geometry.record_size as u64)
        .max(geometry.bytes_per_cluster)
        .max(geometry.bytes_per_sector as u64) as usize;
    let mut record = vec![0u8; read_len];
    volume.seek(SeekFrom::Start(geometry.mft_start_offset))?;
    volume.read_exact(&mut record)?;
    record.truncate(geometry.record_size as usize);

    if &record[0..4] != FILE_RECORD_SIGNATURE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "MFT record 0 has no FILE signature",
        ));
    }
    if !apply_record_fixups(&mut record, geometry.bytes_per_sector as usize) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "MFT record 0 fixup mismatch",
        ));
    }

    let first_attribute = u16::from_le_bytes([record[20], record[21]]) as usize;
    let mut cursor = first_attribute;
    while cursor + 8 <= record.len() {
        let attribute_type = u32::from_le_bytes(record[cursor..cursor + 4].try_into().unwrap());
        if attribute_type == u32::MAX {
            break;
        }
        let attribute_length =
            u32::from_le_bytes(record[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        if attribute_length == 0 || cursor + attribute_length > record.len() {
            break;
        }
        let non_resident = record.get(cursor + 8).copied().unwrap_or(0);
        if attribute_type == ATTRIBUTE_DATA {
            if non_resident == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "MFT $DATA attribute unexpectedly resident",
                ));
            }
            // Valid-data-length (offset 56 of the non-resident header): bytes
            // beyond it in the $MFT data stream are uninitialized zeros with no
            // FILE records, so there is no reason to read them. On a volume
            // whose MFT was preallocated large this trims real I/O; when the
            // MFT is fully initialized (valid == allocated) it is a no-op.
            let valid_data_length = if attribute_length >= 64 {
                le_u64(&record, cursor + 56)
            } else {
                u64::MAX
            };
            let run_offset =
                u16::from_le_bytes([record[cursor + 32], record[cursor + 33]]) as usize;
            if run_offset >= attribute_length {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "MFT $DATA run offset out of bounds",
                ));
            }
            let runs = &record[cursor + run_offset..cursor + attribute_length];
            let mut extents = decode_data_runs(runs, geometry.bytes_per_cluster)?;
            trim_extents_to_valid_length(
                &mut extents,
                valid_data_length,
                geometry.bytes_per_cluster,
            );
            return Ok(extents);
        }
        cursor += attribute_length;
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "MFT record 0 has no non-resident $DATA attribute",
    ))
}

/// Reads one MFT record (1024 bytes) from the volume, reading the whole
/// containing cluster so the raw read stays cluster/sector aligned (required on
/// 4Kn volumes), then applies the sector fixups.
fn read_mft_record_at(
    volume: &mut File,
    geometry: &MftGeometry,
    physical: u64,
) -> io::Result<Vec<u8>> {
    let cluster = geometry
        .bytes_per_cluster
        .max(geometry.bytes_per_sector as u64)
        .max(NTFS_RECORD_SIZE as u64);
    let base = (physical / cluster) * cluster;
    let within = (physical - base) as usize;
    let mut buffer = vec![0u8; cluster as usize];
    volume.seek(SeekFrom::Start(base))?;
    volume.read_exact(&mut buffer)?;
    if within + NTFS_RECORD_SIZE > buffer.len() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "record spans cluster",
        ));
    }
    let mut record = buffer[within..within + NTFS_RECORD_SIZE].to_vec();
    if &record[0..4] != FILE_RECORD_SIGNATURE
        || !apply_record_fixups(&mut record, geometry.bytes_per_sector as usize)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "MFT record has no valid FILE signature",
        ));
    }
    Ok(record)
}

/// Physical byte offset of logical MFT record `number`, mapped through the
/// `$DATA` extents (the reader's logical stream order).
fn mft_record_physical_offset(data_extents: &[(u64, u64)], number: u64) -> Option<u64> {
    let target = number * NTFS_RECORD_SIZE as u64;
    let mut logical = 0u64;
    for &(start, length) in data_extents {
        if target < logical + length {
            return Some(start + (target - logical));
        }
        logical += length;
    }
    None
}

/// Extracts the raw `$BITMAP` bytes from a single MFT record if it carries that
/// attribute (resident or non-resident); returns `Ok(None)` when the record has
/// no `$BITMAP` so the caller can look elsewhere (e.g. an extension record).
fn extract_bitmap_from_record(
    record: &[u8],
    volume: &mut File,
    geometry: &MftGeometry,
) -> io::Result<Option<Vec<u8>>> {
    let mut cursor = le_u16(record, 20) as usize;
    while cursor + 8 <= record.len() {
        let attribute_type = le_u32(record, cursor);
        if attribute_type == u32::MAX {
            break;
        }
        let attribute_length = le_u32(record, cursor + 4) as usize;
        if attribute_length == 0 || cursor + attribute_length > record.len() {
            break;
        }
        if attribute_type == ATTRIBUTE_BITMAP {
            let non_resident = record.get(cursor + 8).copied().unwrap_or(0);
            if non_resident == 0 {
                let value_offset = le_u16(record, cursor + 20) as usize;
                let value_length = le_u32(record, cursor + 16) as usize;
                let start = cursor + value_offset;
                let end = start + value_length;
                if end <= record.len() {
                    return Ok(Some(record[start..end].to_vec()));
                }
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "resident $BITMAP out of bounds",
                ));
            }
            let real_size = if attribute_length >= 56 {
                le_u64(record, cursor + 48) as usize
            } else {
                usize::MAX
            };
            let run_offset = le_u16(record, cursor + 32) as usize;
            if run_offset >= attribute_length {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "$BITMAP run offset out of bounds",
                ));
            }
            let runs = &record[cursor + run_offset..cursor + attribute_length];
            let extents = decode_data_runs(runs, geometry.bytes_per_cluster)?;
            let mut bitmap = Vec::new();
            let mut buffer = vec![0u8; VOLUME_READ_CHUNK];
            for &(start, length) in &extents {
                let mut offset = 0u64;
                while offset < length {
                    let to_read = ((length - offset) as usize).min(buffer.len());
                    volume.seek(SeekFrom::Start(start + offset))?;
                    volume.read_exact(&mut buffer[..to_read])?;
                    bitmap.extend_from_slice(&buffer[..to_read]);
                    offset += to_read as u64;
                }
            }
            bitmap.truncate(real_size.min(bitmap.len()));
            return Ok(Some(bitmap));
        }
        cursor += attribute_length;
    }
    Ok(None)
}

/// Returns the raw `$ATTRIBUTE_LIST` value bytes (resident inline, or read from
/// its runs when non-resident), or None if the record has no attribute list.
fn read_attribute_list_value(
    record: &[u8],
    volume: &mut File,
    geometry: &MftGeometry,
) -> io::Result<Option<Vec<u8>>> {
    let mut cursor = le_u16(record, 20) as usize;
    while cursor + 8 <= record.len() {
        let attribute_type = le_u32(record, cursor);
        if attribute_type == u32::MAX {
            break;
        }
        let attribute_length = le_u32(record, cursor + 4) as usize;
        if attribute_length == 0 || cursor + attribute_length > record.len() {
            break;
        }
        if attribute_type == ATTRIBUTE_LIST {
            let non_resident = record.get(cursor + 8).copied().unwrap_or(0);
            if non_resident == 0 {
                let value_offset = le_u16(record, cursor + 20) as usize;
                let value_length = le_u32(record, cursor + 16) as usize;
                let start = cursor + value_offset;
                let end = (start + value_length).min(record.len());
                return Ok(Some(record[start..end].to_vec()));
            }
            let real_size = if attribute_length >= 56 {
                le_u64(record, cursor + 48) as usize
            } else {
                usize::MAX
            };
            let run_offset = le_u16(record, cursor + 32) as usize;
            if run_offset >= attribute_length {
                return Ok(None);
            }
            let runs = &record[cursor + run_offset..cursor + attribute_length];
            let extents = decode_data_runs(runs, geometry.bytes_per_cluster)?;
            let mut value = Vec::new();
            let mut buffer = vec![0u8; VOLUME_READ_CHUNK];
            for &(start, length) in &extents {
                let mut offset = 0u64;
                while offset < length {
                    let to_read = ((length - offset) as usize).min(buffer.len());
                    volume.seek(SeekFrom::Start(start + offset))?;
                    volume.read_exact(&mut buffer[..to_read])?;
                    value.extend_from_slice(&buffer[..to_read]);
                    offset += to_read as u64;
                }
            }
            value.truncate(real_size.min(value.len()));
            return Ok(Some(value));
        }
        cursor += attribute_length;
    }
    Ok(None)
}

/// Scans an `$ATTRIBUTE_LIST` value for the MFT record number that holds the
/// `$BITMAP` attribute (its first fragment).
fn attribute_list_bitmap_record(list: &[u8]) -> Option<u64> {
    let mut pos = 0usize;
    while pos + 0x18 <= list.len() {
        let entry_type = le_u32(list, pos);
        let entry_len = le_u16(list, pos + 4) as usize;
        if entry_len < 0x18 || pos + entry_len > list.len() {
            break;
        }
        let starting_vcn = le_u64(list, pos + 8);
        if entry_type == ATTRIBUTE_BITMAP && starting_vcn == 0 {
            let reference = le_u64(list, pos + 16);
            return Some(reference & 0x0000_FFFF_FFFF_FFFF);
        }
        pos += entry_len;
    }
    None
}

/// Reads the `$MFT`'s own `$BITMAP` attribute — one bit per MFT record, set when
/// that record is allocated (in use). Used to skip reading long runs of free
/// records. Returns the raw bitmap bytes (record N -> byte N/8, bit N%8). On a
/// large MFT the `$BITMAP` is relocated to an extension record via the base
/// record's `$ATTRIBUTE_LIST`, which is followed here.
fn read_mft_bitmap(
    volume: &mut File,
    geometry: &MftGeometry,
    data_extents: &[(u64, u64)],
) -> io::Result<Vec<u8>> {
    let record0 = read_mft_record_at(volume, geometry, geometry.mft_start_offset)?;
    if let Some(bitmap) = extract_bitmap_from_record(&record0, volume, geometry)? {
        return Ok(bitmap);
    }
    if let Some(list) = read_attribute_list_value(&record0, volume, geometry)? {
        if let Some(number) = attribute_list_bitmap_record(&list) {
            if let Some(physical) = mft_record_physical_offset(data_extents, number) {
                let extension = read_mft_record_at(volume, geometry, physical)?;
                if let Some(bitmap) = extract_bitmap_from_record(&extension, volume, geometry)? {
                    return Ok(bitmap);
                }
            }
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "MFT record 0 has no $BITMAP attribute",
    ))
}

/// Byte length required to skip a free run before it is worth a seek. Runs
/// shorter than this are read through (a seek costs more than the bytes).
const MFT_SKIP_MIN_BYTES: u64 = 64 * 1024;

/// Filters the MFT `$DATA` extents down to only the regions that hold at least
/// one in-use record, skipping free runs of `>= MFT_SKIP_MIN_BYTES`. Works at
/// cluster granularity so a raw read stays sector/cluster aligned and never
/// splits a record (requires `bytes_per_cluster` to be a whole number of
/// 1024-byte records; the caller falls back to a full read otherwise). Coalesces
/// short free gaps so the read stays mostly sequential.
fn mft_keep_extents(
    data_extents: &[(u64, u64)],
    bitmap: &[u8],
    bytes_per_cluster: u64,
) -> Vec<(u64, u64)> {
    let records_per_cluster = (bytes_per_cluster / NTFS_RECORD_SIZE as u64) as usize;
    if records_per_cluster == 0 {
        return data_extents.to_vec();
    }
    let total_clusters: u64 = data_extents.iter().map(|e| e.1 / bytes_per_cluster).sum();
    if total_clusters == 0 {
        return data_extents.to_vec();
    }

    // A cluster is occupied if any of its records is marked in use.
    let cluster_occupied = |cluster: u64| -> bool {
        let first_record = cluster * records_per_cluster as u64;
        (0..records_per_cluster as u64).any(|offset| {
            let record = first_record + offset;
            let byte = (record / 8) as usize;
            bitmap
                .get(byte)
                .is_some_and(|b| b & (1u8 << (record % 8)) != 0)
        })
    };

    // keep[c]: read cluster c? Occupied clusters, plus free clusters in runs
    // shorter than the skip threshold (read through to stay sequential).
    let skip_min = MFT_SKIP_MIN_BYTES.div_ceil(bytes_per_cluster).max(1);
    let total = total_clusters as usize;
    let mut keep = vec![false; total];
    let mut c = 0usize;
    while c < total {
        if cluster_occupied(c as u64) {
            keep[c] = true;
            c += 1;
            continue;
        }
        let run_start = c;
        while c < total && !cluster_occupied(c as u64) {
            c += 1;
        }
        let run_len = (c - run_start) as u64;
        if run_len < skip_min {
            for slot in keep.iter_mut().take(c).skip(run_start) {
                *slot = true;
            }
        }
    }

    // Map kept logical clusters back to physical byte extents, merging
    // physically-contiguous kept clusters within each source extent.
    let mut result: Vec<(u64, u64)> = Vec::new();
    let mut logical_cluster = 0u64;
    for &(phys_start, length) in data_extents {
        let clusters_here = length / bytes_per_cluster;
        for local in 0..clusters_here {
            let keep_it = keep.get(logical_cluster as usize).copied().unwrap_or(true);
            if keep_it {
                let phys = phys_start + local * bytes_per_cluster;
                match result.last_mut() {
                    Some(last) if last.0 + last.1 == phys => last.1 += bytes_per_cluster,
                    _ => result.push((phys, bytes_per_cluster)),
                }
            }
            logical_cluster += 1;
        }
        // Preserve any sub-cluster tail (valid-length trim can leave one).
        let tail = length % bytes_per_cluster;
        if tail != 0 {
            let phys = phys_start + clusters_here * bytes_per_cluster;
            match result.last_mut() {
                Some(last) if last.0 + last.1 == phys => last.1 += tail,
                _ => result.push((phys, tail)),
            }
        }
    }
    result
}

/// Applies the NTFS Update Sequence Array fixups to one file record: the
/// last two bytes of every sector are replaced on disk by the update
/// sequence number and must be restored from the USA before parsing.
/// Returns false when the check bytes don't match (torn/invalid record).
fn apply_record_fixups(record: &mut [u8], bytes_per_sector: usize) -> bool {
    if record.len() < 8 || bytes_per_sector < 2 {
        return false;
    }
    let usa_offset = u16::from_le_bytes([record[4], record[5]]) as usize;
    let usa_count = u16::from_le_bytes([record[6], record[7]]) as usize;
    if usa_count < 2 || usa_offset + usa_count * 2 > record.len() {
        return false;
    }
    let usn = [record[usa_offset], record[usa_offset + 1]];
    for index in 1..usa_count {
        let sector_end = index * bytes_per_sector;
        if sector_end > record.len() {
            return false;
        }
        if record[sector_end - 2..sector_end] != usn {
            return false;
        }
        let fix = [
            record[usa_offset + index * 2],
            record[usa_offset + index * 2 + 1],
        ];
        record[sector_end - 2] = fix[0];
        record[sector_end - 1] = fix[1];
    }
    true
}

/// Applies fixups across a buffer of whole file records. Records whose
/// check bytes don't match get their signature cleared so parsing skips
/// them instead of reading torn data.
pub(crate) fn apply_mft_fixups(batch: &mut [u8], bytes_per_sector: usize) {
    let mut offset = 0usize;
    while offset + NTFS_RECORD_SIZE <= batch.len() {
        let record = &mut batch[offset..offset + NTFS_RECORD_SIZE];
        if &record[0..4] == FILE_RECORD_SIGNATURE && !apply_record_fixups(record, bytes_per_sector)
        {
            record[0..4].copy_from_slice(b"BAAD");
        }
        offset += NTFS_RECORD_SIZE;
    }
}

/// Phase-decomposition profiler for the raw-MFT scan: times each layer of
/// the pipeline in isolation on the live volume so optimization work chases
/// the real bottleneck. Dev tooling only (mft_phase_bench example).
#[doc(hidden)]
pub fn profile_mft_phases(
    root: &Path,
    worker_count: usize,
) -> Result<String, NtfsEnumerationError> {
    use std::time::Instant;
    let workers = worker_count.max(1);
    let buffer_len = NTFS_RECORD_SIZE * 65_536;

    // Phase 0: direct volume reads of the MFT extents, no chunk layer -
    // the floor set by the device/page cache.
    let mut volume = open_volume_handle(root).map_err(NtfsEnumerationError::Io)?;
    let geometry = read_volume_geometry(&mut volume).map_err(NtfsEnumerationError::Io)?;
    let extents = read_mft_extents(&mut volume, &geometry).map_err(NtfsEnumerationError::Io)?;
    let mut buffer = vec![0u8; buffer_len];
    let started = Instant::now();
    let mut direct_bytes = 0u64;
    for &(start, length) in &extents {
        let mut offset = 0u64;
        while offset < length {
            let to_read = ((length - offset) as usize).min(buffer_len);
            volume
                .seek(SeekFrom::Start(start + offset))
                .map_err(NtfsEnumerationError::Io)?;
            volume
                .read_exact(&mut buffer[..to_read])
                .map_err(NtfsEnumerationError::Io)?;
            offset += to_read as u64;
            direct_bytes += to_read as u64;
        }
    }
    let direct_read_ms = started.elapsed().as_millis();
    drop(volume);

    // Phase 0b: the same direct reads split across 4 reader threads, each
    // with its own volume handle - measures how much NVMe queue depth buys.
    let started = Instant::now();
    let read_threads = 4usize;
    let total: u64 = extents.iter().map(|extent| extent.1).sum();
    // Share aligned to 1 MiB so every partition boundary stays a sector
    // (and cluster) multiple - raw volume reads require aligned offsets.
    let share = ((total / read_threads as u64) >> 20).max(1) << 20;
    let mut partitions: Vec<Vec<(u64, u64)>> = vec![Vec::new(); read_threads];
    {
        let mut cursor = 0u64;
        for &(start, length) in &extents {
            let mut offset = 0u64;
            while offset < length {
                let slot = ((cursor / share) as usize).min(read_threads - 1);
                let slot_end = if slot + 1 == read_threads {
                    u64::MAX
                } else {
                    (slot as u64 + 1) * share
                };
                let take = (length - offset).min(slot_end.saturating_sub(cursor));
                partitions[slot].push((start + offset, take));
                offset += take;
                cursor += take;
            }
        }
    }
    let parallel_bytes: u64 = thread::scope(|scope| {
        let mut handles = Vec::new();
        for partition in &partitions {
            let root = root.to_path_buf();
            handles.push(scope.spawn(move || -> io::Result<u64> {
                let mut volume = open_volume_handle(&root)?;
                let mut buffer = vec![0u8; buffer_len];
                let mut bytes = 0u64;
                for &(start, length) in partition {
                    let mut offset = 0u64;
                    while offset < length {
                        let to_read = ((length - offset) as usize).min(buffer_len);
                        volume.seek(SeekFrom::Start(start + offset))?;
                        volume.read_exact(&mut buffer[..to_read])?;
                        offset += to_read as u64;
                        bytes += to_read as u64;
                    }
                }
                Ok(bytes)
            }));
        }
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap_or(Ok(0)).unwrap_or(0))
            .sum()
    });
    let parallel_read_ms = started.elapsed().as_millis();

    // Phase 1: reads through the production VolumeMftReader chunk layer.
    let mut stream = open_mft_stream(root)?;
    let bytes_per_sector = stream.bytes_per_sector as usize;
    let started = Instant::now();
    let mut stream_bytes = 0u64;
    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        stream_bytes += read as u64;
    }
    let stream_read_ms = started.elapsed().as_millis();

    // Phase 2: + sector fixups.
    let mut stream = open_mft_stream(root)?;
    let started = Instant::now();
    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let full_bytes = (read / NTFS_RECORD_SIZE) * NTFS_RECORD_SIZE;
        apply_mft_fixups(&mut buffer[..full_bytes], bytes_per_sector);
    }
    let fixup_ms = started.elapsed().as_millis();

    // Phase 3: + parallel record parse, results dropped.
    let mut stream = open_mft_stream(root)?;
    let started = Instant::now();
    let mut parsed_entries = 0u64;
    loop {
        let read = stream.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        let full_records = read / NTFS_RECORD_SIZE;
        let full_bytes = full_records * NTFS_RECORD_SIZE;
        apply_mft_fixups(&mut buffer[..full_bytes], bytes_per_sector);
        let (parsed, _extensions) = parse_batch(&buffer[..full_bytes], full_records, workers)?;
        parsed_entries += parsed.len() as u64;
    }
    let parse_ms = started.elapsed().as_millis();

    // Phase 4: the full streaming pipeline with a null event sink.
    let started = Instant::now();
    let summary = enumerate_ntfs_volume_parallel_streaming_summary(root, workers, |_| {})?;
    let full_ms = started.elapsed().as_millis();

    Ok(format!(
        "{{\"direct_read_ms\":{direct_read_ms},\"parallel_read_ms\":{parallel_read_ms},\"parallel_bytes\":{parallel_bytes},\"stream_read_ms\":{stream_read_ms},\"fixup_ms\":{fixup_ms},\"parse_ms\":{parse_ms},\"full_ms\":{full_ms},\"mft_bytes\":{direct_bytes},\"stream_bytes\":{stream_bytes},\"parsed_entries\":{parsed_entries},\"files\":{},\"directories\":{}}}",
        summary.files_seen, summary.directories_seen
    ))
}

/// Opens the MFT byte stream: raw volume access first (the only method that
/// works on modern Windows; requires Administrator), then the legacy
/// metadata-file open as a fallback.
fn open_mft_stream(root: &Path) -> Result<MftStream, NtfsEnumerationError> {
    let volume_error = match open_volume_handle(root) {
        Ok(mut volume) => match read_volume_geometry(&mut volume) {
            Ok(geometry) => {
                if geometry.record_size as usize != NTFS_RECORD_SIZE {
                    io::Error::new(
                        io::ErrorKind::Unsupported,
                        format!(
                            "unsupported MFT record size {} (expected {NTFS_RECORD_SIZE})",
                            geometry.record_size
                        ),
                    )
                } else {
                    match read_mft_extents(&mut volume, &geometry) {
                        Ok(mut extents) => {
                            // Sparse read: skip long runs of free MFT records
                            // (~44% of the MFT is unused on a live volume, ~34%
                            // of it in runs long enough to seek past). Only when
                            // clusters hold whole 1024-byte records so a
                            // cluster-aligned cut never splits one; any bitmap
                            // read failure falls back to the full extents. Set
                            // WINBLAZE_NO_SPARSE_MFT to force a full read.
                            let bpc = geometry.bytes_per_cluster;
                            if std::env::var_os("WINBLAZE_NO_SPARSE_MFT").is_none()
                                && bpc >= NTFS_RECORD_SIZE as u64
                                && bpc % NTFS_RECORD_SIZE as u64 == 0
                            {
                                if let Ok(bitmap) =
                                    read_mft_bitmap(&mut volume, &geometry, &extents)
                                {
                                    let kept = mft_keep_extents(&extents, &bitmap, bpc);
                                    if !kept.is_empty() {
                                        extents = kept;
                                    }
                                }
                            }
                            let reader = VolumeMftReader {
                                volume,
                                extents,
                                extent_index: 0,
                                consumed_in_extent: 0,
                                bytes_per_sector: geometry.bytes_per_sector as usize,
                                chunk: Vec::new(),
                                chunk_pos: 0,
                                chunk_len: 0,
                            };
                            let total_bytes = reader.total_bytes();
                            return Ok(MftStream {
                                bytes_per_sector: geometry.bytes_per_sector,
                                total_bytes,
                                reader: Box::new(reader),
                            });
                        }
                        Err(error) => error,
                    }
                }
            }
            Err(error) => error,
        },
        Err(error) => error,
    };

    match open_ntfs_metadata_file(root, "$MFT") {
        Ok(file) => {
            let total_bytes = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
            Ok(MftStream {
                bytes_per_sector: 512,
                total_bytes,
                reader: Box::new(file),
            })
        }
        Err(_) => Err(NtfsEnumerationError::Io(volume_error)),
    }
}

fn open_ntfs_metadata_file(root: &Path, file_name: &str) -> Result<File, NtfsEnumerationError> {
    let path = root.join(file_name);
    match open_file_with_backup_privilege(&path) {
        Ok(file) => Ok(file),
        Err(first_error) => {
            if enable_backup_privilege().is_ok() {
                open_file_with_backup_privilege(&path)
                    .map_err(|_| NtfsEnumerationError::Io(first_error))
            } else {
                Err(NtfsEnumerationError::Io(first_error))
            }
        }
    }
}

fn open_file_with_backup_privilege(path: &Path) -> io::Result<File> {
    let wide_path: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();

    let handle = unsafe {
        CreateFileW(
            wide_path.as_ptr(),
            GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            null_mut(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_SEQUENTIAL_SCAN,
            null_mut(),
        )
    };

    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }

    let raw_handle = handle as RawHandle;
    Ok(unsafe { File::from_raw_handle(raw_handle) })
}

fn enable_backup_privilege() -> io::Result<()> {
    let mut token: Handle = null_mut();
    let opened = unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )
    };
    if opened == 0 {
        return Err(io::Error::last_os_error());
    }

    let privilege_name: Vec<u16> = "SeBackupPrivilege".encode_utf16().chain(Some(0)).collect();
    let mut luid = Luid::default();
    let looked_up = unsafe { LookupPrivilegeValueW(null(), privilege_name.as_ptr(), &mut luid) };
    if looked_up == 0 {
        unsafe {
            CloseHandle(token);
        }
        return Err(io::Error::last_os_error());
    }

    let mut privileges = TokenPrivileges {
        PrivilegeCount: 1,
        Privileges: [LuidAndAttributes {
            Luid: luid,
            Attributes: SE_PRIVILEGE_ENABLED,
        }],
    };

    let adjusted = unsafe {
        AdjustTokenPrivileges(
            token,
            0,
            &mut privileges,
            std::mem::size_of::<TokenPrivileges>() as u32,
            null_mut(),
            null_mut(),
        )
    };

    let last_error = io::Error::last_os_error();
    unsafe {
        CloseHandle(token);
    }

    if adjusted == 0 {
        Err(last_error)
    } else {
        Ok(())
    }
}

pub fn parse_mft_records(
    root: &Path,
    bytes: &[u8],
) -> Result<NtfsEnumeration, NtfsEnumerationError> {
    parse_mft_records_sequential(root, bytes)
}

pub fn parse_mft_records_parallel(
    root: &Path,
    bytes: &[u8],
    worker_count: usize,
) -> Result<NtfsEnumeration, NtfsEnumerationError> {
    parse_mft_records_parallel_streaming(root, bytes, worker_count, |_| {})
}

pub fn parse_mft_records_parallel_streaming<F>(
    root: &Path,
    bytes: &[u8],
    worker_count: usize,
    on_event: F,
) -> Result<NtfsEnumeration, NtfsEnumerationError>
where
    F: FnMut(ScanEvent),
{
    parse_mft_records_parallel_streaming_impl(root, bytes, worker_count, on_event)
}

fn parse_mft_records_parallel_streaming_impl<F>(
    root: &Path,
    bytes: &[u8],
    worker_count: usize,
    mut on_event: F,
) -> Result<NtfsEnumeration, NtfsEnumerationError>
where
    F: FnMut(ScanEvent),
{
    if worker_count <= 1 || bytes.len() < NTFS_RECORD_SIZE * 2 {
        return parse_mft_records_sequential_streaming(root, bytes, Some(&mut on_event));
    }

    let record_count = bytes.len() / NTFS_RECORD_SIZE;
    if record_count <= 1 {
        return parse_mft_records_sequential_streaming(root, bytes, Some(&mut on_event));
    }

    let workers = worker_count.min(record_count).max(1);
    if workers <= 1 {
        return parse_mft_records_sequential_streaming(root, bytes, Some(&mut on_event));
    }

    let chunk_size = record_count.div_ceil(workers);

    let entries = thread::scope(|s| {
        let mut handles = Vec::with_capacity(workers);
        for worker_index in 0..workers {
            let start_record = worker_index * chunk_size;
            if start_record >= record_count {
                break;
            }
            let end_record = ((worker_index + 1) * chunk_size).min(record_count);
            handles.push(s.spawn(move || parse_record_range(bytes, start_record, end_record)));
        }

        let mut entries: IdHashMap<u64, ParsedNtfsEntry> = IdHashMap::default();
        let mut extensions: Vec<ExtensionSizes> = Vec::new();
        for handle in handles {
            match handle.join() {
                Ok(Ok((chunk_entries, chunk_extensions))) => {
                    for entry in chunk_entries {
                        entries.insert(entry.file_id.0, entry);
                    }
                    extensions.extend(chunk_extensions);
                }
                Ok(Err(error)) => return Err(error),
                Err(_) => {
                    return Err(NtfsEnumerationError::InvalidRecord(String::from(
                        "parallel parser worker panicked",
                    )))
                }
            }
        }
        apply_extension_sizes(&mut entries, extensions);
        Ok(entries)
    })?;

    parse_entries(root, entries, Some(&mut on_event))
}

fn parse_mft_records_sequential(
    root: &Path,
    bytes: &[u8],
) -> Result<NtfsEnumeration, NtfsEnumerationError> {
    parse_mft_records_sequential_streaming(root, bytes, None)
}

fn parse_mft_records_sequential_streaming(
    root: &Path,
    bytes: &[u8],
    on_event: Option<&mut dyn FnMut(ScanEvent)>,
) -> Result<NtfsEnumeration, NtfsEnumerationError> {
    if bytes.len() < NTFS_RECORD_SIZE {
        return Err(NtfsEnumerationError::InvalidRecord(String::from(
            "MFT buffer too small",
        )));
    }

    let mut entries: IdHashMap<u64, ParsedNtfsEntry> = IdHashMap::default();
    let mut extensions: Vec<ExtensionSizes> = Vec::new();

    for record in bytes.chunks(NTFS_RECORD_SIZE) {
        if record.len() < NTFS_RECORD_SIZE || &record[0..4] != FILE_RECORD_SIGNATURE {
            continue;
        }

        match parse_record(record)? {
            ParsedRecordOutcome::Entry(entry) => {
                entries.insert(entry.file_id.0, entry);
            }
            ParsedRecordOutcome::Extension(extension) => extensions.push(extension),
            ParsedRecordOutcome::None => {}
        }
    }

    apply_extension_sizes(&mut entries, extensions);
    parse_entries(root, entries, on_event)
}

/// Folds sizes carried by extension records into their base entries.
fn apply_extension_sizes(
    entries: &mut IdHashMap<u64, ParsedNtfsEntry>,
    extensions: Vec<ExtensionSizes>,
) {
    for extension in extensions {
        if let Some(entry) = entries.get_mut(&extension.base_record) {
            entry.size_bytes = entry.size_bytes.max(extension.size_bytes);
            entry.allocation_bytes = entry.allocation_bytes.max(extension.allocation_bytes);
        }
    }
}

fn parse_record_range(
    shared: &[u8],
    start_record: usize,
    end_record: usize,
) -> Result<(Vec<ParsedNtfsEntry>, Vec<ExtensionSizes>), NtfsEnumerationError> {
    let mut entries = Vec::new();
    let mut extensions = Vec::new();
    for index in start_record..end_record {
        let start = index * NTFS_RECORD_SIZE;
        let end = start + NTFS_RECORD_SIZE;
        let record = &shared[start..end];
        if &record[0..4] != FILE_RECORD_SIGNATURE {
            continue;
        }

        match parse_record(record)? {
            ParsedRecordOutcome::Entry(entry) => entries.push(entry),
            ParsedRecordOutcome::Extension(extension) => extensions.push(extension),
            ParsedRecordOutcome::None => {}
        }
    }

    Ok((entries, extensions))
}

fn parse_entries(
    root: &Path,
    mut entries: IdHashMap<u64, ParsedNtfsEntry>,
    mut on_event: Option<&mut dyn FnMut(ScanEvent)>,
) -> Result<NtfsEnumeration, NtfsEnumerationError> {
    let root_text = root.display().to_string();

    entries.entry(5).or_insert_with(|| ParsedNtfsEntry {
        file_id: FileId(5),
        parent_directory_id: None,
        name: String::from(""),
        is_directory: true,
        has_attribute_list: false,
        size_bytes: 0,
        allocation_bytes: 0,
        attributes: FileAttributes::DIRECTORY,
        created_utc: None,
        modified_utc: None,
        accessed_utc: None,
    });

    let mut resolved_paths: IdHashMap<u64, String> = IdHashMap::default();
    resolved_paths.insert(5, root_text.clone());

    let mut directories = Vec::new();
    let mut files = Vec::new();

    let mut ordered_entries: Vec<_> = entries.values().collect();
    ordered_entries.sort_by_key(|entry| entry.file_id.0);

    if let Some(callback) = on_event.as_mut() {
        (*callback)(ScanEvent::VolumeDiscovered(VolumeRecord {
            id: VolumeId(0),
            mount_point: root_text.clone(),
            label: None,
            file_system: FileSystemKind::Ntfs,
            total_bytes: 0,
            free_bytes: 0,
            root_directory_id: DirectoryId(5),
        }));
    }

    for entry in ordered_entries {
        if entry.is_directory {
            // Only directories carry a materialized path; file paths derive
            // from their parent on demand (see FileRecord docs).
            let full_path = resolve_entry_path(
                entry.file_id.0,
                &entries,
                &mut resolved_paths,
                &root_text,
                &mut Vec::new(),
            );
            let directory = DirectoryRecord {
                id: DirectoryId(entry.file_id.0),
                parent_directory_id: if entry.file_id.0 == 5 {
                    None
                } else {
                    entry.parent_directory_id.map(DirectoryId)
                },
                name: entry.name.clone(),
                full_path,
                direct_bytes: 0,
                total_bytes: 0,
                direct_entries: 0,
                total_entries: 0,
            };
            if let Some(callback) = on_event.as_mut() {
                (*callback)(ScanEvent::DirectoryFound(directory.clone()));
            }
            directories.push(directory);
        } else {
            let file = FileRecord {
                id: FileId(entry.file_id.0),
                parent_directory_id: DirectoryId(entry.parent_directory_id.unwrap_or(5)),
                name: entry.name.clone(),
                full_path: String::new(),
                size_bytes: entry.size_bytes,
                allocation_bytes: entry.allocation_bytes,
                attributes: entry.attributes,
                created_utc: entry.created_utc,
                modified_utc: entry.modified_utc,
                accessed_utc: entry.accessed_utc,
            };
            if let Some(callback) = on_event.as_mut() {
                (*callback)(ScanEvent::FileFound(file.clone()));
            }
            files.push(file);
        }
    }

    directories.sort_by_key(|directory| directory.id.0);
    files.sort_by_key(|file| file.id.0);
    let directories = aggregate_directory_records(&directories, &files);

    let total_size_bytes = sum_u64_saturating(files.iter().map(|file| file.size_bytes));
    let total_allocation_bytes = sum_u64_saturating(files.iter().map(|file| file.allocation_bytes));

    Ok(NtfsEnumeration {
        volume: VolumeRecord {
            id: VolumeId(0),
            mount_point: root_text,
            label: None,
            file_system: FileSystemKind::Ntfs,
            total_bytes: 0,
            free_bytes: 0,
            root_directory_id: DirectoryId(5),
        },
        directories,
        files,
        summary: ScanSummary {
            files_seen: total_file_count(&entries),
            directories_seen: total_directory_count(&entries),
            total_size_bytes,
            total_allocation_bytes,
        },
    })
}

#[derive(Clone, Debug)]
struct ParsedNtfsEntry {
    file_id: FileId,
    parent_directory_id: Option<u64>,
    name: String,
    is_directory: bool,
    has_attribute_list: bool,
    size_bytes: u64,
    allocation_bytes: u64,
    attributes: FileAttributes,
    created_utc: Option<i64>,
    modified_utc: Option<i64>,
    accessed_utc: Option<i64>,
}

/// Sizes found in an extension record (a record whose base reference points
/// at another record): heavily attributed files move their unnamed $DATA out
/// of the base record via $ATTRIBUTE_LIST, so the authoritative sizes live
/// here and must be merged into the base entry.
#[derive(Clone, Copy, Debug)]
struct ExtensionSizes {
    base_record: u64,
    size_bytes: u64,
    allocation_bytes: u64,
}

enum ParsedRecordOutcome {
    None,
    Entry(ParsedNtfsEntry),
    Extension(ExtensionSizes),
}

fn parse_record(record: &[u8]) -> Result<ParsedRecordOutcome, NtfsEnumerationError> {
    if record.len() < NTFS_RECORD_SIZE || &record[0..4] != FILE_RECORD_SIGNATURE {
        return Ok(ParsedRecordOutcome::None);
    }

    // These offsets are fixed header fields inside a full 1024-byte record
    // (guaranteed by the length check above), so they read unchecked.
    let file_index = le_u32(record, 0x2C) as u64;
    let flags = le_u16(record, 0x16);
    // The MFT retains records of deleted files for slot reuse; they still
    // carry the FILE signature but must not be counted.
    if flags & FILE_RECORD_FLAG_IN_USE == 0 {
        return Ok(ParsedRecordOutcome::None);
    }
    let base_record = le_u64(record, 0x20) & 0x0000_FFFF_FFFF_FFFF;
    let is_directory = flags & FILE_RECORD_FLAG_DIRECTORY != 0;
    let first_attr_offset = le_u16(record, 0x14) as usize;
    let mut cursor = first_attr_offset;
    let mut file_name: Option<FileNameAttribute> = None;
    let mut allocation_bytes = 0u64;
    let mut size_bytes = 0u64;
    let mut has_attribute_list = false;

    while cursor + 8 <= record.len() {
        let attribute_type = read_u32(record, cursor)?;
        if attribute_type == u32::MAX {
            break;
        }

        let attribute_length = read_u32(record, cursor + 4)? as usize;
        if attribute_length == 0 || cursor + attribute_length > record.len() {
            break;
        }

        let non_resident = record[cursor + 8];
        // Named $DATA streams (alternate data streams, and metadata streams
        // like $BadClus:$Bad whose sparse size spans the whole volume) must
        // not count toward file size; only the unnamed default stream does.
        let attribute_name_length = record[cursor + 9];
        match attribute_type {
            // $ATTRIBUTE_LIST precedes $FILE_NAME in a record (attributes are
            // type-ordered), so the early break below cannot miss it.
            ATTRIBUTE_LIST => has_attribute_list = true,
            ATTRIBUTE_FILE_NAME if non_resident == 0 => {
                let value_offset = read_u16(record, cursor + 20)? as usize;
                let value_length = read_u32(record, cursor + 16)? as usize;
                if cursor + value_offset + value_length <= record.len() {
                    match parse_file_name(
                        &record[cursor + value_offset..cursor + value_offset + value_length],
                    ) {
                        Ok(name) => {
                            // Records carry one $FILE_NAME per namespace;
                            // last-parsed used to win, surfacing DOS 8.3
                            // names like PROGRA~1 when that one sat last.
                            let replace = file_name.as_ref().is_none_or(|current| {
                                name.namespace_rank() >= current.namespace_rank()
                            });
                            if replace {
                                file_name = Some(name);
                            }
                        }
                        Err(_) => return Ok(ParsedRecordOutcome::None),
                    }
                }
            }
            ATTRIBUTE_DATA if non_resident == 0 && attribute_name_length == 0 => {
                size_bytes = read_u32(record, cursor + 16)? as u64;
                allocation_bytes = size_bytes;
            }
            ATTRIBUTE_DATA if non_resident != 0 && attribute_name_length == 0 => {
                // Allocated/real sizes are only valid on the fragment whose
                // starting VCN is zero. (The starting VCN is read first rather
                // than folded into the match guard because guards cannot use
                // `?`, and the checked read must still skip malformed records.)
                let starting_vcn = read_u64(record, cursor + 16)?;
                if starting_vcn == 0 {
                    allocation_bytes = read_u64(record, cursor + 40)?;
                    size_bytes = read_u64(record, cursor + 48)?;
                }
            }
            _ => {}
        }

        // Only stop early once a Win32-namespace name is in hand: the DOS
        // 8.3 alias sorts before it, so breaking on the first $FILE_NAME
        // surfaced names like PROGRA~1 for every directory.
        if base_record == 0
            && file_name
                .as_ref()
                .is_some_and(|name| name.namespace_rank() == 2)
            && (is_directory || size_bytes != 0 || allocation_bytes != 0)
        {
            break;
        }

        cursor += attribute_length;
    }

    if base_record != 0 {
        // Extension record: carries attributes for `base_record`, never an
        // entry of its own.
        if size_bytes != 0 || allocation_bytes != 0 {
            return Ok(ParsedRecordOutcome::Extension(ExtensionSizes {
                base_record,
                size_bytes,
                allocation_bytes,
            }));
        }
        return Ok(ParsedRecordOutcome::None);
    }

    let file_name = match file_name {
        Some(value) => value,
        None => return Ok(ParsedRecordOutcome::None),
    };

    let attributes = if is_directory {
        FileAttributes::DIRECTORY
    } else {
        FileAttributes::ARCHIVE
    };

    Ok(ParsedRecordOutcome::Entry(ParsedNtfsEntry {
        file_id: FileId(file_index),
        parent_directory_id: file_name.parent_reference,
        name: file_name.name,
        is_directory,
        has_attribute_list,
        size_bytes,
        allocation_bytes,
        attributes,
        created_utc: Some(file_name.created_utc),
        modified_utc: Some(file_name.modified_utc),
        accessed_utc: Some(file_name.accessed_utc),
    }))
}

#[derive(Clone, Debug)]
struct FileNameAttribute {
    parent_reference: Option<u64>,
    created_utc: i64,
    modified_utc: i64,
    accessed_utc: i64,
    namespace: u8,
    name: String,
}

impl FileNameAttribute {
    /// Preference order for picking among a record's $FILE_NAME attributes:
    /// Win32 (1) and Win32&DOS (3) carry the long name, POSIX (0) is rare
    /// but real, and DOS (2) is the 8.3 alias nobody wants to see.
    fn namespace_rank(&self) -> u8 {
        match self.namespace {
            1 | 3 => 2,
            0 => 1,
            _ => 0,
        }
    }
}

fn parse_file_name(bytes: &[u8]) -> Result<FileNameAttribute, NtfsEnumerationError> {
    if bytes.len() < 66 {
        return Err(NtfsEnumerationError::InvalidRecord(String::from(
            "file name attribute too small",
        )));
    }

    // Fixed fields, all within the validated 66-byte minimum: read unchecked.
    let parent_reference = Some(le_u64(bytes, 0) & 0x0000_FFFF_FFFF_FFFF);
    let created_utc = le_i64(bytes, 8);
    let modified_utc = le_i64(bytes, 16);
    let accessed_utc = le_i64(bytes, 24);
    let name_length = bytes[64] as usize;
    let namespace = bytes[65];
    let name_offset = 66usize;
    let name_bytes = name_length.saturating_mul(2);
    if name_offset + name_bytes > bytes.len() {
        return Err(NtfsEnumerationError::InvalidRecord(String::from(
            "file name attribute truncated",
        )));
    }

    // Decode UTF-16 straight into the output String instead of buffering a
    // Vec<u16> first: one allocation per name rather than two, across ~2.85M
    // records.
    let units = bytes[name_offset..name_offset + name_bytes]
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]));
    let mut name = String::with_capacity(name_length);
    for unit in char::decode_utf16(units) {
        match unit {
            Ok(ch) => name.push(ch),
            Err(_) => {
                return Err(NtfsEnumerationError::InvalidRecord(String::from(
                    "invalid utf-16 file name",
                )))
            }
        }
    }

    Ok(FileNameAttribute {
        parent_reference,
        created_utc,
        modified_utc,
        accessed_utc,
        namespace,
        name,
    })
}

fn resolve_entry_path(
    file_id: u64,
    entries: &IdHashMap<u64, ParsedNtfsEntry>,
    resolved: &mut IdHashMap<u64, String>,
    root_text: &str,
    stack: &mut Vec<u64>,
) -> String {
    if let Some(path) = resolved.get(&file_id) {
        return path.clone();
    }

    if stack.contains(&file_id) {
        return root_text.to_string();
    }

    stack.push(file_id);
    let entry = match entries.get(&file_id) {
        Some(entry) => entry,
        None => {
            stack.pop();
            return root_text.to_string();
        }
    };

    let path = match entry.parent_directory_id {
        None => root_text.to_string(),
        Some(parent_id) if parent_id == file_id || file_id == 5 => root_text.to_string(),
        Some(parent_id) => {
            if !entries.contains_key(&parent_id) && parent_id != 5 {
                stack.pop();
                return root_text.to_string();
            }
            let parent_path = resolve_entry_path(parent_id, entries, resolved, root_text, stack);
            if entry.name.is_empty() {
                parent_path
            } else {
                join_path(&parent_path, &entry.name)
            }
        }
    };

    resolved.insert(file_id, path.clone());
    stack.pop();
    path
}

fn join_path(base: &str, child: &str) -> String {
    if base.ends_with('\\') {
        format!("{base}{child}")
    } else {
        format!("{base}\\{child}")
    }
}

pub fn measure_metadata_extraction_overhead(
    bytes: &[u8],
) -> Result<MetadataExtractionSample, NtfsEnumerationError> {
    let mut record_count = 0u64;
    let mut bytes_scanned = 0u64;

    for record in bytes.chunks(NTFS_RECORD_SIZE) {
        if record.len() < NTFS_RECORD_SIZE || &record[0..4] != FILE_RECORD_SIGNATURE {
            continue;
        }

        bytes_scanned = bytes_scanned.saturating_add(record.len() as u64);
        if matches!(parse_record(record)?, ParsedRecordOutcome::Entry(_)) {
            record_count = record_count.saturating_add(1);
        }
    }

    Ok(MetadataExtractionSample {
        records_scanned: record_count,
        bytes_scanned,
        average_bytes_per_record: bytes_scanned.checked_div(record_count).unwrap_or(0),
    })
}

fn total_file_count(entries: &IdHashMap<u64, ParsedNtfsEntry>) -> u64 {
    entries.values().filter(|entry| !entry.is_directory).count() as u64
}

fn total_directory_count(entries: &IdHashMap<u64, ParsedNtfsEntry>) -> u64 {
    entries.values().filter(|entry| entry.is_directory).count() as u64
}

fn sum_u64_saturating(values: impl Iterator<Item = u64>) -> u64 {
    values.fold(0, u64::saturating_add)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MetadataExtractionSample {
    pub records_scanned: u64,
    pub bytes_scanned: u64,
    pub average_bytes_per_record: u64,
}

// Unchecked little-endian reads for offsets the caller has already proven are
// in bounds (fixed header fields within a full 1024-byte record, or fixed
// fields past a validated minimum length). These skip the per-call bounds
// branch and `Result` plumbing the `read_*` helpers carry for variable
// attribute offsets, where a malformed record must still be skipped rather
// than panic. Called ~2.85M times per full-drive scan.
#[inline]
fn le_u16(bytes: &[u8], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

#[inline]
fn le_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

#[inline]
fn le_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

#[inline]
fn le_i64(bytes: &[u8], offset: usize) -> i64 {
    i64::from_le_bytes(bytes[offset..offset + 8].try_into().unwrap())
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16, NtfsEnumerationError> {
    if offset + 2 > bytes.len() {
        return Err(NtfsEnumerationError::InvalidRecord(String::from(
            "u16 out of bounds",
        )));
    }
    Ok(u16::from_le_bytes([bytes[offset], bytes[offset + 1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, NtfsEnumerationError> {
    if offset + 4 > bytes.len() {
        return Err(NtfsEnumerationError::InvalidRecord(String::from(
            "u32 out of bounds",
        )));
    }
    Ok(u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ]))
}

fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, NtfsEnumerationError> {
    if offset + 8 > bytes.len() {
        return Err(NtfsEnumerationError::InvalidRecord(String::from(
            "u64 out of bounds",
        )));
    }
    Ok(u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ]))
}

type Handle = *mut c_void;

const INVALID_HANDLE_VALUE: Handle = -1isize as Handle;
const GENERIC_READ: u32 = 0x8000_0000;
const FILE_SHARE_READ: u32 = 0x0000_0001;
const FILE_SHARE_WRITE: u32 = 0x0000_0002;
const FILE_SHARE_DELETE: u32 = 0x0000_0004;
const OPEN_EXISTING: u32 = 3;
const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
const FILE_FLAG_SEQUENTIAL_SCAN: u32 = 0x0800_0000;
const TOKEN_QUERY: u32 = 0x0008;
const TOKEN_ADJUST_PRIVILEGES: u32 = 0x0020;
const SE_PRIVILEGE_ENABLED: u32 = 0x0000_0002;

#[repr(C)]
#[allow(non_snake_case)]
#[derive(Clone, Copy, Default)]
struct Luid {
    LowPart: u32,
    HighPart: i32,
}

#[repr(C)]
#[allow(non_snake_case)]
#[derive(Clone, Copy)]
struct LuidAndAttributes {
    Luid: Luid,
    Attributes: u32,
}

#[repr(C)]
#[allow(non_snake_case)]
struct TokenPrivileges {
    PrivilegeCount: u32,
    Privileges: [LuidAndAttributes; 1],
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateFileW(
        lpFileName: *const u16,
        dwDesiredAccess: u32,
        dwShareMode: u32,
        lpSecurityAttributes: *mut c_void,
        dwCreationDisposition: u32,
        dwFlagsAndAttributes: u32,
        hTemplateFile: *mut c_void,
    ) -> Handle;

    fn CloseHandle(hObject: Handle) -> i32;

    fn GetCurrentProcess() -> Handle;
}

#[link(name = "advapi32")]
extern "system" {
    fn OpenProcessToken(ProcessHandle: Handle, DesiredAccess: u32, TokenHandle: *mut Handle)
        -> i32;

    fn LookupPrivilegeValueW(
        lpSystemName: *const u16,
        lpName: *const u16,
        lpLuid: *mut Luid,
    ) -> i32;

    fn AdjustTokenPrivileges(
        TokenHandle: Handle,
        DisableAllPrivileges: i32,
        NewState: *mut TokenPrivileges,
        BufferLength: u32,
        PreviousState: *mut TokenPrivileges,
        ReturnLength: *mut u32,
    ) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mft_keep_extents_skips_long_free_runs_and_keeps_short_gaps() {
        // 4 KiB clusters = 4 records/cluster; skip threshold 64 KiB = 16 clusters.
        let bpc = 4096u64;
        let rpc = 4usize;
        // 40 clusters in one physical extent starting at 1 MiB.
        let extents = vec![(1_048_576u64, 40 * bpc)];
        // Occupy clusters 0-1, then a long free run 2..30 (28 clusters, skip),
        // then a short free gap handled implicitly, then clusters 30-39 occupied.
        let mut bitmap = vec![0u8; (40 * rpc).div_ceil(8)];
        let mut set = |cluster: usize| {
            for r in cluster * rpc..cluster * rpc + rpc {
                bitmap[r / 8] |= 1 << (r % 8);
            }
        };
        set(0);
        set(1);
        for c in 30..40 {
            set(c);
        }
        let kept = mft_keep_extents(&extents, &bitmap, bpc);
        // Expect two extents: clusters 0-1, and clusters 30-39.
        assert_eq!(
            kept,
            vec![(1_048_576, 2 * bpc), (1_048_576 + 30 * bpc, 10 * bpc),]
        );
    }

    #[test]
    fn mft_keep_extents_reads_through_short_free_gaps() {
        let bpc = 4096u64;
        let rpc = 4usize;
        let extents = vec![(0u64, 24 * bpc)];
        // Occupy clusters 0 and 5 (4-cluster gap < 16 -> read through), then a
        // trailing free run of 18 clusters (>= 16 -> skipped).
        let mut bitmap = vec![0u8; (24 * rpc).div_ceil(8)];
        for &c in &[0usize, 5] {
            for r in c * rpc..c * rpc + rpc {
                bitmap[r / 8] |= 1 << (r % 8);
            }
        }
        let kept = mft_keep_extents(&extents, &bitmap, bpc);
        // Clusters 0..=5 kept as one run (gap of 4 < 16); 6..24 skipped.
        assert_eq!(kept, vec![(0, 6 * bpc)]);
    }

    #[test]
    fn attribute_list_bitmap_record_finds_bitmap_reference() {
        // Two entries: $DATA (0x80) in record 0, $BITMAP (0xB0) in record 42.
        let mut list = Vec::new();
        let mut entry = |ty: u32, rec: u64| {
            let mut e = vec![0u8; 0x20];
            e[0..4].copy_from_slice(&ty.to_le_bytes());
            e[4..6].copy_from_slice(&0x20u16.to_le_bytes()); // entry length
                                                             // starting_vcn at 0x08 = 0, base reference at 0x10.
            e[16..24].copy_from_slice(&rec.to_le_bytes());
            list.extend_from_slice(&e);
        };
        entry(ATTRIBUTE_DATA, 0);
        entry(ATTRIBUTE_BITMAP, 42);
        assert_eq!(attribute_list_bitmap_record(&list), Some(42));
    }

    #[test]
    fn decode_data_runs_handles_positive_and_negative_deltas() {
        // Run 1: header 0x21 -> 1-byte length (0x10 clusters), 2-byte offset
        // (LCN 0x4000). Run 2: header 0x11 -> 1-byte length (4 clusters),
        // 1-byte signed offset delta -0x10 (LCN 0x3FF0).
        let runs = [0x21, 0x10, 0x00, 0x40, 0x11, 0x04, 0xF0, 0x00];
        let extents = decode_data_runs(&runs, 4096).expect("decode");
        assert_eq!(
            extents,
            vec![(0x4000 * 4096, 0x10 * 4096), (0x3FF0 * 4096, 0x04 * 4096)]
        );
    }

    #[test]
    fn trim_extents_stops_at_valid_data_length() {
        // Two 16 KiB extents (4 KiB clusters), valid length 20 KiB: keep the
        // first extent whole and trim the second to one cluster.
        let mut extents = vec![(0x1000, 16 * 1024), (0x9000, 16 * 1024)];
        trim_extents_to_valid_length(&mut extents, 20 * 1024, 4096);
        assert_eq!(extents, vec![(0x1000, 16 * 1024), (0x9000, 4 * 1024)]);
    }

    #[test]
    fn trim_extents_noop_when_valid_covers_all_or_unknown() {
        let original = vec![(0x1000, 16 * 1024), (0x9000, 16 * 1024)];

        // Valid length exceeds the allocated total: nothing trimmed.
        let mut extents = original.clone();
        trim_extents_to_valid_length(&mut extents, 1 << 30, 4096);
        assert_eq!(extents, original);

        // Unknown valid length (couldn't read header) leaves extents intact.
        let mut extents = original.clone();
        trim_extents_to_valid_length(&mut extents, u64::MAX, 4096);
        assert_eq!(extents, original);

        // A bogus zero valid length must not trim the MFT to nothing.
        let mut extents = original.clone();
        trim_extents_to_valid_length(&mut extents, 0, 4096);
        assert_eq!(extents, original);
    }

    #[test]
    fn decode_data_runs_rejects_sparse_and_empty() {
        // Sparse run: offset size 0.
        let sparse = [0x01, 0x10, 0x00];
        assert!(decode_data_runs(&sparse, 4096).is_err());
        // Empty run list.
        let empty = [0x00];
        assert!(decode_data_runs(&empty, 4096).is_err());
    }

    #[test]
    fn record_fixups_restore_sector_tails() {
        let mut record = vec![0u8; NTFS_RECORD_SIZE];
        record[0..4].copy_from_slice(FILE_RECORD_SIGNATURE);
        // USA at offset 48: count 3 (usn + two sectors of 512).
        record[4] = 48;
        record[6] = 3;
        // Update sequence number 0xAB 0xCD; fixup values 0x11 0x22 and 0x33 0x44.
        record[48] = 0xAB;
        record[49] = 0xCD;
        record[50] = 0x11;
        record[51] = 0x22;
        record[52] = 0x33;
        record[53] = 0x44;
        // On-disk sector tails carry the usn.
        record[510] = 0xAB;
        record[511] = 0xCD;
        record[1022] = 0xAB;
        record[1023] = 0xCD;

        assert!(apply_record_fixups(&mut record, 512));
        assert_eq!(&record[510..512], &[0x11, 0x22]);
        assert_eq!(&record[1022..1024], &[0x33, 0x44]);

        // Tamper a tail: mismatched check bytes must be rejected and the
        // batch helper must clear the signature.
        let mut torn = vec![0u8; NTFS_RECORD_SIZE];
        torn[0..4].copy_from_slice(FILE_RECORD_SIGNATURE);
        torn[4] = 48;
        torn[6] = 3;
        torn[48] = 0xAB;
        torn[49] = 0xCD;
        torn[510] = 0xFF; // wrong
        torn[511] = 0xCD;
        torn[1022] = 0xAB;
        torn[1023] = 0xCD;
        let mut batch = torn.clone();
        apply_mft_fixups(&mut batch, 512);
        assert_eq!(&batch[0..4], b"BAAD");
    }

    fn build_file_name_value(parent: u64, name: &str) -> Vec<u8> {
        build_file_name_value_ns(parent, name, 0)
    }

    fn build_file_name_value_ns(parent: u64, name: &str, namespace: u8) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&parent.to_le_bytes());
        bytes.extend_from_slice(&0i64.to_le_bytes());
        bytes.extend_from_slice(&0i64.to_le_bytes());
        bytes.extend_from_slice(&0i64.to_le_bytes());
        bytes.extend_from_slice(&0i64.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        bytes.extend_from_slice(&0u64.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.extend_from_slice(&0u32.to_le_bytes());
        let utf16: Vec<u16> = name.encode_utf16().collect();
        bytes.push(utf16.len() as u8);
        bytes.push(namespace);
        for unit in utf16 {
            bytes.extend_from_slice(&unit.to_le_bytes());
        }
        bytes
    }

    fn build_resident_attribute(attr_type: u32, value: &[u8]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&attr_type.to_le_bytes());
        let length = 24 + value.len();
        bytes.extend_from_slice(&(length as u32).to_le_bytes());
        bytes.push(0);
        bytes.push(0);
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&0u16.to_le_bytes());
        bytes.extend_from_slice(&(value.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&24u16.to_le_bytes());
        bytes.push(0);
        bytes.push(0);
        bytes.extend_from_slice(value);
        bytes
    }

    fn build_nonresident_attribute(
        attr_type: u32,
        allocation_bytes: u64,
        size_bytes: u64,
    ) -> Vec<u8> {
        let mut bytes = vec![0u8; 56];
        bytes[0..4].copy_from_slice(&attr_type.to_le_bytes());
        bytes[4..8].copy_from_slice(&(56u32).to_le_bytes());
        bytes[8] = 1;
        bytes[40..48].copy_from_slice(&allocation_bytes.to_le_bytes());
        bytes[48..56].copy_from_slice(&size_bytes.to_le_bytes());
        bytes
    }

    fn build_record(index: u64, directory: bool, parent: u64, name: &str, size: u64) -> Vec<u8> {
        let mut record = vec![0u8; NTFS_RECORD_SIZE];
        record[0..4].copy_from_slice(FILE_RECORD_SIGNATURE);
        record[0x14..0x16].copy_from_slice(&0x30u16.to_le_bytes());
        let flags = if directory {
            FILE_RECORD_FLAG_DIRECTORY | 0x0001
        } else {
            0x0001
        };
        record[0x16..0x18].copy_from_slice(&flags.to_le_bytes());

        let file_name =
            build_resident_attribute(ATTRIBUTE_FILE_NAME, &build_file_name_value(parent, name));
        let data_attr = if directory {
            Vec::new()
        } else {
            build_resident_attribute(ATTRIBUTE_DATA, &vec![0u8; size as usize])
        };

        let mut attr_bytes = Vec::new();
        attr_bytes.extend_from_slice(&file_name);
        if !data_attr.is_empty() {
            attr_bytes.extend_from_slice(&data_attr);
        }
        attr_bytes.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());

        record[0x30..0x30 + attr_bytes.len()].copy_from_slice(&attr_bytes);
        record[0x1C..0x20].copy_from_slice(&(NTFS_RECORD_SIZE as u32).to_le_bytes());
        record[0x2C..0x30].copy_from_slice(&(index as u32).to_le_bytes());
        record
    }

    fn build_sparse_record(
        index: u64,
        parent: u64,
        name: &str,
        size_bytes: u64,
        allocation_bytes: u64,
    ) -> Vec<u8> {
        let mut record = vec![0u8; NTFS_RECORD_SIZE];
        record[0..4].copy_from_slice(FILE_RECORD_SIGNATURE);
        record[0x14..0x16].copy_from_slice(&0x30u16.to_le_bytes());
        record[0x16..0x18].copy_from_slice(&0x0001u16.to_le_bytes());

        let file_name =
            build_resident_attribute(ATTRIBUTE_FILE_NAME, &build_file_name_value(parent, name));
        let data_attr = build_nonresident_attribute(ATTRIBUTE_DATA, allocation_bytes, size_bytes);

        let mut attr_bytes = Vec::new();
        attr_bytes.extend_from_slice(&file_name);
        attr_bytes.extend_from_slice(&data_attr);
        attr_bytes.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());

        record[0x30..0x30 + attr_bytes.len()].copy_from_slice(&attr_bytes);
        record[0x1C..0x20].copy_from_slice(&(NTFS_RECORD_SIZE as u32).to_le_bytes());
        record[0x2C..0x30].copy_from_slice(&(index as u32).to_le_bytes());
        record
    }

    fn build_hardlink_record(index: u64, parent: u64, names: &[&str]) -> Vec<u8> {
        let mut record = vec![0u8; NTFS_RECORD_SIZE];
        record[0..4].copy_from_slice(FILE_RECORD_SIGNATURE);
        record[0x14..0x16].copy_from_slice(&0x30u16.to_le_bytes());
        record[0x16..0x18].copy_from_slice(&0x0001u16.to_le_bytes());

        let mut attr_bytes = Vec::new();
        for name in names {
            let file_name =
                build_resident_attribute(ATTRIBUTE_FILE_NAME, &build_file_name_value(parent, name));
            attr_bytes.extend_from_slice(&file_name);
        }
        attr_bytes.extend_from_slice(&build_resident_attribute(ATTRIBUTE_DATA, &[]));
        attr_bytes.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());

        record[0x30..0x30 + attr_bytes.len()].copy_from_slice(&attr_bytes);
        record[0x1C..0x20].copy_from_slice(&(NTFS_RECORD_SIZE as u32).to_le_bytes());
        record[0x2C..0x30].copy_from_slice(&(index as u32).to_le_bytes());
        record
    }

    fn parsed_entry(
        id: u64,
        parent: u64,
        name: &str,
        is_directory: bool,
        size: u64,
    ) -> ParsedNtfsEntry {
        ParsedNtfsEntry {
            file_id: FileId(id),
            parent_directory_id: Some(parent),
            name: name.to_string(),
            is_directory,
            has_attribute_list: false,
            size_bytes: size,
            allocation_bytes: size,
            attributes: if is_directory {
                FileAttributes::DIRECTORY
            } else {
                FileAttributes::ARCHIVE
            },
            created_utc: None,
            modified_utc: None,
            accessed_utc: None,
        }
    }

    fn collect_stream(
        entries: Vec<ParsedNtfsEntry>,
        extensions: Vec<ExtensionSizes>,
    ) -> (ScanSummary, Vec<ScanEvent>) {
        let mut events = Vec::new();
        let mut state = NtfsStreamState::new(String::from(r"C:\"));
        {
            let mut sink = |event: ScanEvent| events.push(event);
            state.emit_root(&mut sink);
            for entry in entries {
                state.ingest_entry(entry, &mut sink);
            }
            for extension in extensions {
                state.ingest_extension(extension, &mut sink);
            }
            state.finish(&mut sink);
        }
        (state.into_summary(), events)
    }

    fn directory_paths(events: &[ScanEvent]) -> Vec<(u64, String)> {
        events
            .iter()
            .filter_map(|event| match event {
                ScanEvent::DirectoryFound(directory) => {
                    Some((directory.id.0, directory.full_path.clone()))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn streaming_root_record_is_emitted_first() {
        let (summary, events) = collect_stream(vec![parsed_entry(10, 5, "Users", true, 0)], vec![]);
        let directories = directory_paths(&events);
        assert_eq!(
            directories[0].0, 5,
            "root record must be the first directory out"
        );
        assert_eq!(directories[0].1, r"C:\");
        assert_eq!(summary.directories_seen, 2);
    }

    #[test]
    fn streaming_resolves_out_of_order_parents_with_real_paths() {
        // Children arrive before their parents: 12 -> 11 -> 10 -> root.
        let (summary, events) = collect_stream(
            vec![
                parsed_entry(12, 11, "deep", true, 0),
                parsed_entry(11, 10, "mid", true, 0),
                parsed_entry(20, 12, "leaf.txt", false, 64),
                parsed_entry(10, 5, "top", true, 0),
            ],
            vec![],
        );
        let directories = directory_paths(&events);
        let deep = directories
            .iter()
            .find(|(id, _)| *id == 12)
            .expect("dir 12");
        assert_eq!(
            deep.1, r"C:\top\mid\deep",
            "orphan cascade must bake real paths"
        );
        assert_eq!(summary.directories_seen, 4);
        assert_eq!(summary.files_seen, 1);
        assert_eq!(summary.total_size_bytes, 64);
    }

    #[test]
    fn streaming_orphans_without_parents_fall_back_to_root() {
        // Parent record 99 never appears; 40 still emits, rooted at C:\.
        let (summary, events) = collect_stream(vec![parsed_entry(40, 99, "lost", true, 0)], vec![]);
        let directories = directory_paths(&events);
        let lost = directories
            .iter()
            .find(|(id, _)| *id == 40)
            .expect("dir 40");
        assert_eq!(lost.1, r"C:\lost");
        assert_eq!(summary.directories_seen, 2);
    }

    #[test]
    fn streaming_breaks_parent_reference_cycles() {
        // 50 and 51 reference each other; both must still emit exactly once.
        let (summary, events) = collect_stream(
            vec![
                parsed_entry(50, 51, "a", true, 0),
                parsed_entry(51, 50, "b", true, 0),
            ],
            vec![],
        );
        let directories = directory_paths(&events);
        assert_eq!(directories.len(), 3, "root + both cycle members");
        assert_eq!(summary.directories_seen, 3);
    }

    #[test]
    fn streaming_extension_after_emit_reissues_corrected_file() {
        let mut base = parsed_entry(60, 5, "big.bin", false, 100);
        base.has_attribute_list = true;
        let (summary, events) = collect_stream(
            vec![base],
            vec![ExtensionSizes {
                base_record: 60,
                size_bytes: 5_000,
                allocation_bytes: 5_000,
            }],
        );
        let file_sizes: Vec<u64> = events
            .iter()
            .filter_map(|event| match event {
                ScanEvent::FileFound(file) => Some(file.size_bytes),
                _ => None,
            })
            .collect();
        assert_eq!(
            file_sizes,
            vec![100, 5_000],
            "corrected record must re-emit"
        );
        assert_eq!(summary.files_seen, 1, "re-emit must not double count");
        assert_eq!(summary.total_size_bytes, 5_000);
    }

    #[test]
    fn streaming_extension_before_base_merges_on_arrival() {
        let mut state = NtfsStreamState::new(String::from(r"C:\"));
        let mut events = Vec::new();
        let mut sink = |event: ScanEvent| events.push(event);
        state.emit_root(&mut sink);
        state.ingest_extension(
            ExtensionSizes {
                base_record: 61,
                size_bytes: 7_000,
                allocation_bytes: 7_000,
            },
            &mut sink,
        );
        state.ingest_entry(parsed_entry(61, 5, "later.bin", false, 10), &mut sink);
        state.finish(&mut sink);
        let summary = state.into_summary();
        assert_eq!(summary.total_size_bytes, 7_000);
        assert_eq!(summary.files_seen, 1);
    }

    #[test]
    fn streaming_summary_matches_batch_parser() {
        // Same fixture as parses_directories_and_files_from_mft_records, via
        // the streaming path: counts must agree with parse_mft_records.
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&build_record(5, true, 5, "", 0));
        bytes.extend_from_slice(&build_record(10, true, 5, "Users", 0));
        bytes.extend_from_slice(&build_record(11, false, 10, "file.txt", 123));

        let batch = parse_mft_records(Path::new(r"C:\"), &bytes).expect("batch parse");

        let (parsed, extensions) =
            parse_batch(&bytes, bytes.len() / NTFS_RECORD_SIZE, 1).expect("parse batch");
        let (summary, _) = collect_stream(parsed, extensions);
        assert_eq!(summary.files_seen, batch.summary.files_seen);
        assert_eq!(summary.directories_seen, batch.summary.directories_seen);
        assert_eq!(summary.total_size_bytes, batch.summary.total_size_bytes);
    }

    #[test]
    fn parses_directories_and_files_from_mft_records() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&build_record(5, true, 5, "", 0));
        bytes.extend_from_slice(&build_record(10, true, 5, "Users", 0));
        bytes.extend_from_slice(&build_record(11, false, 10, "file.txt", 123));

        let result = parse_mft_records(Path::new(r"C:\"), &bytes).expect("parse mft");
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.directories.len(), 2);
        assert_eq!(result.files[0].name, "file.txt");
        // File records no longer store a path; the parent directory carries it.
        assert!(result.files[0].full_path.is_empty());
        let parent = result
            .directories
            .iter()
            .find(|directory| directory.id == result.files[0].parent_directory_id)
            .expect("parent directory");
        assert!(parent.full_path.ends_with(r"\Users"));
        assert_eq!(result.summary.files_seen, 1);
        assert_eq!(result.summary.directories_seen, 2);
        assert_eq!(result.summary.total_size_bytes, 123);
    }

    #[test]
    fn parallel_parser_matches_sequential_parser() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&build_record(5, true, 5, "", 0));
        bytes.extend_from_slice(&build_record(10, true, 5, "Users", 0));
        bytes.extend_from_slice(&build_record(11, false, 10, "file.txt", 123));
        bytes.extend_from_slice(&build_record(12, false, 10, "other.txt", 456));

        let sequential = parse_mft_records(Path::new(r"C:\"), &bytes).expect("sequential");
        let parallel = parse_mft_records_parallel(Path::new(r"C:\"), &bytes, 4).expect("parallel");

        assert_eq!(sequential.files, parallel.files);
        assert_eq!(sequential.directories, parallel.directories);
        assert_eq!(sequential.summary, parallel.summary);
    }

    #[test]
    fn streaming_parser_emits_live_entries() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&build_record(5, true, 5, "", 0));
        bytes.extend_from_slice(&build_record(10, true, 5, "Users", 0));
        bytes.extend_from_slice(&build_record(11, false, 10, "file.txt", 123));

        let mut kinds = Vec::new();
        let result = parse_mft_records_parallel_streaming(Path::new(r"C:\"), &bytes, 4, |event| {
            kinds.push(match event {
                ScanEvent::VolumeDiscovered(_) => "volume",
                ScanEvent::DirectoryFound(_) => "directory",
                ScanEvent::FileFound(_) => "file",
                _ => "other",
            });
        })
        .expect("streaming parse");

        assert_eq!(result.files.len(), 1);
        assert!(kinds.contains(&"volume"));
        assert!(kinds.contains(&"directory"));
        assert!(kinds.contains(&"file"));
    }

    #[test]
    fn metadata_extraction_sample_counts_records() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&build_record(5, true, 5, "", 0));
        bytes.extend_from_slice(&build_record(10, false, 5, "file.txt", 123));

        let sample = measure_metadata_extraction_overhead(&bytes).expect("sample");
        assert_eq!(sample.records_scanned, 2);
        assert_eq!(sample.bytes_scanned, (NTFS_RECORD_SIZE * 2) as u64);
        assert_eq!(sample.average_bytes_per_record, NTFS_RECORD_SIZE as u64);
    }

    #[test]
    fn total_size_bytes_saturate_on_overflow() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&build_record(5, true, 5, "", 0));
        bytes.extend_from_slice(&build_sparse_record(20, 5, "huge-a.bin", u64::MAX, 0));
        bytes.extend_from_slice(&build_sparse_record(21, 5, "huge-b.bin", 1, 0));

        let result = parse_mft_records(Path::new(r"C:\"), &bytes).expect("parse");
        assert_eq!(result.files.len(), 2);
        assert_eq!(result.summary.total_size_bytes, u64::MAX);
    }

    #[test]
    fn invalid_file_name_records_are_skipped() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&build_record(5, true, 5, "", 0));
        let mut invalid = build_record(11, false, 5, "broken.txt", 123);
        invalid[0x8A] = 0x00;
        invalid[0x8B] = 0xD8;
        bytes.extend_from_slice(&invalid);
        bytes.extend_from_slice(&build_record(12, false, 5, "good.txt", 456));

        let result = parse_mft_records(Path::new(r"C:\"), &bytes).expect("parse");
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].name, "good.txt");
    }

    #[test]
    fn sparse_file_accounting_uses_logical_and_allocated_sizes() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&build_record(5, true, 5, "", 0));
        bytes.extend_from_slice(&build_sparse_record(20, 5, "sparse.bin", 4096, 1024));

        let result = parse_mft_records(Path::new(r"C:\"), &bytes).expect("parse");
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].size_bytes, 4096);
        assert_eq!(result.files[0].allocation_bytes, 1024);
        assert_eq!(result.summary.total_size_bytes, 4096);
        assert_eq!(result.summary.total_allocation_bytes, 1024);
    }

    #[test]
    fn win32_name_preferred_over_trailing_dos_alias() {
        // Win32 name first, DOS 8.3 alias last - the alias must not win.
        let mut record = vec![0u8; NTFS_RECORD_SIZE];
        record[0..4].copy_from_slice(FILE_RECORD_SIGNATURE);
        record[0x14..0x16].copy_from_slice(&0x30u16.to_le_bytes());
        record[0x16..0x18].copy_from_slice(&0x0001u16.to_le_bytes());

        let mut attr_bytes = Vec::new();
        attr_bytes.extend_from_slice(&build_resident_attribute(
            ATTRIBUTE_FILE_NAME,
            &build_file_name_value_ns(5, "Program Files", 1),
        ));
        attr_bytes.extend_from_slice(&build_resident_attribute(
            ATTRIBUTE_FILE_NAME,
            &build_file_name_value_ns(5, "PROGRA~1", 2),
        ));
        attr_bytes.extend_from_slice(&build_resident_attribute(ATTRIBUTE_DATA, &[1, 2, 3]));
        attr_bytes.extend_from_slice(&0xFFFF_FFFFu32.to_le_bytes());
        record[0x30..0x30 + attr_bytes.len()].copy_from_slice(&attr_bytes);
        record[0x1C..0x20].copy_from_slice(&(NTFS_RECORD_SIZE as u32).to_le_bytes());
        record[0x2C..0x30].copy_from_slice(&31u32.to_le_bytes());

        let mut bytes = Vec::new();
        bytes.extend_from_slice(&build_record(5, true, 5, "", 0));
        bytes.extend_from_slice(&record);

        let result = parse_mft_records(Path::new(r"C:\"), &bytes).expect("parse");
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.files[0].name, "Program Files");
    }

    #[test]
    fn hardlink_style_records_still_count_as_one_file() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&build_record(5, true, 5, "", 0));
        bytes.extend_from_slice(&build_hardlink_record(30, 5, &["first.txt", "second.txt"]));

        let result = parse_mft_records(Path::new(r"C:\"), &bytes).expect("parse");
        assert_eq!(result.files.len(), 1);
        assert_eq!(result.summary.files_seen, 1);
        assert_eq!(result.files[0].name, "second.txt");
    }
}

/// Fuzz/corpus coverage for the MFT byte parsers. The MFT is untrusted on-disk
/// data (a corrupt volume, a hostile image, a torn record): every parser here
/// takes a raw byte slice and must always return `Ok`/`Err` or a value —
/// never panic or over-read. These feed random, truncated, and
/// signature-forced garbage through each layer and the full streaming pipeline.
#[cfg(test)]
mod fuzz_tests {
    use super::*;

    /// Deterministic xorshift so the corpus is reproducible.
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
        fn bytes(&mut self, len: usize) -> Vec<u8> {
            (0..len).map(|_| self.byte()).collect()
        }
    }

    #[test]
    fn apply_mft_fixups_survives_garbage() {
        let mut rng = Rng(0x1234_5678_9ABC_DEF0);
        for _ in 0..3000 {
            let len = (rng.next_u32() % 8192) as usize;
            let mut buffer = rng.bytes(len);
            // Force a FILE signature on the first record so the fixup path runs.
            if buffer.len() >= 4 && rng.next_u32() & 1 == 0 {
                buffer[0..4].copy_from_slice(FILE_RECORD_SIGNATURE);
            }
            let sector = [512usize, 4096, 0, 1, 65536][(rng.next_u32() % 5) as usize];
            apply_mft_fixups(&mut buffer, sector);
        }
    }

    #[test]
    fn decode_data_runs_survives_garbage() {
        let mut rng = Rng(0xDEAD_BEEF_CAFE_F00D);
        for _ in 0..5000 {
            let len = (rng.next_u32() % 64) as usize;
            let runs = rng.bytes(len);
            let cluster = [512u64, 4096, 0, 65536][(rng.next_u32() % 4) as usize];
            let _ = decode_data_runs(&runs, cluster);
        }
    }

    #[test]
    fn parse_record_survives_garbage() {
        let mut rng = Rng(0x0F0F_0F0F_1111_2222);
        for _ in 0..5000 {
            let mut record = rng.bytes(NTFS_RECORD_SIZE);
            // Half the time force the FILE signature so the attribute loop runs
            // on garbage rather than bouncing off the signature check.
            if rng.next_u32() & 1 == 0 {
                record[0..4].copy_from_slice(FILE_RECORD_SIGNATURE);
            }
            let _ = parse_record(&record);
        }
    }

    #[test]
    fn parse_mft_records_survives_garbage() {
        let mut rng = Rng(0xABCD_1234_5678_9F0E);
        for _ in 0..600 {
            let records = 1 + (rng.next_u32() % 8) as usize;
            let mut bytes = rng.bytes(records * NTFS_RECORD_SIZE);
            for record in 0..records {
                if rng.next_u32() & 1 == 0 {
                    let start = record * NTFS_RECORD_SIZE;
                    bytes[start..start + 4].copy_from_slice(FILE_RECORD_SIGNATURE);
                }
            }
            let _ = parse_mft_records(Path::new(r"C:\"), &bytes);
        }
    }

    #[test]
    fn streaming_pipeline_survives_garbage() {
        // Full production path over garbage: fixups -> parallel parse -> stream
        // state ingest -> finish. Must never panic on a corrupt MFT.
        let mut rng = Rng(0x7777_3333_9999_1111);
        for _ in 0..40 {
            let records = 1 + (rng.next_u32() % 3000) as usize;
            let mut bytes = rng.bytes(records * NTFS_RECORD_SIZE);
            for record in 0..records {
                if !rng.next_u32().is_multiple_of(3) {
                    let start = record * NTFS_RECORD_SIZE;
                    bytes[start..start + 4].copy_from_slice(FILE_RECORD_SIGNATURE);
                }
            }
            apply_mft_fixups(&mut bytes, 512);
            let Ok((parsed, extensions)) = parse_batch(&bytes, records, 4) else {
                continue;
            };
            let mut state = NtfsStreamState::new(String::from(r"C:\"));
            let mut sink = |_event: ScanEvent| {};
            state.emit_root(&mut sink);
            for entry in parsed {
                state.ingest_entry(entry, &mut sink);
            }
            for extension in extensions {
                state.ingest_extension(extension, &mut sink);
            }
            state.finish(&mut sink);
            let _ = state.into_summary();
        }
    }
}
