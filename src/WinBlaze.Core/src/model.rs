#![allow(clippy::module_name_repetitions)]

use core::ops::{BitAnd, BitAndAssign, BitOr, BitOrAssign};
use std::collections::HashMap;

use crate::scan::{ScanProgress, ScanState};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FileId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct DirectoryId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct VolumeId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FileSystemKind {
    Ntfs,
    Refs,
    Fat32,
    ExFat,
    #[default]
    Unknown,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct FileAttributes(pub u32);

impl FileAttributes {
    pub const READ_ONLY: Self = Self(0x0000_0001);
    pub const HIDDEN: Self = Self(0x0000_0002);
    pub const SYSTEM: Self = Self(0x0000_0004);
    pub const DIRECTORY: Self = Self(0x0000_0010);
    pub const ARCHIVE: Self = Self(0x0000_0020);
    pub const REPARSE_POINT: Self = Self(0x0000_0400);

    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn is_directory(self) -> bool {
        self.contains(Self::DIRECTORY)
    }

    pub fn is_reparse_point(self) -> bool {
        self.contains(Self::REPARSE_POINT)
    }
}

impl BitOr for FileAttributes {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for FileAttributes {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

impl BitAnd for FileAttributes {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self(self.0 & rhs.0)
    }
}

impl BitAndAssign for FileAttributes {
    fn bitand_assign(&mut self, rhs: Self) {
        self.0 &= rhs.0;
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileRecord {
    pub id: FileId,
    pub parent_directory_id: DirectoryId,
    pub name: String,
    /// May be empty: scanners no longer materialize per-file paths (storing
    /// one per record dominated index memory and snapshot size). Derive on
    /// demand from the parent directory's `full_path` via
    /// [`derive_file_path`] / [`join_path`]. Directory records always carry
    /// their full path.
    pub full_path: String,
    pub size_bytes: u64,
    pub allocation_bytes: u64,
    pub attributes: FileAttributes,
    pub created_utc: Option<i64>,
    pub modified_utc: Option<i64>,
    pub accessed_utc: Option<i64>,
}

/// Joins a parent directory path and child name with a single backslash.
pub fn join_path(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        return name.to_string();
    }
    if parent.ends_with('\\') {
        format!("{parent}{name}")
    } else {
        format!("{parent}\\{name}")
    }
}

/// Resolves a file's full path: the stored path when present (records from
/// older snapshots still carry one), otherwise parent path + name.
pub fn derive_file_path(
    directories: &std::collections::HashMap<DirectoryId, DirectoryRecord>,
    file: &FileRecord,
) -> String {
    if !file.full_path.is_empty() {
        return file.full_path.clone();
    }
    match directories.get(&file.parent_directory_id) {
        Some(parent) => join_path(&parent.full_path, &file.name),
        None => file.name.clone(),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DirectoryRecord {
    pub id: DirectoryId,
    pub parent_directory_id: Option<DirectoryId>,
    pub name: String,
    pub full_path: String,
    pub direct_bytes: u64,
    pub total_bytes: u64,
    pub direct_entries: u64,
    pub total_entries: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VolumeRecord {
    pub id: VolumeId,
    pub mount_point: String,
    pub label: Option<String>,
    pub file_system: FileSystemKind,
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub root_directory_id: DirectoryId,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanSession {
    pub session_id: u64,
    pub volume_id: VolumeId,
    pub root_path: String,
    pub state: ScanState,
    pub progress: ScanProgress,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ScanSummary {
    pub files_seen: u64,
    pub directories_seen: u64,
    pub total_size_bytes: u64,
    pub total_allocation_bytes: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileLineageRecord {
    pub file_id: FileId,
    pub previous_parent_directory_id: DirectoryId,
    pub current_parent_directory_id: DirectoryId,
    pub previous_full_path: String,
    pub current_full_path: String,
    pub renamed: bool,
    pub moved: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileChangeKind {
    Added,
    Removed,
    Modified,
    Renamed,
    Moved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileChangeRecord {
    pub file_id: FileId,
    pub kind: FileChangeKind,
    pub previous_full_path: Option<String>,
    pub current_full_path: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileChangeSet {
    pub changes: Vec<FileChangeRecord>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DirectoryAggregation {
    pub direct_bytes: u64,
    pub total_bytes: u64,
    pub direct_entries: u64,
    pub total_entries: u64,
}

pub fn aggregate_directory_records(
    directories: &[DirectoryRecord],
    files: &[FileRecord],
) -> Vec<DirectoryRecord> {
    let parent_map: HashMap<DirectoryId, Option<DirectoryId>> = directories
        .iter()
        .map(|directory| (directory.id, directory.parent_directory_id))
        .collect();

    let mut aggregates: HashMap<DirectoryId, DirectoryAggregation> = directories
        .iter()
        .map(|directory| (directory.id, DirectoryAggregation::default()))
        .collect();

    for directory in directories {
        if let Some(parent_directory_id) = directory.parent_directory_id {
            increment_directory_entry_aggregation(
                &mut aggregates,
                &parent_map,
                parent_directory_id,
            );
        }
    }

    for file in files {
        increment_file_aggregation(
            &mut aggregates,
            &parent_map,
            file.parent_directory_id,
            file.size_bytes,
            file.allocation_bytes,
        );
    }

    let mut aggregated_directories = directories.to_vec();
    for directory in &mut aggregated_directories {
        if let Some(aggregate) = aggregates.get(&directory.id) {
            directory.direct_bytes = aggregate.direct_bytes;
            directory.total_bytes = aggregate.total_bytes;
            directory.direct_entries = aggregate.direct_entries;
            directory.total_entries = aggregate.total_entries;
        }
    }

    aggregated_directories
}

fn increment_directory_entry_aggregation(
    aggregates: &mut HashMap<DirectoryId, DirectoryAggregation>,
    parent_map: &HashMap<DirectoryId, Option<DirectoryId>>,
    mut current: DirectoryId,
) {
    let mut is_direct_entry = true;

    while let Some(aggregate) = aggregates.get_mut(&current) {
        if is_direct_entry {
            aggregate.direct_entries = aggregate.direct_entries.saturating_add(1);
            is_direct_entry = false;
        }

        aggregate.total_entries = aggregate.total_entries.saturating_add(1);

        match parent_map.get(&current).and_then(|value| *value) {
            Some(parent) => current = parent,
            None => break,
        }
    }
}

fn increment_file_aggregation(
    aggregates: &mut HashMap<DirectoryId, DirectoryAggregation>,
    parent_map: &HashMap<DirectoryId, Option<DirectoryId>>,
    mut current: DirectoryId,
    size_bytes: u64,
    allocation_bytes: u64,
) {
    let mut is_direct_entry = true;

    while let Some(aggregate) = aggregates.get_mut(&current) {
        if is_direct_entry {
            aggregate.direct_bytes = aggregate.direct_bytes.saturating_add(size_bytes);
            aggregate.direct_entries = aggregate.direct_entries.saturating_add(1);
            is_direct_entry = false;
        }

        aggregate.total_entries = aggregate.total_entries.saturating_add(1);
        aggregate.total_bytes = aggregate.total_bytes.saturating_add(allocation_bytes);

        match parent_map.get(&current).and_then(|value| *value) {
            Some(parent) => current = parent,
            None => break,
        }
    }
}

pub fn detect_file_lineage_change(
    previous: &FileRecord,
    current: &FileRecord,
) -> Option<FileLineageRecord> {
    if previous.id != current.id {
        return None;
    }

    let renamed = previous.name != current.name;
    let moved = previous.parent_directory_id != current.parent_directory_id
        || previous.full_path != current.full_path;

    if !renamed && !moved {
        return None;
    }

    Some(FileLineageRecord {
        file_id: current.id,
        previous_parent_directory_id: previous.parent_directory_id,
        current_parent_directory_id: current.parent_directory_id,
        previous_full_path: previous.full_path.clone(),
        current_full_path: current.full_path.clone(),
        renamed,
        moved,
    })
}

pub fn diff_file_records(previous: &[FileRecord], current: &[FileRecord]) -> FileChangeSet {
    let previous_by_id: HashMap<FileId, &FileRecord> =
        previous.iter().map(|record| (record.id, record)).collect();
    let current_by_id: HashMap<FileId, &FileRecord> =
        current.iter().map(|record| (record.id, record)).collect();

    let mut changes = Vec::new();

    for current_record in current {
        match previous_by_id.get(&current_record.id).copied() {
            None => changes.push(FileChangeRecord {
                file_id: current_record.id,
                kind: FileChangeKind::Added,
                previous_full_path: None,
                current_full_path: Some(current_record.full_path.clone()),
            }),
            Some(previous_record)
                if previous_record.name == current_record.name
                    && previous_record.parent_directory_id
                        == current_record.parent_directory_id
                    && previous_record.size_bytes == current_record.size_bytes
                    && previous_record.allocation_bytes == current_record.allocation_bytes
                    && previous_record.attributes == current_record.attributes
                    && previous_record.created_utc == current_record.created_utc
                    && previous_record.modified_utc == current_record.modified_utc
                    && previous_record.accessed_utc == current_record.accessed_utc => {}
            Some(previous_record) => {
                let kind = match detect_file_lineage_change(previous_record, current_record) {
                    Some(lineage) if lineage.renamed && lineage.moved => FileChangeKind::Moved,
                    Some(lineage) if lineage.renamed => FileChangeKind::Renamed,
                    Some(lineage) if lineage.moved => FileChangeKind::Moved,
                    _ => FileChangeKind::Modified,
                };

                changes.push(FileChangeRecord {
                    file_id: current_record.id,
                    kind,
                    previous_full_path: Some(previous_record.full_path.clone()),
                    current_full_path: Some(current_record.full_path.clone()),
                });
            }
        }
    }

    for previous_record in previous {
        if !current_by_id.contains_key(&previous_record.id) {
            changes.push(FileChangeRecord {
                file_id: previous_record.id,
                kind: FileChangeKind::Removed,
                previous_full_path: Some(previous_record.full_path.clone()),
                current_full_path: None,
            });
        }
    }

    FileChangeSet { changes }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DirectoryId, DirectoryRecord, FileAttributes, FileId, FileRecord};

    #[test]
    fn aggregate_directory_records_rolls_up_files_and_subdirectories() {
        let directories = vec![
            DirectoryRecord {
                id: DirectoryId(1),
                parent_directory_id: None,
                name: String::from("root"),
                full_path: String::from("C:\\root"),
                direct_bytes: 0,
                total_bytes: 0,
                direct_entries: 0,
                total_entries: 0,
            },
            DirectoryRecord {
                id: DirectoryId(2),
                parent_directory_id: Some(DirectoryId(1)),
                name: String::from("child"),
                full_path: String::from("C:\\root\\child"),
                direct_bytes: 0,
                total_bytes: 0,
                direct_entries: 0,
                total_entries: 0,
            },
        ];
        let files = vec![
            FileRecord {
                id: FileId(1),
                parent_directory_id: DirectoryId(1),
                name: String::from("root.txt"),
                full_path: String::from("C:\\root\\root.txt"),
                size_bytes: 10,
                allocation_bytes: 16,
                attributes: FileAttributes::ARCHIVE,
                created_utc: None,
                modified_utc: None,
                accessed_utc: None,
            },
            FileRecord {
                id: FileId(2),
                parent_directory_id: DirectoryId(2),
                name: String::from("child.txt"),
                full_path: String::from("C:\\root\\child\\child.txt"),
                size_bytes: 20,
                allocation_bytes: 32,
                attributes: FileAttributes::ARCHIVE,
                created_utc: None,
                modified_utc: None,
                accessed_utc: None,
            },
        ];

        let aggregated = aggregate_directory_records(&directories, &files);
        let root = aggregated
            .iter()
            .find(|directory| directory.id == DirectoryId(1))
            .expect("root directory");
        let child = aggregated
            .iter()
            .find(|directory| directory.id == DirectoryId(2))
            .expect("child directory");

        assert_eq!(root.direct_bytes, 10);
        assert_eq!(root.total_bytes, 48);
        assert_eq!(root.direct_entries, 2);
        assert_eq!(root.total_entries, 3);
        assert_eq!(child.direct_bytes, 20);
        assert_eq!(child.total_bytes, 32);
        assert_eq!(child.direct_entries, 1);
        assert_eq!(child.total_entries, 1);
    }

    #[test]
    fn derive_file_path_joins_parent_and_falls_back() {
        let mut directories = std::collections::HashMap::new();
        directories.insert(
            DirectoryId(9),
            DirectoryRecord {
                id: DirectoryId(9),
                parent_directory_id: None,
                name: String::from("Users"),
                full_path: String::from(r"C:\Users"),
                direct_bytes: 0,
                total_bytes: 0,
                direct_entries: 0,
                total_entries: 0,
            },
        );

        let file = FileRecord {
            id: FileId(1),
            parent_directory_id: DirectoryId(9),
            name: String::from("file.txt"),
            full_path: String::new(),
            size_bytes: 1,
            allocation_bytes: 1,
            attributes: FileAttributes::ARCHIVE,
            created_utc: None,
            modified_utc: None,
            accessed_utc: None,
        };
        assert_eq!(derive_file_path(&directories, &file), r"C:\Users\file.txt");

        // Stored paths win (older snapshots still carry them).
        let mut stored = file.clone();
        stored.full_path = String::from(r"D:\elsewhere\file.txt");
        assert_eq!(derive_file_path(&directories, &stored), r"D:\elsewhere\file.txt");

        // Unknown parent falls back to the bare name.
        let mut orphan = file.clone();
        orphan.parent_directory_id = DirectoryId(404);
        assert_eq!(derive_file_path(&directories, &orphan), "file.txt");

        // Root paths with a trailing separator don't double it.
        assert_eq!(join_path(r"C:\", "file.txt"), r"C:\file.txt");
        assert_eq!(join_path(r"C:\Users", "file.txt"), r"C:\Users\file.txt");
    }

    #[test]
    fn detect_file_lineage_change_reports_rename_and_move() {
        let previous = FileRecord {
            id: FileId(7),
            parent_directory_id: DirectoryId(1),
            name: String::from("old.txt"),
            full_path: String::from("C:\\root\\old.txt"),
            size_bytes: 1,
            allocation_bytes: 1,
            attributes: FileAttributes::ARCHIVE,
            created_utc: None,
            modified_utc: None,
            accessed_utc: None,
        };
        let current = FileRecord {
            id: FileId(7),
            parent_directory_id: DirectoryId(2),
            name: String::from("new.txt"),
            full_path: String::from("C:\\root\\child\\new.txt"),
            size_bytes: 1,
            allocation_bytes: 1,
            attributes: FileAttributes::ARCHIVE,
            created_utc: None,
            modified_utc: None,
            accessed_utc: None,
        };

        let lineage = detect_file_lineage_change(&previous, &current).expect("lineage change");
        assert_eq!(lineage.file_id, FileId(7));
        assert!(lineage.renamed);
        assert!(lineage.moved);
    }

    #[test]
    fn diff_file_records_classifies_add_modify_and_remove() {
        let previous = vec![FileRecord {
            id: FileId(1),
            parent_directory_id: DirectoryId(1),
            name: String::from("old.txt"),
            full_path: String::from("C:\\root\\old.txt"),
            size_bytes: 10,
            allocation_bytes: 16,
            attributes: FileAttributes::ARCHIVE,
            created_utc: None,
            modified_utc: Some(10),
            accessed_utc: None,
        }];
        let current = vec![
            FileRecord {
                id: FileId(1),
                parent_directory_id: DirectoryId(1),
                name: String::from("old.txt"),
                full_path: String::from("C:\\root\\old.txt"),
                size_bytes: 12,
                allocation_bytes: 16,
                attributes: FileAttributes::ARCHIVE,
                created_utc: None,
                modified_utc: Some(12),
                accessed_utc: None,
            },
            FileRecord {
                id: FileId(2),
                parent_directory_id: DirectoryId(1),
                name: String::from("new.txt"),
                full_path: String::from("C:\\root\\new.txt"),
                size_bytes: 1,
                allocation_bytes: 1,
                attributes: FileAttributes::ARCHIVE,
                created_utc: None,
                modified_utc: None,
                accessed_utc: None,
            },
        ];

        let diff = diff_file_records(&previous, &current);
        assert_eq!(diff.changes.len(), 2);
        assert!(diff
            .changes
            .iter()
            .any(|change| change.kind == FileChangeKind::Modified));
        assert!(diff
            .changes
            .iter()
            .any(|change| change.kind == FileChangeKind::Added));
    }
}
