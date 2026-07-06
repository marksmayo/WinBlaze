use std::{
    collections::HashMap,
    ffi::CString,
    fs::{self, OpenOptions},
    io::Write,
    path::PathBuf,
    ptr::null_mut,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use winblaze_core::{
    DirectoryRecord, FileChangeKind, FileChangeSet, FileLineageRecord, FileRecord, ScanEvent,
    ScanProgress, ScanSession, ScanState, VolumeRecord,
};
use winblaze_index::{
    BufferedIndexTransaction, IndexBackend, IndexRepository, IndexTransaction,
    SqliteIndexRepository, TreeEntry, TreeIndex,
};
use winblaze_scanner::{ScanController, ScanRequest, ScanRuntimeConfig};

use crate::api::{
    WbCStringView, WbCatalogCallback, WbCatalogEntry, WbEvent, WbEventCallback, WbEventKind,
    WbExtensionStat, WbExtensionStatCallback, WbExtensionStatsSnapshot, WbIncrementalChangeSummary,
    WbIndexSnapshotStats, WbLiveDirectory, WbLiveDirectoryBatch, WbNativeError,
    WbScanSessionHandle, WbScanSummary, WbTreeChildrenResult, WbTreeNode, WbTreeNodeCallback,
};

const MAX_JSON_LOG_BYTES: u64 = 2 * 1024 * 1024;
const SNAPSHOT_ENTRY_LIMIT: usize = 8192;
const UI_LIVE_CATALOG_EVENT_LIMIT: usize = SNAPSHOT_ENTRY_LIMIT;
const UI_PROGRESS_MIN_ITEM_DELTA: u64 = 10_000;
const UI_PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(150);
const EXTENSION_STATS_MIN_INTERVAL: Duration = Duration::from_millis(400);
const EXTENSION_STATS_TOP_LIMIT: usize = 40;
const PROGRESS_LOG_MIN_INTERVAL: Duration = Duration::from_secs(1);
const TREE_CHILDREN_LIMIT: usize = 4096;
const DIRECTORY_BATCH_SIZE: usize = 4096;
const DIRECTORY_BATCH_MAX_LATENCY: Duration = Duration::from_millis(250);

/// Read model shared by the tree/catalog/extension-stats queries: the display
/// tree plus derived totals, built once per snapshot instead of re-reading
/// the multi-hundred-MB snapshot file on every FFI call.
struct IndexModel {
    tree: TreeIndex,
    extension_totals: Vec<(String, u64, u64)>,
    cache_read_bytes: u64,
    cache_read_millis: u64,
    cache_decode_millis: u64,
    cache_loaded_from_backup: bool,
}

/// Deferred post-Completed snapshot payload: the published model plus the
/// auxiliary records a full snapshot write still needs.
type DeferredPersist = (
    Arc<IndexModel>,
    Vec<ScanSession>,
    Vec<FileLineageRecord>,
    Vec<FileChangeSet>,
);

static INDEX_MODEL: Mutex<Option<Arc<IndexModel>>> = Mutex::new(None);

/// Serializes the deferred post-Completed snapshot write against the next
/// session's repository open (an immediate incremental rescan reads the
/// snapshot back from disk).
static PERSIST_GATE: Mutex<()> = Mutex::new(());

fn persist_gate_lock() -> std::sync::MutexGuard<'static, ()> {
    match PERSIST_GATE.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn build_index_model(
    transaction: BufferedIndexTransaction,
    cache_stats: Option<(u64, u64, u64, bool)>,
) -> IndexModel {
    let tree = TreeIndex::build(transaction);
    let extension_totals = aggregate_extension_totals(tree.files());

    let (read_bytes, read_millis, decode_millis, from_backup) =
        cache_stats.unwrap_or((0, 0, 0, false));
    IndexModel {
        tree,
        extension_totals,
        cache_read_bytes: read_bytes,
        cache_read_millis: read_millis,
        cache_decode_millis: decode_millis,
        cache_loaded_from_backup: from_backup,
    }
}

/// Aggregates per-extension (bytes, files) over every file in the model.
/// Runs on the post-scan critical path (before `Completed` reaches the UI), so
/// the ~2.3M-file pass is fanned across worker threads: each thread folds a
/// local map over its slice and the (few-hundred-key) maps merge at the end.
fn aggregate_extension_totals(files: &[FileRecord]) -> Vec<(String, u64, u64)> {
    let workers = thread::available_parallelism()
        .map(|value| value.get())
        .unwrap_or(4)
        .clamp(1, 16);
    // Below this a single pass is cheaper than the thread hand-off.
    let chunk = files.len().div_ceil(workers).max(1);

    let mut partials: Vec<HashMap<String, (u64, u64)>> = if files.len() < 64_000 || workers == 1 {
        vec![aggregate_extension_chunk(files)]
    } else {
        thread::scope(|scope| {
            files
                .chunks(chunk)
                .map(|slice| scope.spawn(|| aggregate_extension_chunk(slice)))
                .collect::<Vec<_>>()
                .into_iter()
                .map(|handle| handle.join().unwrap_or_default())
                .collect()
        })
    };

    let mut totals = partials.pop().unwrap_or_default();
    for partial in partials {
        for (extension, (bytes, count)) in partial {
            let entry = totals.entry(extension).or_insert((0, 0));
            entry.0 = entry.0.saturating_add(bytes);
            entry.1 = entry.1.saturating_add(count);
        }
    }
    totals
        .into_iter()
        .map(|(extension, (bytes, files))| (extension, bytes, files))
        .collect()
}

fn aggregate_extension_chunk(files: &[FileRecord]) -> HashMap<String, (u64, u64)> {
    let mut totals: HashMap<String, (u64, u64)> = HashMap::new();
    for file in files {
        // Borrowed fast path: extensions are usually already lowercase, so
        // look the key up without materializing a String per file (2.3M
        // allocations on a full-drive model build otherwise).
        let raw = match file.name.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => ext,
            _ => "",
        };
        let entry = if !raw.contains(|ch: char| ch.is_ascii_uppercase()) {
            match totals.get_mut(raw) {
                Some(entry) => entry,
                None => totals.entry(raw.to_string()).or_insert((0, 0)),
            }
        } else {
            totals.entry(raw.to_ascii_lowercase()).or_insert((0, 0))
        };
        entry.0 = entry.0.saturating_add(file.size_bytes);
        entry.1 = entry.1.saturating_add(1);
    }
    totals
}

