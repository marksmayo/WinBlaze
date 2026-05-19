use crate::model::{DirectoryId, VolumeId};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchScope {
    pub volume_id: Option<VolumeId>,
    pub root_directory_id: Option<DirectoryId>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SizeFilter {
    pub min_bytes: Option<u64>,
    pub max_bytes: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DateFilter {
    pub modified_after_utc: Option<i64>,
    pub modified_before_utc: Option<i64>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MatchMode {
    Exact,
    Prefix,
    Contains,
    #[default]
    Substring,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortField {
    #[default]
    Name,
    SizeBytes,
    AllocationBytes,
    ModifiedUtc,
    Path,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SortDirection {
    #[default]
    Descending,
    Ascending,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchQuery {
    pub scope: SearchScope,
    pub pattern: Option<String>,
    pub match_mode: MatchMode,
    pub extensions: Vec<String>,
    pub include_files: bool,
    pub include_directories: bool,
    pub size: SizeFilter,
    pub modified: DateFilter,
    pub sort_field: SortField,
    pub sort_direction: SortDirection,
    pub limit: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_query_defaults_are_conservative() {
        let query = SearchQuery::default();

        assert!(query.scope.volume_id.is_none());
        assert!(query.scope.root_directory_id.is_none());
        assert!(query.pattern.is_none());
        assert_eq!(query.match_mode, MatchMode::Substring);
        assert!(query.extensions.is_empty());
        assert!(!query.include_files);
        assert!(!query.include_directories);
        assert_eq!(query.sort_field, SortField::Name);
        assert_eq!(query.sort_direction, SortDirection::Descending);
        assert!(query.limit.is_none());
    }
}
