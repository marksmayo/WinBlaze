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

/// Facts shared by every directory on one volume. A non-reparse child
/// directory always lives on its parent's volume, so walkers query these once
/// per volume instead of once per directory.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VolumeContext {
    pub serial: u64,
    /// NTFS returns as many records as fit in every batch, so a batch that
    /// leaves room for a maximum-length record is the directory's last one.
    pub fills_batches: bool,
}

/// FILE_ID_EXTD_DIR_INFO header plus a 255-unit name, rounded to alignment.
const MAX_RECORD_BYTES: usize = 88 + 255 * 2 + 6;

pub struct DirectoryMetadataIterator {
    file: File,
    buffer: Vec<u64>,
    offset: usize,
    volume: VolumeContext,
    exhausted: bool,
    refill: bool,
    final_batch: bool,
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
    fn GetVolumeInformationByHandleW(
        handle: *mut c_void,
        volume_name: *mut u16,
        volume_name_size: u32,
        serial: *mut u32,
        max_component: *mut u32,
        flags: *mut u32,
        filesystem_name: *mut u16,
        filesystem_name_size: u32,
    ) -> i32;
}

const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

/// Cloud-files placeholders (OneDrive and other providers) use
/// IO_REPARSE_TAG_CLOUD or one of its CLOUD_1..CLOUD_F variants.
pub(crate) fn is_cloud_reparse_tag(tag: u32) -> bool {
    tag & 0xFFFF_0FFF == 0x9000_001A
}

/// Attributes as the walk should see them. A cloud placeholder folder is a
/// real folder whose files are held by the provider: unlike a junction or
/// symlink it aliases no other tree, so it is traversed like any folder.
/// Files keep the reparse bit so content features never hydrate them.
pub(crate) fn traversal_attributes(attributes: u32, reparse_tag: u32) -> u32 {
    let cloud_folder = attributes & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT)
        == FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT
        && is_cloud_reparse_tag(reparse_tag);
    if cloud_folder {
        attributes & !FILE_ATTRIBUTE_REPARSE_POINT
    } else {
        attributes
    }
}

pub fn read_directory(path: &Path) -> io::Result<DirectoryMetadataIterator> {
    read_directory_on(path, None)
}

/// Reads `path` on a volume already described by its parent directory, or
/// queries the volume when `volume` is `None` (roots and reparse targets).
pub fn read_directory_on(
    path: &Path,
    volume: Option<VolumeContext>,
) -> io::Result<DirectoryMetadataIterator> {
    let file = OpenOptions::new()
        .access_mode(1)
        .share_mode(7)
        .custom_flags(0x02000000)
        .open(path)?;
    let volume = match volume {
        Some(volume) => volume,
        None => query_volume(&file)?,
    };
    let mut entries = DirectoryMetadataIterator {
        file,
        buffer: DIRECTORY_BUFFER
            .with(|cached| cached.borrow_mut().take())
            .unwrap_or_else(|| vec![0; 2048]),
        offset: 0,
        volume,
        exhausted: false,
        refill: false,
        final_batch: false,
        metadata_reads: 0,
        compact_names: true,
    };
    // Most directories fit one small batch. Retain cached capacity, but avoid
    // requesting a large kernel enumeration buffer for every small directory.
    entries.buffer.truncate(2048);
    entries.fill(20)?;
    Ok(entries)
}

fn query_volume(file: &File) -> io::Result<VolumeContext> {
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
    let mut filesystem = [0u16; 16];
    // SAFETY: live handle; the filesystem-name buffer is writable and sized.
    // Optional outputs are null. Failure only disables the final-batch check.
    let named = unsafe {
        GetVolumeInformationByHandleW(
            file.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            filesystem.as_mut_ptr(),
            filesystem.len() as u32,
        )
    } != 0;
    let ntfs: Vec<u16> = "NTFS\0".encode_utf16().collect();
    Ok(VolumeContext {
        serial: id.volume,
        // A share reports its server's filesystem name ("NTFS") but the
        // redirector need not fill batches, so remote volumes never skip the
        // end-of-directory request.
        fills_batches: named && filesystem[..ntfs.len()] == ntfs[..] && !is_remote(file),
    })
}

