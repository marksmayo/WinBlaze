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

/// Scan-time rules. Excluded paths are relative to the scan root; include
/// patterns match file names using case-insensitive '*' and '?' wildcards.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanRules {
    pub exclude_paths: Vec<String>,
    pub include_files: Vec<String>,
}
impl ScanRules {
    pub fn is_empty(&self) -> bool {
        self.exclude_paths.is_empty() && self.include_files.is_empty()
    }
    pub(crate) fn allows(
        &self,
        root: &std::path::Path,
        path: &std::path::Path,
        directory: bool,
    ) -> bool {
        let relative = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('/', "\\")
            .to_lowercase();
        if self.exclude_paths.iter().any(|rule| {
            let rule = rule
                .trim()
                .trim_matches(['/', '\\'])
                .replace('/', "\\")
                .to_lowercase();
            !rule.is_empty() && (relative == rule || relative.starts_with(&(rule + "\\")))
        }) {
            return false;
        }
        directory
            || self.include_files.is_empty()
            || self.include_files.iter().any(|pattern| {
                wildcard(
                    &pattern.to_lowercase(),
                    &path
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_lowercase(),
                )
            })
    }
}
fn wildcard(pattern: &str, name: &str) -> bool {
    let pattern: Vec<_> = pattern.chars().collect();
    let name: Vec<_> = name.chars().collect();
    let (mut p, mut n, mut star, mut retry) = (0, 0, None, 0);
    while n < name.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some(p);
            p += 1;
            retry = n;
        } else if let Some(index) = star {
            retry += 1;
            n = retry;
            p = index + 1;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == '*' {
        p += 1;
    }
    p == pattern.len()
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanRuntimeConfig {
    pub rules: ScanRules,
    pub backend: ScanBackend,
    pub max_parallelism: usize,
    /// Directory metadata walking has a separate, lower default I/O budget.
    /// The effective worker count never exceeds max_parallelism. Explicit
    /// callers can raise this limit after measuring their storage workload.
    pub directory_walk_max_parallelism: usize,
    pub emit_partial_results: bool,
    pub root_path: PathBuf,
    pub reparse_policy: ReparseTraversalPolicy,
    pub pipeline: ScanPipelineConfig,
    /// Batched directory metadata when supported, with large-fetch FindFirst
    /// and std fallbacks on open failure. Off uses std plus per-file
    /// allocation/identity queries as the reference implementation.
    pub fast_enumeration: bool,
}

impl Default for ScanRuntimeConfig {
    fn default() -> Self {
        Self {
            rules: ScanRules::default(),
            backend: ScanBackend::default(),
            // A derived default left this at 0 (= one worker), silently
            // running every scan single-threaded for any caller that didn't
            // override it — measured at 9x slower on a full-drive walk.
            max_parallelism: std::thread::available_parallelism()
                .map(|parallelism| parallelism.get())
                .unwrap_or(4),
            directory_walk_max_parallelism: 4,
            emit_partial_results: false,
            root_path: PathBuf::new(),
            reparse_policy: ReparseTraversalPolicy::default(),
            pipeline: ScanPipelineConfig::default(),
            fast_enumeration: true,
        }
    }
}

impl ScanRuntimeConfig {
    pub fn directory_walk_worker_count(&self) -> usize {
        self.max_parallelism
            .max(1)
            .min(self.directory_walk_max_parallelism.max(1))
    }

    pub fn normalized_root_path(&self) -> PathBuf {
        normalize_scan_root(&self.root_path)
    }

    pub fn backend_hint(&self) -> ScanBackend {
        // An explicit DirectoryWalk request always wins; auto-selection only
        // upgrades the default NtfsMft preference for paths that support it.
        match self.backend {
            ScanBackend::DirectoryWalk => ScanBackend::DirectoryWalk,
            ScanBackend::NtfsMft if self.root_path.as_os_str().is_empty() => ScanBackend::NtfsMft,
            ScanBackend::NtfsMft => select_scan_backend(&self.root_path),
        }
    }

    pub fn is_long_path(&self) -> bool {
        is_long_path(&self.root_path)
    }

    pub fn follows_reparse_points(&self) -> bool {
        !matches!(self.reparse_policy, ReparseTraversalPolicy::SkipAll)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_hint_honors_explicit_directory_walk_for_volume_roots() {
        let config = ScanRuntimeConfig {
            backend: ScanBackend::DirectoryWalk,
            root_path: PathBuf::from(r"C:\"),
            ..ScanRuntimeConfig::default()
        };
        assert_eq!(config.backend_hint(), ScanBackend::DirectoryWalk);
    }

    #[test]
    fn backend_hint_auto_selects_for_default_mft_preference() {
        let config = ScanRuntimeConfig {
            backend: ScanBackend::NtfsMft,
            root_path: PathBuf::from(r"C:\Users"),
            ..ScanRuntimeConfig::default()
        };
        assert_eq!(config.backend_hint(), ScanBackend::DirectoryWalk);
    }

    #[test]
    fn normalized_root_path_strips_dot_segments() {
        let config = ScanRuntimeConfig {
            root_path: PathBuf::from(r"C:\a\.\b\..\c"),
            ..ScanRuntimeConfig::default()
        };
        assert_eq!(config.normalized_root_path(), PathBuf::from(r"C:\a\c"));
    }

    #[test]
    fn is_long_path_flags_paths_at_the_legacy_limit() {
        let short = ScanRuntimeConfig {
            root_path: PathBuf::from(r"C:\short"),
            ..ScanRuntimeConfig::default()
        };
        assert!(!short.is_long_path());

        let long = ScanRuntimeConfig {
            root_path: PathBuf::from(format!(r"C:\{}", "a".repeat(300))),
            ..ScanRuntimeConfig::default()
        };
        assert!(long.is_long_path());
    }

    #[test]
    fn follows_reparse_points_reflects_policy() {
        // The default policy is SkipAll (disk-usage scans don't descend into
        // reparse points), so the default does NOT follow them.
        assert!(!ScanRuntimeConfig::default().follows_reparse_points());

        let follow = ScanRuntimeConfig {
            reparse_policy: ReparseTraversalPolicy::FollowAll,
            ..ScanRuntimeConfig::default()
        };
        assert!(follow.follows_reparse_points());
    }
}
