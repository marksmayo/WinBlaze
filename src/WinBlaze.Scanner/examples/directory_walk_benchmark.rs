use std::{
    env,
    path::PathBuf,
    time::{Duration, Instant},
};

use winblaze_core::{ScanEvent, ScanIssueKind, ScanIssueSummary};
use winblaze_scanner::{ScanBackend, ScanController, ScanRequest, ScanRuntimeConfig};

#[derive(Default)]
struct EventCounts {
    files: u64,
    directories: u64,
    progress_events: u64,
    issues: u64,
    completed: bool,
    cancelled: bool,
    failed: bool,
    failure_message: String,
    summary_files: u64,
    summary_directories: u64,
    summary_bytes: u64,
    issue_summary: ScanIssueSummary,
}

fn main() {
    let root = env::args().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        eprintln!("usage: directory_walk_benchmark <root>");
        std::process::exit(2);
    });

    let started = Instant::now();
    let (controller, rx) = ScanController::channel();
    let handle = controller.start_scan(ScanRequest {
        root_path: root.clone(),
        config: ScanRuntimeConfig {
            backend: ScanBackend::DirectoryWalk,
            root_path: root.clone(),
            ..ScanRuntimeConfig::default()
        },
    });

    let mut counts = EventCounts::default();
    for event in rx {
        match event {
            ScanEvent::DirectoryFound(_) => counts.directories += 1,
            ScanEvent::FileFound(_) => counts.files += 1,
            ScanEvent::Progress(_) => counts.progress_events += 1,
            ScanEvent::Issue(issue) => {
                counts.issues += 1;
                counts.issue_summary.record(issue, 5);
            }
            ScanEvent::Summary(summary) => {
                counts.summary_files = summary.files_seen;
                counts.summary_directories = summary.directories_seen;
                counts.summary_bytes = summary.total_size_bytes;
            }
            ScanEvent::Completed => {
                counts.completed = true;
                break;
            }
            ScanEvent::Cancelled => {
                counts.cancelled = true;
                break;
            }
            ScanEvent::Failed(message) => {
                counts.failed = true;
                counts.failure_message = message;
                break;
            }
            ScanEvent::SessionStarted(_) | ScanEvent::VolumeDiscovered(_) => {}
        }
    }
    handle.join();

    let elapsed = started.elapsed();
    println!(
        "{{\"root\":\"{}\",\"elapsed_ms\":{},\"files\":{},\"directories\":{},\"summary_files\":{},\"summary_directories\":{},\"summary_bytes\":{},\"progress_events\":{},\"issues\":{},\"issues_by_kind\":{},\"recent_issues\":{},\"completed\":{},\"cancelled\":{},\"failed\":{},\"failure_message\":\"{}\",\"files_per_second\":{}}}",
        json_escape(&root.display().to_string()),
        millis(elapsed),
        counts.files,
        counts.directories,
        counts.summary_files,
        counts.summary_directories,
        counts.summary_bytes,
        counts.progress_events,
        counts.issues,
        issue_counts_json(&counts.issue_summary),
        recent_issues_json(&counts.issue_summary),
        counts.completed,
        counts.cancelled,
        counts.failed,
        json_escape(&counts.failure_message),
        if elapsed.as_millis() == 0 {
            0
        } else {
            counts.files.saturating_mul(1000) / elapsed.as_millis() as u64
        }
    );
}

fn issue_counts_json(summary: &ScanIssueSummary) -> String {
    let values = [
        (ScanIssueKind::PermissionDenied, "permission_denied"),
        (ScanIssueKind::NotFound, "not_found"),
        (ScanIssueKind::SharingViolation, "sharing_violation"),
        (ScanIssueKind::TransientIo, "transient_io"),
        (
            ScanIssueKind::UnsupportedFilesystem,
            "unsupported_filesystem",
        ),
        (ScanIssueKind::Unknown, "unknown"),
        (ScanIssueKind::FastScanUnavailable, "fast_scan_unavailable"),
    ];
    let fields = values
        .iter()
        .map(|(kind, name)| format!("\"{name}\":{}", summary.count(*kind)))
        .collect::<Vec<_>>();
    format!("{{{}}}", fields.join(","))
}

fn recent_issues_json(summary: &ScanIssueSummary) -> String {
    let issues = summary
        .recent
        .iter()
        .map(|issue| {
            format!(
                "{{\"kind\":\"{:?}\",\"path\":\"{}\",\"message\":\"{}\"}}",
                issue.kind,
                json_escape(issue.path.as_deref().unwrap_or("")),
                json_escape(&issue.message)
            )
        })
        .collect::<Vec<_>>();
    format!("[{}]", issues.join(","))
}

fn millis(duration: Duration) -> u128 {
    duration.as_millis()
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            ch if ch.is_control() => escaped.push_str(&format!("\\u{:04x}", ch as u32)),
            ch => escaped.push(ch),
        }
    }
    escaped
}
