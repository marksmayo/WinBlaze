use std::{
    env,
    path::PathBuf,
    time::{Duration, Instant},
};

use winblaze_scanner::{ScanController, ScanRequest, ScanRuntimeConfig};

fn main() {
    let root = env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        eprintln!("usage: mft_scan_repro <root>");
        std::process::exit(2);
    });

    let started = Instant::now();
    let (controller, rx) = ScanController::channel();
    // Default ScanRuntimeConfig backend is NtfsMft; leaving it unset exercises
    // the same production dispatch as controller.rs's real scan path.
    let handle = controller.start_scan(ScanRequest {
        root_path: root.clone(),
        config: ScanRuntimeConfig {
            root_path: root.clone(),
            ..ScanRuntimeConfig::default()
        },
    });

    let mut files = 0u64;
    let mut directories = 0u64;
    let mut last_progress_items = 0u64;
    let mut completed = false;
    let mut failed_message = String::new();

    for event in rx {
        match event {
            winblaze_core::ScanEvent::FileFound(_) => files += 1,
            winblaze_core::ScanEvent::DirectoryFound(_) => directories += 1,
            winblaze_core::ScanEvent::Progress(p) => {
                if p.completed_items.saturating_sub(last_progress_items) >= 100_000 {
                    last_progress_items = p.completed_items;
                    eprintln!(
                        "[{:>7} ms] progress items_done={} items_total={}",
                        started.elapsed().as_millis(),
                        p.completed_items,
                        p.total_items
                    );
                }
            }
            winblaze_core::ScanEvent::Issue(issue) => {
                eprintln!("[issue] {:?}: {}", issue.kind, issue.message);
            }
            winblaze_core::ScanEvent::Completed => {
                completed = true;
                break;
            }
            winblaze_core::ScanEvent::Cancelled => break,
            winblaze_core::ScanEvent::Failed(message) => {
                failed_message = message;
                break;
            }
            _ => {}
        }
    }

    handle.join();

    let elapsed = Duration::from(started.elapsed());
    println!(
        "{{\"root\":\"{}\",\"elapsed_ms\":{},\"files\":{},\"directories\":{},\"completed\":{},\"failed_message\":\"{}\"}}",
        root.display(),
        elapsed.as_millis(),
        files,
        directories,
        completed,
        failed_message.replace('"', "'")
    );
}
