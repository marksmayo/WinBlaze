use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::{FromRawHandle, RawHandle};
use std::path::Path;
use std::thread;
use std::{
    ffi::c_void,
    ptr::{null, null_mut},
};

use winblaze_core::{
    aggregate_directory_records, DirectoryId, DirectoryRecord, FileAttributes, FileId, FileRecord,
    FileSystemKind, ScanEvent, ScanProgress, ScanSummary, VolumeId, VolumeRecord,
};

const FILE_RECORD_SIGNATURE: &[u8; 4] = b"FILE";
const NTFS_RECORD_SIZE: usize = 1024;
const ATTRIBUTE_FILE_NAME: u32 = 0x30;
const ATTRIBUTE_DATA: u32 = 0x80;
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

pub fn enumerate_ntfs_volume_parallel_streaming<F>(
    root: &Path,
    worker_count: usize,
    mut on_event: F,
) -> Result<NtfsEnumeration, NtfsEnumerationError>
where
    F: FnMut(ScanEvent),
{
    let entries = stream_ntfs_entries(root, worker_count, &mut on_event)?;
    parse_entries(root, entries, None)
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
    let entries = stream_ntfs_entries(root, worker_count, &mut on_event)?;
    Ok(summarize_entries(&entries))
}

fn stream_ntfs_entries(
    root: &Path,
    worker_count: usize,
    on_event: &mut dyn FnMut(ScanEvent),
) -> Result<HashMap<u64, ParsedNtfsEntry>, NtfsEnumerationError> {
    let mut file = open_mft_stream(root)?;
    let bytes_per_sector = file.bytes_per_sector as usize;
    let total_records = (file.total_bytes / NTFS_RECORD_SIZE as u64).max(1);
    let workers = worker_count.max(1);
    let records_per_read = workers.saturating_mul(1024).clamp(1024, 65_536);
    let mut buffer = vec![0u8; NTFS_RECORD_SIZE * records_per_read];
    let mut carry: Vec<u8> = Vec::new();
    let mut entries: HashMap<u64, ParsedNtfsEntry> = HashMap::new();
    let mut resolved_paths: HashMap<u64, String> = HashMap::new();
    let mut emitted: HashSet<u64> = HashSet::new();
    let mut pending: Vec<u64> = Vec::new();
    let mut pending_extensions: HashMap<u64, ExtensionSizes> = HashMap::new();
    let mut processed_records = 0u64;
    let root_text = root.display().to_string();

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }

        carry.extend_from_slice(&buffer[..read]);
        let full_records = carry.len() / NTFS_RECORD_SIZE;
        let full_bytes = full_records * NTFS_RECORD_SIZE;

        // Restore the sector-tail bytes the on-disk update sequence array
        // displaced; raw MFT data is unusable without this.
        apply_mft_fixups(&mut carry[..full_bytes], bytes_per_sector);
        let (parsed, extensions) = parse_batch(&carry[..full_bytes], full_records, workers)?;
        processed_records = processed_records.saturating_add(full_records as u64);

        let mut parsed_ids = Vec::with_capacity(parsed.len());
        for mut entry in parsed {
            let file_id = entry.file_id.0;
            if let Some(sizes) = pending_extensions.remove(&file_id) {
                entry.size_bytes = entry.size_bytes.max(sizes.size_bytes);
                entry.allocation_bytes = entry.allocation_bytes.max(sizes.allocation_bytes);
            }
            entries.insert(file_id, entry);
            parsed_ids.push(file_id);
        }

        // Merge sizes carried by extension records into their base entries;
        // if the base was already emitted, re-emit it so downstream
        // consumers upsert the corrected sizes.
        for extension in extensions {
            match entries.get_mut(&extension.base_record) {
                Some(entry) => {
                    entry.size_bytes = entry.size_bytes.max(extension.size_bytes);
                    entry.allocation_bytes =
                        entry.allocation_bytes.max(extension.allocation_bytes);
                    if emitted.remove(&extension.base_record) {
                        parsed_ids.push(extension.base_record);
                    }
                }
                None => {
                    pending_extensions.insert(extension.base_record, extension);
                }
            }
        }

        carry.drain(..full_bytes);
        let progress = ScanProgress {
            completed_items: processed_records,
            total_items: total_records,
            completed_bytes: processed_records.saturating_mul(NTFS_RECORD_SIZE as u64),
            total_bytes: total_records.saturating_mul(NTFS_RECORD_SIZE as u64),
        };
        on_event(ScanEvent::Progress(progress));
        emit_streaming_entries(
            &root_text,
            &entries,
            &parsed_ids,
            &mut pending,
            &mut resolved_paths,
            &mut emitted,
            on_event,
        );
    }

    emit_streaming_entries(
        &root_text,
        &entries,
        &[],
        &mut pending,
        &mut resolved_paths,
        &mut emitted,
        on_event,
    );
    Ok(entries)
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

        let mut parsed = Vec::new();
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

