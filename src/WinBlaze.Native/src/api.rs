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
