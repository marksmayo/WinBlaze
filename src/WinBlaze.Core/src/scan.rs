#![allow(clippy::module_name_repetitions)]

use crate::model::{DirectoryRecord, FileRecord, ScanSummary, VolumeRecord};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ScanIssueKind {
    PermissionDenied,
    NotFound,
    SharingViolation,
    TransientIo,
    UnsupportedFilesystem,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanIssueRecord {
    pub kind: ScanIssueKind,
    pub path: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanProgress {
    pub completed_items: u64,
    pub total_items: u64,
    pub completed_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScanState {
    #[default]
    Idle,
    Initializing,
    Scanning,
    Indexing,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScanEvent {
    SessionStarted(VolumeRecord),
    VolumeDiscovered(VolumeRecord),
    DirectoryFound(DirectoryRecord),
    FileFound(FileRecord),
    Issue(ScanIssueRecord),
    Progress(ScanProgress),
    Summary(ScanSummary),
    Completed,
    Failed(String),
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanRequest {
    pub root_path: String,
    pub follow_reparse_points: bool,
    pub emit_partial_results: bool,
}

pub trait ScanEventSink {
    fn handle_event(&mut self, event: ScanEvent);
}
