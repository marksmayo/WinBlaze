#![allow(non_camel_case_types)]

use core::ffi::c_char;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WbCStringView {
    pub ptr: *const c_char,
    pub len: usize,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WbNativeError {
    pub code: u32,
    pub message: WbCStringView,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WbCatalogEntry {
    pub name: WbCStringView,
    pub path: WbCStringView,
    pub kind: WbCStringView,
    pub size_text: WbCStringView,
    pub description: WbCStringView,
    /// Record id and parent directory id (valid when `has_parent` is set),
    /// so live scan events can be assembled into a tree without waiting for
    /// the post-scan index. Ids share the caveat from `WbTreeNode`: file and
    /// directory counters overlap numerically.
    pub id: u64,
    pub parent_id: u64,
    pub has_parent: u8,
    pub is_directory: u8,
    pub size_bytes: u64,
    /// Physical (on-disk allocation) size. For files this is the file's own
    /// allocation size; for directories/volumes it is the same rolled-up
    /// value already used for `size_bytes` (this crate does not currently
    /// track a separate logical-size rollup for directories).
    pub allocation_bytes: u64,
    pub total_entries: u64,
    pub modified_utc: i64,
    pub has_modified_utc: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WbIndexSnapshotStats {
    pub volumes: u64,
    pub directories: u64,
    pub files: u64,
    pub entries_emitted_limit: u64,
    pub cache_read_bytes: u64,
    pub cache_read_millis: u64,
    pub cache_decode_millis: u64,
    pub cache_loaded_from_backup: u8,
}

/// One entry in the display tree. `id` identifies a directory only when
/// `is_directory` is set — file and directory id counters overlap
/// numerically, so file ids must not be passed back to `wb_tree_children`.
/// The `name` view is valid only for the duration of the callback.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WbTreeNode {
    pub id: u64,
    pub is_directory: u8,
    pub name: WbCStringView,
    pub logical_bytes: u64,
    pub physical_bytes: u64,
    pub file_count: u64,
    pub item_count: u64,
    pub modified_utc: i64,
    pub has_modified_utc: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WbTreeChildrenResult {
    pub emitted: u64,
    pub total: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct WbPermissionRow {
    pub kind: WbCStringView,
    pub account: WbCStringView,
    pub sid: WbCStringView,
    pub rights: WbCStringView,
    pub mask: u32,
    pub inherited: u8,
}
pub type WbPermissionCallback =
    Option<extern "C" fn(row: *const WbPermissionRow, user_data: *mut core::ffi::c_void)>;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct WbDuplicateRow {
    pub group: u64,
    pub logical_bytes: u64,
    pub path: WbCStringView,
    pub digest: WbCStringView,
    pub status: WbCStringView,
    pub is_error: u8,
}
pub type WbDuplicateCallback =
    Option<extern "C" fn(row: *const WbDuplicateRow, user_data: *mut core::ffi::c_void)>;
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct WbDuplicateResult {
    pub groups: u64,
    pub errors: u64,
    pub reclaimable_bytes: u64,
    pub status: u32,
}

/// Complete-index search. Bound-presence flags: logical 1/2, allocation 4/8,
/// modified 16/32; include files 64, directories 128; descending 256.
/// Mode: contains/exact/prefix/regex = 0/1/2/3; sort name/logical/allocated/kind/modified = 0..4.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct WbSearchFilter {
    pub pattern: WbCStringView,
    pub extensions: WbCStringView,
    pub min_logical: u64,
    pub max_logical: u64,
    pub min_allocated: u64,
    pub max_allocated: u64,
    pub modified_after: i64,
    pub modified_before: i64,
    pub offset: u64,
    pub limit: u64,
    pub flags: u32,
    pub mode: u32,
    pub sort: u32,
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct WbSearchResult {
    pub emitted: u64,
    pub total: u64,
    /// 0 success, 1 invalid options, 4 internal failure.
    pub status: u32,
}

/// One directory in a live scan batch (see `WbLiveDirectoryBatch`).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WbLiveDirectory {
    pub id: u64,
    pub parent_id: u64,
    pub has_parent: u8,
    pub name: WbCStringView,
}

/// Borrowed view over a batch of scan-discovered directories. Directories
/// are batched rather than sent one event each — a full drive emits hundreds
/// of thousands, and per-event FFI crossings dominated scan wall-clock.
/// Valid only for the duration of the callback invocation.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WbLiveDirectoryBatch {
    pub items: *const WbLiveDirectory,
    pub count: usize,
}

impl Default for WbLiveDirectoryBatch {
    fn default() -> Self {
        Self {
            items: core::ptr::null(),
            count: 0,
        }
    }
}

pub type WbTreeNodeCallback =
    Option<extern "C" fn(node: *const WbTreeNode, user_data: *mut core::ffi::c_void)>;

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WbCatalogSnapshotKind {
    #[default]
    Volume = 1,
    Directory = 2,
    File = 3,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WbEventKind {
    SessionStarted = 1,
    Progress = 2,
    Summary = 3,
    Completed = 4,
    Cancelled = 5,
    #[default]
    Failed = 6,
    Issue = 7,
    VolumeDiscovered = 8,
    DirectoryFound = 9,
    FileFound = 10,
    IncrementalChanges = 11,
    ExtensionStats = 12,
    DirectoryBatch = 13,
}

/// One row of the live per-extension breakdown (bytes/files aggregated
/// across the whole scan, not just the UI's capped live catalog sample).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WbExtensionStat {
    pub extension: WbCStringView,
    pub description: WbCStringView,
    pub bytes: u64,
    pub files: u64,
}

/// Borrowed view over a set of `WbExtensionStat` rows, sorted by `bytes`
/// descending. Only valid for the duration of the callback invocation that
/// provided it (same lifetime discipline as the `WbCStringView` fields it
/// contains).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WbExtensionStatsSnapshot {
    pub items: *const WbExtensionStat,
    pub count: usize,
}

/// Result of an update check: whether the supplied release payload names a
/// newer version than the running one, plus that version in a fixed inline
/// buffer (no heap/ownership crossing the FFI). `latest` holds `latest_len`
/// UTF-8 bytes (e.g. `v0.9.0`), not NUL-terminated.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WbUpdateCheck {
    pub available: u8,
    pub parsed: u8,
    pub latest_len: u8,
    pub latest: [u8; 32],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WbScanSummary {
    pub files_seen: u64,
    pub directories_seen: u64,
    pub total_size_bytes: u64,
    pub total_allocation_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WbIncrementalChangeSummary {
    pub added: u64,
    pub removed: u64,
    pub modified: u64,
    pub renamed: u64,
    pub moved: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WbEvent {
    pub kind: WbEventKind,
    pub progress_items_done: u64,
    pub progress_items_total: u64,
    pub progress_bytes_done: u64,
    pub progress_bytes_total: u64,
    pub summary: WbScanSummary,
    pub incremental_changes: WbIncrementalChangeSummary,
    pub error: WbNativeError,
    pub catalog_entry: WbCatalogEntry,
    pub extension_stats: WbExtensionStatsSnapshot,
    pub directory_batch: WbLiveDirectoryBatch,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WbScanSessionHandle {
    pub _private: *mut core::ffi::c_void,
}

pub type WbEventCallback =
    Option<extern "C" fn(event: *const WbEvent, user_data: *mut core::ffi::c_void)>;

pub type WbCatalogCallback =
    Option<extern "C" fn(entry: *const WbCatalogEntry, user_data: *mut core::ffi::c_void)>;

pub type WbExtensionStatCallback =
    Option<extern "C" fn(entry: *const WbExtensionStat, user_data: *mut core::ffi::c_void)>;

/// Filename patterns separated by semicolons; flags 1/2/4/8 enable each bound.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct WbCsvFilter {
    pub patterns: WbCStringView,
    pub min_bytes: u64,
    pub max_bytes: u64,
    pub modified_after_utc: i64,
    pub modified_before_utc: i64,
    pub flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct WbStoragePolicy {
    pub now_utc: i64,
    pub warm_after_days: u64,
    pub cold_after_days: u64,
    pub monthly_prices: [f64; 3],
}
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct WbStorageEstimate {
    pub allocated_bytes: [u64; 3],
    pub files: [u64; 3],
    pub unknown_dates: u64,
    pub monthly_cost: f64,
    pub all_hot_cost: f64,
    pub status: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct WbMultiScanResult {
    pub roots: u64,
    pub files: u64,
    pub logical_bytes: u64,
    pub allocated_bytes: u64,
    pub warnings: u64,
    pub status: u32,
}
