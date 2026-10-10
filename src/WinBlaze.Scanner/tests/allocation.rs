use std::{fs, time::Duration};
use winblaze_core::ScanEvent;
use winblaze_scanner::{ScanController, ScanRequest, ScanRuntimeConfig};

#[test]
fn walk_reports_real_allocation_and_counts_hardlinked_storage_once() {
    let root = std::env::temp_dir().join(format!("winblaze-allocation-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("data"), vec![7u8; 5000]).unwrap();
    fs::hard_link(root.join("data"), root.join("alias")).unwrap();
    let expected = winblaze_scanner::file_details::query_file_details(&root.join("data"))
        .unwrap()
        .allocation_bytes;
    assert!(expected >= 5000);
    let (controller, rx) = ScanController::channel();
    let handle = controller.start_scan(ScanRequest {
        root_path: root.clone(),
        config: ScanRuntimeConfig {
            root_path: root.clone(),
            ..Default::default()
        },
    });
    let mut physical = 0;
    let mut logical = 0;
    let mut files = 0;
    'events: loop {
        for event in rx.recv_timeout(Duration::from_secs(10)).unwrap() {
            match event {
                ScanEvent::FileFound(f) => {
                    files += 1;
                    physical += f.allocation_bytes;
                    logical += f.size_bytes;
                }
                ScanEvent::Completed => break 'events,
                _ => {}
            }
        }
    }
    handle.join();
    assert_eq!(files, 2);
    assert_eq!(logical, 10000);
    assert_eq!(physical, expected);
    fs::remove_file(root.join("data")).unwrap();
    fs::remove_file(root.join("alias")).unwrap();
    fs::remove_dir(root).unwrap();
}
#[test]
fn fast_and_fallback_walks_match_storage_with_hardlinks_across_directories() {
    let root =
        std::env::temp_dir().join(format!("winblaze-allocation-cross-{}", std::process::id()));
    fs::create_dir_all(root.join("a")).unwrap();
    fs::create_dir_all(root.join("b")).unwrap();
    fs::write(root.join("a/data"), vec![7u8; 5000]).unwrap();
    fs::hard_link(root.join("a/data"), root.join("b/alias")).unwrap();
    let expected = winblaze_scanner::file_details::query_file_details(&root.join("a/data"))
        .unwrap()
        .allocation_bytes;
    for (fast, workers) in [(false, 1), (false, 20), (true, 1), (true, 20)] {
        let (controller, rx) = ScanController::channel();
        let handle = controller.start_scan(ScanRequest {
            root_path: root.clone(),
            config: ScanRuntimeConfig {
                root_path: root.clone(),
                fast_enumeration: fast,
                max_parallelism: workers,
                directory_walk_max_parallelism: workers,
                ..Default::default()
            },
        });
        let (mut files, mut logical, mut physical) = (0, 0, 0);
        'events: loop {
            for event in rx.recv_timeout(Duration::from_secs(10)).unwrap() {
                match event {
                    ScanEvent::FileFound(file) => {
                        files += 1;
                        logical += file.size_bytes;
                        physical += file.allocation_bytes;
                    }
                    ScanEvent::Issue(issue) => panic!("unexpected issue: {issue:?}"),
                    ScanEvent::Completed => break 'events,
                    _ => {}
                }
            }
        }
        handle.join();
        assert_eq!((files, logical, physical), (2, 10000, expected));
    }
    fs::remove_file(root.join("a/data")).unwrap();
    fs::remove_file(root.join("b/alias")).unwrap();
    fs::remove_dir(root.join("a")).unwrap();
    fs::remove_dir(root.join("b")).unwrap();
    fs::remove_dir(root).unwrap();
}
