//! Raw directory enumeration over `FindFirstFileExW`.
//!
//! std's `read_dir` already requests `FindExInfoBasic` (skipping 8.3 alias
//! lookups) but not `FIND_FIRST_EX_LARGE_FETCH`, which batches directory
//! entries in ~64KB kernel round-trips instead of the small default. The raw
//! iterator also hands back attributes, sizes, and timestamps directly from
//! the `WIN32_FIND_DATAW` the enumeration already produced, so the walker
//! needs no per-entry `metadata()` call at all.

use std::ffi::c_void;
use std::io;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;

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

/// One directory entry, fully described by the enumeration itself.
pub struct FindEntry {
    pub name: String,
    pub attributes: u32,
    pub size_bytes: u64,
    /// FILETIME (100ns ticks since 1601); zero means "not available".
    pub created_utc: u64,
    pub modified_utc: u64,
    pub accessed_utc: u64,
}

pub struct FindIterator {
    handle: isize,
    pending: Option<Box<Win32FindDataW>>,
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

    let mut data: Box<Win32FindDataW> = Box::new(unsafe { std::mem::zeroed() });
    let handle = unsafe {
        FindFirstFileExW(
            pattern.as_ptr(),
            FIND_EX_INFO_BASIC,
            data.as_mut(),
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
        pending: Some(data),
        done: false,
    })
}

impl Iterator for FindIterator {
    type Item = io::Result<FindEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let data = match self.pending.take() {
                Some(data) => data,
                None => {
                    if self.done {
                        return None;
                    }
                    let mut data: Box<Win32FindDataW> = Box::new(unsafe { std::mem::zeroed() });
                    if unsafe { FindNextFileW(self.handle, data.as_mut()) } == 0 {
                        self.done = true;
                        let error = io::Error::last_os_error();
                        return if error.raw_os_error() == Some(ERROR_NO_MORE_FILES) {
                            None
                        } else {
                            Some(Err(error))
                        };
                    }
                    data
                }
            };

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
                name: String::from_utf16_lossy(name_units),
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

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

        let mut raw: Vec<(String, u64, bool)> = read_dir_fast(&root)
            .expect("open")
            .map(|entry| {
                let entry = entry.expect("entry");
                (
                    entry.name.clone(),
                    entry.size_bytes,
                    entry.attributes & 0x10 != 0,
                )
            })
            .collect();
        raw.sort();

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
