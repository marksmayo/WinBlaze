//! Incremental refresh from recorded changes: re-read only the directories
//! the change watch saw change, patch the published model in place, and
//! finish without walking the tree.
//!
//! The patch handles what it can prove matches a full walk: files added to
//! or modified in directories the model already has, each with a single
//! link. Anything else (removals, new or removed directories, hard links,
//! reparse points, an incomplete change record, a model another scan or an
//! import replaced, a read error) declines, and the caller walks the whole
//! tree exactly as before.

use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use winblaze_core::{DirectoryId, FileId, FileRecord, ScanEvent, ScanSession, ScanState};
use winblaze_index::{
    BufferedIndexTransaction, IndexBackend, IndexRepository, SqliteIndexRepository, TreeIndex,
};
use winblaze_scanner::listing;

use super::{IndexModel, INDEX_MODEL};
use crate::api::WbEventCallback;
use crate::change_watch::ChangeWatch;

/// Changes the published model since it was last replaced: set whenever a
/// different model is published or the model is dropped, never by a patch.
pub(super) static MODEL_GENERATION: AtomicU64 = AtomicU64::new(0);

struct WatchSlot {
    watch: ChangeWatch,
    token: u64,
    /// The model generation this watch's changes are relative to: set when
    /// the walk that started the watch publishes its model.
    bound_generation: Option<u64>,
}

static CHANGE_WATCH: Mutex<Option<WatchSlot>> = Mutex::new(None);
static WATCH_TOKENS: AtomicU64 = AtomicU64::new(0);

/// More changed directories than this re-reads more than a walk saves.
const MAX_CHANGED_DIRECTORIES: usize = 256;

fn watch_slot() -> std::sync::MutexGuard<'static, Option<WatchSlot>> {
    CHANGE_WATCH
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Starts recording changes under `root` before a walk of it reads anything,
/// replacing any earlier watch. Returns the token the walk binds its model
/// with, or None when the root cannot be watched.
pub(super) fn begin_change_watch(root: &str) -> Option<u64> {
    let mut slot = watch_slot();
    // Stop the old watch first: two would both hold the directory open.
    *slot = None;
    let watch = ChangeWatch::start(root)?;
    let token = WATCH_TOKENS.fetch_add(1, Ordering::Relaxed) + 1;
    *slot = Some(WatchSlot {
        watch,
        token,
        bound_generation: None,
    });
    Some(token)
}

/// Stops recording: the next refresh walks.
pub(super) fn end_change_watch() {
    *watch_slot() = None;
}

/// Records that the watch started under `token` covers model `generation`.
pub(super) fn bind_change_watch(token: u64, generation: u64) {
    if let Some(slot) = watch_slot().as_mut() {
        if slot.token == token {
            slot.bound_generation = Some(generation);
        }
    }
}

