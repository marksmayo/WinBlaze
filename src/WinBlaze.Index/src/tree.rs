//! Display-oriented tree over an index snapshot.
//!
//! Builds a directory hierarchy with per-directory rollups (logical and
//! physical bytes, file/item counts, newest child timestamp) from the raw
//! records in a [`BufferedIndexTransaction`]. Scanners emit directory records
//! with zeroed totals, so this module is the source of truth for display
//! sizes; `winblaze_core::aggregate_directory_records` remains in place for
//! the search path, which depends on its historical semantics.
//!
//! The structure is arena-style: nodes reference records by index and
//! children are ranges into shared index vectors, so building the tree for a
//! multi-million-record volume adds bookkeeping proportional to the record
//! count without cloning any names or paths.

use std::collections::HashMap;

use winblaze_core::{DirectoryRecord, FileRecord, VolumeRecord};

use crate::store::BufferedIndexTransaction;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DirRollup {
    pub logical_bytes: u64,
    pub physical_bytes: u64,
    pub file_count: u64,
    /// Files plus directories anywhere beneath this directory.
    pub item_count: u64,
    pub modified_utc_max: Option<i64>,
}

#[derive(Clone, Copy, Debug)]
struct DirNode {
    /// Range into `TreeIndex::dir_children`.
    dir_children: (u32, u32),
    /// Range into `TreeIndex::file_children`.
    file_children: (u32, u32),
    rollup: DirRollup,
}

/// A borrowed view of one child entry, ordered largest-physical-first.
#[derive(Clone, Copy, Debug)]
pub enum TreeEntry<'a> {
    Directory {
        record: &'a DirectoryRecord,
        rollup: &'a DirRollup,
    },
    File(&'a FileRecord),
}

impl TreeEntry<'_> {
    pub fn physical_bytes(&self) -> u64 {
        match self {
            TreeEntry::Directory { rollup, .. } => rollup.physical_bytes,
            TreeEntry::File(file) => file.allocation_bytes,
        }
    }
}

pub struct TreeIndex {
    volumes: Vec<VolumeRecord>,
    directories: Vec<DirectoryRecord>,
    files: Vec<FileRecord>,
    nodes: Vec<DirNode>,
    dir_children: Vec<u32>,
    file_children: Vec<u32>,
    dir_index_by_id: HashMap<u64, u32>,
    root: Option<u32>,
}

impl TreeIndex {
    pub fn build(transaction: BufferedIndexTransaction) -> Self {
        let (volumes, mut directories, mut files) = transaction.into_record_vecs();
        directories.sort_unstable_by_key(|directory| directory.id.0);
        files.sort_unstable_by_key(|file| file.id.0);

        let dir_index_by_id: HashMap<u64, u32> = directories
            .iter()
            .enumerate()
            .map(|(index, directory)| (directory.id.0, index as u32))
            .collect();

        // Resolve each directory's structural parent. Self-parents and
        // missing parents make a directory a root candidate; among the
        // candidates one becomes the tree root and the rest are attached to
        // it so orphaned subtrees still contribute to the totals.
        let root = choose_root(&directories, &dir_index_by_id);
        let dir_parent: Vec<Option<u32>> = directories
            .iter()
            .enumerate()
            .map(|(index, directory)| {
                let index = index as u32;
                if Some(index) == root {
                    return None;
                }
                let structural = directory
                    .parent_directory_id
                    .filter(|parent| parent.0 != directory.id.0)
                    .and_then(|parent| dir_index_by_id.get(&parent.0).copied())
                    .filter(|parent| *parent != index);
                // Orphans and secondary roots hang off the chosen root.
                structural.or(root)
            })
            .collect();

        let file_parent: Vec<Option<u32>> = files
            .iter()
            .map(|file| {
                dir_index_by_id
                    .get(&file.parent_directory_id.0)
                    .copied()
                    .or(root)
            })
            .collect();

        // Group children per parent with a counting pass + prefix sums.
        let dir_count = directories.len();
        let mut dir_child_counts = vec![0u32; dir_count];
        for parent in dir_parent.iter().flatten() {
            dir_child_counts[*parent as usize] += 1;
        }
        let mut file_child_counts = vec![0u32; dir_count];
        for parent in file_parent.iter().flatten() {
            file_child_counts[*parent as usize] += 1;
        }

        let mut nodes = Vec::with_capacity(dir_count);
        let mut dir_offset = 0u32;
        let mut file_offset = 0u32;
        for index in 0..dir_count {
            let dir_end = dir_offset + dir_child_counts[index];
            let file_end = file_offset + file_child_counts[index];
            nodes.push(DirNode {
                dir_children: (dir_offset, dir_end),
                file_children: (file_offset, file_end),
                rollup: DirRollup::default(),
            });
            dir_offset = dir_end;
            file_offset = file_end;
        }

        let mut dir_children = vec![0u32; dir_offset as usize];
        let mut dir_fill: Vec<u32> = nodes.iter().map(|node| node.dir_children.0).collect();
        for (child, parent) in dir_parent.iter().enumerate() {
            if let Some(parent) = parent {
                let slot = &mut dir_fill[*parent as usize];
                dir_children[*slot as usize] = child as u32;
                *slot += 1;
            }
        }

        let mut file_children = vec![0u32; file_offset as usize];
        let mut file_fill: Vec<u32> = nodes.iter().map(|node| node.file_children.0).collect();
        for (child, parent) in file_parent.iter().enumerate() {
            if let Some(parent) = parent {
                let slot = &mut file_fill[*parent as usize];
                file_children[*slot as usize] = child as u32;
                *slot += 1;
            }
        }

        let mut tree = Self {
            volumes,
            directories,
            files,
            nodes,
            dir_children,
            file_children,
            dir_index_by_id,
            root,
        };
        tree.compute_rollups();
        tree.sort_children();
        tree
    }