/// Whether `file` was opened through a network redirector: FILE_REMOTE_PROTOCOL_INFO
/// (class 13) is only answered for remote handles.
fn is_remote(file: &File) -> bool {
    let mut info = [0u64; 16];
    // SAFETY: live handle; the buffer is writable and larger than the 116-byte
    // FILE_REMOTE_PROTOCOL_INFO the call fills.
    unsafe {
        GetFileInformationByHandleEx(file.as_raw_handle(), 13, info.as_mut_ptr().cast(), 116) != 0
    }
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

    /// The volume this directory is on, for its non-reparse children.
    pub fn volume(&self) -> VolumeContext {
        self.volume
    }

    /// End of the last record in a filled batch, or `None` if malformed.
    fn batch_end(&self) -> Option<usize> {
        let bytes_len = self.buffer.len() * 8;
        // SAFETY: viewing an initialized u64 buffer as bytes preserves its size.
        let bytes =
            unsafe { std::slice::from_raw_parts(self.buffer.as_ptr().cast::<u8>(), bytes_len) };
        let u32_at = |at: usize| {
            bytes
                .get(at..at + 4)
                .map(|b| u32::from_le_bytes(b.try_into().unwrap()) as usize)
        };
        let mut offset = 0;
        loop {
            let next = u32_at(offset)?;
            if next == 0 {
                return Some(offset + 88 + u32_at(offset + 60)?);
            }
            offset = offset.checked_add(next).filter(|&o| o < bytes_len)?;
        }
    }

    fn fill(&mut self, class: u32) -> io::Result<()> {
        self.metadata_reads += 1;
        let requested = self.buffer.len() * 8;
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
        // A filling filesystem stops a batch only when the next record does
        // not fit. Room for a maximum-length record means this batch ended the
        // directory, so the end-of-directory request can be skipped.
        self.final_batch = self.volume.fills_batches
            && self
                .batch_end()
                .is_some_and(|end| requested.saturating_sub(end) >= MAX_RECORD_BYTES);
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
            attributes: traversal_attributes(u32_at(56), u32_at(68)),
            size_bytes: (u64_at(40) as i64).max(0) as u64,
            allocation_bytes: (u64_at(48) as i64).max(0) as u64,
            identity: (
                self.volume.serial,
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
                if self.final_batch {
                    self.exhausted = true;
                    return None;
                }
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
    #[test]
    fn cloud_placeholder_folders_are_traversed_but_links_and_files_are_not() {
        use super::{is_cloud_reparse_tag, traversal_attributes};
        const DIR: u32 = 0x10;
        const REPARSE: u32 = 0x400;
        const CLOUD: u32 = 0x9000_001A;
        const CLOUD_3: u32 = 0x9000_301A;
        const JUNCTION: u32 = 0xA000_0003;
        const SYMLINK: u32 = 0xA000_000C;
        assert!(is_cloud_reparse_tag(CLOUD) && is_cloud_reparse_tag(CLOUD_3));
        assert!(!is_cloud_reparse_tag(JUNCTION) && !is_cloud_reparse_tag(SYMLINK));
        assert_eq!(traversal_attributes(DIR | REPARSE, CLOUD), DIR);
        assert_eq!(traversal_attributes(DIR | REPARSE, CLOUD_3), DIR);
        // Links still alias other trees; cloud files must never be hydrated.
        assert_eq!(traversal_attributes(DIR | REPARSE, JUNCTION), DIR | REPARSE);
        assert_eq!(traversal_attributes(DIR | REPARSE, SYMLINK), DIR | REPARSE);
        assert_eq!(traversal_attributes(0x20 | REPARSE, CLOUD), 0x20 | REPARSE);
        assert_eq!(traversal_attributes(DIR, 0), DIR);
    }
}