fn normalized_root(path: &str) -> String {
    path.trim_start_matches(r"\\?\")
        .trim_end_matches('\\')
        .to_lowercase()
}

/// Files a patch adds and modifies, read from the changed directories.
#[derive(Default)]
struct FilePatch {
    added: Vec<FileRecord>,
    modified: Vec<FileRecord>,
}

fn same_metadata(old: &FileRecord, new: &FileRecord) -> bool {
    old.size_bytes == new.size_bytes
        && old.allocation_bytes == new.allocation_bytes
        && old.attributes == new.attributes
        && old.timestamps == new.timestamps
}

/// The directory at `relative` (backslash separated, empty for the root),
/// found by name from the root down.
fn resolve_directory(tree: &TreeIndex, relative: &str) -> Option<u64> {
    let mut id = tree.root()?.0.id.0;
    for part in relative.split('\\').filter(|part| !part.is_empty()) {
        let (directories, _) = tree.direct_children(id)?;
        id = directories
            .iter()
            .find(|directory| directory.name == part)?
            .id
            .0;
    }
    Some(id)
}

/// Reads each changed directory and compares it with the model. None when
/// anything is outside what [`TreeIndex::apply_file_changes`] can patch.
fn plan_patch(tree: &TreeIndex, root: &str, changed_paths: &[String]) -> Option<FilePatch> {
    let parents: BTreeSet<&str> = changed_paths
        .iter()
        .map(|path| path.rsplit_once('\\').map_or("", |(parent, _)| parent))
        .collect();
    if parents.len() > MAX_CHANGED_DIRECTORIES {
        return None;
    }
    let mut patch = FilePatch::default();
    for relative in parents {
        let directory_id = resolve_directory(tree, relative)?;
        let path = if relative.is_empty() {
            Path::new(root).to_path_buf()
        } else {
            Path::new(root).join(relative)
        };
        let listing = listing::list_directory(&path, DirectoryId(directory_id)).ok()?;
        let (model_directories, model_files) = tree.direct_children(directory_id)?;
        // A junction, link or cloud folder could have replaced a same-named
        // folder (or the reverse), and the walk descends those by policy: a
        // name match proves nothing, so leave such directories to a walk.
        if listing
            .directories
            .iter()
            .any(|directory| directory.reparse_or_cloud)
        {
            return None;
        }

        // Directory changes are structural: leave them to a walk.
        let mut listed: Vec<&str> = listing
            .directories
            .iter()
            .map(|directory| directory.name.as_str())
            .collect();
        let mut known: Vec<&str> = model_directories
            .iter()
            .map(|directory| directory.name.as_str())
            .collect();
        listed.sort_unstable();
        known.sort_unstable();
        if listed != known {
            return None;
        }

        let mut by_name: HashMap<&str, &FileRecord> = model_files
            .iter()
            .map(|file| (&*file.name, *file))
            .collect();
        for mut file in listing.files {
            match by_name.remove(&*file.name) {
                Some(existing) if same_metadata(existing, &file) => {}
                existing => {
                    // The walk charges a hard-linked file once across the
                    // whole tree, which one directory cannot know.
                    if listing::file_link_count(&path.join(&*file.name)).ok()? != 1 {
                        return None;
                    }
                    match existing {
                        Some(existing) => {
                            file.id = existing.id;
                            patch.modified.push(file);
                        }
                        None => patch.added.push(file),
                    }
                }
            }
        }
        // A file the model has and the directory no longer does: removals
        // are left to a walk.
        if !by_name.is_empty() {
            return None;
        }
    }
    Some(patch)
}

/// Adjusts the model's per-extension totals for the patched files.
fn patch_extension_totals(model: &mut IndexModel, added: &[FileRecord]) {
    let mut totals: HashMap<String, (u64, u64)> = model
        .extension_top
        .drain(..)
        .map(|(extension, bytes, count)| (extension, (bytes, count)))
        .collect();
    let mut buffer = [0u8; 32];
    for file in added {
        let key = super::lowercase_extension(super::file_extension(&file.name), &mut buffer);
        let entry = totals.entry(key.into_owned()).or_insert((0, 0));
        entry.0 = entry.0.saturating_add(file.size_bytes);
        entry.1 = entry.1.saturating_add(1);
    }
    model.extension_top = super::sorted_all_extension_totals(
        totals
            .into_iter()
            .map(|(extension, (bytes, count))| (extension, bytes, count))
            .collect(),
    );
}

/// Refreshes `root` from recorded changes. Returns false, having changed
/// nothing, when the caller must walk instead.
pub(super) fn try_fast_refresh(
    root: &str,
    callback: WbEventCallback,
    user_data: usize,
    index_root: &Path,
    cancelled: &AtomicBool,
) -> bool {
    let generation = MODEL_GENERATION.load(Ordering::SeqCst);
    let changes = {
        let mut slot = watch_slot();
        let Some(slot) = slot.as_mut() else {
            return false;
        };
        if slot.bound_generation != Some(generation)
            || normalized_root(slot.watch.root()) != normalized_root(root)
        {
            return false;
        }
        match slot.watch.take(Duration::from_millis(500)) {
            Some(changes) => changes.paths,
            None => return false,
        }
    };
    // From here on the changes are taken: declining must leave the watch
    // unusable, so a later refresh cannot skip them. A walk starts a new one.
    let decline = || {
        end_change_watch();
        false
    };

    let Some(model) = INDEX_MODEL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
    else {
        return decline();
    };
    let root_matches = model.tree.root().is_some_and(|(record, _)| {
        normalized_root(&model.tree.directory_path(record)) == normalized_root(root)
    });
    if !root_matches {
        return decline();
    }
    let Some(mut patch) = plan_patch(&model.tree, root, &changes) else {
        return decline();
    };
    drop(model);

    // Wait out the previous scan's snapshot write: it holds the model, and
    // a merge must not race it. Held until this refresh's merge is written.
    let gate = super::persist_gate_lock();
    let touched = {
        let mut guard = INDEX_MODEL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // A cancel before the model changes declines: the caller then sees
        // it, starts no walk and reports the session cancelled. Past this
        // point the patch is applied and the refresh completes.
        if MODEL_GENERATION.load(Ordering::SeqCst) != generation || cancelled.load(Ordering::SeqCst)
        {
            return decline();
        }
        let Some(model) = guard.as_mut().and_then(Arc::get_mut) else {
            return decline();
        };
        let first_id = model.tree.max_file_id().map_or(1, |id| id + 1);
        for (offset, file) in patch.added.iter_mut().enumerate() {
            file.id = FileId(first_id + offset as u64);
        }
        let size_changes: Vec<(String, u64, u64)> = patch
            .modified
            .iter()
            .filter_map(|file| {
                let old = model.tree.file_by_id(file.id.0)?;
                Some((file.name.to_string(), old.size_bytes, file.size_bytes))
            })
            .collect();
        let added = patch.added.clone();
        match model.tree.apply_file_changes(patch.added, patch.modified) {
            Ok(touched) => {
                patch_extension_totals(model, &added);
                adjust_modified_extension_sizes(model, &size_changes);
                touched
            }
            Err(_) => return decline(),
        }
    };
    *LAST_REFRESH_TOUCHED
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(touched);

    let Some(model) = INDEX_MODEL
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
    else {
        return decline();
    };
    // Report it the way a walk does: started, totals, completed.
    let volume = model.tree.volumes().first().cloned().unwrap_or_default();
    let summary = model
        .tree
        .root()
        .map(|(_, rollup)| winblaze_core::ScanSummary {
            files_seen: rollup.file_count,
            directories_seen: model.tree.directories().len() as u64,
            total_size_bytes: rollup.logical_bytes,
            total_allocation_bytes: rollup.physical_bytes,
        });
    // The structured log records it like a walk, so log readers see the
    // session start and complete.
    let mut log = super::EventLog::new(super::structured_log_path(index_root));
    let mut events = vec![ScanEvent::SessionStarted(volume)];
    events.extend(summary.map(ScanEvent::Summary));
    events.push(ScanEvent::Completed);
    for event in events {
        log.append_scan_event(&event);
        if let Some(cb) = callback {
            let native = super::convert_event(event);
            cb(&native.event, user_data as *mut core::ffi::c_void);
        }
    }

    // Merge into the saved catalog after the UI has the results, as an
    // incremental walk does: path-matched ids and the change summary.
    let merge = BufferedIndexTransaction::from_records(
        model.tree.volumes(),
        &[ScanSession {
            session_id: 1,
            volume_id: Default::default(),
            root_path: String::new(),
            state: ScanState::Completed,
            progress: Default::default(),
        }],
        model.tree.directories_with_paths(),
        model.tree.files(),
        true,
    );
    drop(model);
    let mut repository = SqliteIndexRepository::open(index_root, IndexBackend::BinaryCache);
    let merged = repository.apply_path_matched_incremental_transaction(&merge);
    if let Ok(change_set) = &merged {
        super::emit_incremental_change_summary(callback, user_data, change_set);
    }
    log.append_flush("deferred", merged.is_ok());
    drop(gate);
    true
}

fn adjust_modified_extension_sizes(model: &mut IndexModel, changes: &[(String, u64, u64)]) {
    if changes.is_empty() {
        return;
    }
    let mut buffer = [0u8; 32];
    for (name, old_size, new_size) in changes {
        let key = super::lowercase_extension(super::file_extension(name), &mut buffer).into_owned();
        if let Some(entry) = model.extension_top.iter_mut().find(|entry| entry.0 == key) {
            entry.1 = entry.1.saturating_sub(*old_size).saturating_add(*new_size);
        }
    }
    let totals = std::mem::take(&mut model.extension_top);
    model.extension_top = super::sorted_all_extension_totals(totals);
}

/// Directory ids whose totals the last fast refresh changed, for the UI to
/// update in place; None after a walk.
pub(super) static LAST_REFRESH_TOUCHED: Mutex<Option<Vec<u64>>> = Mutex::new(None);
