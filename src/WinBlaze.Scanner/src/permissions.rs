//! Owner, group and discretionary ACL in lossless Windows SDDL notation.
use std::{ffi::c_void, io, os::windows::ffi::OsStrExt, path::Path, ptr::null_mut};
#[link(name = "advapi32")]
extern "system" {
    fn GetNamedSecurityInfoW(
        name: *mut u16,
        kind: u32,
        information: u32,
        owner: *mut *mut c_void,
        group: *mut *mut c_void,
        dacl: *mut *mut c_void,
        sacl: *mut *mut c_void,
        descriptor: *mut *mut c_void,
    ) -> u32;
    fn ConvertSecurityDescriptorToStringSecurityDescriptorW(
        descriptor: *mut c_void,
        revision: u32,
        information: u32,
        text: *mut *mut u16,
        length: *mut u32,
    ) -> i32;
}
#[link(name = "kernel32")]
extern "system" {
    fn LocalFree(memory: *mut c_void) -> *mut c_void;
}

pub fn security_descriptor(path: &Path) -> io::Result<String> {
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    if wide[..wide.len() - 1].contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Path contains NUL",
        ));
    }
    let mut descriptor = null_mut();
    let mut text = null_mut();
    let mut length = 0;
    // SAFETY: all Win32 pointers refer to live buffers. The two LocalAlloc
    // output buffers are freed on both successful and failed conversions.
    unsafe {
        let code = GetNamedSecurityInfoW(
            wide.as_mut_ptr(),
            1,
            7,
            null_mut(),
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        );
        if code != 0 {
            return Err(io::Error::from_raw_os_error(code as i32));
        }
        let ok = ConvertSecurityDescriptorToStringSecurityDescriptorW(
            descriptor,
            1,
            7,
            &mut text,
            &mut length,
        );
        let error = io::Error::last_os_error();
        let result = if ok != 0 && !text.is_null() {
            let units = std::slice::from_raw_parts(text, length as usize);
            let length = units
                .iter()
                .position(|&unit| unit == 0)
                .unwrap_or(units.len());
            Ok(String::from_utf16_lossy(&units[..length]))
        } else {
            Err(error)
        };
        if !text.is_null() {
            LocalFree(text.cast());
        }
        LocalFree(descriptor);
        result
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PermissionRow {
    pub kind: String,
    pub account: String,
    pub sid: String,
    pub rights: String,
    pub mask: u32,
    pub inherited: bool,
}
#[link(name = "advapi32")]
extern "system" {
    fn ConvertStringSecurityDescriptorToSecurityDescriptorW(
        text: *const u16,
        revision: u32,
        descriptor: *mut *mut c_void,
        size: *mut u32,
    ) -> i32;
    fn GetSecurityDescriptorOwner(
        descriptor: *mut c_void,
        sid: *mut *mut c_void,
        defaulted: *mut i32,
    ) -> i32;
    fn GetSecurityDescriptorGroup(
        descriptor: *mut c_void,
        sid: *mut *mut c_void,
        defaulted: *mut i32,
    ) -> i32;
    fn GetSecurityDescriptorDacl(
        descriptor: *mut c_void,
        present: *mut i32,
        acl: *mut *mut c_void,
        defaulted: *mut i32,
    ) -> i32;
    fn ConvertSidToStringSidW(sid: *mut c_void, text: *mut *mut u16) -> i32;
    fn LookupAccountSidW(
        system: *const u16,
        sid: *mut c_void,
        name: *mut u16,
        name_size: *mut u32,
        domain: *mut u16,
        domain_size: *mut u32,
        kind: *mut u32,
    ) -> i32;
    fn GetAce(acl: *mut c_void, index: u32, ace: *mut *mut c_void) -> i32;
}
struct LocalAllocation(*mut c_void);
impl Drop for LocalAllocation {
    fn drop(&mut self) {
        // SAFETY: this guard owns a LocalAlloc allocation returned by Win32.
        unsafe {
            LocalFree(self.0);
        }
    }
}
fn check(ok: i32) -> io::Result<()> {
    if ok != 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
fn sid_account(sid: *mut c_void) -> io::Result<(String, String)> {
    if sid.is_null() {
        return Ok((String::new(), String::new()));
    }
    // SAFETY: SID belongs to a validated security descriptor alive during this call;
    // all string buffers and returned allocations are scoped to this function.
    unsafe {
        let mut text = null_mut();
        check(ConvertSidToStringSidW(sid, &mut text))?;
        let allocation = LocalAllocation(text.cast());
        let mut length = 0;
        while *text.add(length) != 0 {
            length += 1;
        }
        let sid_text = String::from_utf16_lossy(std::slice::from_raw_parts(text, length));
        drop(allocation);
        let (mut name_size, mut domain_size, mut kind) = (0, 0, 0);
        LookupAccountSidW(
            std::ptr::null(),
            sid,
            null_mut(),
            &mut name_size,
            null_mut(),
            &mut domain_size,
            &mut kind,
        );
        if name_size == 0 || name_size > 32768 || domain_size > 32768 {
            return Ok((sid_text.clone(), sid_text));
        }
        let mut name = vec![0u16; name_size as usize];
        let mut domain = vec![0u16; domain_size as usize];
        if LookupAccountSidW(
            std::ptr::null(),
            sid,
            name.as_mut_ptr(),
            &mut name_size,
            domain.as_mut_ptr(),
            &mut domain_size,
            &mut kind,
        ) == 0
        {
            return Ok((sid_text.clone(), sid_text));
        }
        let name = String::from_utf16_lossy(&name[..name_size as usize]);
        let domain = String::from_utf16_lossy(&domain[..domain_size as usize]);
        Ok((
            if domain.is_empty() {
                name
            } else {
                format!("{domain}\\{name}")
            },
            sid_text,
        ))
    }
}
fn readable_rights(mask: u32) -> String {
    if mask & 0x001f01ff == 0x001f01ff || mask & 0x10000000 != 0 {
        return "Full control".into();
    }
    let mut rights = Vec::new();
    for (bit, label) in [
        (0x80000000, "Generic read"),
        (0x40000000, "Generic write"),
        (0x20000000, "Generic execute"),
        (1, "Read data/list folder"),
        (2, "Write data/add files"),
        (4, "Append/add folders"),
        (8, "Read extended attributes"),
        (16, "Write extended attributes"),
        (32, "Execute/traverse"),
        (64, "Delete child"),
        (128, "Read attributes"),
        (256, "Write attributes"),
        (0x10000, "Delete"),
        (0x20000, "Read permissions"),
        (0x40000, "Change permissions"),
        (0x80000, "Take ownership"),
        (0x100000, "Synchronize"),
    ] {
        if mask & bit != 0 {
            rights.push(label);
        }
    }
    if rights.is_empty() {
        format!("Mask {mask:#010x}")
    } else {
        format!("{} ({mask:#010x})", rights.join(", "))
    }
}

/// Lists owner/group and individual DACL entries with resolved account names.
/// This is an ACE listing, not an effective-access calculation.
pub fn permission_rows(path: &Path) -> io::Result<Vec<PermissionRow>> {
    permission_rows_from_sddl(&security_descriptor(path)?)
}

/// Reads a saved descriptor into account/rights rows without accessing a file.
pub fn permission_rows_from_sddl(sddl: &str) -> io::Result<Vec<PermissionRow>> {
    if sddl.contains('\0') {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "SDDL contains NUL",
        ));
    }
    let text: Vec<_> = sddl.encode_utf16().chain(Some(0)).collect();
    let mut descriptor = null_mut();
    // SAFETY: Win32 validates the SDDL, returns an owned descriptor and ACE
    // pointers into it. The allocation guard outlives all SID/ACE reads.
    unsafe {
        check(ConvertStringSecurityDescriptorToSecurityDescriptorW(
            text.as_ptr(),
            1,
            &mut descriptor,
            null_mut(),
        ))?;
        let _allocation = LocalAllocation(descriptor);
        let mut rows = Vec::new();
        for (kind, owner) in [("Owner", true), ("Group", false)] {
            let mut sid = null_mut();
            let mut defaulted = 0;
            check(if owner {
                GetSecurityDescriptorOwner(descriptor, &mut sid, &mut defaulted)
            } else {
                GetSecurityDescriptorGroup(descriptor, &mut sid, &mut defaulted)
            })?;
            let (account, sid) = sid_account(sid)?;
            rows.push(PermissionRow {
                kind: kind.into(),
                account,
                sid,
                rights: String::new(),
                mask: 0,
                inherited: false,
            });
        }
        let mut acl = null_mut();
        let mut present = 0;
        let mut defaulted = 0;
        check(GetSecurityDescriptorDacl(
            descriptor,
            &mut present,
            &mut acl,
            &mut defaulted,
        ))?;
        if acl.is_null() || present == 0 {
            rows.push(PermissionRow {
                kind: "Allow".into(),
                account: "Everyone".into(),
                sid: "S-1-1-0".into(),
                rights: "Full control (no discretionary ACL)".into(),
                mask: 0x1f01ff,
                inherited: false,
            });
            return Ok(rows);
        }
        // ACL header: revision/reserved, size, ACE count, reserved.
        let count = std::ptr::read_unaligned(acl.cast::<u8>().add(4).cast::<u16>());
        for index in 0..count {
            let mut ace = null_mut();
            check(GetAce(acl, index as u32, &mut ace))?;
            let header = std::slice::from_raw_parts(ace.cast::<u8>(), 4);
            let size = u16::from_le_bytes([header[2], header[3]]) as usize;
            if size < 8 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Invalid ACE length",
                ));
            }
            let bytes = std::slice::from_raw_parts(ace.cast::<u8>(), size);
            let mask = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
            let (mut offset, mut supported) = (8, true);
            if matches!(header[0], 5 | 6 | 11 | 12) {
                if size < 12 {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Invalid object ACE",
                    ));
                }
                let flags = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
                offset = 12 + usize::from(flags & 1 != 0) * 16 + usize::from(flags & 2 != 0) * 16;
            } else if !matches!(header[0], 0 | 1 | 9 | 10) {
                supported = false;
            }
            let (account, sid) = if supported && size >= offset + 8 {
                sid_account(ace.cast::<u8>().add(offset).cast())?
            } else {
                (format!("ACE type {}", header[0]), String::new())
            };
            rows.push(PermissionRow {
                kind: match header[0] {
                    0 | 5 | 9 | 11 => "Allow",
                    1 | 6 | 10 | 12 => "Deny",
                    _ => "Other",
                }
                .into(),
                account,
                sid,
                rights: readable_rights(mask),
                mask,
                inherited: header[1] & 0x10 != 0,
            });
        }
        if count == 0 {
            rows.push(PermissionRow {
                kind: "Information".into(),
                account: String::new(),
                sid: String::new(),
                rights: "Empty DACL: no access grants".into(),
                mask: 0,
                inherited: false,
            });
        }
        Ok(rows)
    }
}