fn summarize_entries(entries: &HashMap<u64, ParsedNtfsEntry>) -> ScanSummary {
    let mut files_seen = 0u64;
    let mut directories_seen = 0u64;
    let mut total_size_bytes = 0u64;
    let mut total_allocation_bytes = 0u64;
    for entry in entries.values() {
        if entry.is_directory {
            directories_seen = directories_seen.saturating_add(1);
        } else {
            files_seen = files_seen.saturating_add(1);
            total_size_bytes = total_size_bytes.saturating_add(entry.size_bytes);
            total_allocation_bytes = total_allocation_bytes.saturating_add(entry.allocation_bytes);
        }
    }

    // parse_entries() inserts a synthetic root (record 5) when the MFT bytes
    // did not include one; mirror that so both completion paths report the
    // same directory count.
    if !entries.contains_key(&5) {
        directories_seen = directories_seen.saturating_add(1);
    }

    ScanSummary {
        files_seen,
        directories_seen,
        total_size_bytes,
        total_allocation_bytes,
    }
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
        if self.chunk_pos >= self.chunk_len {
            self.fill_chunk()?;
            if self.chunk_len == 0 {
                return Ok(0);
            }
        }
        let available = self.chunk_len - self.chunk_pos;
        let count = available.min(buf.len());
        buf[..count].copy_from_slice(&self.chunk[self.chunk_pos..self.chunk_pos + count]);
        self.chunk_pos += count;
        Ok(count)
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

        current_lcn += offset_delta;
        if current_lcn < 0 || run_length == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "data run points before volume start",
            ));
        }
        extents.push((
            current_lcn as u64 * bytes_per_cluster,
            run_length * bytes_per_cluster,
        ));
    }

    if extents.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "MFT data attribute has no runs",
        ));
    }
    Ok(extents)
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
            let run_offset =
                u16::from_le_bytes([record[cursor + 32], record[cursor + 33]]) as usize;
            if run_offset >= attribute_length {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "MFT $DATA run offset out of bounds",
                ));
            }
            let runs = &record[cursor + run_offset..cursor + attribute_length];
            return decode_data_runs(runs, geometry.bytes_per_cluster);
        }
        cursor += attribute_length;
    }

    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "MFT record 0 has no non-resident $DATA attribute",
    ))
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
        if &record[0..4] == FILE_RECORD_SIGNATURE
            && !apply_record_fixups(record, bytes_per_sector)
        {
            record[0..4].copy_from_slice(b"BAAD");
        }
        offset += NTFS_RECORD_SIZE;
    }
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
                        Ok(extents) => {
                            let reader = VolumeMftReader {
                                volume,
                                extents,
                                extent_index: 0,
                                consumed_in_extent: 0,
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

        let mut entries: HashMap<u64, ParsedNtfsEntry> = HashMap::new();
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

    let mut entries: HashMap<u64, ParsedNtfsEntry> = HashMap::new();
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
    entries: &mut HashMap<u64, ParsedNtfsEntry>,
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
    mut entries: HashMap<u64, ParsedNtfsEntry>,
    mut on_event: Option<&mut dyn FnMut(ScanEvent)>,
) -> Result<NtfsEnumeration, NtfsEnumerationError> {
    let root_text = root.display().to_string();

    entries.entry(5).or_insert_with(|| ParsedNtfsEntry {
        file_id: FileId(5),
        parent_directory_id: None,
        name: String::from(""),
        is_directory: true,
        size_bytes: 0,
        allocation_bytes: 0,
        attributes: FileAttributes::DIRECTORY,
        created_utc: None,
        modified_utc: None,
        accessed_utc: None,
    });

    let mut resolved_paths = HashMap::new();
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

fn emit_streaming_entries(
    root_text: &str,
    entries: &HashMap<u64, ParsedNtfsEntry>,
    candidate_ids: &[u64],
    pending: &mut Vec<u64>,
    resolved_paths: &mut HashMap<u64, String>,
    emitted: &mut HashSet<u64>,
    on_event: &mut dyn FnMut(ScanEvent),
) {
    // An empty candidate list marks the final pass: every entry not yet
    // emitted goes out now, falling back to the root path for records whose
    // parent never appeared in the MFT (e.g. orphaned records).
    let final_pass = candidate_ids.is_empty();
    let mut ids: Vec<_> = if final_pass {
        pending.clear();
        entries.keys().copied().collect()
    } else {
        let mut ids = std::mem::take(pending);
        ids.extend_from_slice(candidate_ids);
        ids
    };
    ids.sort_unstable();

    for file_id in ids {
        if file_id == 5 {
            emitted.insert(file_id);
            continue;
        }

        if emitted.contains(&file_id) {
            continue;
        }

        let Some(entry) = entries.get(&file_id) else {
            continue;
        };

        // MFT records mostly arrive parent-before-child, but not always.
        // Emitting an entry before its parent record has been parsed would
        // bake the root-fallback path into the streamed (and persisted)
        // record, so defer it until a later batch or the final pass.
        if !final_pass && !parent_chain_known(file_id, entries, resolved_paths) {
            pending.push(file_id);
            continue;
        }

        emitted.insert(file_id);

        if entry.is_directory {
            // Only directories carry a materialized path; file paths derive
            // from their parent on demand (see FileRecord docs).
            let full_path =
                resolve_entry_path(file_id, entries, resolved_paths, root_text, &mut Vec::new());
            let directory = DirectoryRecord {
                id: DirectoryId(entry.file_id.0),
                parent_directory_id: entry.parent_directory_id.map(DirectoryId),
                name: entry.name.clone(),
                full_path,
                direct_bytes: 0,
                total_bytes: 0,
                direct_entries: 0,
                total_entries: 0,
            };
            on_event(ScanEvent::DirectoryFound(directory));
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
            on_event(ScanEvent::FileFound(file));
        }
    }
}

/// Whether every link in `file_id`'s parent chain is either already
/// path-resolved or present in `entries`, i.e. resolving the path now will
/// not silently fall back to the root because a parent record has simply not
/// been parsed yet. Cycles are treated as known — `resolve_entry_path` has
/// its own cycle guard.
fn parent_chain_known(
    file_id: u64,
    entries: &HashMap<u64, ParsedNtfsEntry>,
    resolved_paths: &HashMap<u64, String>,
) -> bool {
    let mut current = file_id;
    let mut hops = 0usize;
    loop {
        if current == 5 {
            return true;
        }
        if hops > 4096 {
            return true;
        }
        hops += 1;

        let Some(entry) = entries.get(&current) else {
            return false;
        };
        match entry.parent_directory_id {
            None => return true,
            Some(parent) if parent == current => return true,
            Some(parent) => {
                if parent == 5 || resolved_paths.contains_key(&parent) {
                    return true;
                }
                if !entries.contains_key(&parent) {
                    return false;
                }
                current = parent;
            }
        }
    }
}

#[derive(Clone, Debug)]
struct ParsedNtfsEntry {
    file_id: FileId,
    parent_directory_id: Option<u64>,
    name: String,
    is_directory: bool,
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

    let file_index = read_u32(record, 0x2C)? as u64;
    let flags = read_u16(record, 0x16)?;
    // The MFT retains records of deleted files for slot reuse; they still
    // carry the FILE signature but must not be counted.
    if flags & FILE_RECORD_FLAG_IN_USE == 0 {
        return Ok(ParsedRecordOutcome::None);
    }
    let base_record = read_u64(record, 0x20)? & 0x0000_FFFF_FFFF_FFFF;
    let is_directory = flags & FILE_RECORD_FLAG_DIRECTORY != 0;
    let first_attr_offset = read_u16(record, 0x14)? as usize;
    let mut cursor = first_attr_offset;
    let mut file_name: Option<FileNameAttribute> = None;
    let mut allocation_bytes = 0u64;
    let mut size_bytes = 0u64;

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
            ATTRIBUTE_FILE_NAME if non_resident == 0 => {
                let value_offset = read_u16(record, cursor + 20)? as usize;
                let value_length = read_u32(record, cursor + 16)? as usize;
                if cursor + value_offset + value_length <= record.len() {
                    match parse_file_name(
                        &record[cursor + value_offset..cursor + value_offset + value_length],
                    ) {
                        Ok(name) => file_name = Some(name),
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
                // starting VCN is zero.
                if read_u64(record, cursor + 16)? == 0 {
                    allocation_bytes = read_u64(record, cursor + 40)?;
                    size_bytes = read_u64(record, cursor + 48)?;
                }
            }
            _ => {}
        }

        if base_record == 0
            && file_name.is_some()
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
    name: String,
}

fn parse_file_name(bytes: &[u8]) -> Result<FileNameAttribute, NtfsEnumerationError> {
    if bytes.len() < 66 {
        return Err(NtfsEnumerationError::InvalidRecord(String::from(
            "file name attribute too small",
        )));
    }

    let parent_reference = Some(read_u64(bytes, 0)? & 0x0000_FFFF_FFFF_FFFF);
    let created_utc = read_i64(bytes, 8)?;
    let modified_utc = read_i64(bytes, 16)?;
    let accessed_utc = read_i64(bytes, 24)?;
    let name_length = bytes[64] as usize;
    let name_offset = 66usize;
    let name_bytes = name_length.saturating_mul(2);
    if name_offset + name_bytes > bytes.len() {
        return Err(NtfsEnumerationError::InvalidRecord(String::from(
            "file name attribute truncated",
        )));
    }

    let mut utf16 = Vec::with_capacity(name_length);
    for chunk in bytes[name_offset..name_offset + name_bytes].chunks_exact(2) {
        utf16.push(u16::from_le_bytes([chunk[0], chunk[1]]));
    }

    let name = String::from_utf16(&utf16).map_err(|_| {
        NtfsEnumerationError::InvalidRecord(String::from("invalid utf-16 file name"))
    })?;

    Ok(FileNameAttribute {
        parent_reference,
        created_utc,
        modified_utc,
        accessed_utc,
        name,
    })
}

fn resolve_entry_path(
    file_id: u64,
    entries: &HashMap<u64, ParsedNtfsEntry>,
    resolved: &mut HashMap<u64, String>,
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

fn total_file_count(entries: &HashMap<u64, ParsedNtfsEntry>) -> u64 {
    entries.values().filter(|entry| !entry.is_directory).count() as u64
}

fn total_directory_count(entries: &HashMap<u64, ParsedNtfsEntry>) -> u64 {
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

fn read_i64(bytes: &[u8], offset: usize) -> Result<i64, NtfsEnumerationError> {
    if offset + 8 > bytes.len() {
        return Err(NtfsEnumerationError::InvalidRecord(String::from(
            "i64 out of bounds",
        )));
    }
    Ok(i64::from_le_bytes([
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
        bytes.push(0);
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