    /// Bottom-up rollups without recursion: BFS order from the root, then a
    /// reverse sweep accumulates each directory into its parent. Directories
    /// unreachable from the root (only possible with cyclic parent links in
    /// corrupt data) keep zero rollups.
    fn compute_rollups(&mut self) {
        // Direct file contributions first.
        for index in 0..self.nodes.len() {
            let (start, end) = self.nodes[index].file_children;
            let mut rollup = DirRollup::default();
            for slot in start..end {
                let file = &self.files[self.file_child_at(slot)];
                rollup.logical_bytes = rollup.logical_bytes.saturating_add(file.size_bytes);
                rollup.physical_bytes = rollup.physical_bytes.saturating_add(file.allocation_bytes);
                rollup.file_count += 1;
                rollup.item_count += 1;
                rollup.modified_utc_max = max_option(rollup.modified_utc_max, file.modified_utc);
            }
            self.nodes[index].rollup = rollup;
        }

        let Some(root) = self.root else {
            return;
        };

        // BFS order (parents before children).
        let mut order = Vec::with_capacity(self.nodes.len());
        let mut parent_of = vec![u32::MAX; self.nodes.len()];
        order.push(root);
        let mut cursor = 0;
        while cursor < order.len() {
            let current = order[cursor];
            cursor += 1;
            let (start, end) = self.nodes[current as usize].dir_children;
            for slot in start..end {
                let child = self.dir_children[slot as usize];
                parent_of[child as usize] = current;
                order.push(child);
            }
        }

        // Reverse sweep: children accumulate into parents.
        for &index in order.iter().rev() {
            let parent = parent_of[index as usize];
            if parent == u32::MAX {
                continue;
            }
            let child_rollup = self.nodes[index as usize].rollup;
            let parent_rollup = &mut self.nodes[parent as usize].rollup;
            parent_rollup.logical_bytes = parent_rollup
                .logical_bytes
                .saturating_add(child_rollup.logical_bytes);
            parent_rollup.physical_bytes = parent_rollup
                .physical_bytes
                .saturating_add(child_rollup.physical_bytes);
            parent_rollup.file_count = parent_rollup
                .file_count
                .saturating_add(child_rollup.file_count);
            parent_rollup.item_count = parent_rollup
                .item_count
                .saturating_add(child_rollup.item_count.saturating_add(1));
            parent_rollup.modified_utc_max = max_option(
                parent_rollup.modified_utc_max,
                child_rollup.modified_utc_max,
            );
        }
    }

