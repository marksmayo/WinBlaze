use std::{
    collections::{HashMap, HashSet},
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc,
    },
    thread,
};

use winblaze_core::{ScanEvent, ScanIssueKind, ScanIssueRecord};

use crate::errors::{classify_io_error, ScanErrorKind};
use crate::filesystem::build_scan_access_plan;
use crate::ntfs::{enumerate_ntfs_volume_parallel_streaming, NtfsEnumerationError};
use crate::pipeline::ScanEventPipeline;
use crate::types::ScanRuntimeConfig;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanRequest {
    pub root_path: PathBuf,
    pub config: ScanRuntimeConfig,
}

impl From<ScanRequest> for winblaze_core::ScanRequest {
    fn from(value: ScanRequest) -> Self {
        Self {
            root_path: value.root_path.display().to_string(),
            follow_reparse_points: value.config.follows_reparse_points(),
            emit_partial_results: value.config.emit_partial_results,
        }
    }
}

pub struct ScanHandle {
    cancelled: Arc<AtomicBool>,
    join: Option<thread::JoinHandle<()>>,
}

impl ScanHandle {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn join(mut self) {
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

pub struct ScanController {
    event_tx: Sender<ScanEvent>,
}

impl ScanController {
    pub fn new(event_tx: Sender<ScanEvent>) -> Self {
        Self { event_tx }
    }

    pub fn start_scan(&self, request: ScanRequest) -> ScanHandle {
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_thread = Arc::clone(&cancelled);
        let event_tx = self.event_tx.clone();
        let access_plan = build_scan_access_plan(&request.root_path, request.config.backend_hint());
        let pipeline_config = request.config.pipeline;
        let worker_count = request.config.max_parallelism.max(1);

        let join = thread::spawn(move || {
            let mut pipeline = ScanEventPipeline::new(event_tx, pipeline_config);
            let mut issue_keys = HashSet::new();
            if access_plan.primary_backend == crate::types::ScanBackend::NtfsMft {
                pipeline.emit_session_started(provisional_volume_record(&access_plan));
                match enumerate_ntfs_volume_parallel_streaming(
                    &access_plan.selected_root,
                    worker_count,
                    |event| {
                        emit_deduplicated_event(&mut pipeline, &mut issue_keys, event);
                    },
                ) {
                    Ok(result) => {
                        let crate::ntfs::NtfsEnumeration { summary, .. } = result;
                        pipeline.emit_summary(summary);
                        pipeline.emit_completed();
                    }
                    Err(error) => {
                        emit_deduplicated_issue(
                            &mut pipeline,
                            &mut issue_keys,
                            convert_ntfs_error(
                                &error,
                                access_plan.selected_root.display().to_string(),
                            ),
                        );
                        run_fallback_scan(
                            &mut pipeline,
                            &cancelled_thread,
                            &access_plan.selected_root,
                            &mut issue_keys,
                        );
                    }
                }
            } else {
                run_fallback_scan(
                    &mut pipeline,
                    &cancelled_thread,
                    &access_plan.selected_root,
                    &mut issue_keys,
                );
            }
        });

        ScanHandle {
            cancelled,
            join: Some(join),
        }
    }

    pub fn channel() -> (Self, Receiver<ScanEvent>) {
        let (event_tx, event_rx) = mpsc::channel();
        (Self::new(event_tx), event_rx)
    }
}

fn run_fallback_scan(
    pipeline: &mut ScanEventPipeline,
    cancelled: &AtomicBool,
    selected_root: &std::path::Path,
    issue_keys: &mut HashSet<String>,
) {
    pipeline.emit_session_started(winblaze_core::VolumeRecord {
        id: winblaze_core::VolumeId(0),
        mount_point: selected_root.display().to_string(),
        label: None,
        file_system: winblaze_core::FileSystemKind::Unknown,
        total_bytes: 0,
        free_bytes: 0,
        root_directory_id: winblaze_core::DirectoryId(0),
    });

    if !validate_directory_walk_root(pipeline, selected_root, issue_keys) {
        pipeline.emit_summary(winblaze_core::ScanSummary {
            files_seen: 0,
            directories_seen: 0,
            total_size_bytes: 0,
            total_allocation_bytes: 0,
        });
        pipeline.emit_completed();
        return;
    }

    let mut walk_state = DirectoryWalkState::new();
    let mut directory_ids = HashMap::new();
    let root_id = winblaze_core::DirectoryId(5);
    directory_ids.insert(selected_root.to_path_buf(), root_id);
    emit_directory_record(
        pipeline,
        &mut walk_state,
        root_id,
        None,
        selected_root,
        selected_root
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| selected_root.display().to_string()),
        0,
    );
    walk_state.maybe_emit_progress(pipeline, true);

    walk_directory_tree(
        pipeline,
        cancelled,
        selected_root,
        root_id,
        &mut walk_state,
        &mut directory_ids,
        issue_keys,
    );

