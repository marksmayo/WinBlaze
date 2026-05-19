pub mod controller;
pub mod errors;
pub mod filesystem;
pub mod ntfs;
pub mod performance;
pub mod pipeline;
pub mod policy;
pub mod scheduler;
#[cfg(test)]
mod tests;
pub mod types;

pub use controller::{ScanController, ScanHandle, ScanRequest};
pub use errors::{
    classify_io_error, is_permission_failure, is_transient_io_error, ScanErrorKind, ScanErrorRecord,
};
pub use filesystem::{
    build_scan_access_plan, discover_available_drive_roots, discover_volume_root, is_long_path,
    normalize_scan_root, select_scan_backend, ScanAccessPlan, VolumeRootCandidate,
};
pub use ntfs::{
    enumerate_ntfs_volume, enumerate_ntfs_volume_parallel, parse_mft_records,
    parse_mft_records_parallel, NtfsEnumeration, NtfsEnumerationError,
};
pub use performance::{ScanMemorySample, ScanPipelineConfig, ScanThroughputSample};
pub use pipeline::ScanEventPipeline;
pub use policy::{
    classify_reparse_target, should_descend_into_reparse_target, should_follow_reparse_target,
    ReparseTargetKind, ReparseTraversalPolicy,
};
pub use scheduler::{ScanScheduler, WorkerCount};
pub use types::{ScanBackend, ScanRuntimeConfig};
