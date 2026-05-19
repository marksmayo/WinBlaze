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