    if cancelled.load(Ordering::SeqCst) {
        pipeline.emit_cancelled();
    } else {
        pipeline.emit_summary(winblaze_core::ScanSummary {
            files_seen: walk_state.files_seen,
            directories_seen: walk_state.directories_seen,
            total_size_bytes: walk_state.total_size_bytes,
            total_allocation_bytes: walk_state.total_allocation_bytes,
        });
        pipeline.emit_completed();
    }
}

fn validate_directory_walk_root(
    pipeline: &mut ScanEventPipeline,
    selected_root: &std::path::Path,
    issue_keys: &mut HashSet<String>,
) -> bool {
    match fs::metadata(selected_root) {
        Ok(metadata) if metadata.is_dir() => true,
        Ok(_) => {
            emit_deduplicated_issue(
                pipeline,
                issue_keys,
                ScanIssueRecord {
                    kind: ScanIssueKind::NotFound,
                    path: Some(selected_root.display().to_string()),
                    message: "scan root is not a directory".to_string(),
                },
            );
            false
        }
        Err(error) => {
            emit_deduplicated_issue(
                pipeline,
                issue_keys,
                convert_io_error(&error, selected_root.display().to_string()),
            );
            false
        }
    }
}

fn convert_ntfs_error(error: &NtfsEnumerationError, path: String) -> ScanIssueRecord {
    match error {
        NtfsEnumerationError::Io(io_error) => {
            let kind = match classify_io_error(io_error) {
                ScanErrorKind::PermissionDenied => ScanIssueKind::PermissionDenied,
                ScanErrorKind::NotFound => ScanIssueKind::NotFound,
                ScanErrorKind::SharingViolation => ScanIssueKind::SharingViolation,
                ScanErrorKind::TransientIo => ScanIssueKind::TransientIo,
                ScanErrorKind::UnsupportedFilesystem => ScanIssueKind::UnsupportedFilesystem,
                ScanErrorKind::Unknown => ScanIssueKind::Unknown,
            };
            ScanIssueRecord {
                kind,
                path: Some(path),
                message: io_error.to_string(),
            }
        }
        NtfsEnumerationError::InvalidRecord(message) => ScanIssueRecord {
            kind: ScanIssueKind::Unknown,
            path: Some(path),
            message: message.clone(),
        },
    }
}

fn walk_directory_tree(
    pipeline: &mut ScanEventPipeline,
    cancelled: &AtomicBool,
    directory: &std::path::Path,
    parent_directory_id: winblaze_core::DirectoryId,
    walk_state: &mut DirectoryWalkState,
    directory_ids: &mut HashMap<PathBuf, winblaze_core::DirectoryId>,
    issue_keys: &mut HashSet<String>,
) {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            emit_deduplicated_issue(
                pipeline,
                issue_keys,
                convert_io_error(&error, directory.display().to_string()),
            );
            return;
        }
    };

    for entry_result in entries {
        if cancelled.load(Ordering::SeqCst) {
            return;
        }

        let entry = match entry_result {
            Ok(entry) => entry,
            Err(error) => {
                emit_deduplicated_issue(
                    pipeline,
                    issue_keys,
                    convert_io_error(&error, directory.display().to_string()),
                );
                continue;
            }
        };

        let entry_path = entry.path();
        let entry_name = entry.file_name().to_string_lossy().to_string();

        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                emit_deduplicated_issue(
                    pipeline,
                    issue_keys,
                    convert_io_error(&error, entry_path.display().to_string()),
                );
                continue;
            }
        };

        if file_type.is_dir() {
            let directory_id = walk_state.next_directory_id();
            directory_ids.insert(entry_path.clone(), directory_id);
            emit_directory_record(
                pipeline,
                walk_state,
                directory_id,
                Some(parent_directory_id),
                &entry_path,
                entry_name,
                0,
            );
            walk_directory_tree(
                pipeline,
                cancelled,
                &entry_path,
                directory_id,
                walk_state,
                directory_ids,
                issue_keys,
            );
        } else {
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(error) => {
                    emit_deduplicated_issue(
                        pipeline,
                        issue_keys,
                        convert_io_error(&error, entry_path.display().to_string()),
                    );
                    continue;
                }
            };
            emit_file_record(
                pipeline,
                walk_state,
                parent_directory_id,
                &entry_path,
                entry_name,
                metadata.len(),
            );
        }
    }
}

fn emit_directory_record(
    pipeline: &mut ScanEventPipeline,
    walk_state: &mut DirectoryWalkState,
    directory_id: winblaze_core::DirectoryId,
    parent_directory_id: Option<winblaze_core::DirectoryId>,
    full_path: &std::path::Path,
    name: String,
    allocation_bytes: u64,
) {
    walk_state.directories_seen = walk_state.directories_seen.saturating_add(1);
    walk_state.total_allocation_bytes = walk_state
        .total_allocation_bytes
        .saturating_add(allocation_bytes);

    pipeline.emit_directory(winblaze_core::DirectoryRecord {
        id: directory_id,
        parent_directory_id,
        name,
        full_path: full_path.display().to_string(),
        direct_bytes: 0,
        total_bytes: 0,
        direct_entries: 0,
        total_entries: 0,
    });
    walk_state.maybe_emit_progress(pipeline, false);
}

