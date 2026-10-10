use std::collections::HashSet;
use winblaze_core::ScanEvent;
use winblaze_scanner::{ScanBackend, ScanController, ScanRequest, ScanRuntimeConfig};

#[test]
fn parallel_leaf_directories_complete_and_release_waiting_workers() {
    let temp = std::env::temp_dir();
    let root = temp.join(format!("winblaze-empty-leaves-{}", std::process::id()));
    for directory in 0..64 {
        std::fs::create_dir_all(root.join(format!("leaf-{directory}"))).unwrap();
    }
    let (controller, events) = ScanController::channel();
    let handle = controller.start_scan(ScanRequest {
        root_path: root.clone(),
        config: ScanRuntimeConfig {
            root_path: root.clone(),
            backend: ScanBackend::DirectoryWalk,
            max_parallelism: 4,
            ..ScanRuntimeConfig::default()
        },
    });
    let mut directories = 0;
    let mut completed = false;
    while !completed {
        let batch = events.recv_timeout(std::time::Duration::from_secs(10));
        if batch.is_err() {
            handle.cancel();
        }
        for event in batch.expect("empty leaf workers did not terminate") {
            match event {
                ScanEvent::DirectoryFound(_) => directories += 1,
                ScanEvent::Summary(summary) => {
                    assert_eq!((summary.files_seen, summary.directories_seen), (0, 65));
                    assert_eq!(
                        (summary.total_size_bytes, summary.total_allocation_bytes),
                        (0, 0)
                    );
                }
                ScanEvent::Completed => completed = true,
                ScanEvent::FileFound(_) | ScanEvent::Failed(_) | ScanEvent::Cancelled => {
                    panic!("unexpected leaf scan event")
                }
                _ => {}
            }
        }
    }
    handle.join();
    assert_eq!(directories, 65);
    assert!(root
        .canonicalize()
        .unwrap()
        .starts_with(temp.canonicalize().unwrap()));
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn parallel_empty_files_keep_counts_and_nonempty_byte_totals_exact() {
    let temp = std::env::temp_dir();
    let root = temp.join(format!("winblaze-empty-totals-{}", std::process::id()));
    for directory in 0..3 {
        let path = root.join(format!("directory-{directory}"));
        std::fs::create_dir_all(&path).unwrap();
        for file in 0..16 {
            std::fs::write(path.join(format!("empty-{file}")), []).unwrap();
        }
    }
    std::fs::write(root.join("payload"), b"12345678").unwrap();
    let (controller, events) = ScanController::channel();
    let handle = controller.start_scan(ScanRequest {
        root_path: root.clone(),
        config: ScanRuntimeConfig {
            root_path: root.clone(),
            backend: ScanBackend::DirectoryWalk,
            max_parallelism: 4,
            ..ScanRuntimeConfig::default()
        },
    });
    let mut ids = HashSet::new();
    let mut logical = 0;
    let mut allocated = 0;
    let mut directories = 0;
    let mut summary = None;
    let mut completed = false;
    for event in events.into_iter().flatten() {
        match event {
            ScanEvent::FileFound(file) => {
                assert!(ids.insert(file.id));
                logical += file.size_bytes;
                allocated += file.allocation_bytes;
            }
            ScanEvent::DirectoryFound(_) => directories += 1,
            ScanEvent::Summary(value) => summary = Some(value),
            ScanEvent::Completed => {
                completed = true;
                break;
            }
            ScanEvent::Failed(error) => panic!("scan failed: {error}"),
            ScanEvent::Issue(issue) => panic!("unexpected issue: {issue:?}"),
            _ => {}
        }
    }
    handle.join();
    assert!(completed);
    assert_eq!((ids.len(), directories, logical), (49, 4, 8));
    assert!(allocated > 0);
    let summary = summary.unwrap();
    assert_eq!((summary.files_seen, summary.directories_seen), (49, 4));
    assert_eq!(
        (summary.total_size_bytes, summary.total_allocation_bytes),
        (logical, allocated)
    );
    assert!(root
        .canonicalize()
        .unwrap()
        .starts_with(temp.canonicalize().unwrap()));
    std::fs::remove_dir_all(root).unwrap();
}
