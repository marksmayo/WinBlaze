use std::{
    fs,
    time::{SystemTime, UNIX_EPOCH},
};
use winblaze_scanner::{directory_metadata::read_directory, file_details::query_file_details};

#[test]
fn long_names_survive_multiple_directory_batches_and_reused_scratch() {
    let root = std::env::temp_dir().join(format!(
        "winblaze-multi-batch-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    let mut expected = std::collections::HashMap::new();
    for id in 0..512 {
        let name = format!("{id:04}-{}-Ω.dat", "x".repeat(210));
        let size = id % 17;
        fs::write(root.join(&name), vec![42; size]).unwrap();
        expected.insert(std::ffi::OsString::from(name), size as u64);
    }
    let mut identities = std::collections::HashSet::new();
    for entry in read_directory(&root).unwrap() {
        let entry = entry.unwrap();
        assert_eq!(expected.remove(&*entry.name), Some(entry.size_bytes));
        assert_ne!(entry.identity.1, 0);
        assert!(identities.insert(entry.identity));
        let details = query_file_details(&root.join(&entry.name)).unwrap();
        assert_eq!(entry.allocation_bytes, details.allocation_bytes);
        fs::remove_file(root.join(entry.name)).unwrap();
    }
    assert!(expected.is_empty());
    assert_eq!(identities.len(), 512);
    assert_eq!(read_directory(&root).unwrap().count(), 0);
    fs::remove_dir(root).unwrap();
}

#[test]
fn directory_metadata_matches_file_allocation_and_preserves_exact_names() {
    let root = std::env::temp_dir().join(format!(
        "winblaze-batched-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    for size in [0, 256, 1024, 5000, 131072] {
        fs::write(root.join(format!("unicode-Ω-{size}")), vec![42; size]).unwrap();
    }
    let entries = read_directory(&root)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(entries.len(), 5);
    for entry in entries {
        let path = root.join(&entry.name);
        assert_eq!(
            entry.allocation_bytes,
            query_file_details(&path).unwrap().allocation_bytes
        );
        assert_eq!(entry.size_bytes, fs::metadata(&path).unwrap().len());
        assert_ne!(entry.identity.1, 0);
        fs::remove_file(path).unwrap();
    }
    fs::remove_dir(root).unwrap();
}
#[test]
fn sparse_and_compressed_entries_match_windows_physical_allocation() {
    use std::{
        fs::OpenOptions,
        io::{Seek, SeekFrom, Write},
        os::windows::io::AsRawHandle,
    };
    #[link(name = "kernel32")]
    extern "system" {
        fn GetCompressedFileSizeW(path: *const u16, high: *mut u32) -> u32;
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
    let root = std::env::temp_dir().join(format!(
        "winblaze-special-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    for (name, code) in [("sparse", 0x900c4), ("compressed", 0x9c040)] {
        let mut file = OpenOptions::new()
            .create_new(true)
            .read(true)
            .write(true)
            .open(root.join(name))
            .unwrap();
        let compression = 1u16;
        let (input, size) = if name == "compressed" {
            ((&compression as *const u16).cast(), 2)
        } else {
            (std::ptr::null(), 0)
        };
        let mut returned = 0;
        // SAFETY: valid writable file handle and compression buffer; synchronous request.
        assert_ne!(
            unsafe {
                DeviceIoControl(
                    file.as_raw_handle(),
                    code,
                    input,
                    size,
                    std::ptr::null_mut(),
                    0,
                    &mut returned,
                    std::ptr::null_mut(),
                )
            },
            0,
            "{}",
            std::io::Error::last_os_error()
        );
        file.seek(SeekFrom::Start(1024 * 1024)).unwrap();
        file.write_all(&[1; 8192]).unwrap();
    }
    for entry in read_directory(&root).unwrap() {
        let entry = entry.unwrap();
        let path = root.join(&entry.name);
        let expected = query_file_details(&path).unwrap().allocation_bytes;
        assert!(expected < entry.size_bytes);
        use std::os::windows::ffi::OsStrExt;
        let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let mut high = 0;
        // SAFETY: NUL-terminated path and writable high DWORD for the OS oracle.
        let low = unsafe { GetCompressedFileSizeW(wide.as_ptr(), &mut high) };
        assert_ne!(low, u32::MAX);
        assert_eq!(
            entry.allocation_bytes,
            (u64::from(high) << 32) | u64::from(low)
        );
        assert_eq!(entry.allocation_bytes, expected, "{:?}", entry.name);
        fs::remove_file(path).unwrap();
    }
    fs::remove_dir(root).unwrap();
}
#[test]
fn directory_metadata_handles_empty_missing_and_long_exact_utf16_paths() {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    let root = std::env::temp_dir().join(format!(
        "winblaze-long-enum-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir(&root).unwrap();
    assert_eq!(read_directory(&root).unwrap().count(), 0);
    assert!(read_directory(&root.join("missing")).is_err());
    let mut directory = root.clone();
    for _ in 0..5 {
        directory = directory.join("directory-with-a-long-component-name-0123456789");
        fs::create_dir(&directory).unwrap();
    }
    let units = [b'x' as u16, 0xd800, b'z' as u16];
    let name = std::ffi::OsString::from_wide(&units);
    fs::write(directory.join(&name), [1u8; 5000]).unwrap();
    let entries = read_directory(&directory)
        .unwrap()
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name.encode_wide().collect::<Vec<_>>(), units);
    assert_eq!(
        entries[0].allocation_bytes,
        query_file_details(&directory.join(&name))
            .unwrap()
            .allocation_bytes
    );
    fs::remove_file(directory.join(name)).unwrap();
    while directory != root {
        fs::remove_dir(&directory).unwrap();
        directory.pop();
    }
    fs::remove_dir(root).unwrap();
}
