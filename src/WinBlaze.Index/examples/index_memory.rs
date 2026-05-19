use std::env;
use std::fs;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use winblaze_core::{DirectoryId, FileAttributes, FileId, FileRecord};
use winblaze_index::{
    BufferedIndexTransaction, IndexBackend, IndexRepository, IndexTransaction,
    SqliteIndexRepository,
};

fn main() {
    let files = env::args()
        .nth(1)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(10_000);
    let root = env::temp_dir().join(format!(
        "winblaze-index-memory-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or_default()
    ));
    fs::create_dir_all(&root).expect("create benchmark directory");

    let started = Instant::now();
    let mut repo = SqliteIndexRepository::open(&root, IndexBackend::BinaryCache);
    let mut tx = BufferedIndexTransaction::default();
    for id in 1..=files {
        tx.upsert_file(&FileRecord {
            id: FileId(id),
            parent_directory_id: DirectoryId(10),
            name: format!("file-{id:08}.bin"),
            full_path: format!("C:\\bench\\dir\\file-{id:08}.bin"),
            size_bytes: 0,
            allocation_bytes: 0,
            attributes: FileAttributes::ARCHIVE,
            created_utc: None,
            modified_utc: None,
            accessed_utc: None,
        });
    }
    repo.apply_transaction(&tx).expect("persist transaction");
    let elapsed_ms = started.elapsed().as_millis();
    let snapshot_path = root.join("winblaze.index.bin");
    let snapshot_bytes = fs::metadata(&snapshot_path)
        .map(|metadata| metadata.len())
        .unwrap_or_default();
    let working_set_bytes = current_working_set_bytes();
    let _ = fs::remove_dir_all(&root);

    println!(
        "{{\"files\":{files},\"elapsed_ms\":{elapsed_ms},\"working_set_bytes\":{working_set_bytes},\"working_set_bytes_per_file\":{},\"snapshot_bytes\":{snapshot_bytes},\"snapshot_bytes_per_file\":{}}}",
        if files == 0 { 0 } else { working_set_bytes / files },
        if files == 0 { 0 } else { snapshot_bytes / files }
    );
}

#[cfg(windows)]
fn current_working_set_bytes() -> u64 {
    #[repr(C)]
    struct ProcessMemoryCounters {
        cb: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "psapi")]
    extern "system" {
        fn GetProcessMemoryInfo(
            process: *mut core::ffi::c_void,
            counters: *mut ProcessMemoryCounters,
            size: u32,
        ) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn GetCurrentProcess() -> *mut core::ffi::c_void;
    }

    let mut counters = ProcessMemoryCounters {
        cb: std::mem::size_of::<ProcessMemoryCounters>() as u32,
        page_fault_count: 0,
        peak_working_set_size: 0,
        working_set_size: 0,
        quota_peak_paged_pool_usage: 0,
        quota_paged_pool_usage: 0,
        quota_peak_non_paged_pool_usage: 0,
        quota_non_paged_pool_usage: 0,
        pagefile_usage: 0,
        peak_pagefile_usage: 0,
    };

    let ok = unsafe {
        GetProcessMemoryInfo(
            GetCurrentProcess(),
            &mut counters,
            std::mem::size_of::<ProcessMemoryCounters>() as u32,
        )
    };
    if ok == 0 {
        0
    } else {
        counters.working_set_size as u64
    }
}

#[cfg(not(windows))]
fn current_working_set_bytes() -> u64 {
    0
}
