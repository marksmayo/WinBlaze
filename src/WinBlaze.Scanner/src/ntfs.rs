use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{self, Read};
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
    let mut mft_bytes = Vec::new();
    open_ntfs_metadata_file(root, "$MFT")?.read_to_end(&mut mft_bytes)?;
    parse_mft_records(root, &mft_bytes)
}

pub fn enumerate_ntfs_volume_parallel(
    root: &Path,
    worker_count: usize,
) -> Result<NtfsEnumeration, NtfsEnumerationError> {
    let mut mft_bytes = Vec::new();
    open_ntfs_metadata_file(root, "$MFT")?.read_to_end(&mut mft_bytes)?;
    parse_mft_records_parallel(root, &mft_bytes, worker_count)
}

pub fn enumerate_ntfs_volume_parallel_streaming<F>(
    root: &Path,
    worker_count: usize,
    on_event: F,
) -> Result<NtfsEnumeration, NtfsEnumerationError>
where
    F: FnMut(ScanEvent),
{
    let mut file = open_ntfs_metadata_file(root, "$MFT")?;
    let total_records = file
        .metadata()
        .ok()
        .map(|metadata| (metadata.len() / NTFS_RECORD_SIZE as u64).max(1))
        .unwrap_or(1);
    let records_per_read = worker_count.max(1).saturating_mul(1024).clamp(1024, 65_536);
    let mut buffer = vec![0u8; NTFS_RECORD_SIZE * records_per_read];
    let mut carry = Vec::new();
    let mut entries: HashMap<u64, ParsedNtfsEntry> = HashMap::new();
    let mut resolved_paths: HashMap<u64, String> = HashMap::new();
    let mut emitted: HashSet<u64> = HashSet::new();
    let mut on_event = on_event;
    let mut processed_records = 0u64;

    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }

        carry.extend_from_slice(&buffer[..read]);
        let full_records = carry.len() / NTFS_RECORD_SIZE;
        let full_bytes = full_records * NTFS_RECORD_SIZE;

        let mut parsed_ids = Vec::new();
        for record in carry[..full_bytes].chunks(NTFS_RECORD_SIZE) {
            if record.len() < NTFS_RECORD_SIZE || &record[0..4] != FILE_RECORD_SIGNATURE {
                continue;
            }

            processed_records = processed_records.saturating_add(1);
            if let Some(entry) = parse_record(record)? {
                let file_id = entry.file_id.0;
                entries.insert(file_id, entry);
                parsed_ids.push(file_id);
            }
        }

        carry = carry[full_bytes..].to_vec();
        let progress = ScanProgress {
            completed_items: processed_records,
            total_items: total_records,
            completed_bytes: processed_records.saturating_mul(NTFS_RECORD_SIZE as u64),
            total_bytes: total_records.saturating_mul(NTFS_RECORD_SIZE as u64),
        };
        on_event(ScanEvent::Progress(progress));
        emit_streaming_entries(
            root,
            &entries,
            &parsed_ids,
            &mut resolved_paths,
            &mut emitted,
            &mut on_event,
        );
    }

    emit_streaming_entries(
        root,
        &entries,
        &[],
        &mut resolved_paths,
        &mut emitted,
        &mut on_event,
    );
    parse_entries(root, entries, None)
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
    let mut token: HANDLE = null_mut();
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
    let mut luid = LUID::default();
    let looked_up = unsafe { LookupPrivilegeValueW(null(), privilege_name.as_ptr(), &mut luid) };
    if looked_up == 0 {
        unsafe {
            CloseHandle(token);
        }
        return Err(io::Error::last_os_error());
    }

    let mut privileges = TOKEN_PRIVILEGES {
        PrivilegeCount: 1,
        Privileges: [LUID_AND_ATTRIBUTES {
            Luid: luid,
            Attributes: SE_PRIVILEGE_ENABLED,
        }],
    };

    let adjusted = unsafe {
        AdjustTokenPrivileges(
            token,
            0,
            &mut privileges,
            std::mem::size_of::<TOKEN_PRIVILEGES>() as u32,
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
        for handle in handles {
            match handle.join() {
                Ok(Ok(chunk_entries)) => {
                    for entry in chunk_entries {
                        entries.insert(entry.file_id.0, entry);
                    }
                }
                Ok(Err(error)) => return Err(error),
                Err(_) => {
                    return Err(NtfsEnumerationError::InvalidRecord(String::from(
                        "parallel parser worker panicked",
                    )))
                }
            }
        }
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

    for record in bytes.chunks(NTFS_RECORD_SIZE) {
        if record.len() < NTFS_RECORD_SIZE || &record[0..4] != FILE_RECORD_SIGNATURE {
            continue;
        }

        if let Some(entry) = parse_record(record)? {
            entries.insert(entry.file_id.0, entry);
        }
    }

    parse_entries(root, entries, on_event)
}

fn parse_record_range(
    shared: &[u8],
    start_record: usize,
    end_record: usize,
) -> Result<Vec<ParsedNtfsEntry>, NtfsEnumerationError> {
    let mut entries = Vec::new();
    for index in start_record..end_record {
        let start = index * NTFS_RECORD_SIZE;
        let end = start + NTFS_RECORD_SIZE;
        let record = &shared[start..end];
        if &record[0..4] != FILE_RECORD_SIGNATURE {
            continue;
        }

        if let Some(entry) = parse_record(record)? {
            entries.push(entry);
        }
    }

    Ok(entries)
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
        let full_path = resolve_entry_path(
            entry.file_id.0,
            &entries,
            &mut resolved_paths,
            &root_text,
            &mut Vec::new(),
        );

        if entry.is_directory {
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
                full_path,
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

fn emit_streaming_entries<F>(
    root: &Path,
    entries: &HashMap<u64, ParsedNtfsEntry>,
    candidate_ids: &[u64],
    resolved_paths: &mut HashMap<u64, String>,
    emitted: &mut HashSet<u64>,
    on_event: &mut F,
) where
    F: FnMut(ScanEvent),
{
    let root_text = root.display().to_string();
    let mut ids: Vec<_> = if candidate_ids.is_empty() {
        entries.keys().copied().collect()
    } else {
        candidate_ids.to_vec()
    };
    ids.sort_unstable();

    for file_id in ids {
        if file_id == 5 {
            emitted.insert(file_id);
            continue;
        }

        if !emitted.insert(file_id) {
            continue;
        }

        let Some(entry) = entries.get(&file_id) else {
            continue;
        };

        let full_path = resolve_entry_path(
            file_id,
            entries,
            resolved_paths,
            &root_text,
            &mut Vec::new(),
        );

        if entry.is_directory {
            let directory = DirectoryRecord {
                id: DirectoryId(entry.file_id.0),
                parent_directory_id: entry.parent_directory_id.map(DirectoryId),
                name: entry.name.clone(),
                full_path: full_path.clone(),
                direct_bytes: 0,
                total_bytes: 0,
                direct_entries: 0,
                total_entries: 0,
            };
            on_event(ScanEvent::DirectoryFound(directory));
            emitted.insert(file_id);
        } else {
            let file = FileRecord {
                id: FileId(entry.file_id.0),
                parent_directory_id: DirectoryId(entry.parent_directory_id.unwrap_or(5)),
                name: entry.name.clone(),
                full_path: full_path.clone(),
                size_bytes: entry.size_bytes,
                allocation_bytes: entry.allocation_bytes,
                attributes: entry.attributes,
                created_utc: entry.created_utc,
                modified_utc: entry.modified_utc,
                accessed_utc: entry.accessed_utc,
            };
            on_event(ScanEvent::FileFound(file));
            emitted.insert(file_id);
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

fn parse_record(record: &[u8]) -> Result<Option<ParsedNtfsEntry>, NtfsEnumerationError> {
    if record.len() < NTFS_RECORD_SIZE || &record[0..4] != FILE_RECORD_SIGNATURE {
        return Ok(None);
    }

    let file_index = read_u32(record, 0x2C)? as u64;
    let flags = read_u16(record, 0x16)?;
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
        match attribute_type {
            ATTRIBUTE_FILE_NAME if non_resident == 0 => {
                let value_offset = read_u16(record, cursor + 20)? as usize;
                let value_length = read_u32(record, cursor + 16)? as usize;
                if cursor + value_offset + value_length <= record.len() {
                    match parse_file_name(
                        &record[cursor + value_offset..cursor + value_offset + value_length],
                    ) {
                        Ok(name) => file_name = Some(name),
                        Err(_) => return Ok(None),
                    }
                }
            }
            ATTRIBUTE_DATA if non_resident == 0 => {
                size_bytes = read_u32(record, cursor + 16)? as u64;
                allocation_bytes = size_bytes;
            }
            ATTRIBUTE_DATA if non_resident != 0 => {
                allocation_bytes = read_u64(record, cursor + 40)?;
                size_bytes = read_u64(record, cursor + 48)?;
            }
            _ => {}
        }

        if file_name.is_some() && (is_directory || size_bytes != 0 || allocation_bytes != 0) {
            break;
        }

        cursor += attribute_length;
    }

    let file_name = match file_name {
        Some(value) => value,
        None => return Ok(None),
    };

    let attributes = if is_directory {
        FileAttributes::DIRECTORY
    } else {
        FileAttributes::ARCHIVE
    };

    Ok(Some(ParsedNtfsEntry {
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
        if parse_record(record)?.is_some() {
            record_count = record_count.saturating_add(1);
        }
    }

    Ok(MetadataExtractionSample {
        records_scanned: record_count,
        bytes_scanned,
        average_bytes_per_record: if record_count == 0 {
            0
        } else {
            bytes_scanned / record_count
        },
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

type HANDLE = *mut c_void;

const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
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
struct LUID {
    LowPart: u32,
    HighPart: i32,
}

#[repr(C)]
#[allow(non_snake_case)]
#[derive(Clone, Copy)]
struct LUID_AND_ATTRIBUTES {
    Luid: LUID,
    Attributes: u32,
}

#[repr(C)]
#[allow(non_snake_case)]
struct TOKEN_PRIVILEGES {
    PrivilegeCount: u32,
    Privileges: [LUID_AND_ATTRIBUTES; 1],
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
    ) -> HANDLE;

    fn CloseHandle(hObject: HANDLE) -> i32;

    fn GetCurrentProcess() -> HANDLE;
}

#[link(name = "advapi32")]
extern "system" {
    fn OpenProcessToken(ProcessHandle: HANDLE, DesiredAccess: u32, TokenHandle: *mut HANDLE)
        -> i32;

    fn LookupPrivilegeValueW(
        lpSystemName: *const u16,
        lpName: *const u16,
        lpLuid: *mut LUID,
    ) -> i32;

    fn AdjustTokenPrivileges(
        TokenHandle: HANDLE,
        DisableAllPrivileges: i32,
        NewState: *mut TOKEN_PRIVILEGES,
        BufferLength: u32,
        PreviousState: *mut TOKEN_PRIVILEGES,
        ReturnLength: *mut u32,
    ) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert!(result.files[0].full_path.ends_with(r"\Users\file.txt"));
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
