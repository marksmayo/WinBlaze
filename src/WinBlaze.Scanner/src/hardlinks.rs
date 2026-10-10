//! Explicit duplicate consolidation. The chosen master supplies shared metadata.
use std::{
    ffi::c_void,
    fs::{File, OpenOptions},
    io::{self, Read},
    os::windows::{
        ffi::OsStrExt,
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::Path,
    sync::atomic::{AtomicU64, Ordering},
};
#[repr(C)]
struct RenameInfo {
    replace: u32,
    root: *mut c_void,
    length: u32,
    name: [u16; 1],
}
#[repr(C)]
struct StreamInfo {
    size: i64,
    name: [u16; 296],
}
#[link(name = "kernel32")]
extern "system" {
    fn SetFileInformationByHandle(
        handle: *mut c_void,
        class: u32,
        info: *const c_void,
        size: u32,
    ) -> i32;
    fn FindFirstStreamW(
        path: *const u16,
        level: u32,
        info: *mut StreamInfo,
        flags: u32,
    ) -> *mut c_void;
    fn FindNextStreamW(handle: *mut c_void, info: *mut StreamInfo) -> i32;
    fn FindClose(handle: isize) -> i32;
}
fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn open_guard(path: &Path, delete: bool) -> io::Result<File> {
    let file = OpenOptions::new()
        .access_mode(0x80000000 | if delete { 0x10000 } else { 0 })
        .share_mode(1) // Readers only: no concurrent content writes or path replacement.
        .custom_flags(0x00200000 | 0x00100000)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.file_attributes() & (0x1 | 0x4 | 0x400 | 0x1000 | 0x4000 | 0x40000 | 0x400000)
            != 0
    {
        return Err(invalid(
            "Requires ordinary writable, locally available files",
        ));
    }
    Ok(file)
}
fn no_named_streams(path: &Path) -> io::Result<()> {
    let wide: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut info = StreamInfo {
        size: 0,
        name: [0; 296],
    };
    // SAFETY: terminated path and ABI-compatible output buffer.
    let handle = unsafe { FindFirstStreamW(wide.as_ptr(), 0, &mut info, 0) };
    if handle as isize == -1 {
        return Err(io::Error::last_os_error());
    }
    let result = loop {
        let length = info
            .name
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(info.name.len());
        if String::from_utf16_lossy(&info.name[..length]) != "::$DATA" {
            break Err(invalid(
                "Named streams must be removed or handled separately",
            ));
        }
        // SAFETY: live enumeration handle and output buffer.
        if unsafe { FindNextStreamW(handle, &mut info) } == 0 {
            let error = io::Error::last_os_error();
            break if error.raw_os_error() == Some(38) {
                Ok(())
            } else {
                Err(error)
            };
        }
    };
    // SAFETY: this function owns the enumeration handle.
    unsafe {
        FindClose(handle as isize);
    }
    result
}
fn rename_handle(file: &File, path: &Path) -> io::Result<()> {
    let name: Vec<u16> = path.as_os_str().encode_wide().collect();
    let offset = std::mem::offset_of!(RenameInfo, name);
    let bytes = offset + name.len() * 2;
    if bytes > u32::MAX as usize || name.contains(&0) {
        return Err(invalid("Invalid backup path"));
    }
    let mut storage = vec![0u64; bytes.div_ceil(8)];
    // SAFETY: aligned storage covers the flexible Win32 rename structure;
    // absolute UTF-16 destination and live DELETE-capable handle. Never overwrite.
    let ok = unsafe {
        let info = storage.as_mut_ptr().cast::<RenameInfo>();
        (*info).length = (name.len() * 2) as u32;
        std::ptr::copy_nonoverlapping(
            name.as_ptr(),
            storage.as_mut_ptr().cast::<u8>().add(offset).cast(),
            name.len(),
        );
        SetFileInformationByHandle(
            file.as_raw_handle(),
            3,
            storage.as_ptr().cast(),
            bytes as u32,
        )
    };
    if ok == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
/// The master remains write/rename-locked while a byte-verified duplicate is
/// renamed by its own DELETE-capable handle. A failed link restores that handle
/// without overwriting an unexpected destination. Metadata/ACLs are shared with
/// the master; differing owner/group/DACL or named streams are refused.
pub fn consolidate(master: &Path, duplicate: &Path) -> io::Result<()> {
    for path in [master, duplicate] {
        let metadata = std::fs::symlink_metadata(path)?;
        if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 {
            return Err(invalid(
                "Reparse points and directories cannot be consolidated",
            ));
        }
    }
    let master = std::fs::canonicalize(master)?;
    let duplicate = std::fs::canonicalize(duplicate)?;
    if master == duplicate {
        return Err(invalid("Choose two different paths"));
    }
    let mut source = open_guard(&master, false)?;
    let mut target = open_guard(&duplicate, true)?;
    let before_source = source.metadata()?;
    let before_target = target.metadata()?;
    if before_source.len() != before_target.len() {
        return Err(invalid("Files differ"));
    }
    no_named_streams(&master)?;
    no_named_streams(&duplicate)?;
    if crate::permissions::security_descriptor(&master)?
        != crate::permissions::security_descriptor(&duplicate)?
    {
        return Err(invalid("Owner, group or permissions differ"));
    }
    let mut left = [0u8; 65536];
    let mut right = [0u8; 65536];
    let mut remaining = before_source.len();
    while remaining > 0 {
        let count = remaining.min(left.len() as u64) as usize;
        source.read_exact(&mut left[..count])?;
        target.read_exact(&mut right[..count])?;
        if left[..count] != right[..count] {
            return Err(invalid("Files differ"));
        }
        remaining -= count as u64;
    }
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let backup = duplicate.with_file_name(format!(
        ".winblaze-link-backup-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    rename_handle(&target, &backup)?;
    if let Err(error) = std::fs::hard_link(&master, &duplicate) {
        return match rename_handle(&target, &duplicate) {
            Ok(()) => Err(error),
            Err(rollback) => Err(io::Error::other(format!(
                "Link failed: {error}; rollback failed: {rollback}. Original retained at {}",
                backup.display()
            ))),
        };
    }
    let delete = 1u32;
    // SAFETY: owned original-file handle with DELETE access; the new alias exists.
    if unsafe {
        SetFileInformationByHandle(target.as_raw_handle(), 4, (&delete as *const u32).cast(), 4)
    } == 0
    {
        return Err(io::Error::other(format!(
            "Link created, but original backup could not be deleted: {} ({})",
            backup.display(),
            io::Error::last_os_error()
        )));
    }
    Ok(())
}
