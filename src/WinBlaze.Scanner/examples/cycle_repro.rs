//! Reproduces the self-referential reparse-cycle walk, printing every scan
//! event, to diagnose missing cycle-skip issues.

use std::fs;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use winblaze_scanner::{ScanController, ScanRequest, ScanRuntimeConfig};

fn main() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos();
    let root = std::env::temp_dir().join(format!("winblaze-cycle-repro-{unique}"));
    fs::create_dir_all(&root).expect("create root");
    fs::write(root.join("real.txt"), b"hello").expect("write file");
    let loop_link = root.join("loop");
    std::os::windows::fs::symlink_dir(&root, &loop_link).expect("create symlink");

    let (controller, rx) = ScanController::channel();
    let request = ScanRequest {
        root_path: root.clone(),
        config: ScanRuntimeConfig {
            root_path: root.clone(),
            ..ScanRuntimeConfig::default()
        },
    };
    let handle = controller.start_scan(request);

    loop {
        match rx.recv_timeout(Duration::from_secs(5)) {
            Ok(event) => {
                println!("{event:?}");
                if matches!(event, winblaze_core::ScanEvent::Completed) {
                    break;
                }
            }
            Err(_) => {
                println!("TIMEOUT");
                break;
            }
        }
    }
    handle.join();
    let _ = fs::remove_dir_all(&root);
}
