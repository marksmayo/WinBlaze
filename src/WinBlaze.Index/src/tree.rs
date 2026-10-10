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

use winblaze_core::IdHashMap;

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

/// Upper bound on the memoized largest-file order (see
/// [`TreeIndex::for_each_largest_file`]): the cleanup UI never asks for more,
/// and capping keeps the first query's partial sort cheap.
const LARGEST_FILE_CACHE_CAP: usize = 10_000;

/// Optional export metadata, kept out of the hot-path file record.
#[derive(Clone, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct FileExportMetadata {
    pub owner: Option<String>,
    pub volume_identity: Option<String>,
    pub file_identity: Option<String>,
    pub link_count: Option<u32>,
}
enum DirectoryLookup {
    Dense { first: u64, len: usize },
    Sparse(IdHashMap<u64, u32>),
}

impl DirectoryLookup {
    fn new(directories: &[DirectoryRecord]) -> Self {
        let first = directories.first().map_or(0, |directory| directory.id.0);
        if directories.len() <= u32::MAX as usize
            && directories
                .iter()
                .enumerate()
                .all(|(index, directory)| first.checked_add(index as u64) == Some(directory.id.0))
        {
            Self::Dense {
                first,
                len: directories.len(),
            }
        } else {
            Self::Sparse(
                directories
                    .iter()
                    .enumerate()
                    .map(|(index, directory)| (directory.id.0, index as u32))
                    .collect(),
            )
        }
    }

    fn get(&self, id: &u64) -> Option<u32> {
        match self {
            Self::Dense { first, len } => {
                let index = id.checked_sub(*first)?;
                (index < *len as u64).then_some(index as u32)
            }
            Self::Sparse(indices) => indices.get(id).copied(),
        }
    }

    fn contains_key(&self, id: &u64) -> bool {
        self.get(id).is_some()
    }
}

pub struct TreeIndex {
    export_metadata: winblaze_core::IdHashMap<u64, FileExportMetadata>,
    volumes: Vec<VolumeRecord>,
    directories: Vec<DirectoryRecord>,
    files: Vec<FileRecord>,
    nodes: Vec<DirNode>,
    dir_children: std::sync::OnceLock<Vec<u32>>,
    pending_dir_children: std::sync::Mutex<Option<Vec<u32>>>,
    file_children: std::sync::OnceLock<Vec<u32>>,
    dir_index_by_id: DirectoryLookup,
    root: Option<u32>,
    /// File indices ordered by allocation size descending, capped at
    /// [`LARGEST_FILE_CACHE_CAP`]. Built lazily on the first largest-files
    /// query so repeated opens of the cleanup view don't re-scan every file.
    largest_files: std::sync::OnceLock<Vec<u32>>,
}

impl TreeIndex {
    pub fn file_export_metadata(&self, id: u64) -> Option<&FileExportMetadata> {
        self.export_metadata.get(&id)
    }
    pub fn set_file_export_metadata(&mut self, id: u64, metadata: FileExportMetadata) {
        self.export_metadata.insert(id, metadata);
    }