fn set_index_model(model: IndexModel) {
    set_index_model_arc(Arc::new(model));
}

fn set_index_model_arc(model: Arc<IndexModel>) {
    let mut guard = match INDEX_MODEL.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    *guard = Some(model);
}

fn invalidate_index_model() {
    let mut guard = match INDEX_MODEL.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    *guard = None;
}

/// Returns the cached model, loading it from the persisted snapshot when
/// absent. The lock is held across the disk load so concurrent callers don't
/// each pay the read.
fn get_or_load_index_model() -> Arc<IndexModel> {
    let mut guard = match INDEX_MODEL.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(model) = guard.as_ref() {
        return Arc::clone(model);
    }

    let repository = SqliteIndexRepository::open(&index_storage_root(), IndexBackend::BinaryCache);
    let cache_stats = {
        let snapshot = repository.snapshot();
        (
            snapshot.cache_read_bytes,
            snapshot.cache_read_millis,
            snapshot.cache_decode_millis,
            snapshot.cache_loaded_from_backup,
        )
    };
    let model = Arc::new(build_index_model(
        repository.into_transaction(),
        Some(cache_stats),
    ));
    *guard = Some(Arc::clone(&model));
    model
}

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

struct PendingLiveDirectory {
    id: u64,
    parent_id: u64,
    has_parent: bool,
    // Built as a CString at push time rather than cloned into a String and
    // re-allocated into a CString at flush: one allocation per directory
    // instead of two, across the hundreds of thousands a full drive emits.
    name: CString,
}

struct UiEventForwarder {
    callback: WbEventCallback,
    user_data: usize,
    live_catalog_events: usize,
    last_progress_at: Option<Instant>,
    last_progress_items: u64,
    extension_totals: HashMap<String, (u64, u64)>,
    last_extension_stats_at: Option<Instant>,
    // Directory id -> full path, accumulated from directory events so the
    // capped set of forwarded file events can carry a derived path (file
    // records no longer store one).
    directory_paths: winblaze_core::IdHashMap<u64, String>,
    // Directories waiting to be delivered as one DirectoryBatch event: a
    // full drive discovers hundreds of thousands, and one FFI crossing per
    // directory dominated scan wall-clock.
    pending_directories: Vec<PendingLiveDirectory>,
    last_directory_batch_at: Option<Instant>,
}

impl UiEventForwarder {
    fn new(callback: WbEventCallback, user_data: usize) -> Self {
        Self {
            callback,
            user_data,
            live_catalog_events: 0,
            last_progress_at: None,
            last_progress_items: 0,
            extension_totals: HashMap::new(),
            last_extension_stats_at: None,
            directory_paths: Default::default(),
            pending_directories: Vec::new(),
            last_directory_batch_at: None,
        }
    }

    fn forward(&mut self, event: &ScanEvent) {
        // Extension totals are aggregated from every FileFound event, not
        // just the ones actually forwarded to the UI: should_forward() below
        // caps individual catalog events at UI_LIVE_CATALOG_EVENT_LIMIT, but
        // the aggregate breakdown must stay accurate for the whole scan.
        if let ScanEvent::FileFound(file) = event {
            self.record_extension(file);
        }

        if let ScanEvent::DirectoryFound(directory) = event {
            // Track directory paths only while file events can still be
            // forwarded: they exist solely to derive paths for that capped
            // set.
            if self.live_catalog_events < UI_LIVE_CATALOG_EVENT_LIMIT {
                self.directory_paths
                    .insert(directory.id.0, directory.full_path.clone());
            }

            self.pending_directories.push(PendingLiveDirectory {
                id: directory.id.0,
                parent_id: directory
                    .parent_directory_id
                    .map(|parent| parent.0)
                    .unwrap_or(0),
                has_parent: directory.parent_directory_id.is_some(),
                name: CString::new(directory.name.as_str()).unwrap_or_default(),
            });
            let now = Instant::now();
            let latency_due = self
                .last_directory_batch_at
                .is_none_or(|last| now.duration_since(last) >= DIRECTORY_BATCH_MAX_LATENCY);
            if self.pending_directories.len() >= DIRECTORY_BATCH_SIZE || latency_due {
                self.flush_directory_batch();
            }
            return;
        }

        // The tree must be complete before summary/terminal states reach the
        // UI (their handlers snapshot and reload).
        if matches!(
            event,
            ScanEvent::Summary(_)
                | ScanEvent::Completed
                | ScanEvent::Cancelled
                | ScanEvent::Failed(_)
        ) {
            self.flush_directory_batch();
        }

        if let Some(cb) = self.callback {
            if self.should_forward(event) {
                let native_event = match event {
                    ScanEvent::FileFound(file) if file.full_path.is_empty() => {
                        let mut file = file.clone();
                        if let Some(parent) = self.directory_paths.get(&file.parent_directory_id.0)
                        {
                            file.full_path = winblaze_core::join_path(parent, &file.name);
                        }
                        convert_event(ScanEvent::FileFound(file))
                    }
                    _ => convert_event(event.clone()),
                };
                cb(
                    &native_event.event as *const WbEvent,
                    self.user_data as *mut core::ffi::c_void,
                );
            }
        }

        let force_emit = matches!(event, ScanEvent::Completed | ScanEvent::Summary(_));
        self.maybe_emit_extension_stats(force_emit);
    }

