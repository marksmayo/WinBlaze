use std::{
    ffi::CString,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    ptr::null_mut,
    sync::{mpsc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use winblaze_core::{
    DirectoryRecord, FileChangeKind, FileChangeSet, FileRecord, ScanEvent, ScanProgress,
    ScanSession, ScanState, VolumeRecord,
};
use winblaze_index::{
    BufferedIndexTransaction, IndexBackend, IndexRepository, IndexTransaction,
    SqliteIndexRepository,
};
use winblaze_scanner::{ScanController, ScanRequest, ScanRuntimeConfig};

use crate::api::{
    WbCStringView, WbCatalogCallback, WbCatalogEntry, WbEvent, WbEventCallback, WbEventKind,
    WbIncrementalChangeSummary, WbIndexSnapshotStats, WbNativeError, WbScanSessionHandle,
    WbScanSummary,
};

const MAX_JSON_LOG_BYTES: u64 = 2 * 1024 * 1024;
const SNAPSHOT_ENTRY_LIMIT: usize = 8192;
const UI_LIVE_CATALOG_EVENT_LIMIT: usize = SNAPSHOT_ENTRY_LIMIT;
const UI_PROGRESS_MIN_ITEM_DELTA: u64 = 10_000;
const UI_PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(150);

pub struct NativeSession {
    controller: ScanController,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
    scan_handle: Mutex<Option<winblaze_scanner::ScanHandle>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScanPersistenceMode {
    ReplaceSnapshot,
    IncrementalRescan,
}

struct OwnedCatalogEntry {
    entry: WbCatalogEntry,
    _strings: Vec<CString>,
}

struct OwnedEvent {
    event: WbEvent,
    _strings: Vec<CString>,
}

struct UiEventForwarder {
    callback: WbEventCallback,
    user_data: usize,
    live_catalog_events: usize,
    last_progress_at: Option<Instant>,
    last_progress_items: u64,
}

impl UiEventForwarder {
    fn new(callback: WbEventCallback, user_data: usize) -> Self {
        Self {
            callback,
            user_data,
            live_catalog_events: 0,
            last_progress_at: None,
            last_progress_items: 0,
        }
    }

    fn forward(&mut self, event: &ScanEvent) {
        let Some(cb) = self.callback else {
            return;
        };
        if !self.should_forward(event) {
            return;
        }

        let native_event = convert_event(event.clone());
        cb(
            &native_event.event as *const WbEvent,
            self.user_data as *mut core::ffi::c_void,
        );
    }

    fn should_forward(&mut self, event: &ScanEvent) -> bool {
        match event {
            ScanEvent::DirectoryFound(_) | ScanEvent::FileFound(_) => {
                if self.live_catalog_events >= UI_LIVE_CATALOG_EVENT_LIMIT {
                    return false;
                }
                self.live_catalog_events += 1;
                true
            }
            ScanEvent::Progress(progress) => self.should_forward_progress(progress),
            ScanEvent::SessionStarted(_)
            | ScanEvent::VolumeDiscovered(_)
            | ScanEvent::Issue(_)
            | ScanEvent::Summary(_)
            | ScanEvent::Completed
            | ScanEvent::Cancelled
            | ScanEvent::Failed(_) => true,
        }
    }

    fn should_forward_progress(&mut self, progress: &ScanProgress) -> bool {
        let now = Instant::now();
        let item_delta = progress
            .completed_items
            .saturating_sub(self.last_progress_items);
        let interval_elapsed = self
            .last_progress_at
            .is_none_or(|last| now.duration_since(last) >= UI_PROGRESS_MIN_INTERVAL);
        let is_complete =
            progress.total_items > 0 && progress.completed_items >= progress.total_items;

        if self.last_progress_at.is_none()
            || is_complete
            || item_delta >= UI_PROGRESS_MIN_ITEM_DELTA
            || interval_elapsed
        {
            self.last_progress_at = Some(now);
            self.last_progress_items = progress.completed_items;
            return true;
        }

        false
    }
}

impl NativeSession {
    fn new(
        callback: WbEventCallback,
        user_data: *mut core::ffi::c_void,
        root_path: String,
        persistence_mode: ScanPersistenceMode,
    ) -> Self {
        let (controller, rx) = ScanController::channel();
        let root_path_for_config = root_path.clone();
        let session = Self {
            controller,
            worker: Mutex::new(None),
            scan_handle: Mutex::new(None),
        };

        let max_parallelism = thread::available_parallelism()
            .map(|parallelism| parallelism.get())
            .unwrap_or(4);
        let request = ScanRequest {
            root_path: root_path.clone().into(),
            config: ScanRuntimeConfig {
                root_path: root_path_for_config.into(),
                max_parallelism,
                ..ScanRuntimeConfig::default()
            },
        };
        let scan_handle = session.controller.start_scan(request);
        *session.scan_handle.lock().expect("lock poisoned") = Some(scan_handle);

        let user_data = user_data as usize;
        let index_root = index_storage_root();
        let worker = thread::spawn(move || {
            forward_events(rx, callback, user_data, index_root, persistence_mode)
        });
        *session.worker.lock().expect("lock poisoned") = Some(worker);

        session
    }

    fn cancel(&self) {
        if let Some(handle) = self.scan_handle.lock().expect("lock poisoned").as_ref() {
            handle.cancel();
        }
    }

    fn join(self) {
        self.cancel();
        if let Some(handle) = self.scan_handle.lock().expect("lock poisoned").take() {
            handle.join();
        }
        if let Some(worker) = self.worker.lock().expect("lock poisoned").take() {
            let _ = worker.join();
        }
    }
}

fn forward_events(
    rx: mpsc::Receiver<ScanEvent>,
    callback: WbEventCallback,
    user_data: usize,
    index_root: PathBuf,
    persistence_mode: ScanPersistenceMode,
) {
    let mut repository = match persistence_mode {
        ScanPersistenceMode::ReplaceSnapshot => {
            SqliteIndexRepository::open_empty(&index_root, IndexBackend::BinaryCache)
        }
        ScanPersistenceMode::IncrementalRescan => {
            SqliteIndexRepository::open(&index_root, IndexBackend::BinaryCache)
        }
    };
    let mut transaction = BufferedIndexTransaction::default();
    let log_path = structured_log_path(&index_root);
    let mut flushed = false;
    let mut ui_forwarder = UiEventForwarder::new(callback, user_data);

    for event in rx {
        append_scan_event_log(&log_path, &event);
        persist_scan_event(&mut transaction, &event);
        ui_forwarder.forward(&event);

        if should_flush_index(&event, persistence_mode) {
            let result = apply_scan_transaction(&mut repository, &transaction, persistence_mode);
            if let Ok(Some(change_set)) = &result {
                emit_incremental_change_summary(callback, user_data, change_set);
            }
            flushed = result.is_ok();
            append_index_flush_log(&log_path, &event, result.is_ok());
        }
    }

    if !flushed {
        let result = apply_scan_transaction(&mut repository, &transaction, persistence_mode);
        if let Ok(Some(change_set)) = &result {
            emit_incremental_change_summary(callback, user_data, change_set);
        }
        append_index_flush_log(&log_path, &ScanEvent::Completed, result.is_ok());
    }
}

fn should_flush_index(event: &ScanEvent, persistence_mode: ScanPersistenceMode) -> bool {
    match persistence_mode {
        ScanPersistenceMode::ReplaceSnapshot => matches!(
            event,
            ScanEvent::SessionStarted(_)
                | ScanEvent::VolumeDiscovered(_)
                | ScanEvent::Summary(_)
                | ScanEvent::Completed
                | ScanEvent::Cancelled
                | ScanEvent::Failed(_)
        ),
        ScanPersistenceMode::IncrementalRescan => {
            matches!(event, ScanEvent::Summary(_))
        }
    }
}

fn apply_scan_transaction(
    repository: &mut SqliteIndexRepository,
    transaction: &BufferedIndexTransaction,
    persistence_mode: ScanPersistenceMode,
) -> Result<Option<FileChangeSet>, winblaze_index::IndexStorageError> {
    match persistence_mode {
        ScanPersistenceMode::ReplaceSnapshot => {
            repository.apply_transaction(transaction).map(|_| None)
        }
        ScanPersistenceMode::IncrementalRescan => repository
            .apply_path_matched_incremental_transaction(transaction)
            .map(Some),
    }
}

fn emit_incremental_change_summary(
    callback: WbEventCallback,
    user_data: usize,
    change_set: &FileChangeSet,
) {
    let Some(cb) = callback else {
        return;
    };
    let native_event = WbEvent {
        kind: WbEventKind::IncrementalChanges,
        incremental_changes: incremental_change_summary(change_set),
        ..WbEvent::default()
    };
    cb(
        &native_event as *const WbEvent,
        user_data as *mut core::ffi::c_void,
    );
}

fn incremental_change_summary(change_set: &FileChangeSet) -> WbIncrementalChangeSummary {
    let mut summary = WbIncrementalChangeSummary::default();
    for change in &change_set.changes {
        match change.kind {
            FileChangeKind::Added => summary.added += 1,
            FileChangeKind::Removed => summary.removed += 1,
            FileChangeKind::Modified => summary.modified += 1,
            FileChangeKind::Renamed => summary.renamed += 1,
            FileChangeKind::Moved => summary.moved += 1,
        }
    }
    summary
}

fn persist_scan_event(transaction: &mut BufferedIndexTransaction, event: &ScanEvent) {
    match event {
        ScanEvent::SessionStarted(volume) | ScanEvent::VolumeDiscovered(volume) => {
            transaction.upsert_volume(volume);
            transaction.upsert_session(&ScanSession {
                session_id: 1,
                volume_id: volume.id,
                root_path: volume.mount_point.clone(),
                state: ScanState::Scanning,
                progress: Default::default(),
            });
        }
        ScanEvent::DirectoryFound(directory) => transaction.upsert_directory(directory),
        ScanEvent::FileFound(file) => transaction.upsert_file(file),
        ScanEvent::Progress(_) | ScanEvent::Issue(_) | ScanEvent::Summary(_) => {}
        ScanEvent::Completed | ScanEvent::Cancelled | ScanEvent::Failed(_) => {
            transaction.upsert_session(&ScanSession {
                session_id: 1,
                volume_id: Default::default(),
                root_path: String::new(),
                state: match event {
                    ScanEvent::Completed => ScanState::Completed,
                    ScanEvent::Cancelled => ScanState::Cancelled,
                    _ => ScanState::Failed,
                },
                progress: Default::default(),
            });
        }
    }
}

fn index_storage_root() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join("WinBlaze").join("index")
}

fn structured_log_path(index_root: &std::path::Path) -> PathBuf {
    index_root
        .parent()
        .map(|root| root.join("logs"))
        .unwrap_or_else(std::env::temp_dir)
        .join("events.jsonl")
}

fn append_scan_event_log(path: &std::path::Path, event: &ScanEvent) {
    match event {
        ScanEvent::DirectoryFound(_) | ScanEvent::FileFound(_) => return,
        _ => {}
    }

    let payload = match event {
        ScanEvent::SessionStarted(volume) => format!(
            r#""event":"scanner.session_started","root":"{}","volume_id":{}"#,
            json_escape(&volume.mount_point),
            volume.id.0
        ),
        ScanEvent::VolumeDiscovered(volume) => format!(
            r#""event":"scanner.volume_discovered","root":"{}","volume_id":{}"#,
            json_escape(&volume.mount_point),
            volume.id.0
        ),
        ScanEvent::Progress(progress) => format!(
            r#""event":"scanner.progress","items_done":{},"items_total":{},"bytes_done":{},"bytes_total":{}"#,
            progress.completed_items,
            progress.total_items,
            progress.completed_bytes,
            progress.total_bytes
        ),
        ScanEvent::Summary(summary) => format!(
            r#""event":"scanner.summary","files":{},"directories":{},"bytes":{},"allocated_bytes":{}"#,
            summary.files_seen,
            summary.directories_seen,
            summary.total_size_bytes,
            summary.total_allocation_bytes
        ),
        ScanEvent::Completed => r#""event":"scanner.completed""#.to_string(),
        ScanEvent::Cancelled => r#""event":"scanner.cancelled""#.to_string(),
        ScanEvent::Failed(message) => format!(
            r#""event":"scanner.failed","message":"{}""#,
            json_escape(message)
        ),
        ScanEvent::Issue(issue) => format!(
            r#""event":"scanner.issue","kind":"{:?}","path":"{}","message":"{}""#,
            issue.kind,
            json_escape(issue.path.as_deref().unwrap_or("")),
            json_escape(&issue.message)
        ),
        ScanEvent::DirectoryFound(_) | ScanEvent::FileFound(_) => unreachable!(),
    };

    append_json_log(path, &payload);
}

fn append_index_flush_log(path: &std::path::Path, event: &ScanEvent, ok: bool) {
    append_json_log(
        path,
        &format!(
            r#""event":"index.flush","trigger":"{}","ok":{}"#,
            event_name(event),
            ok
        ),
    );
}

fn append_json_log(path: &std::path::Path, payload: &str) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    rotate_log_if_needed(path, MAX_JSON_LOG_BYTES);
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let _ = writeln!(
        file,
        r#"{{"ts_ms":{},"component":"native",{} }}"#,
        now_ms(),
        payload
    );
}

fn rotate_log_if_needed(path: &std::path::Path, max_bytes: u64) {
    let Ok(metadata) = fs::metadata(path) else {
        return;
    };
    if metadata.len() < max_bytes {
        return;
    }

    let rotated = path.with_extension(
        match path.extension().and_then(|extension| extension.to_str()) {
            Some(extension) if !extension.is_empty() => format!("{extension}.1"),
            _ => "1".to_string(),
        },
    );
    let _ = fs::remove_file(&rotated);
    let _ = fs::rename(path, rotated);
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

fn event_name(event: &ScanEvent) -> &'static str {
    match event {
        ScanEvent::SessionStarted(_) => "session_started",
        ScanEvent::VolumeDiscovered(_) => "volume_discovered",
        ScanEvent::DirectoryFound(_) => "directory_found",
        ScanEvent::FileFound(_) => "file_found",
        ScanEvent::Progress(_) => "progress",
        ScanEvent::Summary(_) => "summary",
        ScanEvent::Completed => "completed",
        ScanEvent::Cancelled => "cancelled",
        ScanEvent::Failed(_) => "failed",
        ScanEvent::Issue(_) => "issue",
    }
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

fn convert_event(event: ScanEvent) -> OwnedEvent {
    match event {
        ScanEvent::SessionStarted(volume) => {
            let catalog = catalog_entry_from_volume(&volume);
            OwnedEvent {
                event: WbEvent {
                    kind: WbEventKind::SessionStarted,
                    catalog_entry: catalog.entry,
                    ..WbEvent::default()
                },
                _strings: catalog._strings,
            }
        }
        ScanEvent::VolumeDiscovered(volume) => {
            let catalog = catalog_entry_from_volume(&volume);
            OwnedEvent {
                event: WbEvent {
                    kind: WbEventKind::VolumeDiscovered,
                    catalog_entry: catalog.entry,
                    ..WbEvent::default()
                },
                _strings: catalog._strings,
            }
        }
        ScanEvent::DirectoryFound(directory) => {
            let catalog = catalog_entry_from_directory(&directory);
            OwnedEvent {
                event: WbEvent {
                    kind: WbEventKind::DirectoryFound,
                    catalog_entry: catalog.entry,
                    ..WbEvent::default()
                },
                _strings: catalog._strings,
            }
        }
        ScanEvent::FileFound(file) => {
            let catalog = catalog_entry_from_file(&file);
            OwnedEvent {
                event: WbEvent {
                    kind: WbEventKind::FileFound,
                    catalog_entry: catalog.entry,
                    ..WbEvent::default()
                },
                _strings: catalog._strings,
            }
        }
        ScanEvent::Progress(ScanProgress {
            completed_items,
            total_items,
            completed_bytes,
            total_bytes,
        }) => OwnedEvent {
            event: WbEvent {
                kind: WbEventKind::Progress,
                progress_items_done: completed_items,
                progress_items_total: total_items,
                progress_bytes_done: completed_bytes,
                progress_bytes_total: total_bytes,
                ..WbEvent::default()
            },
            _strings: Vec::new(),
        },
        ScanEvent::Summary(summary) => OwnedEvent {
            event: WbEvent {
                kind: WbEventKind::Summary,
                summary: WbScanSummary {
                    files_seen: summary.files_seen,
                    directories_seen: summary.directories_seen,
                    total_size_bytes: summary.total_size_bytes,
                    total_allocation_bytes: summary.total_allocation_bytes,
                },
                ..WbEvent::default()
            },
            _strings: Vec::new(),
        },
        ScanEvent::Completed => OwnedEvent {
            event: WbEvent {
                kind: WbEventKind::Completed,
                ..WbEvent::default()
            },
            _strings: Vec::new(),
        },
        ScanEvent::Cancelled => OwnedEvent {
            event: WbEvent {
                kind: WbEventKind::Cancelled,
                ..WbEvent::default()
            },
            _strings: Vec::new(),
        },
        ScanEvent::Issue(issue) => {
            let message = match issue.path.as_deref() {
                Some(path) if !path.is_empty() => format!("{path}: {}", issue.message),
                _ => issue.message,
            };
            let mut strings = Vec::new();
            let message = c_view(message, &mut strings);
            let code = match issue.kind {
                winblaze_core::ScanIssueKind::PermissionDenied => 10,
                winblaze_core::ScanIssueKind::NotFound => 11,
                winblaze_core::ScanIssueKind::SharingViolation => 12,
                winblaze_core::ScanIssueKind::TransientIo => 13,
                winblaze_core::ScanIssueKind::UnsupportedFilesystem => 14,
                winblaze_core::ScanIssueKind::Unknown => 15,
            };
            OwnedEvent {
                event: WbEvent {
                    kind: WbEventKind::Issue,
                    error: WbNativeError { code, message },
                    ..WbEvent::default()
                },
                _strings: strings,
            }
        }
        ScanEvent::Failed(message) => {
            let mut strings = Vec::new();
            let message = c_view(message, &mut strings);
            OwnedEvent {
                event: WbEvent {
                    kind: WbEventKind::Failed,
                    error: WbNativeError { code: 1, message },
                    ..WbEvent::default()
                },
                _strings: strings,
            }
        }
    }
}

fn c_view(text: String, strings: &mut Vec<CString>) -> WbCStringView {
    let c_string = CString::new(text).unwrap_or_default();
    let view = WbCStringView {
        ptr: c_string.as_ptr(),
        len: c_string.as_bytes().len(),
    };
    strings.push(c_string);
    view
}

fn load_catalog_entries_with_stats(
    callback: WbCatalogCallback,
    user_data: *mut core::ffi::c_void,
) -> WbIndexSnapshotStats {
    let repository = SqliteIndexRepository::open(&index_storage_root(), IndexBackend::BinaryCache);
    let snapshot = repository.snapshot();
    let volumes = repository.snapshot_volumes();
    let directories = repository.snapshot_directories();
    let files = repository.snapshot_files();

    let stats = WbIndexSnapshotStats {
        volumes: volumes.len() as u64,
        directories: directories.len() as u64,
        files: files.len() as u64,
        entries_emitted_limit: SNAPSHOT_ENTRY_LIMIT as u64,
        cache_read_bytes: snapshot.cache_read_bytes,
        cache_read_millis: snapshot.cache_read_millis,
        cache_decode_millis: snapshot.cache_decode_millis,
        cache_loaded_from_backup: u8::from(snapshot.cache_loaded_from_backup),
    };

    if let Some(cb) = callback {
        let mut emitted = 0usize;
        for volume in volumes {
            if emitted >= SNAPSHOT_ENTRY_LIMIT {
                return stats;
            }
            let entry = catalog_entry_from_volume(&volume);
            cb(&entry.entry as *const WbCatalogEntry, user_data);
            emitted += 1;
        }
        for directory in directories {
            if emitted >= SNAPSHOT_ENTRY_LIMIT {
                return stats;
            }
            let entry = catalog_entry_from_directory(&directory);
            cb(&entry.entry as *const WbCatalogEntry, user_data);
            emitted += 1;
        }
        for file in files {
            if emitted >= SNAPSHOT_ENTRY_LIMIT {
                return stats;
            }
            let entry = catalog_entry_from_file(&file);
            cb(&entry.entry as *const WbCatalogEntry, user_data);
            emitted += 1;
        }
    }

    stats
}

fn load_catalog_entries(callback: WbCatalogCallback, user_data: *mut core::ffi::c_void) {
    load_catalog_entries_with_stats(callback, user_data);
}

fn index_snapshot_stats() -> WbIndexSnapshotStats {
    load_catalog_entries_with_stats(None, std::ptr::null_mut())
}

fn catalog_entry_from_volume(volume: &VolumeRecord) -> OwnedCatalogEntry {
    let mut strings = Vec::new();
    let entry = WbCatalogEntry {
        name: c_view(
            volume
                .label
                .clone()
                .unwrap_or_else(|| volume.mount_point.clone()),
            &mut strings,
        ),
        path: c_view(volume.mount_point.clone(), &mut strings),
        kind: c_view("Volume".to_string(), &mut strings),
        size_text: c_view(format_size(volume.total_bytes), &mut strings),
        description: c_view(
            format!("Root directory id {}", volume.root_directory_id.0),
            &mut strings,
        ),
        size_bytes: volume.total_bytes,
        modified_utc: 0,
        has_modified_utc: 0,
    };
    OwnedCatalogEntry {
        entry,
        _strings: strings,
    }
}

fn catalog_entry_from_directory(directory: &DirectoryRecord) -> OwnedCatalogEntry {
    let mut strings = Vec::new();
    let entry = WbCatalogEntry {
        name: c_view(directory.name.clone(), &mut strings),
        path: c_view(directory.full_path.clone(), &mut strings),
        kind: c_view("Directory".to_string(), &mut strings),
        size_text: c_view(format_size(directory.total_bytes), &mut strings),
        description: c_view(
            format!(
                "{} direct entries, {} total entries",
                directory.direct_entries, directory.total_entries
            ),
            &mut strings,
        ),
        size_bytes: directory.total_bytes,
        modified_utc: 0,
        has_modified_utc: 0,
    };
    OwnedCatalogEntry {
        entry,
        _strings: strings,
    }
}

fn catalog_entry_from_file(file: &FileRecord) -> OwnedCatalogEntry {
    let mut strings = Vec::new();
    let entry = WbCatalogEntry {
        name: c_view(file.name.clone(), &mut strings),
        path: c_view(file.full_path.clone(), &mut strings),
        kind: c_view("File".to_string(), &mut strings),
        size_text: c_view(format_size(file.size_bytes), &mut strings),
        description: c_view(
            format!("allocation {} bytes", file.allocation_bytes),
            &mut strings,
        ),
        size_bytes: file.size_bytes,
        modified_utc: file.modified_utc.unwrap_or(0),
        has_modified_utc: u8::from(file.modified_utc.is_some()),
    };
    OwnedCatalogEntry {
        entry,
        _strings: strings,
    }
}

fn format_size(size: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;
    const TB: f64 = GB * 1024.0;
    let size_f = size as f64;
    if size_f >= TB {
        format!("{size_f:.1} TB", size_f = size_f / TB)
    } else if size_f >= GB {
        format!("{size_f:.1} GB", size_f = size_f / GB)
    } else if size_f >= MB {
        format!("{size_f:.1} MB", size_f = size_f / MB)
    } else if size_f >= KB {
        format!("{size_f:.1} KB", size_f = size_f / KB)
    } else {
        format!("{size} B")
    }
}

#[no_mangle]
pub extern "C" fn wb_scan_session_start(
    root_path: WbCStringView,
    callback: WbEventCallback,
    user_data: *mut core::ffi::c_void,
) -> WbScanSessionHandle {
    start_scan_session(
        root_path,
        callback,
        user_data,
        ScanPersistenceMode::ReplaceSnapshot,
    )
}

#[no_mangle]
pub extern "C" fn wb_scan_session_start_incremental(
    root_path: WbCStringView,
    callback: WbEventCallback,
    user_data: *mut core::ffi::c_void,
) -> WbScanSessionHandle {
    start_scan_session(
        root_path,
        callback,
        user_data,
        ScanPersistenceMode::IncrementalRescan,
    )
}

fn start_scan_session(
    root_path: WbCStringView,
    callback: WbEventCallback,
    user_data: *mut core::ffi::c_void,
    persistence_mode: ScanPersistenceMode,
) -> WbScanSessionHandle {
    if root_path.ptr.is_null() {
        return WbScanSessionHandle {
            _private: null_mut(),
        };
    }

    let path = String::from_utf8_lossy(unsafe {
        std::slice::from_raw_parts(root_path.ptr.cast::<u8>(), root_path.len)
    })
    .to_string();

    let session = Box::new(NativeSession::new(
        callback,
        user_data,
        path,
        persistence_mode,
    ));
    WbScanSessionHandle {
        _private: Box::into_raw(session).cast(),
    }
}

#[no_mangle]
pub extern "C" fn wb_index_snapshot_load(
    callback: WbCatalogCallback,
    user_data: *mut core::ffi::c_void,
) {
    load_catalog_entries(callback, user_data);
}

#[no_mangle]
pub extern "C" fn wb_index_snapshot_load_with_stats(
    callback: WbCatalogCallback,
    user_data: *mut core::ffi::c_void,
) -> WbIndexSnapshotStats {
    load_catalog_entries_with_stats(callback, user_data)
}

#[no_mangle]
pub extern "C" fn wb_index_snapshot_stats() -> WbIndexSnapshotStats {
    index_snapshot_stats()
}

#[no_mangle]
pub extern "C" fn wb_scan_session_cancel(handle: WbScanSessionHandle) {
    if handle._private.is_null() {
        return;
    }

    let session = unsafe { &*(handle._private.cast::<NativeSession>()) };
    session.cancel();
}

#[no_mangle]
pub extern "C" fn wb_scan_session_destroy(handle: WbScanSessionHandle) {
    if handle._private.is_null() {
        return;
    }

    let session = unsafe { Box::from_raw(handle._private.cast::<NativeSession>()) };
    session.join();
}
