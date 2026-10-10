#![cfg(windows)]

use std::{fs, path::PathBuf, time::Duration};
use winblaze_core::ScanEvent;
use winblaze_scanner::{
    performance::ScanPipelineConfig, ScanBackend, ScanController, ScanRequest, ScanRuntimeConfig,
};

#[repr(C)]
struct ThreadEntry {
    size: u32,
    usage: u32,
    thread_id: u32,
    process_id: u32,
    base_priority: i32,
    delta_priority: i32,
    flags: u32,
}

#[allow(non_snake_case)]
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateToolhelp32Snapshot(flags: u32, process: u32) -> isize;
    fn Thread32First(snapshot: isize, entry: *mut ThreadEntry) -> i32;
    fn Thread32Next(snapshot: isize, entry: *mut ThreadEntry) -> i32;
    fn CloseHandle(handle: isize) -> i32;
}

fn process_threads() -> usize {
    // SAFETY: TH32CS_SNAPTHREAD takes no pointers and returns an owned handle.
    let snapshot = unsafe { CreateToolhelp32Snapshot(4, 0) };
    assert_ne!(snapshot, -1);
    struct Snapshot(isize);
    impl Drop for Snapshot {
        fn drop(&mut self) {
            // SAFETY: this guard exclusively owns a valid snapshot handle.
            unsafe { CloseHandle(self.0) };
        }
    }
    let _snapshot = Snapshot(snapshot);
    let mut entry = ThreadEntry {
        size: std::mem::size_of::<ThreadEntry>() as u32,
        usage: 0,
        thread_id: 0,
        process_id: 0,
        base_priority: 0,
        delta_priority: 0,
        flags: 0,
    };
    // SAFETY: entry has the documented THREADENTRY32 layout and live writable storage.
    let mut found = unsafe { Thread32First(snapshot, &mut entry) };
    assert_ne!(found, 0);
    let mut count = 0;
    while found != 0 {
        count += usize::from(entry.process_id == std::process::id());
        entry.size = std::mem::size_of::<ThreadEntry>() as u32;
        // SAFETY: the same owned snapshot and writable entry remain live.
        found = unsafe { Thread32Next(snapshot, &mut entry) };
    }
    count
}

#[test]
fn a_single_directory_chain_scans_completely_without_idle_worker_threads() {
    let root: PathBuf =
        std::env::temp_dir().join(format!("wb-worker-budget-{}", std::process::id()));
    let leaf = root.join("one").join("leaf");
    fs::create_dir_all(&leaf).unwrap();
    for index in 0..16 {
        fs::write(leaf.join(format!("file-{index}.txt")), b"abcd").unwrap();
    }
    let (controller, receiver) = ScanController::bounded_channel(1);
    let before = process_threads();
    let handle = controller.start_scan(ScanRequest {
        root_path: root.clone(),
        config: ScanRuntimeConfig {
            root_path: root.clone(),
            backend: ScanBackend::DirectoryWalk,
            max_parallelism: 4,
            directory_walk_max_parallelism: 4,
            pipeline: ScanPipelineConfig {
                batch_size: 1,
                ..Default::default()
            },
            ..Default::default()
        },
    });
    let mut files = Vec::new();
    while files.is_empty() {
        for event in receiver.recv_timeout(Duration::from_secs(5)).unwrap() {
            if let ScanEvent::FileFound(file) = event {
                files.push(file);
            }
        }
    }
    // A one-batch receiver holds traversal open while observing actual OS resources.
    // This measures a public scan, without hooks into its scheduler implementation.
    let during = process_threads();
    let mut completed = false;
    while !completed {
        for event in receiver.recv_timeout(Duration::from_secs(5)).unwrap() {
            match event {
                ScanEvent::FileFound(file) => files.push(file),
                ScanEvent::Completed => completed = true,
                ScanEvent::Failed(error) => panic!("scan failed: {error}"),
                _ => {}
            }
        }
    }
    handle.join();
    fs::remove_dir_all(root).unwrap();
    assert_eq!(files.len(), 16);
    assert_eq!(files.iter().map(|file| file.size_bytes).sum::<u64>(), 64);
    assert!(
        during <= before + 1,
        "A single chain needs only its scan thread: baseline {before}, observed {during}"
    );
}
