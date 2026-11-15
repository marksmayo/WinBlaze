//! Directory walks open each child directory relative to its parent's handle,
//! so the kernel parses one path component per open instead of the whole
//! path, and release every parent handle once its children are open.
use std::{
    ffi::OsStr,
    fs,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use winblaze_core::ScanEvent;
use winblaze_scanner::{
    directory_metadata::{open_child_directory, read_directory, read_directory_handle},
    ScanController, ScanRequest, ScanRuntimeConfig,
};

fn unique_root(prefix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "{prefix}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn child_directories_open_relative_to_their_parent_handle() {
    use std::os::windows::ffi::OsStringExt;
    let root = unique_root("winblaze-relative-open");
    fs::create_dir_all(root.join("child")).unwrap();
    fs::write(root.join("child").join("a.txt"), b"aaa").unwrap();
    fs::write(root.join("child").join("b.txt"), b"bbbbb").unwrap();
    fs::write(root.join("file.txt"), b"f").unwrap();
    let odd = std::ffi::OsString::from_wide(&[b'o' as u16, 0xd800, b'd' as u16]);
    fs::create_dir(root.join(&odd)).unwrap();
    fs::write(root.join(&odd).join("inner.bin"), [0u8; 7]).unwrap();

    let parent = read_directory(&root).unwrap();
    let volume = parent.volume();
    let handle = parent.directory_handle().clone();
    let by_path = read_directory(&root.join("child"))
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (entry.name.to_string_lossy().into_owned(), entry.size_bytes)
        })
        .collect::<Vec<_>>();
    let child = open_child_directory(&handle, OsStr::new("child")).unwrap();
    let mut relative = read_directory_handle(child, volume)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (entry.name.to_string_lossy().into_owned(), entry.size_bytes)
        })
        .collect::<Vec<_>>();
    let mut expected = by_path;
    relative.sort();
    expected.sort();
    assert_eq!(relative, expected);
    assert_eq!(
        expected,
        vec![("a.txt".to_owned(), 3), ("b.txt".to_owned(), 5)]
    );

    // Names are used exactly: an unpaired surrogate still opens.
    let odd_child = open_child_directory(&handle, &odd).unwrap();
    let inner: Vec<_> = read_directory_handle(odd_child, volume)
        .unwrap()
        .map(|entry| entry.unwrap().size_bytes)
        .collect();
    assert_eq!(inner, vec![7]);

    // Failures map to the same io error kinds a path open reports.
    assert_eq!(
        open_child_directory(&handle, OsStr::new("missing"))
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::NotFound
    );
    assert!(open_child_directory(&handle, OsStr::new("file.txt")).is_err());
    drop(parent);
    drop(handle);
    fs::remove_dir_all(&root).unwrap();
}

fn scan_fixture(root: &std::path::Path, workers: usize) -> (Vec<String>, u64, u64) {
    let (controller, rx) = ScanController::channel();
    let handle = controller.start_scan(ScanRequest {
        root_path: root.to_path_buf(),
        config: ScanRuntimeConfig {
            root_path: root.to_path_buf(),
            max_parallelism: workers,
            directory_walk_max_parallelism: workers,
            ..ScanRuntimeConfig::default()
        },
    });
    let mut paths = Vec::new();
    let mut summary = None;
    let deadline = std::time::Instant::now() + Duration::from_secs(60);
    'outer: loop {
        let remaining = deadline.saturating_duration_since(std::time::Instant::now());
        for event in rx.recv_timeout(remaining).expect("scan events") {
            match event {
                ScanEvent::DirectoryFound(directory) => paths.push(directory.full_path),
                ScanEvent::Summary(value) => summary = Some(value),
                ScanEvent::Completed => break 'outer,
                ScanEvent::Failed(error) => panic!("scan failed: {error}"),
                _ => {}
            }
        }
    }
    handle.join();
    paths.sort();
    let summary = summary.expect("summary");
    (paths, summary.directories_seen, summary.files_seen)
}

#[test]
fn a_parallel_walk_holds_no_parent_directory_handles_after_it_ends() {
    let root = unique_root("winblaze-handle-release");
    // Branching at the top so more than one worker engages, then one deep
    // chain so parent handles must be handed down the task queue.
    for branch in 0..3 {
        let mut directory = root.join(format!("branch-{branch}"));
        fs::create_dir_all(&directory).unwrap();
        for level in 0..40 {
            directory = directory.join(format!("d{level:02}"));
            fs::create_dir(&directory).unwrap();
            fs::write(directory.join("leaf.txt"), [level as u8; 3]).unwrap();
        }
    }
    let (paths, directories, files) = scan_fixture(&root, 4);
    assert_eq!(directories, 1 + 3 * 41);
    assert_eq!(files, 3 * 40);
    assert_eq!(paths.len(), 1 + 3 * 41);
    assert_eq!(
        winblaze_scanner::controller::held_parent_directory_handles(),
        0,
        "a finished walk must not keep any parent directory open"
    );
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn work_stealing_walk_reports_the_same_tree_as_a_single_worker() {
    let root = unique_root("winblaze-steal");
    for wide in 0..3 {
        for index in 0..40 {
            let directory = root
                .join(format!("wide-{wide}"))
                .join(format!("sub-{index:02}"));
            fs::create_dir_all(&directory).unwrap();
            for file in 0..3 {
                fs::write(directory.join(format!("f{file}.bin")), [1u8; 4]).unwrap();
            }
        }
    }
    let mut chain = root.join("chain");
    for level in 0..30 {
        chain = chain.join(format!("l{level}"));
        fs::create_dir_all(&chain).unwrap();
    }
    fs::write(chain.join("end.txt"), b"end").unwrap();
    let single = scan_fixture(&root, 1);
    let parallel = scan_fixture(&root, 4);
    assert_eq!(parallel, single);
    assert_eq!(single.1, 1 + 3 + 3 * 40 + 31);
    assert_eq!(single.2, 3 * 40 * 3 + 1);
    assert_eq!(
        winblaze_scanner::controller::held_parent_directory_handles(),
        0
    );
    fs::remove_dir_all(&root).unwrap();
}

#[test]
fn terminal_scans_use_larger_event_batches_than_live_scans() {
    use winblaze_scanner::ScanPipelineConfig;
    let live = ScanPipelineConfig::default();
    let terminal = ScanPipelineConfig::terminal();
    assert!(terminal.batch_size >= 4 * live.batch_size);
    assert!(terminal.max_in_flight_events >= terminal.batch_size);
}
