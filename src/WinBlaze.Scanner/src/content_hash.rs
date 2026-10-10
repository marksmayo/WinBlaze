//! Streaming content digests through the Windows CNG provider.
use std::{
    ffi::c_void,
    io::{self, Read},
    path::Path,
    ptr::null_mut,
};
type Handle = *mut c_void;
#[link(name = "bcrypt")]
extern "system" {
    fn BCryptOpenAlgorithmProvider(
        out: *mut Handle,
        name: *const u16,
        implementation: *const u16,
        flags: u32,
    ) -> i32;
    fn BCryptCloseAlgorithmProvider(handle: Handle, flags: u32) -> i32;
    fn BCryptGetProperty(
        handle: Handle,
        name: *const u16,
        output: *mut u8,
        size: u32,
        returned: *mut u32,
        flags: u32,
    ) -> i32;
    fn BCryptCreateHash(
        provider: Handle,
        hash: *mut Handle,
        object: *mut u8,
        object_bytes: u32,
        secret: *mut u8,
        secret_bytes: u32,
        flags: u32,
    ) -> i32;
    fn BCryptHashData(hash: Handle, data: *const u8, bytes: u32, flags: u32) -> i32;
    fn BCryptFinishHash(hash: Handle, output: *mut u8, bytes: u32, flags: u32) -> i32;
    fn BCryptDestroyHash(hash: Handle) -> i32;
}
fn checked(status: i32) -> io::Result<()> {
    if status == 0 {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "Windows hash provider status {status:#x}"
        )))
    }
}
struct Digest {
    provider: Handle,
    hash: Handle,
    object: Vec<u8>,
    output_bytes: u32,
}
impl Drop for Digest {
    fn drop(&mut self) {
        // SAFETY: owned live CNG handles; the object buffer outlives hash destruction.
        unsafe {
            if !self.hash.is_null() {
                BCryptDestroyHash(self.hash);
            }
            if !self.provider.is_null() {
                BCryptCloseAlgorithmProvider(self.provider, 0);
            }
        }
    }
}
impl Digest {
    fn new(algorithm: &str) -> io::Result<Self> {
        let name: Vec<u16> = algorithm.encode_utf16().chain(Some(0)).collect();
        let mut digest = Self {
            provider: null_mut(),
            hash: null_mut(),
            object: Vec::new(),
            output_bytes: 0,
        };
        // SAFETY: valid UTF-16 algorithm name, output handle and property buffers.
        unsafe {
            checked(BCryptOpenAlgorithmProvider(
                &mut digest.provider,
                name.as_ptr(),
                null_mut(),
                0,
            ))?;
            let property = |name: &str| -> io::Result<u32> {
                let name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
                let mut value = 0_u32;
                let mut returned = 0;
                checked(BCryptGetProperty(
                    digest.provider,
                    name.as_ptr(),
                    (&mut value as *mut u32).cast(),
                    4,
                    &mut returned,
                    0,
                ))?;
                if returned != 4 {
                    return Err(io::Error::other("Invalid CNG property length"));
                }
                Ok(value)
            };
            let object_bytes = property("ObjectLength")?;
            digest.output_bytes = property("HashDigestLength")?;
            if object_bytes > 65536 || digest.output_bytes > 128 {
                return Err(io::Error::other("Invalid CNG buffer dimensions"));
            }
            digest.object.resize(object_bytes as usize, 0);
            checked(BCryptCreateHash(
                digest.provider,
                &mut digest.hash,
                digest.object.as_mut_ptr(),
                object_bytes,
                null_mut(),
                0,
                0,
            ))?;
        }
        Ok(digest)
    }
    fn update(&mut self, bytes: &[u8]) -> io::Result<()> {
        // SAFETY: live hash and borrowed readable input, bounded to a 64KiB block.
        checked(unsafe { BCryptHashData(self.hash, bytes.as_ptr(), bytes.len() as u32, 0) })
    }
    fn finish(self) -> io::Result<String> {
        let mut bytes = vec![0_u8; self.output_bytes as usize];
        // SAFETY: live hash and a correctly sized writable digest buffer.
        checked(unsafe { BCryptFinishHash(self.hash, bytes.as_mut_ptr(), self.output_bytes, 0) })?;
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut text = String::with_capacity(bytes.len() * 2);
        for byte in bytes {
            text.push(HEX[(byte >> 4) as usize] as char);
            text.push(HEX[(byte & 15) as usize] as char);
        }
        Ok(text)
    }
}
pub(crate) fn file_digest(path: &Path, algorithm: &str) -> io::Result<String> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || crate::content::offline_content(&metadata)
    {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "Skipped offline/cloud or reparse content",
        ));
    }
    let mut input = crate::content::open_local_file(path)?;
    let before = input.metadata()?;
    let mut digest = Digest::new(algorithm)?;
    let mut buffer = [0_u8; 65536];
    loop {
        let bytes = input.read(&mut buffer)?;
        if bytes == 0 {
            break;
        }
        digest.update(&buffer[..bytes])?;
    }
    let after = input.metadata()?;
    if before.len() != after.len() || before.modified()? != after.modified()? {
        return Err(io::Error::other("File changed during content hashing"));
    }
    digest.finish()
}
