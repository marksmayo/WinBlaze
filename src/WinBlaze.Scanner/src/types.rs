use std::path::PathBuf;

use crate::filesystem::{is_long_path, normalize_scan_root, select_scan_backend};
use crate::performance::ScanPipelineConfig;
use crate::policy::ReparseTraversalPolicy;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ScanBackend {
    #[default]
    NtfsMft,
    DirectoryWalk,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanRuntimeConfig {
    pub backend: ScanBackend,
    pub max_parallelism: usize,
    pub emit_partial_results: bool,
    pub root_path: PathBuf,
    pub reparse_policy: ReparseTraversalPolicy,
    pub pipeline: ScanPipelineConfig,
}

impl Default for ScanRuntimeConfig {
    fn default() -> Self {
        Self {
            backend: ScanBackend::default(),
            // A derived default left this at 0 (= one worker), silently
            // running every scan single-threaded for any caller that didn't
            // override it — measured at 9x slower on a full-drive walk.
            max_parallelism: std::thread::available_parallelism()
                .map(|parallelism| parallelism.get())
                .unwrap_or(4),
            emit_partial_results: false,
            root_path: PathBuf::new(),
            reparse_policy: ReparseTraversalPolicy::default(),
            pipeline: ScanPipelineConfig::default(),
        }
    }
}

impl ScanRuntimeConfig {
    pub fn normalized_root_path(&self) -> PathBuf {
        normalize_scan_root(&self.root_path)
    }

    pub fn backend_hint(&self) -> ScanBackend {
        if self.root_path.as_os_str().is_empty() {
            self.backend
        } else {
            select_scan_backend(&self.root_path)
        }
    }

    pub fn is_long_path(&self) -> bool {
        is_long_path(&self.root_path)
    }

    pub fn follows_reparse_points(&self) -> bool {
        !matches!(self.reparse_policy, ReparseTraversalPolicy::SkipAll)
    }
}
