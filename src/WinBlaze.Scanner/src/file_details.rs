//! Read-only Windows allocation and physical identity queries.
use std::fs::OpenOptions;
use std::io;
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;

#[derive(Debug)]
pub struct FileDetails {
    pub allocation_bytes: u64,
    pub identity: (u64, u128),
    pub links: u32,
}

#[repr(C)]
#[derive(Default)]
struct StandardInfo {
    allocation: i64,
    length: i64,
    links: u32,
    delete_pending: u8,
    directory: u8,
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
        handle: *mut core::ffi::c_void,
        class: u32,
        info: *mut core::ffi::c_void,
        size: u32,
    ) -> i32;
}

pub fn query_file_details(path: &Path) -> io::Result<FileDetails> {
    let file = OpenOptions::new()
        .access_mode(0)
        .share_mode(7)
        .custom_flags(0x02000000 | 0x00200000)
        .open(path)?;
    let mut standard = StandardInfo::default();
    let mut id = IdInfo::default();
    // SAFETY: the handle is live and both output buffers match the Win32 ABI.
    unsafe {
        if GetFileInformationByHandleEx(
            file.as_raw_handle(),
            1,
            (&mut standard as *mut StandardInfo).cast(),
            std::mem::size_of::<StandardInfo>() as u32,
        ) == 0
            || (standard.links > 1
                && GetFileInformationByHandleEx(
                    file.as_raw_handle(),
                    18,
                    (&mut id as *mut IdInfo).cast(),
                    std::mem::size_of::<IdInfo>() as u32,
                ) == 0)
        {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(FileDetails {
        allocation_bytes: standard.allocation.max(0) as u64,
        identity: (id.volume, u128::from_le_bytes(id.id)),
        links: standard.links,
    })
}

/// Queries physical identity even for a single-link file, without content access.
pub fn physical_identity(path: &Path) -> io::Result<(u64, u128)> {
    let file = OpenOptions::new()
        .access_mode(0)
        .share_mode(7)
        .custom_flags(0x02000000 | 0x00200000 | 0x00100000)
        .open(path)?;
    let mut id = IdInfo::default();
    // SAFETY: live metadata-only handle and ABI-compatible fixed-size output.
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
    Ok((id.volume, u128::from_le_bytes(id.id)))
}
