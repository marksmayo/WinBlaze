use winblaze_core::{
    aggregate_directory_records, DirectoryRecord, FileRecord, MatchMode, SearchQuery,
    SortDirection, SortField, VolumeRecord,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexRecordKind {
    Volume,
    Directory,
    File,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IndexSearchHit {
    pub kind: IndexRecordKind,
    pub name: String,
    pub full_path: String,
    pub size_bytes: u64,
    pub allocation_bytes: u64,
    pub modified_utc: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IndexCatalog {
    pub volumes: Vec<VolumeRecord>,
    pub directories: Vec<DirectoryRecord>,
    pub files: Vec<FileRecord>,
    pub aggregated_directories: Vec<DirectoryRecord>,
}

impl IndexCatalog {
    pub fn from_transaction(transaction: &crate::store::BufferedIndexTransaction) -> Self {
        let directories = transaction.snapshot_directories();
        // Path-materialized: search matches patterns against full paths, and
        // scanners no longer store one per file.
        let files = transaction.snapshot_files_with_paths();
        let aggregated_directories = aggregate_directory_records(&directories, &files);

        Self {
            volumes: transaction.snapshot_volumes(),
            directories,
            files,
            aggregated_directories,
        }
    }

    pub fn apply_incremental_files(
        &mut self,
        transaction: &mut crate::store::BufferedIndexTransaction,
        previous: &[FileRecord],
        current: &[FileRecord],
    ) -> winblaze_core::FileChangeSet {
        let change_set = transaction.apply_incremental_files(previous, current);
        self.files = transaction.snapshot_files();
        self.aggregated_directories = aggregate_directory_records(&self.directories, &self.files);
        change_set
    }

    pub fn search(&self, query: &SearchQuery) -> Vec<IndexSearchHit> {
        let candidate_count = usize::from(query.include_files) * self.files.len()
            + usize::from(query.include_directories) * self.aggregated_directories.len();
        if query.limit == Some(0)
            || candidate_count == 0
            || query
                .size
                .min_bytes
                .zip(query.size.max_bytes)
                .is_some_and(|(min, max)| min > max)
            || query
                .modified
                .modified_after_utc
                .zip(query.modified.modified_before_utc)
                .is_some_and(|(after, before)| after >= before)
        {
            return Vec::new();
        }
        // Text predicates compare borrowed UTF-8 bytes case-insensitively.
        // Do not allocate query normalization or a result buffer before a match.
        let pattern = query.pattern.as_deref();
        let extensions = PreparedExtensions::new(&query.extensions);
        let mut hits = Vec::new();

        if query.include_files {
            hits.extend(self.files.iter().filter_map(|file| {
                let file_params = FileMatchParams {
                    name: &file.name,
                    full_path: &file.full_path,
                    size_bytes: file.size_bytes,
                    allocation_bytes: file.allocation_bytes,
                    modified_utc: file.modified_utc(),
                };
                if matches_query(query, pattern, extensions.as_slice(), &file_params) {
                    Some(CandidateHit::File(file))
                } else {
                    None
                }
            }));
        }

        if query.include_directories {
            hits.extend(self.aggregated_directories.iter().filter_map(|directory| {
                let file_params = FileMatchParams {
                    name: &directory.name,
                    full_path: &directory.full_path,
                    size_bytes: directory.total_bytes,
                    allocation_bytes: directory.total_bytes,
                    modified_utc: None,
                };
                if matches_query(query, pattern, extensions.as_slice(), &file_params) {
                    Some(CandidateHit::Directory(directory))
                } else {
                    None
                }
            }));
        }

        if let Some(limit) = query.limit.filter(|&limit| limit < hits.len()) {
            if matches!(query.sort_field, SortField::Name | SortField::Path) {
                // The ordinal tie-break reproduces stable ascending order and
                // its reversed descending order, without allocating lowercase
                // keys for all matches. Only the selected prefix is sorted.
                let mut order: Vec<usize> = (0..hits.len()).collect();
                let compare = |a: &usize, b: &usize| {
                    let left = hits[*a].params();
                    let right = hits[*b].params();
                    let (left, right) = if query.sort_field == SortField::Name {
                        (left.name, right.name)
                    } else {
                        (left.full_path, right.full_path)
                    };
                    let ordering = left
                        .bytes()
                        .map(|byte| byte.to_ascii_lowercase())
                        .cmp(right.bytes().map(|byte| byte.to_ascii_lowercase()))
                        .then_with(|| a.cmp(b));
                    if query.sort_direction == SortDirection::Descending {
                        ordering.reverse()
                    } else {
                        ordering
                    }
                };
                order.select_nth_unstable_by(limit - 1, compare);
                order.truncate(limit);
                order.sort_unstable_by(compare);
                hits = order.into_iter().map(|index| hits[index]).collect();
            } else {
                sort_hits(&mut hits, query.sort_field, query.sort_direction);
            }
        } else {
            sort_hits(&mut hits, query.sort_field, query.sort_direction);
        }

        if let Some(limit) = query.limit {
            hits.truncate(limit);
        }

        hits.into_iter().map(CandidateHit::into_owned).collect()
    }
}

struct FileMatchParams<'a> {
    name: &'a str,
    full_path: &'a str,
    size_bytes: u64,
    allocation_bytes: u64,
    modified_utc: Option<i64>,
}

// Borrowed query filters retain case-insensitive matching without owned keys.
fn matches_query(
    query: &SearchQuery,
    pattern_lower: Option<&str>,
    extensions: &[&str],
    file_params: &FileMatchParams,
) -> bool {
    if !pattern_lower.is_none_or(|pattern| {
        matches_text(
            pattern,
            &query.match_mode,
            file_params.name,
            file_params.full_path,
        )
    }) {
        return false;
    }

    if !extensions.is_empty() {
        // Compare the raw extension case-insensitively against the already
        // lowercased query extensions â€” no per-entry String allocation.
        let extension = std::path::Path::new(file_params.full_path)
            .extension()
            .and_then(|value| value.to_str());

        match extension {
            Some(raw) if extensions.iter().any(|ext| ext.eq_ignore_ascii_case(raw)) => {}
            _ => return false,
        }
    }

    if let Some(min_bytes) = query.size.min_bytes {
        if file_params.size_bytes < min_bytes {
            return false;
        }
    }

    if let Some(max_bytes) = query.size.max_bytes {
        if file_params.size_bytes > max_bytes {
            return false;
        }
    }

    if let Some(after) = query.modified.modified_after_utc {
        if file_params.modified_utc.is_none_or(|value| value <= after) {
            return false;
        }
    }

    if let Some(before) = query.modified.modified_before_utc {
        if file_params.modified_utc.is_none_or(|value| value >= before) {
            return false;
        }
    }

    let _ = file_params.allocation_bytes;
    true
}

fn starts_with_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    if haystack.len() < needle.len() {
        return false;
    }
    haystack.as_bytes()[..needle.len()].eq_ignore_ascii_case(needle.as_bytes())
}

fn contains_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

// Every mode compares ASCII case without changing UTF-8 bytes.
fn matches_text(pattern: &str, mode: &MatchMode, name: &str, full_path: &str) -> bool {
    match mode {
        MatchMode::Exact => {
            name.eq_ignore_ascii_case(pattern) || full_path.eq_ignore_ascii_case(pattern)
        }
        MatchMode::Prefix => {
            starts_with_ignore_ascii_case(name, pattern)
                || starts_with_ignore_ascii_case(full_path, pattern)
        }
        MatchMode::Contains | MatchMode::Substring => {
            contains_ignore_ascii_case(name, pattern)
                || contains_ignore_ascii_case(full_path, pattern)
        }
    }
}

fn normalize_extension(extension: &str) -> Option<&str> {
    let normalized = extension.trim().trim_start_matches('.');
    (!normalized.is_empty()).then_some(normalized)
}

// Most UI queries select only a handful of extensions. Normalize once into
// borrowed stack slots, with an owned pointer vector for longer filter lists.
struct PreparedExtensions<'a> {
    inline: [&'a str; 8],
    len: usize,
    overflow: Vec<&'a str>,
}
impl<'a> PreparedExtensions<'a> {
    fn new(extensions: &'a [String]) -> Self {
        let mut prepared = Self {
            inline: [""; 8],
            len: 0,
            overflow: Vec::new(),
        };
        for extension in extensions.iter().filter_map(|ext| normalize_extension(ext)) {
            if prepared.len < prepared.inline.len() {
                prepared.inline[prepared.len] = extension;
            } else {
                if prepared.overflow.is_empty() {
                    prepared.overflow.extend_from_slice(&prepared.inline);
                }
                prepared.overflow.push(extension);
            }
            prepared.len += 1;
        }
        prepared
    }
    fn as_slice(&self) -> &[&str] {
        if self.overflow.is_empty() {
            &self.inline[..self.len]
        } else {
            &self.overflow
        }
    }
}

#[derive(Clone, Copy)]
enum CandidateHit<'a> {
    File(&'a FileRecord),
    Directory(&'a DirectoryRecord),
}

impl CandidateHit<'_> {
    fn params(&self) -> FileMatchParams<'_> {
        match self {
            Self::File(file) => FileMatchParams {
                name: &file.name,
                full_path: &file.full_path,
                size_bytes: file.size_bytes,
                allocation_bytes: file.allocation_bytes,
                modified_utc: file.modified_utc(),
            },
            Self::Directory(directory) => FileMatchParams {
                name: &directory.name,
                full_path: &directory.full_path,
                size_bytes: directory.total_bytes,
                allocation_bytes: directory.total_bytes,
                modified_utc: None,
            },
        }
    }

    fn into_owned(self) -> IndexSearchHit {
        let params = self.params();
        IndexSearchHit {
            kind: match self {
                Self::File(_) => IndexRecordKind::File,
                Self::Directory(_) => IndexRecordKind::Directory,
            },
            name: params.name.to_owned(),
            full_path: params.full_path.to_owned(),
            size_bytes: params.size_bytes,
            allocation_bytes: params.allocation_bytes,
            modified_utc: params.modified_utc,
        }
    }
}