    fn file_child_at(&self, slot: u32) -> usize {
        self.file_children[slot as usize] as usize
    }

    /// Sorts every directory's child lists largest-physical-first so callers
    /// can stream children in display order.
    fn sort_children(&mut self) {
        for index in 0..self.nodes.len() {
            let (dir_start, dir_end) = self.nodes[index].dir_children;
            let nodes = &self.nodes;
            self.dir_children[dir_start as usize..dir_end as usize].sort_unstable_by(|a, b| {
                nodes[*b as usize]
                    .rollup
                    .physical_bytes
                    .cmp(&nodes[*a as usize].rollup.physical_bytes)
            });

            let (file_start, file_end) = self.nodes[index].file_children;
            let files = &self.files;
            self.file_children[file_start as usize..file_end as usize].sort_unstable_by(|a, b| {
                files[*b as usize]
                    .allocation_bytes
                    .cmp(&files[*a as usize].allocation_bytes)
            });
        }
    }

    pub fn volumes(&self) -> &[VolumeRecord] {
        &self.volumes
    }

    pub fn directories(&self) -> &[DirectoryRecord] {
        &self.directories
    }

    pub fn files(&self) -> &[FileRecord] {
        &self.files
    }

    pub fn root(&self) -> Option<(&DirectoryRecord, &DirRollup)> {
        let root = self.root? as usize;
        Some((&self.directories[root], &self.nodes[root].rollup))
    }

    pub fn rollup_for(&self, directory_id: u64) -> Option<&DirRollup> {
        let index = *self.dir_index_by_id.get(&directory_id)? as usize;
        Some(&self.nodes[index].rollup)
    }

    /// Visits the `limit` largest files on the volume by allocation size,
    /// descending. Powers "large files" cleanup views.
    pub fn for_each_largest_file(&self, limit: usize, mut visit: impl FnMut(&FileRecord)) {
        let take = limit.min(self.files.len());
        if take == 0 {
            return;
        }
        let mut order: Vec<u32> = (0..self.files.len() as u32).collect();
        let compare = |a: &u32, b: &u32| {
            self.files[*b as usize]
                .allocation_bytes
                .cmp(&self.files[*a as usize].allocation_bytes)
        };
        if take < order.len() {
            order.select_nth_unstable_by(take - 1, compare);
            order.truncate(take);
        }
        order.sort_unstable_by(compare);
        for index in order {
            visit(&self.files[index as usize]);
        }
    }

    pub fn directory_full_path(&self, directory_id: u64) -> Option<&str> {
        let index = *self.dir_index_by_id.get(&directory_id)? as usize;
        Some(self.directories[index].full_path.as_str())
    }

    /// Display path for a file record: the stored path when present (records
    /// from older snapshots carry one), otherwise parent path + name.
    pub fn file_display_path(&self, file: &FileRecord) -> String {
        if !file.full_path.is_empty() {
            return file.full_path.clone();
        }
        match self.directory_full_path(file.parent_directory_id.0) {
            Some(parent) => winblaze_core::join_path(parent, &file.name),
            None => file.name.clone(),
        }
    }

    /// Total direct children (directories + files) of `directory_id`.
    pub fn child_count(&self, directory_id: u64) -> Option<u64> {
        let index = *self.dir_index_by_id.get(&directory_id)? as usize;
        let node = &self.nodes[index];
        Some(
            u64::from(node.dir_children.1 - node.dir_children.0)
                + u64::from(node.file_children.1 - node.file_children.0),
        )
    }

