use std::path::Path;

use winblaze_core::FileAttributes;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReparseTraversalPolicy {
    SkipAll,
    FollowMountPointsAndJunctions,
    #[default]
    FollowAll,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReparseTargetKind {
    Junction,
    Symlink,
    MountPoint,
}

pub fn classify_reparse_target(
    path: &Path,
    attributes: FileAttributes,
) -> Option<ReparseTargetKind> {
    if !attributes.is_reparse_point() {
        return None;
    }

    let text = path.as_os_str().to_string_lossy();
    if text.starts_with(r"\\?\Volume{") || text.starts_with(r"\\.\Volume") {
        Some(ReparseTargetKind::MountPoint)
    } else if attributes.is_directory() {
        Some(ReparseTargetKind::Junction)
    } else {
        Some(ReparseTargetKind::Symlink)
    }
}

pub fn should_follow_reparse_target(
    kind: ReparseTargetKind,
    policy: ReparseTraversalPolicy,
) -> bool {
    match policy {
        ReparseTraversalPolicy::SkipAll => false,
        ReparseTraversalPolicy::FollowMountPointsAndJunctions => {
            !matches!(kind, ReparseTargetKind::Symlink)
        }
        ReparseTraversalPolicy::FollowAll => true,
    }
}

pub fn should_descend_into_reparse_target(
    path: &Path,
    attributes: FileAttributes,
    policy: ReparseTraversalPolicy,
) -> bool {
    classify_reparse_target(path, attributes)
        .map(|kind| should_follow_reparse_target(kind, policy))
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use winblaze_core::FileAttributes;

    #[test]
    fn classify_reparse_target_identifies_directories_and_mount_points() {
        let junction = classify_reparse_target(
            Path::new(r"C:\\Users\\markm\\Links"),
            FileAttributes::DIRECTORY | FileAttributes::REPARSE_POINT,
        );
        assert!(matches!(junction, Some(ReparseTargetKind::Junction)));

        let mount_point = classify_reparse_target(
            Path::new(r"\\?\Volume{00000000-0000-0000-0000-000000000000}\\"),
            FileAttributes::REPARSE_POINT,
        );
        assert!(matches!(mount_point, Some(ReparseTargetKind::MountPoint)));
    }

    #[test]
    fn should_follow_reparse_target_honors_policy() {
        assert!(should_follow_reparse_target(
            ReparseTargetKind::MountPoint,
            ReparseTraversalPolicy::FollowMountPointsAndJunctions
        ));
        assert!(!should_follow_reparse_target(
            ReparseTargetKind::Symlink,
            ReparseTraversalPolicy::FollowMountPointsAndJunctions
        ));
        assert!(!should_follow_reparse_target(
            ReparseTargetKind::Junction,
            ReparseTraversalPolicy::SkipAll
        ));
    }
}
