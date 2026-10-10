use winblaze_core::ScanEvent;
use winblaze_scanner::{ScanBackend, ScanController, ScanRequest, ScanRuntimeConfig};

#[test]
fn published_directory_paths_preserve_hierarchy_without_retaining_formatting_slack() {
    let root =
        std::env::temp_dir().join(format!("wb-directory-path-budget-{}", std::process::id()));
    let child = root.join("long-directory-name-for-allocation-budget");
    std::fs::create_dir_all(&child).unwrap();
    std::fs::write(child.join("leaf.bin"), b"contents").unwrap();
    let (controller, rx) = ScanController::channel();
    let handle = controller.start_scan(ScanRequest {
        root_path: root.clone(),
        config: ScanRuntimeConfig {
            root_path: root.clone(),
            backend: ScanBackend::DirectoryWalk,
            ..ScanRuntimeConfig::default()
        },
    });
    let mut directories = Vec::new();
    let mut completed = false;
    for event in rx.into_iter().flatten() {
        match event {
            ScanEvent::DirectoryFound(directory) => directories.push(directory),
            ScanEvent::Completed => {
                completed = true;
                break;
            }
            ScanEvent::Failed(message) => panic!("scan failed: {message}"),
            ScanEvent::Cancelled | ScanEvent::Issue(_) => panic!("owned fixture must complete"),
            _ => {}
        }
    }
    handle.join();
    std::fs::remove_dir_all(&root).unwrap();
    assert!(completed);
    assert_eq!(directories.len(), 2);
    let root_record = directories
        .iter()
        .find(|d| d.full_path == root.display().to_string())
        .unwrap();
    let child_record = directories
        .iter()
        .find(|d| d.full_path == child.display().to_string())
        .unwrap();
    assert_eq!(child_record.parent_directory_id, Some(root_record.id));
    for directory in &directories {
        assert_eq!(
            directory.full_path.capacity(),
            directory.full_path.len(),
            "published directory path retains {} spare formatting bytes",
            directory.full_path.capacity() - directory.full_path.len()
        );
    }
}

#[test]
fn published_file_names_preserve_exact_unicode_and_metadata() {
    let root = std::env::temp_dir().join(format!("wb-record-name-budget-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let names = [
        "ascii-short.txt".to_owned(),
        format!("{}.bin", "a".repeat(150)),
        format!("{}.bin", "\u{00e9}".repeat(33)),
        format!("{}.bin", "\u{1f600}".repeat(17)),
    ];
    for name in &names {
        std::fs::write(root.join(name), b"exact contents").unwrap();
    }
    let (controller, rx) = ScanController::channel();
    let handle = controller.start_scan(ScanRequest {
        root_path: root.clone(),
        config: ScanRuntimeConfig {
            root_path: root.clone(),
            backend: ScanBackend::DirectoryWalk,
            ..ScanRuntimeConfig::default()
        },
    });
    let mut files = Vec::new();
    let mut completed = false;
    for event in rx.into_iter().flatten() {
        match event {
            ScanEvent::FileFound(file) => files.push(file),
            ScanEvent::Completed => {
                completed = true;
                break;
            }
            ScanEvent::Failed(message) => panic!("scan failed: {message}"),
            ScanEvent::Cancelled | ScanEvent::Issue(_) => panic!("owned fixture must complete"),
            _ => {}
        }
    }
    handle.join();
    std::fs::remove_dir_all(root).unwrap();
    assert!(completed);
    assert_eq!(files.len(), names.len());
    for name in &names {
        let file = files
            .iter()
            .find(|file| file.name.as_ref() == name.as_str())
            .unwrap();
        assert_eq!(file.size_bytes, 14);
        assert!(file.modified_utc().is_some());
    }
}