    /// Streams the direct children of `directory_id` in descending physical
    /// size, skipping the first `offset` entries and stopping after `limit`
    /// emissions. Returns the number emitted, or None if the directory is
    /// unknown. The offset lets callers page through directories larger
    /// than their per-call cap.
    pub fn for_each_child(
        &self,
        directory_id: u64,
        offset: usize,
        limit: usize,
        mut visit: impl FnMut(TreeEntry<'_>),
    ) -> Option<u64> {
        let index = *self.dir_index_by_id.get(&directory_id)? as usize;
        let node = &self.nodes[index];

        // Both lists are pre-sorted physical-desc; merge them.
        let mut dir_cursor = node.dir_children.0 as usize;
        let dir_end = node.dir_children.1 as usize;
        let mut file_cursor = node.file_children.0 as usize;
        let file_end = node.file_children.1 as usize;
        let mut skipped = 0u64;
        let mut emitted = 0u64;

        while emitted < limit as u64 && (dir_cursor < dir_end || file_cursor < file_end) {
            let next_dir_physical = (dir_cursor < dir_end).then(|| {
                let child = self.dir_children[dir_cursor] as usize;
                self.nodes[child].rollup.physical_bytes
            });
            let next_file_physical = (file_cursor < file_end)
                .then(|| self.files[self.file_children[file_cursor] as usize].allocation_bytes);

            let take_dir = match (next_dir_physical, next_file_physical) {
                (Some(dir_bytes), Some(file_bytes)) => dir_bytes >= file_bytes,
                (Some(_), None) => true,
                (None, _) => false,
            };

            if take_dir {
                let child = self.dir_children[dir_cursor] as usize;
                dir_cursor += 1;
                if skipped < offset as u64 {
                    skipped += 1;
                    continue;
                }
                visit(TreeEntry::Directory {
                    record: &self.directories[child],
                    rollup: &self.nodes[child].rollup,
                });
            } else {
                let child = self.file_children[file_cursor] as usize;
                file_cursor += 1;
                if skipped < offset as u64 {
                    skipped += 1;
                    continue;
                }
                visit(TreeEntry::File(&self.files[child]));
            }
            emitted += 1;
        }

        Some(emitted)
    }
}

/// Picks the root directory: a record whose parent is missing or itself.
/// Prefers id 5 (the NTFS root record number, which both scanner backends
/// use for the scan root), then the largest candidate by subtree membership
/// proxy (smallest id) for stability.
fn choose_root(
    directories: &[DirectoryRecord],
    dir_index_by_id: &HashMap<u64, u32>,
) -> Option<u32> {
    let mut fallback: Option<u32> = None;
    for (index, directory) in directories.iter().enumerate() {
        let is_root_like = match directory.parent_directory_id {
            None => true,
            Some(parent) => parent.0 == directory.id.0 || !dir_index_by_id.contains_key(&parent.0),
        };
        if !is_root_like {
            continue;
        }
        if directory.id.0 == 5 {
            return Some(index as u32);
        }
        if fallback.is_none() {
            fallback = Some(index as u32);
        }
    }
    fallback
}

fn max_option(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.max(b)),
        (Some(a), None) => Some(a),
        (None, b) => b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::IndexTransaction;
    use winblaze_core::{DirectoryId, FileAttributes, FileId};

    fn dir(id: u64, parent: Option<u64>, name: &str) -> DirectoryRecord {
        DirectoryRecord {
            id: DirectoryId(id),
            parent_directory_id: parent.map(DirectoryId),
            name: name.to_string(),
            full_path: format!(r"C:\{name}"),
            direct_bytes: 0,
            total_bytes: 0,
            direct_entries: 0,
            total_entries: 0,
        }
    }

    fn file(id: u64, parent: u64, name: &str, logical: u64, physical: u64) -> FileRecord {
        FileRecord {
            id: FileId(id),
            parent_directory_id: DirectoryId(parent),
            name: name.to_string(),
            full_path: format!(r"C:\{name}"),
            size_bytes: logical,
            allocation_bytes: physical,
            attributes: FileAttributes::ARCHIVE,
            created_utc: None,
            modified_utc: Some(id as i64 * 100),
            accessed_utc: None,
        }
    }

    fn build(dirs: Vec<DirectoryRecord>, files: Vec<FileRecord>) -> TreeIndex {
        let mut transaction = BufferedIndexTransaction::default();
        for directory in &dirs {
            transaction.upsert_directory(directory);
        }
        for file in &files {
            transaction.upsert_file(file);
        }
        TreeIndex::build(transaction)
    }

