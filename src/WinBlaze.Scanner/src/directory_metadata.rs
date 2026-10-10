//! Batched, read-only directory metadata from the supported Win32 API.
use std::{
    cell::RefCell,
    ffi::{c_void, OsStr, OsString},
    fs::{File, OpenOptions},
    io,
    os::windows::{ffi::OsStringExt, fs::OpenOptionsExt, io::AsRawHandle},
    path::Path,
};

thread_local! {
    // One idle buffer per scanner worker; active/nested iterators own theirs.
    static DIRECTORY_BUFFER: RefCell<Option<Vec<u64>>> = const { RefCell::new(None) };
}

/// Exact filesystem name; short ASCII names require no owned heap buffer.
pub enum EntryName {
    Short(winblaze_core::FileName),
    Original(OsString),
}

impl EntryName {
    fn from_wide(units: &[u16]) -> Self {
        if units.len() <= 22 && units.iter().all(|&unit| unit < 128) {
            let mut bytes = [0u8; 22];
            for (byte, &unit) in bytes.iter_mut().zip(units) {
                *byte = unit as u8;
            }
            Self::Short(
                std::str::from_utf8(&bytes[..units.len()])
                    .expect("ASCII name")
                    .into(),
            )
        } else {
            Self::Original(OsString::from_wide(units))
        }
    }

    pub(crate) fn into_display(self) -> winblaze_core::FileName {
        match self {
            Self::Short(name) => name,
            Self::Original(name) => name
                .into_string()
                .unwrap_or_else(|name| name.to_string_lossy().into_owned())
                .into(),
        }
    }
}

impl From<OsString> for EntryName {
    fn from(name: OsString) -> Self {
        Self::Original(name)
    }
}

impl std::ops::Deref for EntryName {
    type Target = OsStr;
    fn deref(&self) -> &OsStr {
        match self {
            Self::Short(name) => OsStr::new(name.as_ref()),
            Self::Original(name) => name,
        }
    }
}

impl AsRef<OsStr> for EntryName {
    fn as_ref(&self) -> &OsStr {
        self
    }
}

impl AsRef<Path> for EntryName {
    fn as_ref(&self) -> &Path {
        Path::new(&**self)
    }
}

impl std::fmt::Debug for EntryName {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&**self, formatter)
    }
}

impl PartialEq<&str> for EntryName {
    fn eq(&self, other: &&str) -> bool {
        &**self == OsStr::new(other)
    }
}

pub struct DirectoryEntry {
    pub name: EntryName,
    pub attributes: u32,
    pub size_bytes: u64,
    pub allocation_bytes: u64,
    pub identity: (u64, u128),
    pub created_utc: u64,
    pub modified_utc: u64,
    pub accessed_utc: u64,
}

pub struct DirectoryMetadataIterator {
    file: File,
    buffer: Vec<u64>,
    offset: usize,
    volume: u64,
    exhausted: bool,
    refill: bool,
    metadata_reads: usize,
    compact_names: bool,
}

#[repr(C)]
#[derive(Default)]
struct IdInfo {
    volume: u64,
    id: [u8; 16],
}
#[link(name = "kernel32")]
extern "system" {
    fn GetFileInformationByHandleEx(
        handle: *mut c_void,
        class: u32,
        info: *mut c_void,
        size: u32,
    ) -> i32;
}

