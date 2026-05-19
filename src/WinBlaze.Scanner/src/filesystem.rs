use std::path::{Component, Path, PathBuf};

use crate::types::ScanBackend;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VolumeRootCandidate {
    pub normalized_path: PathBuf,
    pub root_path: PathBuf,
    pub drive_letter: Option<char>,
    pub is_unc: bool,
    pub is_long_path: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScanAccessPlan {
    pub requested_root: PathBuf,
    pub selected_root: PathBuf,
    pub root_candidate: Option<VolumeRootCandidate>,
    pub primary_backend: ScanBackend,
    pub fallback_backend: ScanBackend,
    pub available_drive_roots: Vec<PathBuf>,
}

pub fn discover_available_drive_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    for letter in b'A'..=b'Z' {
        let root = PathBuf::from(format!("{}:\\", letter as char));
        if root.exists() {
            roots.push(root);
        }
    }

    roots
}

pub fn build_scan_access_plan(root: &Path, preferred_backend: ScanBackend) -> ScanAccessPlan {
    let requested_root = root.to_path_buf();
    let normalized_root = normalize_scan_root(root);
    let root_candidate = discover_volume_root(&normalized_root);
    let available_drive_roots = discover_available_drive_roots();
    let requested_volume_root = root_candidate
        .as_ref()
        .is_some_and(|candidate| candidate.normalized_path == candidate.root_path);
    let selected_root = if normalized_root.as_os_str().is_empty() {
        available_drive_roots
            .first()
            .cloned()
            .unwrap_or_else(|| PathBuf::from(r"C:\"))
    } else if requested_volume_root {
        root_candidate
            .as_ref()
            .map(|candidate| candidate.root_path.clone())
            .unwrap_or_else(|| normalized_root.clone())
    } else {
        normalized_root.clone()
    };

    let primary_backend = match preferred_backend {
        ScanBackend::NtfsMft if requested_volume_root => ScanBackend::NtfsMft,
        ScanBackend::NtfsMft => ScanBackend::DirectoryWalk,
        ScanBackend::DirectoryWalk => ScanBackend::DirectoryWalk,
    };
    let fallback_backend = match primary_backend {
        ScanBackend::NtfsMft => ScanBackend::DirectoryWalk,
        ScanBackend::DirectoryWalk => ScanBackend::DirectoryWalk,
    };

    ScanAccessPlan {
        requested_root,
        selected_root,
        root_candidate,
        primary_backend,
        fallback_backend,
        available_drive_roots,
    }
}

pub fn normalize_scan_root(input: &Path) -> PathBuf {
    let mut output = PathBuf::new();

    for component in input.components() {
        match component {
            Component::Prefix(prefix) => output.push(prefix.as_os_str()),
            Component::RootDir => output.push(std::path::MAIN_SEPARATOR.to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = output.pop();
            }
            Component::Normal(part) => output.push(part),
        }
    }

    output
}

pub fn is_long_path(path: &Path) -> bool {
    let text = path.as_os_str().to_string_lossy();
    text.len() >= 260 && !text.starts_with(r"\\?\")
}

pub fn discover_volume_root(path: &Path) -> Option<VolumeRootCandidate> {
    let normalized_path = normalize_scan_root(path);
    let mut components = normalized_path.components();
    let first = components.next()?;

    match first {
        Component::Prefix(prefix) => match prefix.kind() {
            std::path::Prefix::Disk(letter) | std::path::Prefix::VerbatimDisk(letter) => {
                let drive_letter = Some((letter as char).to_ascii_uppercase());
                let root_path =
                    PathBuf::from(format!("{}:\\", (letter as char).to_ascii_uppercase()));
                Some(VolumeRootCandidate {
                    normalized_path,
                    root_path,
                    drive_letter,
                    is_unc: false,
                    is_long_path: is_long_path(path),
                })
            }
            std::path::Prefix::UNC(server, share)
            | std::path::Prefix::VerbatimUNC(server, share) => {
                let root_path = PathBuf::from(format!(
                    r"\\{}\{}\",
                    server.to_string_lossy(),
                    share.to_string_lossy()
                ));
                Some(VolumeRootCandidate {
                    normalized_path,
                    root_path,
                    drive_letter: None,
                    is_unc: true,
                    is_long_path: is_long_path(path),
                })
            }
            _ => None,
        },
        _ => None,
    }
}

pub fn select_scan_backend(path: &Path) -> ScanBackend {
    if discover_volume_root(path)
        .is_some_and(|candidate| candidate.normalized_path == candidate.root_path)
    {
        ScanBackend::NtfsMft
    } else {
        ScanBackend::DirectoryWalk
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_scan_root_removes_dot_segments() {
        let normalized = normalize_scan_root(Path::new(r"C:\Users\.\markm\..\Public"));
        assert_eq!(normalized, PathBuf::from(r"C:\Users\Public"));
    }

    #[test]
    fn discover_volume_root_recognizes_drive_roots() {
        let candidate = discover_volume_root(Path::new(r"C:\Users\markm"))
            .expect("drive root should be discovered");
        assert_eq!(candidate.drive_letter, Some('C'));
        assert_eq!(candidate.root_path, PathBuf::from(r"C:\"));
        assert!(!candidate.is_unc);
    }

    #[test]
    fn discover_available_drive_roots_includes_system_drive() {
        let roots = discover_available_drive_roots();
        assert!(roots.iter().any(|root| root == &PathBuf::from(r"C:\")));
    }

    #[test]
    fn build_scan_access_plan_selects_drive_root_and_fallback_backend() {
        let plan = build_scan_access_plan(Path::new(r"C:\Users\markm"), ScanBackend::NtfsMft);
        assert_eq!(plan.selected_root, PathBuf::from(r"C:\Users\markm"));
        assert_eq!(plan.primary_backend, ScanBackend::DirectoryWalk);
        assert_eq!(plan.fallback_backend, ScanBackend::DirectoryWalk);
    }

    #[test]
    fn build_scan_access_plan_uses_mft_for_requested_volume_roots() {
        let plan = build_scan_access_plan(Path::new(r"C:\"), ScanBackend::NtfsMft);
        assert_eq!(plan.selected_root, PathBuf::from(r"C:\"));
        assert_eq!(plan.primary_backend, ScanBackend::NtfsMft);
        assert_eq!(plan.fallback_backend, ScanBackend::DirectoryWalk);
    }

    #[test]
    fn select_scan_backend_prefers_walk_for_relative_paths() {
        assert_eq!(
            select_scan_backend(Path::new(r"relative\path")),
            ScanBackend::DirectoryWalk
        );
    }
}