    pub fn build(transaction: BufferedIndexTransaction) -> Self {
        let (volumes, directories, files) = transaction.into_record_vecs();
        // Sort an index array by id and gather once, rather than
        // `sort_unstable_by_key` on the records themselves: the records are
        // ~100-byte structs and pdqsort swaps them O(n log n) times, whereas
        // applying a sorted index permutation moves the records O(n) times
        // without duplicating their array. ~2.3M files on a full drive.
        let directories = sort_records_by_id(directories, |directory| directory.id.0);
        let files = sort_records_by_id(files, |file| file.id.0);

        let dir_index_by_id = DirectoryLookup::new(&directories);

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
                    .and_then(|parent| dir_index_by_id.get(&parent.0))
                    .filter(|parent| *parent != index);
                // Orphans and secondary roots hang off the chosen root.
                structural.or(root)
            })
            .collect();

        // Resolve file parents in each pass instead of retaining a second
        // file-sized array. Walk-scan directory IDs use an arithmetic lookup;
        // sparse IDs retain the same map and orphan-to-root semantics.
        let file_parent = |file: &FileRecord| {
            dir_index_by_id
                .get(&file.parent_directory_id.0)
                .or(root)
                .unwrap_or(u32::MAX)
        };

        // Group children per parent with a counting pass + prefix sums.
        let dir_count = directories.len();
        let mut dir_child_counts = vec![0u32; dir_count];
        for parent in dir_parent.iter().flatten() {
            dir_child_counts[*parent as usize] += 1;
        }
        let mut file_child_counts = vec![0u32; dir_count];
        for parent in files.iter().map(&file_parent) {
            if parent != u32::MAX {
                file_child_counts[parent as usize] += 1;
            }
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
        let mut dir_fill = dir_child_counts;
        for (cursor, node) in dir_fill.iter_mut().zip(&nodes) {
            *cursor = node.dir_children.0;
        }
        for (child, parent) in dir_parent.iter().enumerate() {
            if let Some(parent) = parent {
                let slot = &mut dir_fill[*parent as usize];
                dir_children[*slot as usize] = child as u32;
                *slot += 1;
            }
        }

        // Ingest scratch is not needed by rollups or display ordering.
        drop(dir_parent);
        drop(dir_fill);
        drop(file_child_counts);
        let mut tree = Self {
            export_metadata: Default::default(),
            volumes,
            directories,
            files,
            nodes,
            dir_children: std::sync::OnceLock::new(),
            pending_dir_children: std::sync::Mutex::new(None),
            file_children: std::sync::OnceLock::new(),
            dir_index_by_id,
            root,
            largest_files: std::sync::OnceLock::new(),
        };
        tree.compute_rollups(&dir_children);
        *tree.pending_dir_children.get_mut().expect("new tree mutex") = Some(dir_children);
        tree
    }

    /// Bottom-up rollups without recursion: BFS order from the root, then a
    /// reverse sweep accumulates each directory into its parent. Directories
    /// unreachable from the root (only possible with cyclic parent links in
    /// corrupt data) keep zero rollups.
    fn compute_rollups(&mut self, dir_children: &[u32]) {
        // Direct file contributions first.
        for file in &self.files {
            if let Some(index) = self
                .dir_index_by_id
                .get(&file.parent_directory_id.0)
                .or(self.root)
            {
                let rollup = &mut self.nodes[index as usize].rollup;
                rollup.logical_bytes = rollup.logical_bytes.saturating_add(file.size_bytes);
                rollup.physical_bytes = rollup.physical_bytes.saturating_add(file.allocation_bytes);
                rollup.file_count += 1;
                rollup.item_count += 1;
                rollup.modified_utc_max = max_option(rollup.modified_utc_max, file.modified_utc());
            }
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
                let child = dir_children[slot as usize];
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

    fn ordered_directories(&self) -> &[u32] {
        self.dir_children.get_or_init(|| {
            let mut children = self
                .pending_dir_children
                .lock()
                .expect("child order mutex")
                .take()
                .unwrap_or_default();
            for node in &self.nodes {
                children[node.dir_children.0 as usize..node.dir_children.1 as usize]
                    .sort_unstable_by(|a, b| {
                        self.nodes[*b as usize]
                            .rollup
                            .physical_bytes
                            .cmp(&self.nodes[*a as usize].rollup.physical_bytes)
                    });
            }
            children
        })
    }

    fn ordered_files(&self) -> &[u32] {
        self.file_children.get_or_init(|| {
            let count = self.nodes.last().map_or(0, |node| node.file_children.1) as usize;
            let mut children = vec![0; count];
            let mut cursors: Vec<u32> =
                self.nodes.iter().map(|node| node.file_children.0).collect();
            for (index, file) in self.files.iter().enumerate() {
                if let Some(parent) = self
                    .dir_index_by_id
                    .get(&file.parent_directory_id.0)
                    .or(self.root)
                {
                    let cursor = &mut cursors[parent as usize];
                    children[*cursor as usize] = index as u32;
                    *cursor += 1;
                }
            }
            for node in &self.nodes {
                children[node.file_children.0 as usize..node.file_children.1 as usize]
                    .sort_unstable_by(|a, b| {
                        self.files[*b as usize]
                            .allocation_bytes
                            .cmp(&self.files[*a as usize].allocation_bytes)
                    });
            }
            children
        })
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
        let index = self.dir_index_by_id.get(&directory_id)? as usize;
        Some(&self.nodes[index].rollup)
    }

    /// Visits the `limit` largest files on the volume by allocation size,
    /// descending. Powers "large files" cleanup views.
    pub fn for_each_largest_file(&self, limit: usize, mut visit: impl FnMut(&FileRecord)) {
        let take = limit.min(self.files.len());
        if take == 0 {
            return;
        }

        let compare = |a: &u32, b: &u32| {
            self.files[*b as usize]
                .allocation_bytes
                .cmp(&self.files[*a as usize].allocation_bytes)
                .then_with(|| {
                    self.files[*a as usize]
                        .id
                        .0
                        .cmp(&self.files[*b as usize].id.0)
                })
        };

        // Requests within the cache cap are served from a memoized order
        // built once; larger requests fall back to a one-off partial sort.
        if take <= LARGEST_FILE_CACHE_CAP {
            let cached = self.largest_files.get_or_init(|| {
                let cap = LARGEST_FILE_CACHE_CAP.min(self.files.len());
                let mut order: Vec<u32> = (0..self.files.len() as u32).collect();
                if cap < order.len() {
                    order.select_nth_unstable_by(cap - 1, compare);
                    order.truncate(cap);
                }
                order.sort_unstable_by(compare);
                order.shrink_to_fit();
                order
            });
            for &index in cached.iter().take(take) {
                visit(&self.files[index as usize]);
            }
            return;
        }

        let mut order: Vec<u32> = (0..self.files.len() as u32).collect();
        if take < order.len() {
            order.select_nth_unstable_by(take - 1, compare);
            order.truncate(take);
        }
        order.sort_unstable_by(compare);
        for index in order {
            visit(&self.files[index as usize]);
        }
    }

    /// Visits a stable page of largest files without limiting the total result set.
    pub fn for_each_largest_file_page(
        &self,
        offset: usize,
        limit: usize,
        mut visit: impl FnMut(&FileRecord),
    ) {
        if offset >= self.files.len() || limit == 0 {
            return;
        }
        let mut position = 0;
        self.for_each_largest_file(offset.saturating_add(limit), |file| {
            if position >= offset {
                visit(file);
            }
            position += 1;
        });
    }

    pub fn directory_full_path(&self, directory_id: u64) -> Option<&str> {
        let index = self.dir_index_by_id.get(&directory_id)? as usize;
        Some(self.directories[index].full_path.as_str())
    }

    /// Display path for a file record: the stored path when present (records
    /// from older snapshots carry one), otherwise parent path + name.
    pub fn file_display_path(&self, file: &FileRecord) -> String {
        if !file.full_path.is_empty() {
            return file.full_path.to_string();
        }
        match self.directory_full_path(file.parent_directory_id.0) {
            Some(parent) => winblaze_core::join_path(parent, &file.name),
            None => file.name.to_string(),
        }
    }

    /// Matches catalog display paths with the export source-protection semantics.
    /// ASCII paths are compared in fragments without materializing each path.
    pub fn contains_file_display_path(&self, path: &str) -> bool {
        let target = path.trim_start_matches(r"\\?\").to_lowercase();
        self.files.iter().any(|file| {
            if !file.full_path.is_empty() {
                let source = file.full_path.trim_start_matches(r"\\?\");
                if source.is_ascii() && target.is_ascii() {
                    return source.eq_ignore_ascii_case(&target);
                }
            } else {
                let raw_parent = self
                    .directory_full_path(file.parent_directory_id.0)
                    .unwrap_or("");
                let parent = raw_parent.trim_start_matches(r"\\?\");
                if parent.is_ascii()
                    && file.name.is_ascii()
                    && target.is_ascii()
                    && !file.name.contains('\\')
                {
                    let separator = !raw_parent.is_empty() && !raw_parent.ends_with('\\');
                    let length = parent.len() + usize::from(separator) + file.name.len();
                    return target.len() == length
                        && target[..parent.len()].eq_ignore_ascii_case(parent)
                        && (!separator || target.as_bytes()[parent.len()] == b'\\')
                        && target[parent.len() + usize::from(separator)..]
                            .eq_ignore_ascii_case(&file.name);
                }
            }
            self.file_display_path(file)
                .trim_start_matches(r"\\?\")
                .to_lowercase()
                == target
        })
    }

    /// Total direct children (directories + files) of `directory_id`.
    pub fn child_count(&self, directory_id: u64) -> Option<u64> {
        let index = self.dir_index_by_id.get(&directory_id)? as usize;
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
        let index = self.dir_index_by_id.get(&directory_id)? as usize;
        let node = &self.nodes[index];

        // Both lists are pre-sorted physical-desc; merge them.
        if limit == 0 {
            return Some(0);
        }
        let dir_children = if node.dir_children.0 == node.dir_children.1 {
            &[][..]
        } else {
            self.ordered_directories()
        };
        let file_children = if node.file_children.0 == node.file_children.1 {
            &[][..]
        } else {
            self.ordered_files()
        };
        let mut dir_cursor = node.dir_children.0 as usize;
        let dir_end = node.dir_children.1 as usize;
        let mut file_cursor = node.file_children.0 as usize;
        let file_end = node.file_children.1 as usize;
        let mut skipped = 0u64;
        let mut emitted = 0u64;

        while emitted < limit as u64 && (dir_cursor < dir_end || file_cursor < file_end) {
            let next_dir_physical = (dir_cursor < dir_end).then(|| {
                let child = dir_children[dir_cursor] as usize;
                self.nodes[child].rollup.physical_bytes
            });
            let next_file_physical = (file_cursor < file_end)
                .then(|| self.files[file_children[file_cursor] as usize].allocation_bytes);

            let take_dir = match (next_dir_physical, next_file_physical) {
                (Some(dir_bytes), Some(file_bytes)) => dir_bytes >= file_bytes,
                (Some(_), None) => true,
                (None, _) => false,
            };

            if take_dir {
                let child = dir_children[dir_cursor] as usize;
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
                let child = file_children[file_cursor] as usize;
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

/// Sorts compact keys, then applies their permutation in O(n) record swaps.
/// Keeping the original record allocation avoids a volume-sized temporary copy.
fn sort_records_by_id<T>(mut records: Vec<T>, key: impl Fn(&T) -> u64) -> Vec<T> {
    // Reopened snapshots and ordered scanner output already have the desired
    // order. Check it without allocating the volume-sized permutation array.
    if records.iter().is_sorted_by_key(&key) {
        return records;
    }
    // Directory-walk IDs are a dense range, but worker completion can shuffle
    // their arrival order. Place each record directly at its ID offset, without
    // allocating sort keys. Sparse MFT IDs retain the general path below.
    let (minimum, maximum) = records
        .iter()
        .fold((u64::MAX, 0), |(minimum, maximum), record| {
            let id = key(record);
            (minimum.min(id), maximum.max(id))
        });
    if maximum
        .checked_sub(minimum)
        .and_then(|range| range.checked_add(1))
        == u64::try_from(records.len()).ok()
    {
        let mut unique = true;
        'positions: for position in 0..records.len() {
            loop {
                let id = key(&records[position]);
                let destination = (id - minimum) as usize;
                if destination == position {
                    break;
                }
                // A duplicate must not produce an endless permutation cycle.
                // Swaps preserve all records, so the general sort remains safe.
                if key(&records[destination]) == id {
                    unique = false;
                    break 'positions;
                }
                records.swap(position, destination);
            }
        }
        if unique {
            return records;
        }
    }
    // Decorate each index with its id key so the sort compares a compact
    // (u64, u32) array rather than chasing pointers into the ~100-byte records
    // (which would miss cache on every comparison). Permutation cycles then
    // swap large records O(n) times instead of O(n log n).
    let mut keyed: Vec<(u64, u32)> = records
        .iter()
        .enumerate()
        .map(|(index, record)| (key(record), index as u32))
        .collect();
    keyed.sort_unstable_by_key(|&(id, _)| id);
    // Reuse the key slots as original-position -> sorted-position mapping.
    // The original indices remain intact while this inverse is constructed.
    // Apply its cycles in place: each swap settles at least one record. This
    // avoids a second ~140-byte-per-file array on multi-million-entry scans.
    for destination in 0..keyed.len() {
        let original = keyed[destination].1 as usize;
        keyed[original].0 = destination as u64;
    }
    for position in 0..keyed.len() {
        while keyed[position].0 as usize != position {
            let destination = keyed[position].0 as usize;
            records.swap(position, destination);
            keyed.swap(position, destination);
        }
    }
    records
}

/// Picks the root directory: a record whose parent is missing or itself.
/// Prefers id 5 (the NTFS root record number, which both scanner backends
/// use for the scan root), then the largest candidate by subtree membership
/// proxy (smallest id) for stability.
fn choose_root(directories: &[DirectoryRecord], dir_index_by_id: &DirectoryLookup) -> Option<u32> {
    let is_root_like = |directory: &DirectoryRecord| match directory.parent_directory_id {
        None => true,
        Some(parent) => parent.0 == directory.id.0 || !dir_index_by_id.contains_key(&parent.0),
    };

    // The scan root is record 5 on both backends; check it directly instead
    // of scanning every directory to find it.
    if let Some(index) = dir_index_by_id.get(&5) {
        if is_root_like(&directories[index as usize]) {
            return Some(index);
        }
    }

    directories
        .iter()
        .position(is_root_like)
        .map(|index| index as u32)
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
            name: name.to_string().into(),
            full_path: format!(r"C:\{name}").into(),
            size_bytes: logical,
            allocation_bytes: physical,
            attributes: FileAttributes::ARCHIVE,
            timestamps: winblaze_core::FileTimestamps::new(None, Some(id as i64 * 100), None),
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
            vec![
                dir(5, None, ""),
                dir(10, Some(5), "big"),
                dir(11, Some(5), "small"),
            ],
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
                    TreeEntry::File(file) => (file.name.to_string(), file.allocation_bytes),
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