pub fn read_directory(path: &Path) -> io::Result<DirectoryMetadataIterator> {
    let file = OpenOptions::new()
        .access_mode(1)
        .share_mode(7)
        .custom_flags(0x02000000)
        .open(path)?;
    let mut id = IdInfo::default();
    // SAFETY: live directory handle and ABI-compatible, writable FILE_ID_INFO.
    if unsafe {
        GetFileInformationByHandleEx(
            file.as_raw_handle(),
            18,
            (&mut id as *mut IdInfo).cast(),
            std::mem::size_of::<IdInfo>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let mut entries = DirectoryMetadataIterator {
        file,
        buffer: DIRECTORY_BUFFER
            .with(|cached| cached.borrow_mut().take())
            .unwrap_or_else(|| vec![0; 2048]),
        offset: 0,
        volume: id.volume,
        exhausted: false,
        refill: false,
        metadata_reads: 0,
        compact_names: true,
    };
    // Most directories fit one small batch. Retain cached capacity, but avoid
    // requesting a large kernel enumeration buffer for every small directory.
    entries.buffer.truncate(2048);
    entries.fill(20)?;
    Ok(entries)
}

impl Drop for DirectoryMetadataIterator {
    fn drop(&mut self) {
        let buffer = std::mem::take(&mut self.buffer);
        // Large dense-directory batches are active scratch, not idle storage.
        // Keep the established 32 KiB cache bound at terminal delivery.
        if buffer.capacity() > 4096 {
            return;
        }
        // During thread-local teardown the cache can already be unavailable.
        // Never retain more than one idle buffer, including nested iterators.
        let _ = DIRECTORY_BUFFER.try_with(|cached| {
            if let Ok(mut cached) = cached.try_borrow_mut() {
                if cached.is_none() {
                    *cached = Some(buffer);
                }
            }
        });
    }
}

impl DirectoryMetadataIterator {
    /// Number of directory metadata requests, including the EOF request.
    pub fn metadata_reads(&self) -> usize {
        self.metadata_reads
    }

    fn fill(&mut self, class: u32) -> io::Result<()> {
        self.metadata_reads += 1;
        // SAFETY: the buffer is writable and aligned to eight bytes as
        // required for FILE_ID_EXTD_DIR_INFO. The owned handle stays live.
        if unsafe {
            GetFileInformationByHandleEx(
                self.file.as_raw_handle(),
                class,
                self.buffer.as_mut_ptr().cast(),
                (self.buffer.len() * 8) as u32,
            )
        } == 0
        {
            let error = io::Error::last_os_error();
            if error.raw_os_error() == Some(18) {
                self.exhausted = true;
                return Ok(());
            }
            return Err(error);
        }
        // A successful continuation establishes that this directory needs
        // multiple batches. Preserve the returned records and use a larger
        // buffer on subsequent calls; an EOF continuation never grows it.
        let continuation_words = if self.compact_names { 8192 } else { 4096 };
        if class == 19 && self.buffer.len() < continuation_words {
            self.buffer.resize(continuation_words, 0);
        }
        self.offset = 0;
        self.refill = false;
        Ok(())
    }
    fn record(&mut self) -> io::Result<Option<DirectoryEntry>> {
        // SAFETY: viewing an initialized u64 buffer as bytes preserves its size
        // and lifetime; decoding below uses bounds checks, never struct casts.
        let bytes = unsafe {
            std::slice::from_raw_parts(self.buffer.as_ptr().cast::<u8>(), self.buffer.len() * 8)
        };
        let record = bytes
            .get(self.offset..)
            .filter(|r| r.len() >= 88)
            .ok_or_else(invalid_record)?;
        let u32_at = |at| u32::from_le_bytes(record[at..at + 4].try_into().unwrap());
        let u64_at = |at| u64::from_le_bytes(record[at..at + 8].try_into().unwrap());
        let next = u32_at(0) as usize;
        let length = u32_at(60) as usize;
        let name_bytes = record
            .get(88..88 + length)
            .filter(|_| {
                length.is_multiple_of(2)
                    && (next == 0 || (next >= 88 + length && next.is_multiple_of(8)))
            })
            .ok_or_else(invalid_record)?;
        // SAFETY: the u64-backed record is eight-byte aligned, its offset is
        // validated as a multiple of eight, and the name starts at byte 88.
        // Windows UTF-16 is little endian; length and bounds were checked above.
        let name_units =
            unsafe { std::slice::from_raw_parts(name_bytes.as_ptr().cast::<u16>(), length / 2) };
        self.compact_names &= name_units.len() <= 22 && name_units.iter().all(|&unit| unit < 128);
        // Dot records are structural entries, not results. Skip them before
        // allocating or converting a filename (two per traversed directory).
        let entry = (!matches!(name_units, [46] | [46, 46])).then(|| DirectoryEntry {
            name: EntryName::from_wide(name_units),
            attributes: u32_at(56),
            size_bytes: (u64_at(40) as i64).max(0) as u64,
            allocation_bytes: (u64_at(48) as i64).max(0) as u64,
            identity: (
                self.volume,
                u128::from_le_bytes(record[72..88].try_into().unwrap()),
            ),
            created_utc: u64_at(8),
            accessed_utc: u64_at(16),
            modified_utc: u64_at(24),
        });
        if next == 0 {
            self.refill = true;
        } else {
            self.offset = self
                .offset
                .checked_add(next)
                .filter(|&offset| offset < bytes.len())
                .ok_or_else(invalid_record)?;
        }
        Ok(entry)
    }
}
impl Iterator for DirectoryMetadataIterator {
    type Item = io::Result<DirectoryEntry>;
    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.exhausted {
                return None;
            }
            if self.refill {
                if let Err(error) = self.fill(19) {
                    self.exhausted = true;
                    return Some(Err(error));
                }
                if self.exhausted {
                    return None;
                }
            }
            match self.record() {
                Ok(None) => continue,
                Ok(Some(entry)) => return Some(Ok(entry)),
                Err(error) => {
                    self.exhausted = true;
                    return Some(Err(error));
                }
            }
        }
    }
}
fn invalid_record() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Invalid batched directory metadata record",
    )
}

#[cfg(test)]
mod tests {
    use super::EntryName;
    use std::os::windows::ffi::OsStrExt;

    #[test]
    fn compact_names_preserve_exact_windows_paths_and_lossy_display_contract() {
        for units in [
            "file-000-000-0000.bin".encode_utf16().collect::<Vec<_>>(),
            "日😀.txt".encode_utf16().collect(),
            "a-long-name-beyond-inline-capacity.txt"
                .encode_utf16()
                .collect(),
            vec![b'a' as u16, 0xd800, b'b' as u16],
        ] {
            let name = EntryName::from_wide(&units);
            assert_eq!(name.encode_wide().collect::<Vec<_>>(), units);
            let expected = String::from_utf16_lossy(&units);
            assert_eq!(name.into_display().as_ref(), expected);
        }
    }
}
