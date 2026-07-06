//! Raw directory enumeration over `FindFirstFileExW`.
//!
//! std's `read_dir` already requests `FindExInfoBasic` (skipping 8.3 alias
//! lookups) but not `FIND_FIRST_EX_LARGE_FETCH`, which batches directory
//! entries in ~64KB kernel round-trips instead of the small default. The raw
//! iterator also hands back attributes, sizes, and timestamps directly from
//! the `WIN32_FIND_DATAW` the enumeration already produced, so the walker
//! needs no per-entry `metadata()` call at all.

use std::ffi::{c_void, OsString};
use std::fs;
use std::io;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

#[repr(C)]
#[allow(non_snake_case)]
pub struct Win32FindDataW {
    dwFileAttributes: u32,
    ftCreationTime: [u32; 2],
    ftLastAccessTime: [u32; 2],
    ftLastWriteTime: [u32; 2],
    nFileSizeHigh: u32,
    nFileSizeLow: u32,
    dwReserved0: u32,
    dwReserved1: u32,
    cFileName: [u16; 260],
    cAlternateFileName: [u16; 14],
}

const INVALID_HANDLE_VALUE: isize = -1;
const FIND_EX_INFO_BASIC: u32 = 1;
const FIND_EX_SEARCH_NAME_MATCH: u32 = 0;
const FIND_FIRST_EX_LARGE_FETCH: u32 = 2;
const ERROR_NO_MORE_FILES: i32 = 18;
/// How many mid-directory `FindNextFileW` failures to surface (and skip
/// past) before giving up on the handle: preserves the per-entry error
/// recovery of the `fs::read_dir` loop this replaced, while still
/// guaranteeing termination if the handle is persistently broken.
const FIND_ERROR_BUDGET: u8 = 8;

#[link(name = "kernel32")]
extern "system" {
    fn FindFirstFileExW(
        lpFileName: *const u16,
        fInfoLevelId: u32,
        lpFindFileData: *mut Win32FindDataW,
        fSearchOp: u32,
        lpSearchFilter: *mut c_void,
        dwAdditionalFlags: u32,
    ) -> isize;

    fn FindNextFileW(hFindFile: isize, lpFindFileData: *mut Win32FindDataW) -> i32;

    fn FindClose(hFindFile: isize) -> i32;
}

/// A directory-enumeration error, carrying the specific path it occurred on
/// when known (a per-entry `metadata()` failure) so callers report the exact
/// file rather than the parent directory; `None` for handle-level failures
/// that no single entry owns.
pub struct EnumerationError {
    pub error: io::Error,
    pub path: Option<PathBuf>,
}

impl EnumerationError {
    fn bare(error: io::Error) -> Self {
        Self { error, path: None }
    }
}

/// One directory entry, fully described by the enumeration itself.
///
/// `name` is the exact on-disk name (`OsString` round-trips unpaired UTF-16
/// surrogates, which are legal on NTFS); anything that joins paths or reopens
/// the entry must use it verbatim and only lossy-convert for display.
pub struct FindEntry {
    pub name: OsString,
    pub attributes: u32,
    pub size_bytes: u64,
    /// FILETIME (100ns ticks since 1601); zero means "not available".
    pub created_utc: u64,
    pub modified_utc: u64,
    pub accessed_utc: u64,
}

pub struct FindIterator {
    handle: isize,
    /// Reusable find-data buffer: `FindEntry` copies everything out before
    /// the next `FindNextFileW` call, so one allocation serves the whole
    /// directory instead of one per entry.
    buffer: Box<Win32FindDataW>,
    /// The first entry (filled by `FindFirstFileExW`) is pending in `buffer`.
    first_pending: bool,
    error_budget: u8,
    done: bool,
}

/// Opens `directory` for enumeration. Fails like `fs::read_dir` does (access
/// denied, not found, ...); callers keep their existing error handling.
pub fn read_dir_fast(directory: &Path) -> io::Result<FindIterator> {
    let mut pattern: Vec<u16> = Vec::with_capacity(directory.as_os_str().len() + 7);
    let raw: Vec<u16> = directory.as_os_str().encode_wide().collect();
    // Verbatim-prefix long absolute paths: unlike std, the raw API does no
    // implicit conversion and would fail once a walk descends past MAX_PATH.
    let verbatim = raw.len() >= 240
        && !raw.starts_with(&[u16::from(b'\\'), u16::from(b'\\')])
        && raw.get(1) == Some(&u16::from(b':'));
    if verbatim {
        pattern.extend(r"\\?\".encode_utf16());
    }
    pattern.extend_from_slice(&raw);
    if !matches!(
        pattern.last(),
        Some(&last) if last == u16::from(b'\\') || last == u16::from(b'/')
    ) {
        pattern.push(u16::from(b'\\'));
    }
    pattern.push(u16::from(b'*'));
    pattern.push(0);

    let mut buffer: Box<Win32FindDataW> = Box::new(unsafe { std::mem::zeroed() });
    let handle = unsafe {
        FindFirstFileExW(
            pattern.as_ptr(),
            FIND_EX_INFO_BASIC,
            buffer.as_mut(),
            FIND_EX_SEARCH_NAME_MATCH,
            std::ptr::null_mut(),
            FIND_FIRST_EX_LARGE_FETCH,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    Ok(FindIterator {
        handle,
        buffer,
        first_pending: true,
        error_budget: FIND_ERROR_BUDGET,
        done: false,
    })
}

impl Iterator for FindIterator {
    type Item = Result<FindEntry, EnumerationError>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.done {
                return None;
            }
            if self.first_pending {
                self.first_pending = false;
            } else if unsafe { FindNextFileW(self.handle, self.buffer.as_mut()) } == 0 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() == Some(ERROR_NO_MORE_FILES) {
                    self.done = true;
                    return None;
                }
                // Transient mid-directory failure: report it and keep
                // enumerating, up to a budget that bounds a persistently
                // failing handle.
                if self.error_budget == 0 {
                    self.done = true;
                } else {
                    self.error_budget -= 1;
                }
                return Some(Err(EnumerationError::bare(error)));
            }

            let data = self.buffer.as_ref();
            let name_len = data
                .cFileName
                .iter()
                .position(|&unit| unit == 0)
                .unwrap_or(data.cFileName.len());
            let name_units = &data.cFileName[..name_len];
            if name_units == [u16::from(b'.')] || name_units == [u16::from(b'.'); 2] {
                continue;
            }

            return Some(Ok(FindEntry {
                name: OsString::from_wide(name_units),
                attributes: data.dwFileAttributes,
                size_bytes: (u64::from(data.nFileSizeHigh) << 32) | u64::from(data.nFileSizeLow),
                created_utc: filetime(data.ftCreationTime),
                modified_utc: filetime(data.ftLastWriteTime),
                accessed_utc: filetime(data.ftLastAccessTime),
            }));
        }
    }
}

