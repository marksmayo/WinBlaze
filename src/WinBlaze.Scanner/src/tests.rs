use std::{
    collections::HashSet,
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use winblaze_core::{ScanEvent, ScanIssueKind};

use crate::{
    discover_volume_root, normalize_scan_root, select_scan_backend, ScanController, ScanRequest,
    ScanRuntimeConfig,
};

#[test]
fn scanner_emits_progress_and_summary_events() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("winblaze-scanner-events-{unique}"));
    let nested = root.join("nested");
    fs::create_dir_all(&nested).expect("create event fixture");
    fs::write(nested.join("sample.txt"), b"sample").expect("write event fixture file");

    let (controller, rx) = ScanController::channel();
    let request = ScanRequest {
        root_path: root.clone(),
        config: ScanRuntimeConfig {
            root_path: root.clone(),
            ..ScanRuntimeConfig::default()
        },
    };

    let handle = controller.start_scan(request);

    let mut saw_issue = false;
    let mut saw_directory = false;
    let mut saw_progress = false;
    let mut saw_summary = false;
    let mut saw_completed = false;
    let mut first_session = None;
    for _ in 0..32 {
        let event = rx.recv_timeout(Duration::from_secs(1)).expect("scan event");
        match event {
            ScanEvent::Issue(_) => saw_issue = true,
            ScanEvent::DirectoryFound(_) => saw_directory = true,
            ScanEvent::FileFound(_) => {}
            ScanEvent::Progress(_) => saw_progress = true,
            ScanEvent::Summary(_) => saw_summary = true,
            ScanEvent::Completed => {
                saw_completed = true;
                break;
            }
            ScanEvent::SessionStarted(_) => {
                first_session = Some(());
            }
            other => panic!("unexpected first event: {other:?}"),
        }
    }

    let _ = fs::remove_dir_all(&root);
    assert!(first_session.is_some(), "expected a session start event");
    assert!(saw_directory, "expected a real directory event");
    assert!(saw_progress, "expected a progress event");
    assert!(saw_summary, "expected a summary event");
    assert!(saw_completed, "expected a completion event");
    assert!(saw_issue || saw_directory);

    handle.join();
}

#[test]
fn directory_walk_scans_real_fixture_with_expected_totals() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("winblaze-scanner-fixture-{unique}"));
    let nested = root.join("alpha").join("nested");
    fs::create_dir_all(&nested).expect("create fixture directories");
    fs::write(root.join("root.txt"), b"root").expect("write root file");
    fs::write(root.join("alpha").join("child.bin"), [7_u8; 11]).expect("write child file");
    fs::write(nested.join("leaf.log"), [3_u8; 17]).expect("write nested file");

    let (controller, rx) = ScanController::channel();
    let request = ScanRequest {
        root_path: root.clone(),
        config: ScanRuntimeConfig {
            root_path: root.clone(),
            ..ScanRuntimeConfig::default()
        },
    };
    let handle = controller.start_scan(request);

    let mut files = HashSet::new();
    let mut directories = HashSet::new();
    let mut progress_events = 0;
    let mut summary = None;
    let mut completed = false;
    for _ in 0..64 {
        let event = rx.recv_timeout(Duration::from_secs(2)).expect("scan event");
        match event {
            ScanEvent::DirectoryFound(directory) => {
                directories.insert(directory.full_path);
            }
            ScanEvent::FileFound(file) => {
                files.insert((file.name, file.size_bytes));
            }
            ScanEvent::Progress(_) => {
                progress_events += 1;
            }
            ScanEvent::Summary(scan_summary) => {
                summary = Some(scan_summary);
            }
            ScanEvent::Completed => {
                completed = true;
                break;
            }
            ScanEvent::SessionStarted(_) | ScanEvent::VolumeDiscovered(_) => {}
            other => panic!("unexpected fixture scan event: {other:?}"),
        }
    }

    handle.join();
    let _ = fs::remove_dir_all(&root);

    assert!(completed, "fixture scan should complete");
    assert!(progress_events > 0, "expected progress events");
    assert!(directories.contains(&root.display().to_string()));
    assert!(directories.contains(&root.join("alpha").display().to_string()));
    assert!(directories.contains(&nested.display().to_string()));
    assert!(files.contains(&("root.txt".to_string(), 4)));
    assert!(files.contains(&("child.bin".to_string(), 11)));
    assert!(files.contains(&("leaf.log".to_string(), 17)));

    let summary = summary.expect("summary event");
    assert_eq!(summary.files_seen, 3);
    assert_eq!(summary.directories_seen, 3);
    assert_eq!(summary.total_size_bytes, 32);
}