fn sort_hits(hits: &mut [CandidateHit<'_>], sort_field: SortField, sort_direction: SortDirection) {
    // Sort borrowed candidates; names and full paths are copied only after the
    // result limit is applied. Preserve the existing stable textual ordering.
    match sort_field {
        SortField::Name => hits.sort_by_cached_key(|hit| hit.params().name.to_ascii_lowercase()),
        SortField::Path => {
            hits.sort_by_cached_key(|hit| hit.params().full_path.to_ascii_lowercase())
        }
        SortField::SizeBytes => hits.sort_unstable_by_key(|hit| hit.params().size_bytes),
        SortField::AllocationBytes => {
            hits.sort_unstable_by_key(|hit| hit.params().allocation_bytes)
        }
        SortField::ModifiedUtc => hits.sort_unstable_by_key(|hit| hit.params().modified_utc),
    }
    if sort_direction == SortDirection::Descending {
        hits.reverse();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{BufferedIndexTransaction, IndexTransaction};
    use winblaze_core::{
        DirectoryId, DirectoryRecord, FileAttributes, FileId, FileSystemKind, VolumeId,
        VolumeRecord,
    };

    #[test]
    fn search_filters_and_sorts_records() {
        let mut transaction = BufferedIndexTransaction::default();
        transaction.upsert_volume(&VolumeRecord {
            id: VolumeId(1),
            mount_point: String::from("C:\\"),
            label: Some(String::from("System")),
            file_system: FileSystemKind::Ntfs,
            total_bytes: 1024,
            free_bytes: 512,
            root_directory_id: DirectoryId(10),
        });
        transaction.upsert_directory(&DirectoryRecord {
            id: DirectoryId(10),
            parent_directory_id: None,
            name: String::from("root"),
            full_path: String::from("C:\\root"),
            direct_bytes: 100,
            total_bytes: 200,
            direct_entries: 2,
            total_entries: 4,
        });
        transaction.upsert_file(&FileRecord {
            id: FileId(1),
            parent_directory_id: DirectoryId(10),
            name: String::from("alpha.log").into(),
            full_path: String::from("C:\\root\\alpha.log").into(),
            size_bytes: 10,
            allocation_bytes: 16,
            attributes: FileAttributes::ARCHIVE,
            timestamps: winblaze_core::FileTimestamps::new(None, Some(10), None),
        });
        transaction.upsert_file(&FileRecord {
            id: FileId(2),
            parent_directory_id: DirectoryId(10),
            name: String::from("beta.txt").into(),
            full_path: String::from("C:\\root\\beta.txt").into(),
            size_bytes: 20,
            allocation_bytes: 32,
            attributes: FileAttributes::ARCHIVE,
            timestamps: winblaze_core::FileTimestamps::new(None, Some(20), None),
        });

        let catalog = IndexCatalog::from_transaction(&transaction);
        let query = SearchQuery {
            include_files: true,
            pattern: Some(String::from("a")),
            extensions: vec![String::from("log")],
            sort_field: SortField::SizeBytes,
            sort_direction: SortDirection::Descending,
            ..SearchQuery::default()
        };

        let hits = catalog.search(&query);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].name, "alpha.log");
        assert_eq!(hits[0].kind, IndexRecordKind::File);
    }

    #[test]
    fn incremental_updates_refresh_the_catalog_snapshot() {
        let mut transaction = BufferedIndexTransaction::default();
        let mut catalog = IndexCatalog::from_transaction(&transaction);

        let previous = vec![FileRecord {
            id: winblaze_core::FileId(1),
            parent_directory_id: winblaze_core::DirectoryId(10),
            name: String::from("old.txt").into(),
            full_path: String::from("C:\\root\\old.txt").into(),
            size_bytes: 10,
            allocation_bytes: 16,
            attributes: FileAttributes::ARCHIVE,
            timestamps: winblaze_core::FileTimestamps::new(None, Some(10), None),
        }];
        let current = vec![FileRecord {
            id: winblaze_core::FileId(1),
            parent_directory_id: winblaze_core::DirectoryId(10),
            name: String::from("new.txt").into(),
            full_path: String::from("C:\\root\\new.txt").into(),
            size_bytes: 11,
            allocation_bytes: 16,
            attributes: FileAttributes::ARCHIVE,
            timestamps: winblaze_core::FileTimestamps::new(None, Some(11), None),
        }];

        let change_set = catalog.apply_incremental_files(&mut transaction, &previous, &current);
        assert_eq!(change_set.changes.len(), 1);
        assert_eq!(catalog.files.len(), 1);
        assert_eq!(catalog.files[0].name.as_ref(), "new.txt");
    }

    fn sample_catalog() -> IndexCatalog {
        let mut transaction = BufferedIndexTransaction::default();
        transaction.upsert_directory(&DirectoryRecord {
            id: DirectoryId(10),
            parent_directory_id: None,
            name: String::from("root"),
            full_path: String::from("C:\\root"),
            direct_bytes: 100,
            total_bytes: 200,
            direct_entries: 3,
            total_entries: 3,
        });
        for (id, name, path, size, alloc, modified) in [
            (
                1u64,
                "alpha.log",
                "C:\\root\\alpha.log",
                10u64,
                16u64,
                10i64,
            ),
            (2, "beta.txt", "C:\\root\\beta.txt", 20, 32, 20),
            (3, "gamma.txt", "C:\\root\\gamma.txt", 30, 48, 30),
        ] {
            transaction.upsert_file(&FileRecord {
                id: FileId(id),
                parent_directory_id: DirectoryId(10),
                name: String::from(name).into(),
                full_path: String::from(path).into(),
                size_bytes: size,
                allocation_bytes: alloc,
                attributes: FileAttributes::ARCHIVE,
                timestamps: winblaze_core::FileTimestamps::new(None, Some(modified), None),
            });
        }
        IndexCatalog::from_transaction(&transaction)
    }

    fn names(hits: Vec<IndexSearchHit>) -> Vec<String> {
        hits.into_iter().map(|hit| hit.name).collect()
    }

    #[test]
    fn search_matches_directories_with_prefix_and_name_sort() {
        let query = SearchQuery {
            include_files: false,
            include_directories: true,
            pattern: Some(String::from("ro")),
            match_mode: MatchMode::Prefix,
            sort_field: SortField::Name,
            sort_direction: SortDirection::Ascending,
            ..SearchQuery::default()
        };
        let hits = sample_catalog().search(&query);
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].kind, IndexRecordKind::Directory);
        assert_eq!(hits[0].name, "root");
    }

    #[test]
    fn search_returns_nothing_when_pattern_matches_no_record() {
        let query = SearchQuery {
            include_files: true,
            pattern: Some(String::from("zzz-no-such-name")),
            ..SearchQuery::default()
        };
        assert!(sample_catalog().search(&query).is_empty());
    }

    #[test]
    fn search_applies_size_bounds() {
        let mut query = SearchQuery {
            include_files: true,
            ..SearchQuery::default()
        };
        query.size.min_bytes = Some(15);
        query.size.max_bytes = Some(25);
        assert_eq!(names(sample_catalog().search(&query)), ["beta.txt"]);
    }

    #[test]
    fn search_applies_modified_bounds() {
        let mut after = SearchQuery {
            include_files: true,
            sort_field: SortField::ModifiedUtc,
            sort_direction: SortDirection::Ascending,
            ..SearchQuery::default()
        };
        after.modified.modified_after_utc = Some(15);
        assert_eq!(
            names(sample_catalog().search(&after)),
            ["beta.txt", "gamma.txt"]
        );

        let mut before = SearchQuery {
            include_files: true,
            sort_field: SortField::ModifiedUtc,
            sort_direction: SortDirection::Ascending,
            ..SearchQuery::default()
        };
        before.modified.modified_before_utc = Some(25);
        assert_eq!(
            names(sample_catalog().search(&before)),
            ["alpha.log", "beta.txt"]
        );
    }

    #[test]
    fn search_honors_each_match_mode() {
        let catalog = sample_catalog();

        let exact = SearchQuery {
            include_files: true,
            pattern: Some(String::from("BETA.TXT")),
            match_mode: MatchMode::Exact,
            ..SearchQuery::default()
        };
        assert_eq!(names(catalog.search(&exact)), ["beta.txt"]);

        let contains = SearchQuery {
            include_files: true,
            pattern: Some(String::from("amm")),
            match_mode: MatchMode::Contains,
            ..SearchQuery::default()
        };
        assert_eq!(names(catalog.search(&contains)), ["gamma.txt"]);

        // An empty pattern under Contains matches every record.
        let empty = SearchQuery {
            include_files: true,
            pattern: Some(String::new()),
            match_mode: MatchMode::Contains,
            ..SearchQuery::default()
        };
        assert_eq!(catalog.search(&empty).len(), 3);
    }

    #[test]
    fn search_sorts_by_each_field_and_direction() {
        let catalog = sample_catalog();

        let by_name = SearchQuery {
            include_files: true,
            sort_field: SortField::Name,
            sort_direction: SortDirection::Ascending,
            ..SearchQuery::default()
        };
        assert_eq!(
            names(catalog.search(&by_name)),
            ["alpha.log", "beta.txt", "gamma.txt"]
        );

        let by_path_desc = SearchQuery {
            include_files: true,
            sort_field: SortField::Path,
            sort_direction: SortDirection::Descending,
            ..SearchQuery::default()
        };
        assert_eq!(
            names(catalog.search(&by_path_desc)),
            ["gamma.txt", "beta.txt", "alpha.log"]
        );

        let by_alloc = SearchQuery {
            include_files: true,
            sort_field: SortField::AllocationBytes,
            sort_direction: SortDirection::Ascending,
            ..SearchQuery::default()
        };
        let allocs: Vec<u64> = catalog
            .search(&by_alloc)
            .into_iter()
            .map(|hit| hit.allocation_bytes)
            .collect();
        assert_eq!(allocs, [16, 32, 48]);

        let by_modified_desc = SearchQuery {
            include_files: true,
            sort_field: SortField::ModifiedUtc,
            sort_direction: SortDirection::Descending,
            ..SearchQuery::default()
        };
        let mods: Vec<Option<i64>> = catalog
            .search(&by_modified_desc)
            .into_iter()
            .map(|hit| hit.modified_utc)
            .collect();
        assert_eq!(mods, [Some(30), Some(20), Some(10)]);
    }

    #[test]
    fn search_ignores_extensions_that_normalize_to_nothing() {
        // "." and "" normalize away, so no extension filter is applied.
        let query = SearchQuery {
            include_files: true,
            extensions: vec![String::from("."), String::new()],
            ..SearchQuery::default()
        };
        assert_eq!(sample_catalog().search(&query).len(), 3);
    }

    #[test]
    fn search_filters_by_extension_case_insensitively() {
        let query = SearchQuery {
            include_files: true,
            extensions: vec![String::from("TXT")],
            sort_field: SortField::Name,
            sort_direction: SortDirection::Ascending,
            ..SearchQuery::default()
        };
        assert_eq!(
            names(sample_catalog().search(&query)),
            ["beta.txt", "gamma.txt"]
        );
    }
}
