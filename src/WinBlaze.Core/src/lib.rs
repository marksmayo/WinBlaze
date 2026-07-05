#![forbid(unsafe_code)]

pub mod diagnostics;
pub mod ffi;
pub mod hashing;
pub mod model;
pub mod query;
pub mod scan;

pub use diagnostics::ScanIssueSummary;
pub use ffi::{WbCStringView, WbError, WbProgress};
pub use hashing::{BuildIdHasher, IdHashMap, IdHashSet};
pub use model::{
    aggregate_directory_records, derive_file_path, detect_file_lineage_change,
    diff_file_records, join_path, DirectoryAggregation, DirectoryId, DirectoryRecord,
    FileAttributes, FileChangeKind, FileChangeRecord, FileChangeSet, FileId, FileLineageRecord,
    FileRecord, FileSystemKind, ScanSession, ScanSummary, VolumeId, VolumeRecord,
};
pub use query::{
    DateFilter, MatchMode, SearchQuery, SearchScope, SizeFilter, SortDirection, SortField,
};
pub use scan::{ScanEvent, ScanProgress, ScanRequest, ScanState};
pub use scan::{ScanIssueKind, ScanIssueRecord};