    fn flush_directory_batch(&mut self) {
        self.last_directory_batch_at = Some(Instant::now());
        if self.pending_directories.is_empty() {
            return;
        }
        let Some(cb) = self.callback else {
            self.pending_directories.clear();
            return;
        };

        let mut names: Vec<CString> = Vec::with_capacity(self.pending_directories.len());
        let mut items: Vec<WbLiveDirectory> = Vec::with_capacity(self.pending_directories.len());
        for directory in self.pending_directories.drain(..) {
            let name = directory.name;
            items.push(WbLiveDirectory {
                id: directory.id,
                parent_id: directory.parent_id,
                has_parent: u8::from(directory.has_parent),
                name: WbCStringView {
                    ptr: name.as_ptr(),
                    len: name.as_bytes().len(),
                },
            });
            names.push(name);
        }

        let event = WbEvent {
            kind: WbEventKind::DirectoryBatch,
            directory_batch: WbLiveDirectoryBatch {
                items: items.as_ptr(),
                count: items.len(),
            },
            ..WbEvent::default()
        };
        cb(
            &event as *const WbEvent,
            self.user_data as *mut core::ffi::c_void,
        );
    }

    fn record_extension(&mut self, file: &FileRecord) {
        // Runs once per scanned file, so avoid extension_key's per-call
        // String: look up by borrowed &str and only allocate when a new
        // extension is first seen (or the rare mixed-case one needs
        // lowercasing).
        let extension = match file.name.rsplit_once('.') {
            Some((stem, extension)) if !stem.is_empty() && !extension.is_empty() => extension,
            _ => "",
        };

        if extension.bytes().any(|byte| byte.is_ascii_uppercase()) {
            let totals = self
                .extension_totals
                .entry(extension.to_ascii_lowercase())
                .or_insert((0, 0));
            totals.0 = totals.0.saturating_add(file.size_bytes);
            totals.1 = totals.1.saturating_add(1);
            return;
        }

        if let Some(totals) = self.extension_totals.get_mut(extension) {
            totals.0 = totals.0.saturating_add(file.size_bytes);
            totals.1 = totals.1.saturating_add(1);
        } else {
            self.extension_totals
                .insert(extension.to_string(), (file.size_bytes, 1));
        }
    }

    fn maybe_emit_extension_stats(&mut self, force: bool) {
        let Some(cb) = self.callback else {
            return;
        };
        if self.extension_totals.is_empty() {
            return;
        }
        let now = Instant::now();
        let due = force
            || self
                .last_extension_stats_at
                .is_none_or(|last| now.duration_since(last) >= EXTENSION_STATS_MIN_INTERVAL);
        if !due {
            return;
        }
        self.last_extension_stats_at = Some(now);

        let stats: Vec<(String, u64, u64)> = self
            .extension_totals
            .iter()
            .map(|(extension, (bytes, files))| (extension.clone(), *bytes, *files))
            .collect();
        emit_extension_stats(cb, self.user_data, stats);
    }

