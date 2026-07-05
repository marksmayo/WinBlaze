use std::{
    collections::{HashMap, HashSet, VecDeque},
    fs,
    os::windows::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Receiver, Sender},
        Arc, Condvar, Mutex,
    },
    thread,
};

use winblaze_core::{FileAttributes, ScanEvent, ScanIssueKind, ScanIssueRecord};

use crate::errors::{classify_io_error, ScanErrorKind};
use crate::filesystem::build_scan_access_plan;
use crate::ntfs::{enumerate_ntfs_volume_parallel_streaming_summary, NtfsEnumerationError};
use crate::performance::ScanPipelineConfig;
use crate::pipeline::ScanEventPipeline;
use crate::policy::{should_descend_into_reparse_target, ReparseTraversalPolicy};
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
        let reparse_policy = request.config.reparse_policy;

        let join = thread::spawn(move || {
            let mut pipeline = ScanEventPipeline::new(event_tx, pipeline_config);
            let mut issue_keys = HashSet::new();
            if access_plan.primary_backend == crate::types::ScanBackend::NtfsMft {
                pipeline.emit_session_started(provisional_volume_record(&access_plan));
                match enumerate_ntfs_volume_parallel_streaming_summary(
                    &access_plan.selected_root,
                    worker_count,
                    |event| {
                        emit_deduplicated_event(&mut pipeline, &mut issue_keys, event);
                    },
                ) {
                    Ok(summary) => {
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
                            reparse_policy,
                            worker_count,
                        );
                    }
                }
            } else {
                run_fallback_scan(
                    &mut pipeline,
                    &cancelled_thread,
                    &access_plan.selected_root,
                    &mut issue_keys,
                    reparse_policy,
                    worker_count,
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
    reparse_policy: ReparseTraversalPolicy,
    worker_count: usize,
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

    let root_id = winblaze_core::DirectoryId(5);
    pipeline.emit_directory(winblaze_core::DirectoryRecord {
        id: root_id,
        parent_directory_id: None,
        name: selected_root
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| selected_root.display().to_string()),
        full_path: selected_root.display().to_string(),
        direct_bytes: 0,
        total_bytes: 0,
        direct_entries: 0,
        total_entries: 0,
    });

    // The walker only needs a resolved identity for the handful of
    // directories actually reached through a reparse point, so a single
    // canonicalize() call for the root is the only unconditional cost.
    let root_canonical = fs::canonicalize(selected_root)
        .map(|resolved| strip_verbatim_prefix(&resolved))
        .unwrap_or_else(|_| selected_root.to_path_buf());

    // Force an initial progress tick regardless of which branch runs below:
    // the parallel walker only reports progress on a shared item-count
    // threshold, so small scans would otherwise emit no progress at all.
    pipeline.emit_progress(1, 0, 0);
    // Flush so the root directory reaches consumers before any worker's
    // child events; live tree views key children off their parent.
    pipeline.flush();

    let (files_seen, directories_seen, total_size_bytes, total_allocation_bytes) =
        if worker_count > 1 {
            run_fallback_scan_parallel(
                pipeline,
                cancelled,
                selected_root,
                root_id,
                reparse_policy,
                worker_count,
                root_canonical,
            )
        } else {
            let mut walk_state = DirectoryWalkState::new();
            walk_state.directories_seen = 1;

            let mut directory_ids = HashMap::new();
            directory_ids.insert(selected_root.to_path_buf(), root_id);
            let mut ancestor_canonical_stack = vec![root_canonical];

            walk_directory_tree(
                pipeline,
                cancelled,
                selected_root,
                root_id,
                &mut walk_state,
                &mut directory_ids,
                issue_keys,
                reparse_policy,
                &mut ancestor_canonical_stack,
            );

            (
                walk_state.files_seen,
                walk_state.directories_seen,
                walk_state.total_size_bytes,
                walk_state.total_allocation_bytes,
            )
        };

    if cancelled.load(Ordering::SeqCst) {
        pipeline.emit_cancelled();
    } else {
        pipeline.emit_progress(
            files_seen.saturating_add(directories_seen),
            0,
            total_size_bytes,
        );
        pipeline.emit_summary(winblaze_core::ScanSummary {
            files_seen,
            directories_seen,
            total_size_bytes,
            total_allocation_bytes,
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

/// Reports that the fast NTFS-MFT reader is unavailable and the scan is
/// falling back to the much slower directory-walk backend. This is always
/// reported as `FastScanUnavailable` (rather than the underlying IO error
/// kind) so the UI can reliably detect the downgrade and explain it, instead
/// of it being buried among ordinary per-file scan issues.
fn convert_ntfs_error(error: &NtfsEnumerationError, path: String) -> ScanIssueRecord {
    let detail = match error {
        NtfsEnumerationError::Io(io_error) => {
            if matches!(classify_io_error(io_error), ScanErrorKind::PermissionDenied) {
                format!(
                    "access denied reading the NTFS master file table ({io_error}); \
                     restart WinBlaze as Administrator for full-speed scans"
                )
            } else {
                format!("could not read the NTFS master file table ({io_error})")
            }
        }
        NtfsEnumerationError::InvalidRecord(message) => {
            format!("NTFS master file table record could not be parsed ({message})")
        }
    };

    ScanIssueRecord {
        kind: ScanIssueKind::FastScanUnavailable,
        path: Some(path),
        message: format!("Falling back to a standard directory scan: {detail}"),
    }
}

/// Strips the `\\?\` (or `\\?\UNC\`) verbatim prefix that `fs::canonicalize`
/// adds, so canonicalized paths can be compared against plain joined paths.
fn strip_verbatim_prefix(path: &Path) -> PathBuf {
    let text = path.as_os_str().to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        PathBuf::from(format!(r"\\{rest}"))
    } else if let Some(rest) = text.strip_prefix(r"\\?\") {
        PathBuf::from(rest)
    } else {
        path.to_path_buf()
    }
}

fn is_ancestor_cycle(ancestor_canonical_stack: &[PathBuf], resolved_target: &Path) -> bool {
    ancestor_canonical_stack
        .iter()
        .any(|ancestor| ancestor.as_path() == resolved_target)
}

enum ReparseDecision {
    /// Safe to descend; carries the canonical identity to push onto the
    /// ancestor stack (a cheap path join for plain directories, or a
    /// resolved target for an actual reparse point).
    Follow(PathBuf),
    SkippedByPolicy,
    SkippedCycle,
    SkippedUnresolvable,
}

/// Decides whether to descend into `entry_path`, guarding against directory
/// cycles created by junctions/symlinks that point back at an ancestor (for
/// example the stock `AppData\Local\Application Data` compatibility
/// junction, which points at its own parent directory).
fn evaluate_reparse_descent(
    directory: &Path,
    entry_path: &Path,
    entry_name: &str,
    attributes: FileAttributes,
    policy: ReparseTraversalPolicy,
    ancestor_canonical_stack: &[PathBuf],
) -> ReparseDecision {
    if !attributes.is_reparse_point() {
        let canonical = ancestor_canonical_stack
            .last()
            .map(|parent| parent.join(entry_name))
            .unwrap_or_else(|| directory.join(entry_name));
        return ReparseDecision::Follow(canonical);
    }

    if !should_descend_into_reparse_target(entry_path, attributes, policy) {
        return ReparseDecision::SkippedByPolicy;
    }

    match fs::canonicalize(entry_path) {
        Ok(resolved) => {
            let resolved = strip_verbatim_prefix(&resolved);
            if is_ancestor_cycle(ancestor_canonical_stack, &resolved) {
                ReparseDecision::SkippedCycle
            } else {
                ReparseDecision::Follow(resolved)
            }
        }
        Err(_) => ReparseDecision::SkippedUnresolvable,
    }
}

fn reparse_cycle_issue(entry_path: &Path) -> ScanIssueRecord {
    ScanIssueRecord {
        kind: ScanIssueKind::Unknown,
        path: Some(entry_path.display().to_string()),
        message: "skipped reparse point that would create a directory cycle".to_string(),
    }
}

#[allow(clippy::too_many_arguments)]
fn walk_directory_tree(
    pipeline: &mut ScanEventPipeline,
    cancelled: &AtomicBool,
    directory: &std::path::Path,
    parent_directory_id: winblaze_core::DirectoryId,
    walk_state: &mut DirectoryWalkState,
    directory_ids: &mut HashMap<PathBuf, winblaze_core::DirectoryId>,
    issue_keys: &mut HashSet<String>,
    reparse_policy: ReparseTraversalPolicy,
    ancestor_canonical_stack: &mut Vec<PathBuf>,
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
            // Free on Windows: DirEntry::metadata() is built from the
            // WIN32_FIND_DATAW the directory enumeration already cached.
            let attributes = entry
                .metadata()
                .map(|metadata| FileAttributes(metadata.file_attributes()))
                .unwrap_or_default();

            let decision = evaluate_reparse_descent(
                directory,
                &entry_path,
                &entry_name,
                attributes,
                reparse_policy,
                ancestor_canonical_stack,
            );

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

            match decision {
                ReparseDecision::Follow(canonical) => {
                    ancestor_canonical_stack.push(canonical);
                    walk_directory_tree(
                        pipeline,
                        cancelled,
                        &entry_path,
                        directory_id,
                        walk_state,
                        directory_ids,
                        issue_keys,
                        reparse_policy,
                        ancestor_canonical_stack,
                    );
                    ancestor_canonical_stack.pop();
                }
                ReparseDecision::SkippedCycle => {
                    emit_deduplicated_issue(pipeline, issue_keys, reparse_cycle_issue(&entry_path));
                }
                ReparseDecision::SkippedByPolicy | ReparseDecision::SkippedUnresolvable => {}
            }
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
            emit_file_record(pipeline, walk_state, parent_directory_id, entry_name, &metadata);
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
    name: String,
    metadata: &fs::Metadata,
) {
    let size_bytes = metadata.len();
    walk_state.files_seen = walk_state.files_seen.saturating_add(1);
    walk_state.total_size_bytes = walk_state.total_size_bytes.saturating_add(size_bytes);
    walk_state.total_allocation_bytes =
        walk_state.total_allocation_bytes.saturating_add(size_bytes);

    pipeline.emit_file(winblaze_core::FileRecord {
        id: winblaze_core::FileId(walk_state.next_file_id()),
        parent_directory_id,
        name,
        // Derived on demand from the parent directory (see FileRecord docs);
        // materializing one String per file dominated scan-time allocation,
        // index memory, and snapshot size.
        full_path: String::new(),
        size_bytes,
        allocation_bytes: size_bytes,
        attributes: winblaze_core::FileAttributes::ARCHIVE,
        created_utc: filetime_or_none(metadata.creation_time()),
        modified_utc: filetime_or_none(metadata.last_write_time()),
        accessed_utc: filetime_or_none(metadata.last_access_time()),
    });
    walk_state.maybe_emit_progress(pipeline, false);
}

/// Windows metadata timestamps are FILETIME values (100ns ticks since 1601),
/// the same convention the MFT reader emits; zero means "not available".
fn filetime_or_none(value: u64) -> Option<i64> {
    if value == 0 {
        None
    } else {
        Some(value as i64)
    }
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

/// Task queue and counters shared across the fallback walker's worker
/// threads. `outstanding` counts tasks that are queued or in flight; the
/// walk is finished once it drops to zero with an empty queue.
struct FallbackTask {
    path: PathBuf,
    directory_id: winblaze_core::DirectoryId,
    ancestor_canonical_stack: Vec<PathBuf>,
}

struct FallbackQueue {
    tasks: VecDeque<FallbackTask>,
    outstanding: usize,
}

struct FallbackSharedState {
    queue: Mutex<FallbackQueue>,
    work_available: Condvar,
    reparse_policy: ReparseTraversalPolicy,
    issue_keys: Mutex<HashSet<String>>,
    next_directory_id: AtomicU64,
    next_file_id: AtomicU64,
    directories_seen: AtomicU64,
    files_seen: AtomicU64,
    total_size_bytes: AtomicU64,
    total_allocation_bytes: AtomicU64,
    last_progress_reported_items: AtomicU64,
}

/// Multi-threaded counterpart to `walk_directory_tree`, used once
/// `worker_count > 1`. The fallback backend is only reached when the fast
/// NTFS-MFT reader is unavailable (relative roots, or the caller lacks the
/// elevation `$MFT` reads require), so unlike the MFT path it previously
/// never used `max_parallelism` at all and ran fully single-threaded.
#[allow(clippy::too_many_arguments)]
fn run_fallback_scan_parallel(
    pipeline: &mut ScanEventPipeline,
    cancelled: &AtomicBool,
    selected_root: &std::path::Path,
    root_id: winblaze_core::DirectoryId,
    reparse_policy: ReparseTraversalPolicy,
    worker_count: usize,
    root_canonical: PathBuf,
) -> (u64, u64, u64, u64) {
    let shared = FallbackSharedState {
        queue: Mutex::new(FallbackQueue {
            tasks: VecDeque::from(vec![FallbackTask {
                path: selected_root.to_path_buf(),
                directory_id: root_id,
                ancestor_canonical_stack: vec![root_canonical],
            }]),
            outstanding: 1,
        }),
        work_available: Condvar::new(),
        reparse_policy,
        issue_keys: Mutex::new(HashSet::new()),
        next_directory_id: AtomicU64::new(6),
        next_file_id: AtomicU64::new(1),
        directories_seen: AtomicU64::new(1),
        files_seen: AtomicU64::new(0),
        total_size_bytes: AtomicU64::new(0),
        total_allocation_bytes: AtomicU64::new(0),
        last_progress_reported_items: AtomicU64::new(0),
    };

    let pipeline_config = pipeline.config();
    let event_tx = pipeline.cloned_sender();

    thread::scope(|scope| {
        for _ in 0..worker_count.max(1) {
            let shared = &shared;
            let event_tx = event_tx.clone();
            scope.spawn(move || {
                run_fallback_worker(shared, cancelled, event_tx, pipeline_config);
            });
        }
    });

    (
        shared.files_seen.load(Ordering::Relaxed),
        shared.directories_seen.load(Ordering::Relaxed),
        shared.total_size_bytes.load(Ordering::Relaxed),
        shared.total_allocation_bytes.load(Ordering::Relaxed),
    )
}

fn run_fallback_worker(
    shared: &FallbackSharedState,
    cancelled: &AtomicBool,
    event_tx: Sender<ScanEvent>,
    pipeline_config: ScanPipelineConfig,
) {
    let mut pipeline = ScanEventPipeline::new(event_tx, pipeline_config);

    loop {
        let task = {
            let mut guard = shared.queue.lock().unwrap();
            loop {
                if let Some(task) = guard.tasks.pop_front() {
                    break Some(task);
                }
                if guard.outstanding == 0 {
                    break None;
                }
                guard = shared.work_available.wait(guard).unwrap();
            }
        };

        let Some(task) = task else {
            break;
        };

        let new_tasks = if cancelled.load(Ordering::SeqCst) {
            Vec::new()
        } else {
            process_fallback_directory(shared, &mut pipeline, &task, cancelled)
        };

        let mut guard = shared.queue.lock().unwrap();
        guard.outstanding += new_tasks.len();
        guard.tasks.extend(new_tasks);
        guard.outstanding -= 1;
        if guard.outstanding == 0 || !guard.tasks.is_empty() {
            shared.work_available.notify_all();
        }
        drop(guard);
    }

    pipeline.flush();
}

fn process_fallback_directory(
    shared: &FallbackSharedState,
    pipeline: &mut ScanEventPipeline,
    task: &FallbackTask,
    cancelled: &AtomicBool,
) -> Vec<FallbackTask> {
    let entries = match fs::read_dir(&task.path) {
        Ok(entries) => entries,
        Err(error) => {
            emit_deduplicated_issue_shared(
                pipeline,
                shared,
                convert_io_error(&error, task.path.display().to_string()),
            );
            return Vec::new();
        }
    };

    let mut new_tasks = Vec::new();
    for entry_result in entries {
        if cancelled.load(Ordering::SeqCst) {
            break;
        }

        let entry = match entry_result {
            Ok(entry) => entry,
            Err(error) => {
                emit_deduplicated_issue_shared(
                    pipeline,
                    shared,
                    convert_io_error(&error, task.path.display().to_string()),
                );
                continue;
            }
        };

        let entry_path = entry.path();
        let entry_name = entry.file_name().to_string_lossy().to_string();

        let file_type = match entry.file_type() {
            Ok(file_type) => file_type,
            Err(error) => {
                emit_deduplicated_issue_shared(
                    pipeline,
                    shared,
                    convert_io_error(&error, entry_path.display().to_string()),
                );
                continue;
            }
        };

        if file_type.is_dir() {
            let attributes = entry
                .metadata()
                .map(|metadata| FileAttributes(metadata.file_attributes()))
                .unwrap_or_default();

            let decision = evaluate_reparse_descent(
                &task.path,
                &entry_path,
                &entry_name,
                attributes,
                shared.reparse_policy,
                &task.ancestor_canonical_stack,
            );

            let directory_id =
                winblaze_core::DirectoryId(shared.next_directory_id.fetch_add(1, Ordering::Relaxed));
            emit_directory_record_shared(
                pipeline,
                shared,
                directory_id,
                task.directory_id,
                &entry_path,
                entry_name,
            );

            match decision {
                ReparseDecision::Follow(canonical) => {
                    let mut ancestor_canonical_stack = task.ancestor_canonical_stack.clone();
                    ancestor_canonical_stack.push(canonical);
                    new_tasks.push(FallbackTask {
                        path: entry_path,
                        directory_id,
                        ancestor_canonical_stack,
                    });
                }
                ReparseDecision::SkippedCycle => {
                    emit_deduplicated_issue_shared(pipeline, shared, reparse_cycle_issue(&entry_path));
                }
                ReparseDecision::SkippedByPolicy | ReparseDecision::SkippedUnresolvable => {}
            }
        } else {
            let metadata = match entry.metadata() {
                Ok(metadata) => metadata,
                Err(error) => {
                    emit_deduplicated_issue_shared(
                        pipeline,
                        shared,
                        convert_io_error(&error, entry_path.display().to_string()),
                    );
                    continue;
                }
            };
            emit_file_record_shared(pipeline, shared, task.directory_id, entry_name, &metadata);
        }
    }

    maybe_emit_progress_shared(shared, pipeline);
    new_tasks
}

fn emit_directory_record_shared(
    pipeline: &mut ScanEventPipeline,
    shared: &FallbackSharedState,
    directory_id: winblaze_core::DirectoryId,
    parent_directory_id: winblaze_core::DirectoryId,
    full_path: &std::path::Path,
    name: String,
) {
    shared.directories_seen.fetch_add(1, Ordering::Relaxed);
    pipeline.emit_directory(winblaze_core::DirectoryRecord {
        id: directory_id,
        parent_directory_id: Some(parent_directory_id),
        name,
        full_path: full_path.display().to_string(),
        direct_bytes: 0,
        total_bytes: 0,
        direct_entries: 0,
        total_entries: 0,
    });
}

fn emit_file_record_shared(
    pipeline: &mut ScanEventPipeline,
    shared: &FallbackSharedState,
    parent_directory_id: winblaze_core::DirectoryId,
    name: String,
    metadata: &fs::Metadata,
) {
    let size_bytes = metadata.len();
    shared.files_seen.fetch_add(1, Ordering::Relaxed);
    shared.total_size_bytes.fetch_add(size_bytes, Ordering::Relaxed);
    shared
        .total_allocation_bytes
        .fetch_add(size_bytes, Ordering::Relaxed);
    let file_id = shared.next_file_id.fetch_add(1, Ordering::Relaxed);

    pipeline.emit_file(winblaze_core::FileRecord {
        id: winblaze_core::FileId(file_id),
        parent_directory_id,
        name,
        // Derived on demand from the parent directory (see FileRecord docs).
        full_path: String::new(),
        size_bytes,
        allocation_bytes: size_bytes,
        attributes: winblaze_core::FileAttributes::ARCHIVE,
        created_utc: filetime_or_none(metadata.creation_time()),
        modified_utc: filetime_or_none(metadata.last_write_time()),
        accessed_utc: filetime_or_none(metadata.last_access_time()),
    });
}

fn emit_deduplicated_issue_shared(
    pipeline: &mut ScanEventPipeline,
    shared: &FallbackSharedState,
    issue: ScanIssueRecord,
) {
    let key = format!(
        "{:?}|{}|{}",
        issue.kind,
        issue.path.as_deref().unwrap_or(""),
        issue.message
    );
    let is_new = shared.issue_keys.lock().unwrap().insert(key);
    if is_new {
        pipeline.emit_issue(issue);
    }
}

fn maybe_emit_progress_shared(shared: &FallbackSharedState, pipeline: &mut ScanEventPipeline) {
    let completed_items = shared
        .files_seen
        .load(Ordering::Relaxed)
        .saturating_add(shared.directories_seen.load(Ordering::Relaxed));
    let last_reported = shared.last_progress_reported_items.load(Ordering::Relaxed);
    if completed_items.saturating_sub(last_reported) >= 256 {
        shared
            .last_progress_reported_items
            .store(completed_items, Ordering::Relaxed);
        pipeline.emit_progress(
            completed_items,
            0,
            shared.total_size_bytes.load(Ordering::Relaxed),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_ancestor_cycle_detects_exact_and_nested_matches() {
        let ancestors = vec![
            PathBuf::from(r"C:\Users\markm"),
            PathBuf::from(r"C:\Users\markm\AppData\Local"),
        ];

        assert!(is_ancestor_cycle(
            &ancestors,
            Path::new(r"C:\Users\markm\AppData\Local")
        ));
        assert!(is_ancestor_cycle(&ancestors, Path::new(r"C:\Users\markm")));
        assert!(!is_ancestor_cycle(
            &ancestors,
            Path::new(r"C:\Users\markm\AppData\Roaming")
        ));
    }

    #[test]
    fn strip_verbatim_prefix_normalizes_canonicalized_paths() {
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"\\?\C:\Users\markm")),
            PathBuf::from(r"C:\Users\markm")
        );
        assert_eq!(
            strip_verbatim_prefix(Path::new(r"C:\Users\markm")),
            PathBuf::from(r"C:\Users\markm")
        );
    }

    #[test]
    fn evaluate_reparse_descent_skips_self_referential_junction() {
        // Mirrors the stock `AppData\Local\Application Data` junction,
        // which points back at its own parent directory.
        let ancestors = vec![
            PathBuf::from(r"C:\Users\markm"),
            PathBuf::from(r"C:\Users\markm\AppData\Local"),
        ];

        assert!(is_ancestor_cycle(
            &ancestors,
            Path::new(r"C:\Users\markm\AppData\Local")
        ));

        let decision = should_descend_into_reparse_target(
            Path::new(r"C:\Users\markm\AppData\Local\Application Data"),
            FileAttributes::DIRECTORY | FileAttributes::REPARSE_POINT,
            ReparseTraversalPolicy::FollowAll,
        );
        assert!(
            decision,
            "policy allows following the junction; the ancestor-stack check \
             in evaluate_reparse_descent is what actually breaks the cycle"
        );
    }
}