#[test]
fn directory_walk_handles_large_single_directory_fanout() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("winblaze-scanner-fanout-{unique}"));
    fs::create_dir_all(&root).expect("create fanout root");
    for index in 0..4096 {
        fs::write(
            root.join(format!("fanout-{index:04}.bin")),
            [index as u8; 16],
        )
        .expect("write fanout file");
    }

    let (controller, rx) = ScanController::channel();
    let request = ScanRequest {
        root_path: root.clone(),
        config: ScanRuntimeConfig {
            root_path: root.clone(),
            ..ScanRuntimeConfig::default()
        },
    };
    let handle = controller.start_scan(request);

    let mut summary = None;
    let mut completed = false;
    for _ in 0..5000 {
        let event = rx.recv_timeout(Duration::from_secs(5)).expect("scan event");
        match event {
            ScanEvent::Summary(scan_summary) => summary = Some(scan_summary),
            ScanEvent::Completed => {
                completed = true;
                break;
            }
            ScanEvent::SessionStarted(_)
            | ScanEvent::VolumeDiscovered(_)
            | ScanEvent::DirectoryFound(_)
            | ScanEvent::FileFound(_)
            | ScanEvent::Progress(_) => {}
            other => panic!("unexpected fanout scan event: {other:?}"),
        }
    }

    handle.join();
    let _ = fs::remove_dir_all(&root);

    assert!(completed, "fanout scan should complete");
    let summary = summary.expect("summary event");
    assert_eq!(summary.files_seen, 4096);
    assert_eq!(summary.directories_seen, 1);
    assert_eq!(summary.total_size_bytes, 4096 * 16);
}

#[test]
fn directory_walk_reports_missing_root_without_fake_directory_record() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("winblaze-scanner-missing-{unique}"));

    let (controller, rx) = ScanController::channel();
    let request = ScanRequest {
        root_path: root.clone(),
        config: ScanRuntimeConfig {
            root_path: root.clone(),
            ..ScanRuntimeConfig::default()
        },
    };
    let handle = controller.start_scan(request);

    let mut issue = None;
    let mut summary = None;
    let mut saw_directory = false;
    let mut completed = false;
    for _ in 0..16 {
        let event = rx.recv_timeout(Duration::from_secs(2)).expect("scan event");
        match event {
            ScanEvent::Issue(record) => issue = Some(record),
            ScanEvent::Summary(scan_summary) => summary = Some(scan_summary),
            ScanEvent::DirectoryFound(_) => saw_directory = true,
            ScanEvent::Completed => {
                completed = true;
                break;
            }
            ScanEvent::SessionStarted(_) | ScanEvent::Progress(_) => {}
            other => panic!("unexpected missing-root scan event: {other:?}"),
        }
    }

    handle.join();

    assert!(
        completed,
        "missing-root scan should complete with diagnostics"
    );
    assert!(
        !saw_directory,
        "missing root must not be reported as a catalog directory"
    );
    let issue = issue.expect("missing-root issue");
    let expected_path = root.display().to_string();
    assert_eq!(issue.kind, ScanIssueKind::NotFound);
    assert_eq!(issue.path.as_deref(), Some(expected_path.as_str()));
    let summary = summary.expect("summary event");
    assert_eq!(summary.files_seen, 0);
    assert_eq!(summary.directories_seen, 0);
    assert_eq!(summary.total_size_bytes, 0);
}

#[test]
fn directory_walk_reports_file_root_without_fake_directory_record() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("winblaze-scanner-file-root-{unique}.txt"));
    fs::write(&root, b"not a directory").expect("write file root");

    let (controller, rx) = ScanController::channel();
    let request = ScanRequest {
        root_path: root.clone(),
        config: ScanRuntimeConfig {
            root_path: root.clone(),
            ..ScanRuntimeConfig::default()
        },
    };
    let handle = controller.start_scan(request);

    let mut issue = None;
    let mut summary = None;
    let mut saw_directory = false;
    let mut completed = false;
    for _ in 0..16 {
        let event = rx.recv_timeout(Duration::from_secs(2)).expect("scan event");
        match event {
            ScanEvent::Issue(record) => issue = Some(record),
            ScanEvent::Summary(scan_summary) => summary = Some(scan_summary),
            ScanEvent::DirectoryFound(_) => saw_directory = true,
            ScanEvent::Completed => {
                completed = true;
                break;
            }
            ScanEvent::SessionStarted(_) | ScanEvent::Progress(_) => {}
            other => panic!("unexpected file-root scan event: {other:?}"),
        }
    }

    handle.join();
    let _ = fs::remove_file(&root);

    assert!(completed, "file-root scan should complete with diagnostics");
    assert!(
        !saw_directory,
        "file root must not be reported as a catalog directory"
    );
    let issue = issue.expect("file-root issue");
    let expected_path = root.display().to_string();
    assert_eq!(issue.kind, ScanIssueKind::NotFound);
    assert_eq!(issue.path.as_deref(), Some(expected_path.as_str()));
    assert_eq!(issue.message, "scan root is not a directory");
    let summary = summary.expect("summary event");
    assert_eq!(summary.files_seen, 0);
    assert_eq!(summary.directories_seen, 0);
    assert_eq!(summary.total_size_bytes, 0);
}

#[test]
fn filesystem_helpers_normalize_and_discover_drive_roots() {
    let normalized = normalize_scan_root(std::path::Path::new(r"C:\Users\.\markm\..\Public"));
    assert_eq!(normalized, std::path::PathBuf::from(r"C:\Users\Public"));

    let candidate = discover_volume_root(std::path::Path::new(r"C:\Users\markm"))
        .expect("drive roots should be discovered");
    assert_eq!(candidate.drive_letter, Some('C'));
    assert_eq!(candidate.root_path, std::path::PathBuf::from(r"C:\"));
    assert_eq!(
        select_scan_backend(std::path::Path::new(r"relative\path")),
        crate::ScanBackend::DirectoryWalk
    );
}