    #[test]
    fn rollups_track_logical_and_physical_separately() {
        let tree = build(
            vec![dir(5, None, ""), dir(10, Some(5), "sub")],
            vec![
                file(1, 10, "sparse.bin", 4096, 1024),
                file(2, 5, "plain.txt", 100, 100),
            ],
        );

        let (_, root_rollup) = tree.root().expect("root");
        assert_eq!(root_rollup.logical_bytes, 4196);
        assert_eq!(root_rollup.physical_bytes, 1124);
        assert_eq!(root_rollup.file_count, 2);
        // sub dir + 2 files
        assert_eq!(root_rollup.item_count, 3);

        let sub = tree.rollup_for(10).expect("sub rollup");
        assert_eq!(sub.logical_bytes, 4096);
        assert_eq!(sub.physical_bytes, 1024);
        assert_eq!(sub.file_count, 1);
    }

    #[test]
    fn root_found_by_missing_parent_and_orphans_attach_to_it() {
        let tree = build(
            vec![
                dir(5, Some(5), ""),
                dir(10, Some(5), "a"),
                // Orphan: parent record 99 never appears.
                dir(11, Some(99), "orphan"),
            ],
            vec![file(1, 11, "lost.bin", 10, 10)],
        );

        let (record, rollup) = tree.root().expect("root");
        assert_eq!(record.id.0, 5);
        // Orphan subtree contributes to root totals.
        assert_eq!(rollup.physical_bytes, 10);
        assert_eq!(rollup.item_count, 3);
    }

    #[test]
    fn children_stream_merged_physical_desc_with_limit() {
        let tree = build(
            vec![dir(5, None, ""), dir(10, Some(5), "big"), dir(11, Some(5), "small")],
            vec![
                file(1, 10, "inner.bin", 500, 500),
                file(2, 5, "mid.bin", 300, 300),
                file(3, 5, "tiny.bin", 1, 1),
            ],
        );

        let mut seen = Vec::new();
        let emitted = tree
            .for_each_child(5, 0, 3, |entry| {
                seen.push(match entry {
                    TreeEntry::Directory { record, rollup } => {
                        (record.name.clone(), rollup.physical_bytes)
                    }
                    TreeEntry::File(file) => (file.name.clone(), file.allocation_bytes),
                });
            })
            .expect("children");

        assert_eq!(emitted, 3);
        assert_eq!(
            seen,
            vec![
                ("big".to_string(), 500),
                ("mid.bin".to_string(), 300),
                ("tiny.bin".to_string(), 1),
            ]
        );
        // The 0-byte "small" directory ranks fourth, past the limit.
        assert_eq!(tree.child_count(5), Some(4));

        // Paging: offset 3 returns exactly the remaining entry.
        let mut paged = Vec::new();
        let emitted = tree
            .for_each_child(5, 3, 3, |entry| {
                if let TreeEntry::Directory { record, .. } = entry {
                    paged.push(record.name.clone());
                }
            })
            .expect("paged children");
        assert_eq!(emitted, 1);
        assert_eq!(paged, vec!["small".to_string()]);
    }

    #[test]
    fn deep_chain_does_not_overflow_stack() {
        let mut dirs = vec![dir(5, None, "")];
        for id in 6..20_006u64 {
            dirs.push(dir(id, Some(id - 1), "level"));
        }
        let files = vec![file(1, 20_005, "leaf.bin", 7, 7)];

        let tree = build(dirs, files);
        let (_, rollup) = tree.root().expect("root");
        assert_eq!(rollup.physical_bytes, 7);
        assert_eq!(rollup.file_count, 1);
        // 20k directories beneath the root plus the leaf file.
        assert_eq!(rollup.item_count, 20_001);
    }

    #[test]
    fn empty_index_has_no_root() {
        let tree = build(Vec::new(), Vec::new());
        assert!(tree.root().is_none());
        assert!(tree.rollup_for(5).is_none());
        assert!(tree.for_each_child(5, 0, 10, |_| {}).is_none());
    }

    #[test]
    fn modified_max_bubbles_to_root() {
        let tree = build(
            vec![dir(5, None, ""), dir(10, Some(5), "sub")],
            vec![file(1, 5, "old.txt", 1, 1), file(9, 10, "new.txt", 1, 1)],
        );

        let (_, rollup) = tree.root().expect("root");
        assert_eq!(rollup.modified_utc_max, Some(900));
    }
}