fn emit_file_record(
    pipeline: &mut ScanEventPipeline,
    walk_state: &mut DirectoryWalkState,
    parent_directory_id: winblaze_core::DirectoryId,
    full_path: &std::path::Path,
    name: String,
    size_bytes: u64,
) {
    walk_state.files_seen = walk_state.files_seen.saturating_add(1);
    walk_state.total_size_bytes = walk_state.total_size_bytes.saturating_add(size_bytes);
    walk_state.total_allocation_bytes =
        walk_state.total_allocation_bytes.saturating_add(size_bytes);

    pipeline.emit_file(winblaze_core::FileRecord {
        id: winblaze_core::FileId(walk_state.next_file_id()),
        parent_directory_id,
        name,
        full_path: full_path.display().to_string(),
        size_bytes,
        allocation_bytes: size_bytes,
        attributes: winblaze_core::FileAttributes::ARCHIVE,
        created_utc: None,
        modified_utc: None,
        accessed_utc: None,
    });
    walk_state.maybe_emit_progress(pipeline, false);
}

fn convert_io_error(error: &std::io::Error, path: String) -> ScanIssueRecord {
    let kind = match classify_io_error(error) {
        ScanErrorKind::PermissionDenied => ScanIssueKind::PermissionDenied,
        ScanErrorKind::NotFound => ScanIssueKind::NotFound,
        ScanErrorKind::SharingViolation => ScanIssueKind::SharingViolation,
        ScanErrorKind::TransientIo => ScanIssueKind::TransientIo,
        ScanErrorKind::UnsupportedFilesystem => ScanIssueKind::UnsupportedFilesystem,
        ScanErrorKind::Unknown => ScanIssueKind::Unknown,
    };

    ScanIssueRecord {
        kind,
        path: Some(path),
        message: error.to_string(),
    }
}

fn emit_deduplicated_event(
    pipeline: &mut ScanEventPipeline,
    issue_keys: &mut HashSet<String>,
    event: ScanEvent,
) {
    match event {
        ScanEvent::Issue(issue) => emit_deduplicated_issue(pipeline, issue_keys, issue),
        other => pipeline.emit_event(other),
    }
}

fn emit_deduplicated_issue(
    pipeline: &mut ScanEventPipeline,
    issue_keys: &mut HashSet<String>,
    issue: ScanIssueRecord,
) {
    let key = format!(
        "{:?}|{}|{}",
        issue.kind,
        issue.path.as_deref().unwrap_or(""),
        issue.message
    );
    if issue_keys.insert(key) {
        pipeline.emit_issue(issue);
    }
}

struct DirectoryWalkState {
    directories_seen: u64,
    files_seen: u64,
    total_size_bytes: u64,
    total_allocation_bytes: u64,
    next_directory_id: u64,
    next_file_id: u64,
    last_progress_reported_items: u64,
}

impl DirectoryWalkState {
    fn new() -> Self {
        Self {
            directories_seen: 0,
            files_seen: 0,
            total_size_bytes: 0,
            total_allocation_bytes: 0,
            next_directory_id: 6,
            next_file_id: 1,
            last_progress_reported_items: 0,
        }
    }

    fn next_directory_id(&mut self) -> winblaze_core::DirectoryId {
        let id = self.next_directory_id;
        self.next_directory_id = self.next_directory_id.saturating_add(1);
        winblaze_core::DirectoryId(id)
    }

    fn next_file_id(&mut self) -> u64 {
        let id = self.next_file_id;
        self.next_file_id = self.next_file_id.saturating_add(1);
        id
    }

    fn maybe_emit_progress(&mut self, pipeline: &mut ScanEventPipeline, force: bool) {
        let completed_items = self.files_seen.saturating_add(self.directories_seen);
        if force || completed_items.saturating_sub(self.last_progress_reported_items) >= 256 {
            self.last_progress_reported_items = completed_items;
            pipeline.emit_progress(completed_items, 0, self.total_size_bytes);
        }
    }
}

fn provisional_volume_record(
    access_plan: &crate::filesystem::ScanAccessPlan,
) -> winblaze_core::VolumeRecord {
    let label = access_plan
        .root_candidate
        .as_ref()
        .and_then(|candidate| candidate.drive_letter.map(|letter| format!("{letter}:")));

    winblaze_core::VolumeRecord {
        id: winblaze_core::VolumeId(0),
        mount_point: access_plan.selected_root.display().to_string(),
        label,
        file_system: winblaze_core::FileSystemKind::Unknown,
        total_bytes: 0,
        free_bytes: 0,
        root_directory_id: winblaze_core::DirectoryId(0),
    }
}
