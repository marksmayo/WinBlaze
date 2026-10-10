use std::{
    fs,
    io::Write,
    time::{SystemTime, UNIX_EPOCH},
};
#[repr(C)]
#[derive(Default)]
struct IoCounters {
    read_ops: u64,
    write_ops: u64,
    other_ops: u64,
    read_bytes: u64,
    write_bytes: u64,
    other_bytes: u64,
}
#[link(name = "kernel32")]
extern "system" {
    fn GetCurrentProcess() -> *mut core::ffi::c_void;
    fn GetProcessIoCounters(process: *mut core::ffi::c_void, counters: *mut IoCounters) -> i32;
}
fn writes() -> u64 {
    let mut counters = IoCounters::default();
    // SAFETY: process pseudo-handle and correctly laid out writable IO_COUNTERS.
    assert_ne!(
        unsafe { GetProcessIoCounters(GetCurrentProcess(), &mut counters) },
        0
    );
    counters.write_ops
}
#[test]
fn large_atomic_exports_batch_disk_writes_and_preserve_the_complete_output() {
    let path = std::env::temp_dir().join(format!(
        "winblaze-output-budget-{}.csv",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let before = writes();
    winblaze_native::output::atomic_write(&path, |writer| {
        for _ in 0..524_288 {
            writer.write_all(&[b'x'; 64])?;
        }
        Ok(())
    })
    .unwrap();
    let disk_writes = writes() - before;
    let bytes = fs::read(&path).unwrap();
    fs::remove_file(&path).unwrap();
    assert_eq!(bytes.len(), 32 * 1024 * 1024);
    assert!(bytes.iter().all(|&byte| byte == b'x'));
    assert!(
        disk_writes < 128,
        "32 MiB export issued {disk_writes} disk writes"
    );
}
