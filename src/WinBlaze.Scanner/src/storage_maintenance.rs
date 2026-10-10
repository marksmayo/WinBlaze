//! Explicit reversible NTFS maintenance on ordinary, locally available files.
use std::os::windows::{
    fs::{MetadataExt, OpenOptionsExt},
    io::AsRawHandle,
};
use std::{
    fs::{File, OpenOptions},
    io,
    path::Path,
};

#[link(name = "kernel32")]
extern "system" {
    fn DeviceIoControl(
        handle: *mut core::ffi::c_void,
        code: u32,
        input: *const core::ffi::c_void,
        input_size: u32,
        output: *mut core::ffi::c_void,
        output_size: u32,
        returned: *mut u32,
        overlapped: *mut core::ffi::c_void,
    ) -> i32;
}
fn open_local_file(path: &Path) -> io::Result<File> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file()
        || metadata.file_attributes() & (0x400 | 0x1000 | 0x40000 | 0x400000) != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Maintenance requires an ordinary local file",
        ));
    }
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .share_mode(0)
        .custom_flags(0x00200000 | 0x00100000)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.file_attributes() & (0x400 | 0x1000 | 0x40000 | 0x400000) != 0
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "File became unavailable",
        ));
    }
    Ok(file)
}
fn control(file: &File, code: u32, input: &[u8]) -> io::Result<()> {
    let mut returned = 0;
    // SAFETY: live synchronous file handle; input is valid for this call.
    if unsafe {
        DeviceIoControl(
            file.as_raw_handle(),
            code,
            input.as_ptr().cast(),
            input.len() as u32,
            std::ptr::null_mut(),
            0,
            &mut returned,
            std::ptr::null_mut(),
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
/// Changes NTFS compression without changing content. An exclusive handle
/// prevents simultaneous writes; the file must be locally available.
pub fn set_compression(path: &Path, enabled: bool) -> io::Result<()> {
    let file = open_local_file(path)?;
    let format = if enabled { 1u16 } else { 0u16 };
    control(&file, 0x9c040, &format.to_le_bytes())
}

/// Reclaims fully zero 64KiB ranges. An exclusive read/write handle prevents
/// concurrent writes between byte verification and the NTFS hole operation.
pub fn sparsify_zeros(path: &Path) -> io::Result<()> {
    use std::io::Read;
    let mut file = open_local_file(path)?;
    if file.metadata()?.file_attributes() & 0x800 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Decompress the file before sparsifying",
        ));
    }
    control(&file, 0x900c4, &[])?;
    let mut buffer = vec![0u8; 64 * 1024];
    let mut offset = 0i64;
    let mut zero_start = None;
    loop {
        let mut count = 0;
        while count < buffer.len() {
            let read = file.read(&mut buffer[count..])?;
            if read == 0 {
                break;
            }
            count += read;
        }
        let zero = count == buffer.len() && buffer.iter().all(|byte| *byte == 0);
        if zero && zero_start.is_none() {
            zero_start = Some(offset);
        }
        if !zero {
            if let Some(start) = zero_start.take() {
                let mut range = [0u8; 16];
                range[..8].copy_from_slice(&start.to_le_bytes());
                range[8..].copy_from_slice(&offset.to_le_bytes());
                control(&file, 0x980c8, &range)?;
            }
        }
        offset = offset
            .checked_add(count as i64)
            .ok_or_else(|| io::Error::other("File offset overflow"))?;
        if count < buffer.len() {
            break;
        }
    }
    Ok(())
}
