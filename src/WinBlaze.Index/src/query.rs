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
        // Pre-compute per-query invariants once rather than re-computing inside every
        // per-entry predicate call.
        //
        // 1. Lowercase the pattern a single time; matches_text receives a &str so it
        //    never needs to allocate for Prefix/Contains/Substring modes.
        // 2. Normalize extensions a single time; the inner loop previously called
        //    normalize_extension() for every (entry × extension candidate) pair.
        let pattern_lower: Option<String> = query.pattern.as_deref().map(str::to_ascii_lowercase);
        let normalized_extensions: Vec<String> = query
            .extensions
            .iter()
            .filter_map(|ext| normalize_extension(ext))
            .collect();

        let candidate_count = usize::from(query.include_files) * self.files.len()
            + usize::from(query.include_directories) * self.aggregated_directories.len();
        let initial_capacity = query.limit.unwrap_or(256).min(candidate_count);
        let mut hits = Vec::with_capacity(initial_capacity);

        if query.include_files {
            hits.extend(self.files.iter().filter_map(|file| {
                let file_params = FileMatchParams {
                    name: &file.name,
                    full_path: &file.full_path,
                    size_bytes: file.size_bytes,
                    allocation_bytes: file.allocation_bytes,
                    modified_utc: file.modified_utc,
                };
                if matches_query(
                    query,
                    pattern_lower.as_deref(),
                    &normalized_extensions,
                    &file_params,
                ) {
                    Some(IndexSearchHit {
                        kind: IndexRecordKind::File,
                        name: file.name.clone(),
                        full_path: file.full_path.clone(),
                        size_bytes: file.size_bytes,
                        allocation_bytes: file.allocation_bytes,
                        modified_utc: file.modified_utc,
                    })
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
                if matches_query(
                    query,
                    pattern_lower.as_deref(),
                    &normalized_extensions,
                    &file_params,
                ) {
                    Some(IndexSearchHit {
                        kind: IndexRecordKind::Directory,
                        name: directory.name.clone(),
                        full_path: directory.full_path.clone(),
                        size_bytes: directory.total_bytes,
                        allocation_bytes: directory.total_bytes,
                        modified_utc: None,
                    })
                } else {
                    None
                }
            }));
        }

        sort_hits(&mut hits, query.sort_field, query.sort_direction);

        if let Some(limit) = query.limit {
            hits.truncate(limit);
        }

        hits
    }
}

struct FileMatchParams<'a> {
    name: &'a str,
    full_path: &'a str,
    size_bytes: u64,
    allocation_bytes: u64,
    modified_utc: Option<i64>,
}

// `pattern_lower` is the already-lowercased search pattern (None when no pattern).
// `normalized_extensions` are pre-normalized extension strings (empty when no filter).
// Both are computed once per query in `IndexCatalog::search` rather than per entry.
fn matches_query(
    query: &SearchQuery,
    pattern_lower: Option<&str>,
    normalized_extensions: &[String],
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

    if !normalized_extensions.is_empty() {
        // Compare the raw extension case-insensitively against the already
        // lowercased query extensions — no per-entry String allocation.
        let extension = std::path::Path::new(file_params.full_path)
            .extension()
            .and_then(|value| value.to_str());

        match extension {
            Some(raw)
                if normalized_extensions
                    .iter()
                    .any(|ext| ext.eq_ignore_ascii_case(raw)) => {}
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

// `pattern` is already lowercased by the caller (IndexCatalog::search).
// Exact mode uses eq_ignore_ascii_case directly and does not need a lowercased pattern.
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

fn normalize_extension(extension: &str) -> Option<String> {
    let normalized = extension
        .trim()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn sort_hits(hits: &mut [IndexSearchHit], sort_field: SortField, sort_direction: SortDirection) {
    // sort_unstable_by_cached_key calls the key closure exactly once per element (O(N))
    // rather than once per comparison (O(N log N)).  For Name and Path sorts this
    // eliminates the O(N log N) String allocations that the previous sort_by approach
    // incurred by calling to_ascii_lowercase() inside the comparator.
    //
    // sort_unstable_by_key is used for numeric fields (zero allocation, no need to cache).
    match sort_field {
        SortField::Name => {
            hits.sort_by_cached_key(|h| h.name.to_ascii_lowercase());
        }
        SortField::Path => {
            hits.sort_by_cached_key(|h| h.full_path.to_ascii_lowercase());
        }
        SortField::SizeBytes => hits.sort_unstable_by_key(|h| h.size_bytes),
        SortField::AllocationBytes => hits.sort_unstable_by_key(|h| h.allocation_bytes),
        SortField::ModifiedUtc => hits.sort_unstable_by_key(|h| h.modified_utc),
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
            name: String::from("alpha.log"),
            full_path: String::from("C:\\root\\alpha.log"),
            size_bytes: 10,
            allocation_bytes: 16,
            attributes: FileAttributes::ARCHIVE,
            created_utc: None,
            modified_utc: Some(10),
            accessed_utc: None,
        });
        transaction.upsert_file(&FileRecord {
            id: FileId(2),
            parent_directory_id: DirectoryId(10),
            name: String::from("beta.txt"),
            full_path: String::from("C:\\root\\beta.txt"),
            size_bytes: 20,
            allocation_bytes: 32,
            attributes: FileAttributes::ARCHIVE,
            created_utc: None,
            modified_utc: Some(20),
            accessed_utc: None,
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
            name: String::from("old.txt"),
            full_path: String::from("C:\\root\\old.txt"),
            size_bytes: 10,
            allocation_bytes: 16,
            attributes: FileAttributes::ARCHIVE,
            created_utc: None,
            modified_utc: Some(10),
            accessed_utc: None,
        }];
        let current = vec![FileRecord {
            id: winblaze_core::FileId(1),
            parent_directory_id: winblaze_core::DirectoryId(10),
            name: String::from("new.txt"),
            full_path: String::from("C:\\root\\new.txt"),
            size_bytes: 11,
            allocation_bytes: 16,
            attributes: FileAttributes::ARCHIVE,
            created_utc: None,
            modified_utc: Some(11),
            accessed_utc: None,
        }];

        let change_set = catalog.apply_incremental_files(&mut transaction, &previous, &current);
        assert_eq!(change_set.changes.len(), 1);
        assert_eq!(catalog.files.len(), 1);
        assert_eq!(catalog.files[0].name, "new.txt");
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
                name: String::from(name),
                full_path: String::from(path),
                size_bytes: size,
                allocation_bytes: alloc,
                attributes: FileAttributes::ARCHIVE,
                created_utc: None,
                modified_utc: Some(modified),
                accessed_utc: None,
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
