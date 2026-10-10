use std::{fs, time::Duration};
use winblaze_core::ScanEvent;
use winblaze_scanner::{ScanController, ScanRequest, ScanRules, ScanRuntimeConfig};
#[test]
fn excludes_subtrees_before_descent_and_includes_only_matching_files() {
    let root = std::env::temp_dir().join(format!("winblaze-rules-{}", std::process::id()));
    fs::create_dir_all(root.join("skip/deep")).unwrap();
    fs::create_dir_all(root.join("keep")).unwrap();
    fs::write(root.join("skip/deep/hidden.bin"), b"excluded").unwrap();
    fs::write(root.join("keep/chosen.BIN"), b"included").unwrap();
    fs::write(root.join("keep/other.txt"), b"excluded").unwrap();
    for workers in [1, 4] {
        let (controller, rx) = ScanController::channel();
        let handle = controller.start_scan(ScanRequest {
            root_path: root.clone(),
            config: ScanRuntimeConfig {
                root_path: root.clone(),
                max_parallelism: workers,
                rules: ScanRules {
                    exclude_paths: vec!["skip".into()],
                    include_files: vec!["*.bin".into()],
                },
                ..Default::default()
            },
        });
        let mut files = Vec::new();
        let mut directories = Vec::new();
        'events: loop {
            for event in rx.recv_timeout(Duration::from_secs(10)).unwrap() {
                match event {
                    ScanEvent::FileFound(file) => files.push(file.name),
                    ScanEvent::DirectoryFound(directory) => directories.push(directory.name),
                    ScanEvent::Completed => break 'events,
                    _ => {}
                }
            }
        }
        handle.join();
        assert_eq!(
            files.iter().map(AsRef::as_ref).collect::<Vec<&str>>(),
            ["chosen.BIN"]
        );
        assert!(!directories
            .iter()
            .any(|name| name == "skip" || name == "deep"));
    }
    fs::remove_dir_all(root).unwrap();
}