    fn should_forward(&mut self, event: &ScanEvent) -> bool {
        match event {
            // Directories are delivered as DirectoryBatch events by
            // forward() and never reach this check.
            ScanEvent::DirectoryFound(_) => false,
            ScanEvent::FileFound(_) => {
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
        let Self {
            controller,
            worker,
            scan_handle,
        } = self;
        if let Some(handle) = scan_handle.lock().expect("lock poisoned").take() {
            handle.join();
        }
        // The event-forwarding worker's `for event in rx` loop only ends once
        // every Sender is gone; the controller holds the original one, so it
        // must drop before joining the worker or this blocks forever.
        drop(controller);
        let worker_handle = worker.lock().expect("lock poisoned").take();
        if let Some(worker_handle) = worker_handle {
            let _ = worker_handle.join();
        }
    }
}

fn forward_events(
    rx: mpsc::Receiver<Vec<ScanEvent>>,
    callback: WbEventCallback,
    user_data: usize,
    index_root: PathBuf,
    persistence_mode: ScanPersistenceMode,
) {
    let mut repository = {
        let _gate = persist_gate_lock();
        match persistence_mode {
            ScanPersistenceMode::ReplaceSnapshot => {
                SqliteIndexRepository::open_empty(&index_root, IndexBackend::BinaryCache)
            }
            ScanPersistenceMode::IncrementalRescan => {
                SqliteIndexRepository::open(&index_root, IndexBackend::BinaryCache)
            }
        }
    };
    let mut transaction = BufferedIndexTransaction::default();
    let mut log = EventLog::new(structured_log_path(&index_root));
    let mut flushed = false;
    // True while the transaction holds volume/directory/file records that a
    // successful flush has not yet written. Session-state bookkeeping alone
    // does not set it, so the Completed event that follows a Summary no
    // longer re-serializes the entire index (measured at ~20s for a full
    // C:\ scan) just to record that the session finished.
    let mut records_dirty = false;
    let mut model_published = false;
    // Full-scan snapshots write AFTER Completed reaches the UI: the write is
    // ~1.6s for a full C:\ index and the UI only needs the in-memory model.
    let mut deferred_persist: Option<DeferredPersist> = None;
    // Acquired BEFORE Completed is forwarded and held until the deferred
    // write finishes, so a session started the instant the UI sees
    // Completed blocks on the gate instead of reading the stale snapshot.
    // This deliberately widens the critical section past just the disk
    // write: it is the minimum span that closes the rescan race. The only
    // cost is that a *second* concurrent ReplaceSnapshot scan would wait on
    // this one's finalization — which the single-session UI never starts.
    let mut persist_guard: Option<std::sync::MutexGuard<'static, ()>> = None;
    let mut ui_forwarder = UiEventForwarder::new(callback, user_data);
    // Reserve the transaction maps once, from the first Progress event's total
    // record count, so the millions of inserts that follow don't rehash the
    // maps ~20 times as they grow. total_items is the volume's total MFT record
    // count; the split below reflects the typical ~50% files / ~12% dirs share
    // and only over-reserves a bounded amount when a volume skews.
    let mut reserved = false;

    for event in rx.into_iter().flatten() {
        log.append_scan_event(&event);

        if !reserved {
            if let ScanEvent::Progress(progress) = &event {
                if progress.total_items > 0 {
                    let total = progress.total_items as usize;
                    transaction.reserve(total / 2, total / 8);
                    reserved = true;
                }
            }
        }

        // Publish the read model BEFORE the UI learns the scan ended: its
        // snapshot reload then finds a hot cache instead of re-reading the
        // multi-hundred-MB snapshot from disk on the UI thread. Full scans
        // additionally defer the snapshot write until after Completed is
        // forwarded - the UI only needs the in-memory model, and the disk
        // write was the last ~1.6s of perceived scan time.
        if !model_published && matches!(event, ScanEvent::Completed | ScanEvent::Cancelled) {
            match persistence_mode {
                ScanPersistenceMode::ReplaceSnapshot => {
                    // Record the terminal session state before the parts are
                    // cloned: persist_scan_event applies this same upsert
                    // later, but into the by-then-emptied transaction, and
                    // the snapshot must not say the scan is still running.
                    transaction.upsert_session(&ScanSession {
                        session_id: 1,
                        volume_id: Default::default(),
                        root_path: String::new(),
                        state: if matches!(event, ScanEvent::Completed) {
                            ScanState::Completed
                        } else {
                            ScanState::Cancelled
                        },
                        progress: Default::default(),
                    });
                    let (sessions, lineages, changes) = transaction.auxiliary_parts();
                    let model = Arc::new(build_index_model(std::mem::take(&mut transaction), None));
                    set_index_model_arc(Arc::clone(&model));
                    persist_guard = Some(persist_gate_lock());
                    deferred_persist = Some((model, sessions, lineages, changes));
                    records_dirty = false;
                    model_published = true;
                }
                ScanPersistenceMode::IncrementalRescan => {
                    if flushed && !records_dirty {
                        set_index_model(build_index_model(repository.take_state(), None));
                        model_published = true;
                    }
                }
            }
        }

        ui_forwarder.forward(&event);

        let flush_requested = should_flush_index(&event, persistence_mode);
        let flush_trigger = event_name(&event);
        records_dirty |= persist_scan_event(&mut transaction, event);

        if flush_requested && (records_dirty || !flushed) && !model_published {
            let result = apply_scan_transaction(&mut repository, &transaction, persistence_mode);
            if let Ok(Some(change_set)) = &result {
                emit_incremental_change_summary(callback, user_data, change_set);
            }
            flushed = result.is_ok();
            if result.is_ok() {
                records_dirty = false;
            }
            log.append_flush(flush_trigger, result.is_ok());
        }
    }

    if let Some((model, sessions, lineages, changes)) = deferred_persist {
        let result = repository.persist_sorted_records(
            model.tree.volumes(),
            &sessions,
            model.tree.directories(),
            model.tree.files(),
            &lineages,
            &changes,
        );
        log.append_flush("deferred", result.is_ok());
        drop(persist_guard);
        return;
    }
    drop(persist_guard);

    if !model_published {
        if !flushed || records_dirty {
            let result = apply_scan_transaction(&mut repository, &transaction, persistence_mode);
            if let Ok(Some(change_set)) = &result {
                emit_incremental_change_summary(callback, user_data, change_set);
            }
            log.append_flush("completed", result.is_ok());
        }

        // For a replace-snapshot scan the transaction IS the new state; an
        // incremental rescan merged into the repository, so take its state.
        let model = match persistence_mode {
            ScanPersistenceMode::ReplaceSnapshot => build_index_model(transaction, None),
            ScanPersistenceMode::IncrementalRescan => {
                build_index_model(repository.into_transaction(), None)
            }
        };
        set_index_model(model);
    }
}

fn should_flush_index(event: &ScanEvent, persistence_mode: ScanPersistenceMode) -> bool {
    match persistence_mode {
        // Summary/Completed/Cancelled deliberately absent: full-scan
        // snapshots write once, deferred until after Completed reaches the
        // UI. Failed still lands partial data immediately.
        ScanPersistenceMode::ReplaceSnapshot => matches!(
            event,
            ScanEvent::SessionStarted(_) | ScanEvent::VolumeDiscovered(_) | ScanEvent::Failed(_)
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
        // persist_transaction rather than apply_transaction: scan flushes
        // only write — nothing reads back through this repository instance —
        // and apply_transaction's clone-into-state doubles a multi-GB
        // working set per flush.
        ScanPersistenceMode::ReplaceSnapshot => {
            repository.persist_transaction(transaction).map(|_| None)
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

/// Lowercased extension without the leading dot, or an empty string for
/// files with no extension (e.g. `README`, `Makefile`).
/// Reference implementation of the extension-key normalization; the model
/// build inlines a borrowed fast path of this. Kept for the unit tests that
/// pin the normalization rules.
#[cfg(test)]
fn extension_key(file_name: &str) -> String {
    match file_name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() && !ext.is_empty() => ext.to_ascii_lowercase(),
        _ => String::new(),
    }
}

/// Best-effort friendly label for the extension breakdown table. Unknown
/// extensions fall back to "<EXT> File"; matches the spirit (not an exact
/// copy) of the descriptions shown by tools like WizTree.
fn extension_description(extension: &str) -> String {
    if extension.is_empty() {
        return "No extension".to_string();
    }
    let description = match extension {
        "exe" => "Application",
        "dll" => "Application extension",
        "sys" => "System file",
        "msi" => "Windows Installer package",
        "msix" | "appx" => "App package",
        "zip" => "Compressed archive",
        "7z" => "7-Zip archive",
        "rar" => "RAR archive",
        "tar" | "gz" | "tgz" => "Compressed archive",
        "iso" => "Disc image file",
        "vhd" | "vhdx" => "Hard disk image file",
        "pdf" => "PDF document",
        "doc" | "docx" => "Word document",
        "xls" | "xlsx" => "Excel spreadsheet",
        "ppt" | "pptx" => "PowerPoint presentation",
        "txt" => "Text document",
        "log" => "Log file",
        "json" => "JSON file",
        "xml" => "XML file",
        "ini" | "cfg" | "config" => "Configuration file",
        "jpg" | "jpeg" | "png" | "gif" | "bmp" | "webp" => "Image file",
        "mp3" | "wav" | "flac" | "aac" => "Audio file",
        "mp4" | "mkv" | "avi" | "mov" | "webm" => "Video file",
        "js" | "mjs" | "cjs" => "JavaScript source file",
        "ts" | "tsx" => "TypeScript source file",
        "py" => "Python source file",
        "rs" => "Rust source file",
        "c" | "h" => "C source/header file",
        "cpp" | "cc" | "hpp" => "C++ source/header file",
        "java" | "class" | "jar" => "Java file",
        "obj" | "o" => "Object file",
        "lib" | "a" => "Object file library",
        "pdb" => "Program debug database",
        "dat" | "bin" => "Binary data file",
        "db" | "sqlite" | "sqlite3" => "Database file",
        "cache" | "tmp" => "Temporary file",
        "node" => "Node native module",
        "wasm" => "WebAssembly module",
        "sh" | "ps1" | "bat" | "cmd" => "Script file",
        "yml" | "yaml" | "toml" => "Configuration file",
        "md" => "Markdown document",
        "html" | "htm" => "HTML document",
        "css" => "Stylesheet",
        _ => return format!("{} File", extension.to_ascii_uppercase()),
    };
    description.to_string()
}

struct OwnedExtensionStats {
    items: Vec<WbExtensionStat>,
    _strings: Vec<CString>,
}

/// Sorts extension totals by bytes descending and keeps only the top N, so
/// the FFI payload (and the UI table) stay bounded regardless of how many
/// distinct extensions a scan encounters.
fn sorted_top_extension_totals(mut totals: Vec<(String, u64, u64)>) -> Vec<(String, u64, u64)> {
    totals.sort_by_key(|entry| std::cmp::Reverse(entry.1));
    totals.truncate(EXTENSION_STATS_TOP_LIMIT);
    totals
}

fn build_extension_stats(totals: Vec<(String, u64, u64)>) -> OwnedExtensionStats {
    let totals = sorted_top_extension_totals(totals);
    let mut strings = Vec::new();
    let items = totals
        .into_iter()
        .map(|(extension, bytes, files)| WbExtensionStat {
            extension: c_view(extension.clone(), &mut strings),
            description: c_view(extension_description(&extension), &mut strings),
            bytes,
            files,
        })
        .collect();
    OwnedExtensionStats {
        items,
        _strings: strings,
    }
}

fn emit_extension_stats(
    cb: extern "C" fn(event: *const WbEvent, user_data: *mut core::ffi::c_void),
    user_data: usize,
    totals: Vec<(String, u64, u64)>,
) {
    let owned = build_extension_stats(totals);
    let native_event = WbEvent {
        kind: WbEventKind::ExtensionStats,
        extension_stats: WbExtensionStatsSnapshot {
            items: owned.items.as_ptr(),
            count: owned.items.len(),
        },
        ..WbEvent::default()
    };
    cb(
        &native_event as *const WbEvent,
        user_data as *mut core::ffi::c_void,
    );
}

/// Folds `event` into the transaction, taking ownership so records move in
/// without a per-record clone. Returns whether catalog records (volumes,
/// directories, files) changed — session-state bookkeeping alone returns
/// false so callers can skip redundant full-snapshot flushes.
fn persist_scan_event(transaction: &mut BufferedIndexTransaction, event: ScanEvent) -> bool {
    match event {
        ScanEvent::SessionStarted(volume) | ScanEvent::VolumeDiscovered(volume) => {
            transaction.upsert_session(&ScanSession {
                session_id: 1,
                volume_id: volume.id,
                root_path: volume.mount_point.clone(),
                state: ScanState::Scanning,
                progress: Default::default(),
            });
            transaction.insert_volume(volume);
            true
        }
        ScanEvent::DirectoryFound(directory) => {
            transaction.insert_directory(directory);
            true
        }
        ScanEvent::FileFound(file) => {
            transaction.insert_file(file);
            true
        }
        ScanEvent::Progress(_) | ScanEvent::Issue(_) | ScanEvent::Summary(_) => false,
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
            false
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

/// Structured-event log writer that keeps one open handle instead of paying
/// create_dir_all + rotation stat + open + close for every line — a full
/// drive scan emits thousands of progress events, which previously meant
/// four filesystem operations on the log file per event. Progress lines are
/// additionally throttled to one per second; they only exist for post-hoc
/// timing analysis, and the summary line carries the final totals.
struct EventLog {
    path: PathBuf,
    file: Option<fs::File>,
    written_bytes: u64,
    last_progress_log: Option<Instant>,
    /// Reused across appends so composing a log line does not allocate each
    /// time. (Only low-volume events reach here — file/directory events return
    /// early — so this is a tidiness win, not a hot-path one.)
    line_buf: String,
}

impl EventLog {
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            file: None,
            written_bytes: 0,
            last_progress_log: None,
            line_buf: String::new(),
        }
    }

    fn append_scan_event(&mut self, event: &ScanEvent) {
        match event {
            ScanEvent::DirectoryFound(_) | ScanEvent::FileFound(_) => return,
            ScanEvent::Progress(_) => {
                let now = Instant::now();
                if self
                    .last_progress_log
                    .is_some_and(|last| now.duration_since(last) < PROGRESS_LOG_MIN_INTERVAL)
                {
                    return;
                }
                self.last_progress_log = Some(now);
            }
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

        self.append(&payload);
    }

    fn append_flush(&mut self, trigger: &str, ok: bool) {
        self.append(&format!(
            r#""event":"index.flush","trigger":"{trigger}","ok":{ok}"#
        ));
    }

    fn append(&mut self, payload: &str) {
        if self.written_bytes >= MAX_JSON_LOG_BYTES {
            self.file = None;
            rotate_log_if_needed(&self.path, MAX_JSON_LOG_BYTES);
            self.written_bytes = 0;
        }

        if self.file.is_none() {
            self.open();
        }
        if self.file.is_none() {
            return;
        }

        use std::fmt::Write as _;
        self.line_buf.clear();
        let _ = writeln!(
            self.line_buf,
            "{{\"ts_ms\":{},\"component\":\"native\",{} }}",
            now_ms(),
            payload
        );
        let file = self.file.as_mut().expect("checked above");
        if file.write_all(self.line_buf.as_bytes()).is_ok() {
            self.written_bytes = self
                .written_bytes
                .saturating_add(self.line_buf.len() as u64);
        } else {
            self.file = None;
        }
    }

    fn open(&mut self) {
        if let Some(parent) = self.path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        rotate_log_if_needed(&self.path, MAX_JSON_LOG_BYTES);
        self.written_bytes = fs::metadata(&self.path)
            .map(|metadata| metadata.len())
            .unwrap_or(0);
        self.file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .ok();
    }
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

/// Live directory events feed the folder tree, which needs only id, parent
/// linkage, and name. Every directory on the volume flows through here
/// (hundreds of thousands per scan), so this skips the path/kind/size/
/// description strings a full catalog entry would allocate — that formatting
/// dominated scan wall-clock once directory events stopped being capped.
fn convert_directory_event(directory: &DirectoryRecord) -> OwnedEvent {
    let mut strings = Vec::new();
    let entry = WbCatalogEntry {
        name: c_view(directory.name.clone(), &mut strings),
        id: directory.id.0,
        parent_id: directory
            .parent_directory_id
            .map(|parent| parent.0)
            .unwrap_or(0),
        has_parent: u8::from(directory.parent_directory_id.is_some()),
        is_directory: 1,
        total_entries: directory.total_entries,
        ..WbCatalogEntry::default()
    };
    OwnedEvent {
        event: WbEvent {
            kind: WbEventKind::DirectoryFound,
            catalog_entry: entry,
            ..WbEvent::default()
        },
        _strings: strings,
    }
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
        ScanEvent::DirectoryFound(directory) => convert_directory_event(&directory),
        ScanEvent::FileFound(file) => {
            // The forwarder fills full_path for forwarded live events; falls
            // back to the (possibly empty) stored path otherwise.
            let catalog = catalog_entry_from_file(&file, &file.full_path);
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
                winblaze_core::ScanIssueKind::FastScanUnavailable => 16,
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

fn load_extension_stats_from_snapshot(
    callback: WbExtensionStatCallback,
    user_data: *mut core::ffi::c_void,
) {
    let Some(cb) = callback else {
        return;
    };

    let model = get_or_load_index_model();
    let totals = sorted_top_extension_totals(model.extension_totals.clone());

    for (extension, bytes, files) in totals {
        let mut strings = Vec::new();
        let entry = WbExtensionStat {
            extension: c_view(extension.clone(), &mut strings),
            description: c_view(extension_description(&extension), &mut strings),
            bytes,
            files,
        };
        cb(&entry as *const WbExtensionStat, user_data);
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
    // Serve from the shared read model: no per-call snapshot re-read, no
    // cloned record vectors, and at most SNAPSHOT_ENTRY_LIMIT entries emitted
    // (the UI never displays more).
    let model = get_or_load_index_model();
    let tree = &model.tree;

    let stats = WbIndexSnapshotStats {
        volumes: tree.volumes().len() as u64,
        directories: tree.directories().len() as u64,
        files: tree.files().len() as u64,
        entries_emitted_limit: SNAPSHOT_ENTRY_LIMIT as u64,
        cache_read_bytes: model.cache_read_bytes,
        cache_read_millis: model.cache_read_millis,
        cache_decode_millis: model.cache_decode_millis,
        cache_loaded_from_backup: u8::from(model.cache_loaded_from_backup),
    };

    if let Some(cb) = callback {
        let mut emitted = 0usize;
        for volume in tree.volumes() {
            if emitted >= SNAPSHOT_ENTRY_LIMIT {
                return stats;
            }
            let entry = catalog_entry_from_volume(volume);
            cb(&entry.entry as *const WbCatalogEntry, user_data);
            emitted += 1;
        }
        for directory in tree.directories() {
            if emitted >= SNAPSHOT_ENTRY_LIMIT {
                return stats;
            }
            let entry = catalog_entry_from_directory(directory);
            cb(&entry.entry as *const WbCatalogEntry, user_data);
            emitted += 1;
        }
        for file in tree.files() {
            if emitted >= SNAPSHOT_ENTRY_LIMIT {
                return stats;
            }
            let entry = catalog_entry_from_file(file, &tree.file_display_path(file));
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
        id: volume.root_directory_id.0,
        parent_id: 0,
        has_parent: 0,
        is_directory: 1,
        size_bytes: volume.total_bytes,
        allocation_bytes: volume.total_bytes,
        total_entries: 0,
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
        id: directory.id.0,
        parent_id: directory
            .parent_directory_id
            .map(|parent| parent.0)
            .unwrap_or(0),
        has_parent: u8::from(directory.parent_directory_id.is_some()),
        is_directory: 1,
        size_bytes: directory.total_bytes,
        allocation_bytes: directory.total_bytes,
        total_entries: directory.total_entries,
        modified_utc: 0,
        has_modified_utc: 0,
    };
    OwnedCatalogEntry {
        entry,
        _strings: strings,
    }
}

fn catalog_entry_from_file(file: &FileRecord, full_path: &str) -> OwnedCatalogEntry {
    let mut strings = Vec::new();
    let entry = WbCatalogEntry {
        name: c_view(file.name.clone(), &mut strings),
        path: c_view(full_path.to_string(), &mut strings),
        kind: c_view("File".to_string(), &mut strings),
        size_text: c_view(format_size(file.size_bytes), &mut strings),
        description: c_view(
            format!("allocation {} bytes", file.allocation_bytes),
            &mut strings,
        ),
        id: file.id.0,
        parent_id: file.parent_directory_id.0,
        has_parent: 1,
        is_directory: 0,
        size_bytes: file.size_bytes,
        allocation_bytes: file.allocation_bytes,
        total_entries: 1,
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

    // The scan will replace or mutate the snapshot; drop the cached read
    // model so queries rebuild against the fresh state when the scan ends.
    invalidate_index_model();

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
pub extern "C" fn wb_index_snapshot_extension_stats(
    callback: WbExtensionStatCallback,
    user_data: *mut core::ffi::c_void,
) {
    load_extension_stats_from_snapshot(callback, user_data);
}

/// Emits the display-tree root through `callback`. Returns 1 when a root
/// exists, 0 for an empty index. The root node's name is its full mount-point
/// path (child names are bare).
#[no_mangle]
pub extern "C" fn wb_tree_root(
    callback: WbTreeNodeCallback,
    user_data: *mut core::ffi::c_void,
) -> u8 {
    let Some(cb) = callback else {
        return 0;
    };
    let model = get_or_load_index_model();
    let Some((record, rollup)) = model.tree.root() else {
        return 0;
    };

    let mut strings = Vec::new();
    let node = WbTreeNode {
        id: record.id.0,
        is_directory: 1,
        name: c_view(record.full_path.clone(), &mut strings),
        logical_bytes: rollup.logical_bytes,
        physical_bytes: rollup.physical_bytes,
        file_count: rollup.file_count,
        item_count: rollup.item_count,
        modified_utc: rollup.modified_utc_max.unwrap_or_default(),
        has_modified_utc: u8::from(rollup.modified_utc_max.is_some()),
    };
    cb(&node as *const WbTreeNode, user_data);
    1
}

/// Emits the direct children of directory `parent_id` (largest physical size
/// first, at most `TREE_CHILDREN_LIMIT` starting at `offset`), returning how
/// many were emitted and how many exist in total so callers can page and
/// render a "+N more" row.
#[no_mangle]
pub extern "C" fn wb_tree_children(
    parent_id: u64,
    offset: u64,
    callback: WbTreeNodeCallback,
    user_data: *mut core::ffi::c_void,
) -> WbTreeChildrenResult {
    let Some(cb) = callback else {
        return WbTreeChildrenResult::default();
    };
    let model = get_or_load_index_model();
    let total = model.tree.child_count(parent_id).unwrap_or(0);
    let emitted = model
        .tree
        .for_each_child(parent_id, offset as usize, TREE_CHILDREN_LIMIT, |entry| {
            let mut strings = Vec::new();
            let node = match entry {
                TreeEntry::Directory { record, rollup } => WbTreeNode {
                    id: record.id.0,
                    is_directory: 1,
                    name: c_view(record.name.clone(), &mut strings),
                    logical_bytes: rollup.logical_bytes,
                    physical_bytes: rollup.physical_bytes,
                    file_count: rollup.file_count,
                    item_count: rollup.item_count,
                    modified_utc: rollup.modified_utc_max.unwrap_or_default(),
                    has_modified_utc: u8::from(rollup.modified_utc_max.is_some()),
                },
                TreeEntry::File(file) => WbTreeNode {
                    id: file.id.0,
                    is_directory: 0,
                    name: c_view(file.name.clone(), &mut strings),
                    logical_bytes: file.size_bytes,
                    physical_bytes: file.allocation_bytes,
                    file_count: 0,
                    item_count: 0,
                    modified_utc: file.modified_utc.unwrap_or_default(),
                    has_modified_utc: u8::from(file.modified_utc.is_some()),
                },
            };
            cb(&node as *const WbTreeNode, user_data);
        })
        .unwrap_or(0);

    WbTreeChildrenResult { emitted, total }
}

/// Emits the `limit` largest files by allocation size, descending, with
/// derived full paths. Powers cleanup/large-file views.
#[no_mangle]
pub extern "C" fn wb_tree_largest_files(
    limit: u64,
    callback: WbTreeNodeCallback,
    user_data: *mut core::ffi::c_void,
) {
    let Some(cb) = callback else {
        return;
    };
    let model = get_or_load_index_model();
    model.tree.for_each_largest_file(limit as usize, |file| {
        let mut strings = Vec::new();
        let node = WbTreeNode {
            id: file.id.0,
            is_directory: 0,
            // Full display path rather than the bare name: cleanup rows
            // need to show and open the actual location.
            name: c_view(model.tree.file_display_path(file), &mut strings),
            logical_bytes: file.size_bytes,
            physical_bytes: file.allocation_bytes,
            file_count: 0,
            item_count: 0,
            modified_utc: file.modified_utc.unwrap_or_default(),
            has_modified_utc: u8::from(file.modified_utc.is_some()),
        };
        cb(&node as *const WbTreeNode, user_data);
    });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extension_key_lowercases_and_strips_dot() {
        assert_eq!(extension_key("Report.PDF"), "pdf");
        assert_eq!(extension_key("archive.tar.gz"), "gz");
        assert_eq!(extension_key("README"), "");
        assert_eq!(extension_key(".gitignore"), "");
    }

    #[test]
    fn extension_description_covers_known_and_unknown_extensions() {
        assert_eq!(extension_description(""), "No extension");
        assert_eq!(extension_description("exe"), "Application");
        assert_eq!(extension_description("zzzz"), "ZZZZ File");
    }

    #[test]
    fn sorted_top_extension_totals_sorts_by_bytes_desc_and_truncates() {
        let totals: Vec<(String, u64, u64)> = (0..EXTENSION_STATS_TOP_LIMIT + 5)
            .map(|i| (format!("ext{i}"), i as u64, 1))
            .collect();

        let sorted = sorted_top_extension_totals(totals);

        assert_eq!(sorted.len(), EXTENSION_STATS_TOP_LIMIT);
        assert_eq!(sorted[0].0, format!("ext{}", EXTENSION_STATS_TOP_LIMIT + 4));
        assert!(sorted.windows(2).all(|pair| pair[0].1 >= pair[1].1));
    }

    #[test]
    fn build_extension_stats_produces_matching_owned_strings() {
        let owned = build_extension_stats(vec![
            ("rs".to_string(), 100, 3),
            ("exe".to_string(), 500, 1),
        ]);

        assert_eq!(owned.items.len(), 2);
        assert_eq!(owned.items[0].bytes, 500);
        let extension = unsafe {
            std::slice::from_raw_parts(
                owned.items[0].extension.ptr.cast::<u8>(),
                owned.items[0].extension.len,
            )
        };
        assert_eq!(std::str::from_utf8(extension).unwrap(), "exe");
    }
}
