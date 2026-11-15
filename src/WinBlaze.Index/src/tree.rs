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

use std::borrow::Cow;

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

/// `DirRollup` as the tree stores it, 32 bytes instead of 48: counts fit
/// 32 bits (the tree indexes children with u32), and the newest modified
/// time uses `i64::MIN` as "none" (a file record keeps its exact value).
#[derive(Clone, Copy, Debug, Default)]
struct StoredRollup {
    logical: u64,
    physical: u64,
    files: u32,
    items: u32,
    modified: i64,
}

const NO_MODIFIED: i64 = i64::MIN;

impl StoredRollup {
    const fn new() -> Self {
        Self {
            logical: 0,
            physical: 0,
            files: 0,
            items: 0,
            modified: NO_MODIFIED,
        }
    }

    fn to_rollup(self) -> DirRollup {
        DirRollup {
            logical_bytes: self.logical,
            physical_bytes: self.physical,
            file_count: u64::from(self.files),
            item_count: u64::from(self.items),
            modified_utc_max: (self.modified != NO_MODIFIED).then_some(self.modified),
        }
    }

    fn observe_modified(&mut self, modified: Option<i64>) {
        if let Some(modified) = modified {
            if modified > self.modified {
                self.modified = modified;
            }
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct DirNode {
    /// Range into `TreeIndex::dir_children`.
    dir_children: (u32, u32),
    /// Range into `TreeIndex::file_children`.
    file_children: (u32, u32),
    rollup: StoredRollup,
}

/// A borrowed view of one child entry, ordered largest-physical-first.
#[derive(Clone, Copy, Debug)]
pub enum TreeEntry<'a> {
    Directory {
        record: &'a DirectoryRecord,
        rollup: DirRollup,
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

/// Why [`TreeIndex::apply_file_changes`] refused a patch (the tree is unchanged).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilePatchError {
    /// An added file's id is not above every existing and earlier added id.
    IdNotIncreasing,
    /// A file names a parent directory the tree does not have.
    UnknownParent,
    /// A modified file's id is not in the tree.
    UnknownFile,
    /// A modified file moved to another directory.
    ParentChanged,
    /// A modified file's modified time went back, which rollup maxima cannot undo.
    ModifiedTimeDecreased,
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

/// The last parent path built by [`TreeIndex::push_directory_path_cached`].
#[derive(Debug, Default)]
pub struct DirectoryPathCache {
    parent: Option<u32>,
    path: String,
}

pub struct TreeIndex {
    export_metadata: winblaze_core::IdHashMap<u64, FileExportMetadata>,
    volumes: Vec<VolumeRecord>,
    /// Sorted by id. A directory whose path is its parent's path plus its
    /// name stores an empty `full_path` and derives it on demand (see
    /// [`TreeIndex::directory_path`]); roots, orphans and imports whose paths
    /// do not follow from the chain keep theirs.
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
        let mut directories = sort_records_by_id(directories, |directory| directory.id.0);
        let files = sort_records_by_id(files, |file| file.id.0);

        let dir_index_by_id = DirectoryLookup::new(&directories);
        release_derivable_paths(&mut directories, &dir_index_by_id);

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
                rollup: StoredRollup::new(),
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
                rollup.logical = rollup.logical.saturating_add(file.size_bytes);
                rollup.physical = rollup.physical.saturating_add(file.allocation_bytes);
                rollup.files = rollup.files.saturating_add(1);
                rollup.items = rollup.items.saturating_add(1);
                rollup.observe_modified(file.modified_utc());
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
            parent_rollup.logical = parent_rollup.logical.saturating_add(child_rollup.logical);
            parent_rollup.physical = parent_rollup.physical.saturating_add(child_rollup.physical);
            parent_rollup.files = parent_rollup.files.saturating_add(child_rollup.files);
            parent_rollup.items = parent_rollup
                .items
                .saturating_add(child_rollup.items.saturating_add(1));
            if child_rollup.modified > parent_rollup.modified {
                parent_rollup.modified = child_rollup.modified;
            }
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
                            .physical
                            .cmp(&self.nodes[*a as usize].rollup.physical)
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

    /// Position of the directory with `id` in [`TreeIndex::directories`].
    pub fn directory_index(&self, id: u64) -> Option<usize> {
        self.dir_index_by_id.get(&id).map(|index| index as usize)
    }

    /// Position of the root in [`TreeIndex::directories`].
    pub fn root_index(&self) -> Option<usize> {
        self.root.map(|index| index as usize)
    }

    /// Largest file id; files are kept sorted by id.
    pub fn max_file_id(&self) -> Option<u64> {
        self.files.last().map(|file| file.id.0)
    }

    /// The file with `id`, found by binary search (files are sorted by id).
    pub fn file_by_id(&self, id: u64) -> Option<&FileRecord> {
        let index = self
            .files
            .binary_search_by_key(&id, |file| file.id.0)
            .ok()?;
        Some(&self.files[index])
    }

    /// The directory with `id`.
    pub fn directory_by_id(&self, id: u64) -> Option<&DirectoryRecord> {
        let index = self.dir_index_by_id.get(&id)? as usize;
        Some(&self.directories[index])
    }

    /// Direct child directories and files of `directory_id`, unordered.
    pub fn direct_children(
        &self,
        directory_id: u64,
    ) -> Option<(Vec<&DirectoryRecord>, Vec<&FileRecord>)> {
        let index = self.dir_index_by_id.get(&directory_id)? as usize;
        let node = &self.nodes[index];
        let directories = self.ordered_directories()
            [node.dir_children.0 as usize..node.dir_children.1 as usize]
            .iter()
            .map(|child| &self.directories[*child as usize])
            .collect();
        // Building the memoized order sorts every directory's files (~7 ms
        // for 250k files); one directory's files are a linear filter.
        let files = match self.file_children.get() {
            Some(order) => order[node.file_children.0 as usize..node.file_children.1 as usize]
                .iter()
                .map(|child| &self.files[*child as usize])
                .collect(),
            None if node.file_children.0 == node.file_children.1 => Vec::new(),
            None => self
                .files
                .iter()
                .filter(|file| {
                    self.dir_index_by_id
                        .get(&file.parent_directory_id.0)
                        .or(self.root)
                        == Some(index as u32)
                })
                .collect(),
        };
        Some((directories, files))
    }

    /// Builds the memoized child orders now, so the first query that needs
    /// them (often on the UI thread, right after a refresh) does not.
    pub fn warm_child_orders(&self) {
        let _ = self.ordered_directories();
        let _ = self.ordered_files();
    }

    /// The parent of directory `index` exactly as [`TreeIndex::build`]
    /// attached it: its structural parent, else the root.
    fn parent_index(&self, index: u32) -> Option<u32> {
        if Some(index) == self.root {
            return None;
        }
        let directory = &self.directories[index as usize];
        directory
            .parent_directory_id
            .filter(|parent| parent.0 != directory.id.0)
            .and_then(|parent| self.dir_index_by_id.get(&parent.0))
            .filter(|parent| *parent != index)
            .or(self.root)
    }

    /// `index` and its ancestors up to the root. Bounded, so corrupt cyclic
    /// parent links cannot loop.
    fn ancestry(&self, index: u32) -> Vec<u32> {
        let mut chain = vec![index];
        let mut current = index;
        while let Some(parent) = self.parent_index(current) {
            if chain.len() > self.nodes.len() {
                break;
            }
            chain.push(parent);
            current = parent;
        }
        chain
    }

    /// Patches the tree in place for added and modified files, instead of
    /// rebuilding it from every record: a refresh that found one new file in
    /// a 250k-file tree costs microseconds, not a ~13 ms rebuild.
    ///
    /// Added files must carry ids above [`TreeIndex::max_file_id`], in
    /// increasing order, and name an existing parent directory. Modified
    /// files must keep their id and parent, and must not move their modified
    /// time back (the rollup maxima only grow here). Anything else is
    /// refused before the tree is touched; the caller rebuilds instead.
    ///
    /// Returns the ids of the directories whose rollups changed: each changed
    /// file's parent and its ancestors.
    pub fn apply_file_changes(
        &mut self,
        added: Vec<FileRecord>,
        modified: Vec<FileRecord>,
    ) -> Result<Vec<u64>, FilePatchError> {
        // Validate everything first.
        let mut next_id = self.max_file_id().map_or(0, |id| id.saturating_add(1));
        let mut added_parents = Vec::with_capacity(added.len());
        for file in &added {
            if file.id.0 < next_id {
                return Err(FilePatchError::IdNotIncreasing);
            }
            next_id = file.id.0.saturating_add(1);
            let parent = self
                .dir_index_by_id
                .get(&file.parent_directory_id.0)
                .ok_or(FilePatchError::UnknownParent)?;
            added_parents.push(parent);
        }
        let mut modified_slots = Vec::with_capacity(modified.len());
        for file in &modified {
            let slot = self
                .files
                .binary_search_by_key(&file.id.0, |existing| existing.id.0)
                .map_err(|_| FilePatchError::UnknownFile)?;
            let existing = &self.files[slot];
            if existing.parent_directory_id != file.parent_directory_id {
                return Err(FilePatchError::ParentChanged);
            }
            if file.modified_utc() < existing.modified_utc() {
                return Err(FilePatchError::ModifiedTimeDecreased);
            }
            let parent = self
                .dir_index_by_id
                .get(&file.parent_directory_id.0)
                .ok_or(FilePatchError::UnknownParent)?;
            modified_slots.push((slot, parent));
        }

        let mut touched: Vec<u32> = Vec::new();
        let mut resort_files: Vec<u32> = Vec::new();

        for (file, (slot, parent)) in modified.into_iter().zip(modified_slots) {
            let old = std::mem::replace(&mut self.files[slot], file);
            let new = &self.files[slot];
            let (old_logical, old_physical) = (old.size_bytes, old.allocation_bytes);
            let (new_logical, new_physical, new_modified) =
                (new.size_bytes, new.allocation_bytes, new.modified_utc());
            for index in self.ancestry(parent) {
                let rollup = &mut self.nodes[index as usize].rollup;
                rollup.logical = rollup
                    .logical
                    .saturating_sub(old_logical)
                    .saturating_add(new_logical);
                rollup.physical = rollup
                    .physical
                    .saturating_sub(old_physical)
                    .saturating_add(new_physical);
                rollup.observe_modified(new_modified);
                touched.push(index);
            }
            resort_files.push(parent);
        }

        if !added.is_empty() {
            let first_new = self.files.len() as u32;
            // New file children per node, then one pass shifting every
            // node's range past the insertions before it.
            let mut added_per_node = vec![0u32; self.nodes.len()];
            for parent in &added_parents {
                added_per_node[*parent as usize] += 1;
            }
            let old_ranges: Vec<(u32, u32)> =
                self.nodes.iter().map(|node| node.file_children).collect();
            let mut shift = 0u32;
            for (node, extra) in self.nodes.iter_mut().zip(&added_per_node) {
                node.file_children.0 += shift;
                shift += extra;
                node.file_children.1 += shift;
            }
            if let Some(order) = self.file_children.get_mut() {
                let mut grown = Vec::with_capacity(order.len() + added.len());
                let mut pending: Vec<(u32, u32)> = added_parents
                    .iter()
                    .enumerate()
                    .map(|(offset, parent)| (*parent, first_new + offset as u32))
                    .collect();
                pending.sort_unstable();
                let mut cursor = 0;
                for (index, (start, end)) in old_ranges.iter().enumerate() {
                    grown.extend_from_slice(&order[*start as usize..*end as usize]);
                    while cursor < pending.len() && pending[cursor].0 as usize == index {
                        grown.push(pending[cursor].1);
                        cursor += 1;
                    }
                }
                *order = grown;
            }
            for (file, parent) in added.into_iter().zip(added_parents) {
                let (logical, physical, modified) =
                    (file.size_bytes, file.allocation_bytes, file.modified_utc());
                self.files.push(file);
                for index in self.ancestry(parent) {
                    let rollup = &mut self.nodes[index as usize].rollup;
                    rollup.logical = rollup.logical.saturating_add(logical);
                    rollup.physical = rollup.physical.saturating_add(physical);
                    rollup.files = rollup.files.saturating_add(1);
                    rollup.items = rollup.items.saturating_add(1);
                    rollup.observe_modified(modified);
                    touched.push(index);
                }
                resort_files.push(parent);
            }
        }

        // Keep the memoized size orders valid for the changed ranges.
        resort_files.sort_unstable();
        resort_files.dedup();
        if let Some(order) = self.file_children.get_mut() {
            for parent in &resort_files {
                let (start, end) = self.nodes[*parent as usize].file_children;
                let files = &self.files;
                order[start as usize..end as usize].sort_unstable_by(|a, b| {
                    files[*b as usize]
                        .allocation_bytes
                        .cmp(&files[*a as usize].allocation_bytes)
                });
            }
        }
        touched.sort_unstable();
        touched.dedup();
        let mut parents: Vec<u32> = touched
            .iter()
            .filter_map(|index| self.parent_index(*index))
            .collect();
        parents.sort_unstable();
        parents.dedup();
        if let Some(order) = self.dir_children.get_mut() {
            for parent in parents {
                let (start, end) = self.nodes[parent as usize].dir_children;
                let nodes = &self.nodes;
                order[start as usize..end as usize].sort_unstable_by(|a, b| {
                    nodes[*b as usize]
                        .rollup
                        .physical
                        .cmp(&nodes[*a as usize].rollup.physical)
                });
            }
        }
        self.largest_files.take();

        Ok(touched
            .into_iter()
            .map(|index| self.directories[index as usize].id.0)
            .collect())
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

    pub fn root(&self) -> Option<(&DirectoryRecord, DirRollup)> {
        let root = self.root? as usize;
        Some((&self.directories[root], self.nodes[root].rollup.to_rollup()))
    }

    pub fn rollup_for(&self, directory_id: u64) -> Option<DirRollup> {
        let index = self.dir_index_by_id.get(&directory_id)? as usize;
        Some(self.nodes[index].rollup.to_rollup())
    }

    /// The display path of a directory of this tree: its stored path, or the
    /// parent chain's path plus its name when the record stores none.
    pub fn directory_path<'a>(&'a self, directory: &'a DirectoryRecord) -> Cow<'a, str> {
        if !directory.full_path.is_empty() {
            return Cow::Borrowed(&directory.full_path);
        }
        let mut path = String::new();
        self.push_directory_path(directory, &mut path);
        Cow::Owned(path)
    }

    /// Appends [`TreeIndex::directory_path`] to `out`, for callers that reuse
    /// one buffer.
    pub fn push_directory_path(&self, directory: &DirectoryRecord, out: &mut String) {
        if !directory.full_path.is_empty() {
            out.push_str(&directory.full_path);
            return;
        }
        // Collect the chain of derived ancestors (bounded, like `ancestry`).
        let mut chain: Vec<&DirectoryRecord> = vec![directory];
        let mut current = directory;
        while current.full_path.is_empty() {
            let Some(parent) = current
                .parent_directory_id
                .filter(|parent| parent.0 != current.id.0)
                .and_then(|parent| self.dir_index_by_id.get(&parent.0))
            else {
                break;
            };
            let parent = &self.directories[parent as usize];
            if chain.len() > self.nodes.len() {
                break;
            }
            chain.push(parent);
            current = parent;
        }
        let mut ancestors = chain.into_iter().rev();
        if let Some(top) = ancestors.next() {
            if top.full_path.is_empty() {
                out.push_str(&top.name);
            } else {
                out.push_str(&top.full_path);
            }
        }
        for directory in ancestors {
            if !out.is_empty() && !out.ends_with('\\') {
                out.push('\\');
            }
            out.push_str(&directory.name);
        }
    }

    /// [`TreeIndex::push_directory_path`] for callers that visit many
    /// directories in a row: consecutive siblings (the common order of a
    /// walk's ids) reuse their parent's path from `cache` instead of walking
    /// the chain again.
    pub fn push_directory_path_cached(
        &self,
        directory: &DirectoryRecord,
        cache: &mut DirectoryPathCache,
        out: &mut String,
    ) {
        if !directory.full_path.is_empty() {
            out.push_str(&directory.full_path);
            return;
        }
        let Some(parent) = directory
            .parent_directory_id
            .filter(|parent| parent.0 != directory.id.0)
            .and_then(|parent| self.dir_index_by_id.get(&parent.0))
        else {
            out.push_str(&directory.name);
            return;
        };
        if cache.parent != Some(parent) {
            cache.path.clear();
            self.push_directory_path(&self.directories[parent as usize], &mut cache.path);
            cache.parent = Some(parent);
        }
        out.push_str(&cache.path);
        if !out.is_empty() && !out.ends_with('\\') {
            out.push('\\');
        }
        out.push_str(&directory.name);
    }

    /// Every directory with its full path filled in, for writers that need
    /// complete records (snapshots, merges).
    pub fn directories_with_paths(&self) -> impl ExactSizeIterator<Item = DirectoryRecord> + '_ {
        self.directories.iter().map(|directory| {
            if directory.full_path.is_empty() {
                DirectoryRecord {
                    full_path: self.directory_path(directory).into_owned(),
                    ..directory.clone()
                }
            } else {
                directory.clone()
            }
        })
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

    pub fn directory_full_path(&self, directory_id: u64) -> Option<Cow<'_, str>> {
        let index = self.dir_index_by_id.get(&directory_id)? as usize;
        Some(self.directory_path(&self.directories[index]))
    }

    /// Display path for a file record: the stored path when present (records
    /// from older snapshots carry one), otherwise parent path + name.
    pub fn file_display_path(&self, file: &FileRecord) -> String {
        if !file.full_path.is_empty() {
            return file.full_path.to_string();
        }
        match self.directory_full_path(file.parent_directory_id.0) {
            Some(parent) => winblaze_core::join_path(&parent, &file.name),
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
                    .unwrap_or(Cow::Borrowed(""));
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
                self.nodes[child].rollup.physical
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
                    rollup: self.nodes[child].rollup.to_rollup(),
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

/// Empties the stored path of every directory whose path is exactly its
/// parent's path joined with its name: those paths are derived on demand,
/// which saves one heap string per directory of a volume. Decided against
/// the original paths before any is cleared, so a chain of derived
/// directories still ends at a stored one.
fn release_derivable_paths(directories: &mut [DirectoryRecord], lookup: &DirectoryLookup) {
    let derivable: Vec<bool> = directories
        .iter()
        .map(|directory| {
            let Some(parent) = directory
                .parent_directory_id
                .filter(|parent| parent.0 != directory.id.0)
                .and_then(|parent| lookup.get(&parent.0))
            else {
                return false;
            };
            let parent = &directories[parent as usize];
            if parent.full_path.is_empty() || directory.full_path.is_empty() {
                return false;
            }
            let expected_len = parent.full_path.len()
                + usize::from(!parent.full_path.ends_with('\\'))
                + directory.name.len();
            expected_len == directory.full_path.len()
                && directory.full_path.starts_with(parent.full_path.as_str())
                && directory.full_path.ends_with(directory.name.as_str())
                && (parent.full_path.ends_with('\\')
                    || directory.full_path.as_bytes()[parent.full_path.len()] == b'\\')
        })
        .collect();
    for (directory, derivable) in directories.iter_mut().zip(derivable) {
        if derivable {
            directory.full_path = String::new();
        }
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
            name: name.to_string().into(),
            full_path: format!(r"C:\{name}"),
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
    fn stored_rollups_are_thirty_two_bytes_and_nodes_forty_eight() {
        assert_eq!(std::mem::size_of::<StoredRollup>(), 32);
        assert_eq!(std::mem::size_of::<DirNode>(), 48);
    }

    #[test]
    fn derivable_directory_paths_are_released_and_rebuilt_exactly() {
        let walk = |id: u64, parent: Option<u64>, name: &str, path: &str| DirectoryRecord {
            id: DirectoryId(id),
            parent_directory_id: parent.map(DirectoryId),
            name: name.into(),
            full_path: path.to_string(),
        };
        let tree = build(
            vec![
                walk(5, None, "C:\\", "C:\\"),
                walk(6, Some(5), "Users", "C:\\Users"),
                walk(7, Some(6), "markm", "C:\\Users\\markm"),
                walk(
                    8,
                    Some(7),
                    "a-long-directory-name-\u{e9}",
                    "C:\\Users\\markm\\a-long-directory-name-\u{e9}",
                ),
                // An orphan keeps its own path: the root chain cannot derive it.
                walk(9, Some(99), "elsewhere", "D:\\elsewhere"),
                // A reparse target under a different prefix keeps its path.
                walk(10, Some(6), "Link", "E:\\Target"),
            ],
            vec![FileRecord {
                full_path: String::new().into(),
                ..file(1, 8, "leaf.bin", 1, 1)
            }],
        );
        let by_id = |id: u64| tree.directory_by_id(id).unwrap();
        assert!(!by_id(5).full_path.is_empty());
        assert!(by_id(6).full_path.is_empty());
        assert!(by_id(7).full_path.is_empty());
        assert!(by_id(8).full_path.is_empty());
        assert_eq!(by_id(9).full_path, "D:\\elsewhere");
        assert_eq!(by_id(10).full_path, "E:\\Target");
        assert_eq!(tree.directory_path(by_id(5)), "C:\\");
        assert_eq!(tree.directory_path(by_id(6)), "C:\\Users");
        assert_eq!(tree.directory_path(by_id(7)), "C:\\Users\\markm");
        assert_eq!(
            tree.directory_full_path(8).unwrap(),
            "C:\\Users\\markm\\a-long-directory-name-\u{e9}"
        );
        assert_eq!(tree.directory_full_path(9).unwrap(), "D:\\elsewhere");
        assert_eq!(
            tree.file_display_path(&tree.files()[0]),
            "C:\\Users\\markm\\a-long-directory-name-\u{e9}\\leaf.bin"
        );
        let mut scratch = String::from("x");
        scratch.clear();
        tree.push_directory_path(by_id(7), &mut scratch);
        assert_eq!(scratch, "C:\\Users\\markm");
        let with_paths: Vec<String> = tree
            .directories_with_paths()
            .map(|directory| directory.full_path)
            .collect();
        assert_eq!(
            with_paths,
            vec![
                "C:\\",
                "C:\\Users",
                "C:\\Users\\markm",
                "C:\\Users\\markm\\a-long-directory-name-\u{e9}",
                "D:\\elsewhere",
                "E:\\Target",
            ]
        );
    }

    #[test]
    fn cached_directory_paths_match_uncached_ones_in_any_order() {
        let mut dirs = vec![DirectoryRecord {
            id: DirectoryId(5),
            parent_directory_id: None,
            name: "C:\\".into(),
            full_path: "C:\\".into(),
        }];
        let mut next = 6;
        let mut frontier = vec![(5u64, "C:".to_string())];
        for depth in 0..4 {
            let mut deeper = Vec::new();
            for (parent, path) in &frontier {
                for child in 0..3 {
                    let name = format!("d{depth}-{child}");
                    let full = format!("{path}\\{name}");
                    dirs.push(DirectoryRecord {
                        id: DirectoryId(next),
                        parent_directory_id: Some(DirectoryId(*parent)),
                        name: name.as_str().into(),
                        full_path: full.clone(),
                    });
                    deeper.push((next, full));
                    next += 1;
                }
            }
            frontier = deeper;
        }
        let tree = build(dirs, Vec::new());
        let mut cache = DirectoryPathCache::default();
        let mut cached = String::new();
        // Forward, backward and interleaved orders all hit and miss the cache.
        let order: Vec<usize> = (0..tree.directories().len())
            .chain((0..tree.directories().len()).rev())
            .chain((0..tree.directories().len()).step_by(7))
            .collect();
        for index in order {
            let directory = &tree.directories()[index];
            cached.clear();
            tree.push_directory_path_cached(directory, &mut cache, &mut cached);
            assert_eq!(cached, tree.directory_path(directory), "{:?}", directory.id);
        }
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
                        (record.name.to_string(), rollup.physical_bytes)
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
                    paged.push(record.name.to_string());
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

    /// Everything a reader can observe: every directory's rollup, every
    /// directory's ordered children, and the largest files.
    fn observable(tree: &TreeIndex) -> Vec<String> {
        let mut seen = Vec::new();
        for directory in tree.directories() {
            let id = directory.id.0;
            seen.push(format!("{id}: {:?}", tree.rollup_for(id)));
            let mut children = Vec::new();
            tree.for_each_child(id, 0, usize::MAX, |entry| {
                children.push(match entry {
                    TreeEntry::Directory { record, rollup } => {
                        format!("d{}={}", record.id.0, rollup.physical_bytes)
                    }
                    TreeEntry::File(file) => {
                        format!(
                            "f{}={}/{}",
                            file.id.0, file.allocation_bytes, file.size_bytes
                        )
                    }
                });
            });
            seen.push(format!(
                "{id} children {children:?} count {:?}",
                tree.child_count(id)
            ));
        }
        let mut largest = Vec::new();
        tree.for_each_largest_file(100, |file| largest.push(file.id.0));
        seen.push(format!("largest {largest:?}"));
        seen
    }

    fn patch_tree_dirs() -> Vec<DirectoryRecord> {
        vec![
            dir(5, None, ""),
            dir(10, Some(5), "a"),
            dir(11, Some(5), "b"),
            dir(12, Some(10), "a-inner"),
        ]
    }

    fn patch_tree_files() -> Vec<FileRecord> {
        vec![
            file(1, 10, "a1.bin", 300, 300),
            file(2, 11, "b1.bin", 200, 200),
            file(3, 12, "inner.bin", 50, 50),
            file(4, 5, "top.bin", 70, 70),
        ]
    }

    fn assert_patch_matches_rebuild(warm: bool) {
        let mut tree = build(patch_tree_dirs(), patch_tree_files());
        if warm {
            // Initialise every memoized order so the patch must update them.
            let _ = observable(&tree);
        }
        let added = vec![
            file(20, 5, "new-top.bin", 9, 9),
            file(21, 11, "b-big.bin", 1000, 1000),
            file(22, 12, "deep.bin", 5, 5),
        ];
        let mut changed = file(2, 11, "b1.bin", 260, 4096);
        changed.timestamps = winblaze_core::FileTimestamps::new(None, Some(5000), None);
        let mut touched = tree
            .apply_file_changes(added.clone(), vec![changed.clone()])
            .expect("patch applies");
        touched.sort_unstable();
        assert_eq!(touched, vec![5, 10, 11, 12]);

        let mut files = patch_tree_files();
        files[1] = changed;
        files.extend(added);
        let rebuilt = build(patch_tree_dirs(), files);
        assert_eq!(observable(&tree), observable(&rebuilt));
        assert_eq!(tree.files().len(), rebuilt.files().len());
        assert_eq!(tree.max_file_id(), Some(22));
    }

    #[test]
    fn file_patch_matches_a_rebuild_before_any_query() {
        assert_patch_matches_rebuild(false);
    }

    #[test]
    fn file_patch_matches_a_rebuild_after_queries_memoized_orders() {
        assert_patch_matches_rebuild(true);
    }

    #[test]
    fn file_patch_refuses_what_it_cannot_apply_and_leaves_the_tree_unchanged() {
        let mut tree = build(patch_tree_dirs(), patch_tree_files());
        let before = observable(&tree);
        let cases: Vec<(Vec<FileRecord>, Vec<FileRecord>, FilePatchError)> = vec![
            (
                vec![file(4, 5, "reused-id.bin", 1, 1)],
                vec![],
                FilePatchError::IdNotIncreasing,
            ),
            (
                vec![file(30, 99, "orphan.bin", 1, 1)],
                vec![],
                FilePatchError::UnknownParent,
            ),
            (
                vec![],
                vec![file(77, 5, "missing.bin", 1, 1)],
                FilePatchError::UnknownFile,
            ),
            (
                vec![],
                vec![file(1, 11, "a1.bin", 1, 1)],
                FilePatchError::ParentChanged,
            ),
            (
                // A valid add before an invalid modification must not apply.
                vec![file(40, 5, "fine.bin", 1, 1)],
                vec![{
                    let mut older = file(1, 10, "a1.bin", 1, 1);
                    older.timestamps = winblaze_core::FileTimestamps::new(None, Some(1), None);
                    older
                }],
                FilePatchError::ModifiedTimeDecreased,
            ),
        ];
        for (added, modified, expected) in cases {
            assert_eq!(tree.apply_file_changes(added, modified), Err(expected));
            assert_eq!(observable(&tree), before);
        }
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
