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
pub struct WbProgress {
    pub completed_items: u64,
    pub total_items: u64,
    pub completed_bytes: u64,
    pub total_bytes: u64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WbError {
    pub code: u32,
    pub reserved: u32,
    pub message: WbCStringView,
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
pub struct WbScannerHandle {
    pub _private: *mut core::ffi::c_void,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WbIndexHandle {
    pub _private: *mut core::ffi::c_void,
}