impl Drop for FindIterator {
    fn drop(&mut self) {
        unsafe { FindClose(self.handle) };
    }
}

fn filetime(parts: [u32; 2]) -> u64 {
    (u64::from(parts[1]) << 32) | u64::from(parts[0])
}

/// Directory entry source shared by every walker: the raw large-fetch
/// enumeration when enabled (falling back to `fs::read_dir` if the open
/// fails), or `fs::read_dir` outright. Yields one shape either way so the
/// walk loop exists exactly once per walker.
pub enum DirEntries {
    Fast(FindIterator),
    // Boxed: fs::ReadDir is ~600 bytes vs the Fast variant's ~24, so an
    // unboxed enum would size every DirEntries to the larger variant.
    Std(Box<fs::ReadDir>),
}

/// Opens `directory` honoring the fast-enumeration preference.
pub fn read_dir_auto(directory: &Path, fast: bool) -> io::Result<DirEntries> {
    if fast {
        // Fall through to fs::read_dir on open failure so exotic paths keep
        // working; error dirs then fail (and report) through one path.
        if let Ok(entries) = read_dir_fast(directory) {
            return Ok(DirEntries::Fast(entries));
        }
    }
    fs::read_dir(directory).map(|entries| DirEntries::Std(Box::new(entries)))
}

impl Iterator for DirEntries {
    type Item = Result<FindEntry, EnumerationError>;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            DirEntries::Fast(entries) => entries.next(),
            DirEntries::Std(entries) => entries.next().map(|result| {
                use std::os::windows::fs::MetadataExt;
                let entry = result.map_err(EnumerationError::bare)?;
                // Free on Windows: DirEntry::metadata() is built from the
                // WIN32_FIND_DATAW the enumeration already cached. On failure
                // attach the entry's own path so the walker reports the file,
                // not the directory.
                let metadata = entry.metadata().map_err(|error| EnumerationError {
                    error,
                    path: Some(entry.path()),
                })?;
                Ok(FindEntry {
                    name: entry.file_name(),
                    attributes: metadata.file_attributes(),
                    size_bytes: metadata.len(),
                    created_utc: metadata.creation_time(),
                    modified_utc: metadata.last_write_time(),
                    accessed_utc: metadata.last_access_time(),
                })
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn collect_entries(
        entries: impl Iterator<Item = Result<FindEntry, EnumerationError>>,
    ) -> Vec<(String, u64, bool)> {
        let mut collected: Vec<(String, u64, bool)> = entries
            .map(|entry| {
                let entry = match entry {
                    Ok(entry) => entry,
                    Err(error) => panic!("entry: {}", error.error),
                };
                (
                    entry.name.to_string_lossy().into_owned(),
                    entry.size_bytes,
                    entry.attributes & 0x10 != 0,
                )
            })
            .collect();
        collected.sort();
        collected
    }

    #[test]
    fn raw_enumeration_matches_read_dir() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("winblaze-winfind-{unique}"));
        fs::create_dir_all(root.join("nested")).expect("create fixture");
        fs::write(root.join("a.txt"), vec![0u8; 1234]).expect("write a");
        fs::write(root.join("b.bin"), vec![0u8; 42]).expect("write b");

        let raw = collect_entries(read_dir_fast(&root).expect("open"));

        let mut std_entries: Vec<(String, u64, bool)> = fs::read_dir(&root)
            .expect("read_dir")
            .map(|entry| {
                let entry = entry.expect("entry");
                let metadata = entry.metadata().expect("metadata");
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    if metadata.is_dir() { 0 } else { metadata.len() },
                    metadata.is_dir(),
                )
            })
            .collect();
        std_entries.sort();

        assert_eq!(raw, std_entries);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn auto_source_yields_same_entries_fast_and_std() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("winblaze-winfind-auto-{unique}"));
        fs::create_dir_all(root.join("inner")).expect("create fixture");
        fs::write(root.join("c.dat"), vec![0u8; 77]).expect("write c");

        let fast = collect_entries(read_dir_auto(&root, true).expect("fast open"));
        let slow = collect_entries(read_dir_auto(&root, false).expect("std open"));
        assert_eq!(fast, slow);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_directory_errors_like_read_dir() {
        let missing = std::env::temp_dir().join("winblaze-winfind-missing-xyz");
        let error = match read_dir_fast(&missing) {
            Err(error) => error,
            Ok(_) => panic!("must fail"),
        };
        assert_eq!(
            error.kind(),
            fs::read_dir(&missing).expect_err("std must fail").kind()
        );
    }
}
