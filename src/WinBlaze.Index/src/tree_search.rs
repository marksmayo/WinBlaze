//! Complete-index search with bounded output pages and borrowed candidate keys.
use crate::{TreeEntry, TreeIndex};
use std::io;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TextMode {
    #[default]
    Contains,
    Exact,
    Prefix,
    Regex,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SearchSort {
    #[default]
    Name,
    Logical,
    Allocated,
    Kind,
    Modified,
}

#[derive(Debug, Clone)]
pub struct SearchOptions {
    pub pattern: String,
    pub sort: SearchSort,
    pub descending: bool,
    pub mode: TextMode,
    pub min_logical: Option<u64>,
    pub max_logical: Option<u64>,
    pub min_allocated: Option<u64>,
    pub max_allocated: Option<u64>,
    /// Inclusive lower / exclusive upper UTC Unix-second boundaries.
    pub modified_after: Option<i64>,
    pub modified_before: Option<i64>,
    pub extensions: Vec<String>,
    pub include_directories: bool,
    pub include_files: bool,
    pub offset: usize,
    pub limit: usize,
}
impl Default for SearchOptions {
    fn default() -> Self {
        Self {
            pattern: String::new(),
            sort: SearchSort::Name,
            descending: false,
            mode: TextMode::Contains,
            min_logical: None,
            max_logical: None,
            min_allocated: None,
            max_allocated: None,
            modified_after: None,
            modified_before: None,
            extensions: Vec::new(),
            include_directories: true,
            include_files: true,
            offset: 0,
            limit: 50,
        }
    }
}
#[derive(Debug)]
pub struct SearchPage<'a> {
    pub total: usize,
    pub hits: Vec<SearchResult<'a>>,
}
#[derive(Debug)]
pub struct SearchResult<'a> {
    pub entry: TreeEntry<'a>,
    pub path: String,
}
fn name(entry: TreeEntry<'_>) -> &str {
    match entry {
        TreeEntry::Directory { record, .. } => &record.name,
        TreeEntry::File(file) => &file.name,
    }
}
fn path(tree: &TreeIndex, entry: TreeEntry<'_>) -> String {
    match entry {
        TreeEntry::Directory { record, .. } => record.full_path.clone(),
        TreeEntry::File(file) => tree.file_display_path(file),
    }
}
fn entry(tree: &TreeIndex, key: (bool, usize)) -> TreeEntry<'_> {
    if key.0 {
        let record = &tree.directories()[key.1];
        TreeEntry::Directory {
            record,
            rollup: tree.rollup_for(record.id.0).unwrap(),
        }
    } else {
        TreeEntry::File(&tree.files()[key.1])
    }
}
pub fn search<'a>(tree: &'a TreeIndex, options: &SearchOptions) -> io::Result<SearchPage<'a>> {
    if options
        .min_logical
        .zip(options.max_logical)
        .is_some_and(|(min, max)| min > max)
        || options
            .min_allocated
            .zip(options.max_allocated)
            .is_some_and(|(min, max)| min > max)
        || options
            .modified_after
            .zip(options.modified_before)
            .is_some_and(|(min, max)| min >= max)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Reversed search bounds",
        ));
    }
    let extensions: Vec<_> = options
        .extensions
        .iter()
        .map(|value| {
            value
                .trim()
                .trim_start_matches("*.")
                .trim_start_matches('.')
                .to_lowercase()
        })
        .filter(|value| !value.is_empty())
        .collect();
    let pattern = options.pattern.to_lowercase();
    let regex = if options.mode == TextMode::Regex {
        Some(
            regex::RegexBuilder::new(&options.pattern)
                .case_insensitive(true)
                .size_limit(2 * 1024 * 1024)
                .build()
                .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?,
        )
    } else {
        None
    };
    let matches_text = |text: &str| {
        if let Some(regex) = &regex {
            return regex.is_match(text);
        }
        let text = text.to_lowercase();
        match options.mode {
            TextMode::Exact => text == pattern,
            TextMode::Prefix => text.starts_with(&pattern),
            _ => text.contains(&pattern),
        }
    };
    let keys = (0..tree.directories().len())
        .map(|index| (true, index))
        .chain((0..tree.files().len()).map(|index| (false, index)));
    let mut matches: Vec<_> = keys
        .filter(|&key| {
            if (key.0 && !options.include_directories) || (!key.0 && !options.include_files) {
                return false;
            }
            let item = entry(tree, key);
            let (logical, modified) = match item {
                TreeEntry::Directory { rollup, .. } => {
                    (rollup.logical_bytes, rollup.modified_utc_max)
                }
                TreeEntry::File(file) => (file.size_bytes, file.modified_utc()),
            };
            let modified = modified.map(crate::export::unix_seconds);
            if options.min_logical.is_some_and(|min| logical < min)
                || options.max_logical.is_some_and(|max| logical > max)
                || options
                    .min_allocated
                    .is_some_and(|min| item.physical_bytes() < min)
                || options
                    .max_allocated
                    .is_some_and(|max| item.physical_bytes() > max)
                || options
                    .modified_after
                    .is_some_and(|min| modified.is_none_or(|value| value < min))
                || options
                    .modified_before
                    .is_some_and(|max| modified.is_none_or(|value| value >= max))
                || (!extensions.is_empty()
                    && (key.0
                        || !name(item).rsplit_once('.').is_some_and(|(_, ext)| {
                            extensions.iter().any(|value| ext.to_lowercase() == *value)
                        })))
            {
                return false;
            }
            pattern.is_empty() || matches_text(name(item)) || matches_text(&path(tree, item))
        })
        .collect();
    let compare = |&left: &(bool, usize), &right: &(bool, usize)| {
        let left_entry = entry(tree, left);
        let right_entry = entry(tree, right);
        let logical = |entry| match entry {
            TreeEntry::Directory { rollup, .. } => rollup.logical_bytes,
            TreeEntry::File(file) => file.size_bytes,
        };
        let modified = |entry| match entry {
            TreeEntry::Directory { rollup, .. } => rollup.modified_utc_max,
            TreeEntry::File(file) => file.modified_utc(),
        };
        let order = match options.sort {
            SearchSort::Logical => logical(left_entry).cmp(&logical(right_entry)),
            SearchSort::Allocated => left_entry
                .physical_bytes()
                .cmp(&right_entry.physical_bytes()),
            SearchSort::Kind => left.0.cmp(&right.0),
            SearchSort::Modified => modified(left_entry).cmp(&modified(right_entry)),
            SearchSort::Name => std::cmp::Ordering::Equal,
        }
        .then_with(|| {
            name(left_entry)
                .chars()
                .flat_map(char::to_lowercase)
                .cmp(name(right_entry).chars().flat_map(char::to_lowercase))
        })
        .then_with(|| left.cmp(&right));
        if options.descending {
            order.reverse()
        } else {
            order
        }
    };
    let total = matches.len();
    let end = options.offset.saturating_add(options.limit).min(total);
    if options.limit == 0 || options.offset >= total {
        return Ok(SearchPage {
            total,
            hits: Vec::new(),
        });
    }
    // Only sort the prefix needed for this page; preserve the same total order.
    if end < total {
        matches.select_nth_unstable_by(end, compare);
        matches.truncate(end);
    }
    matches.sort_unstable_by(compare);
    let hits = matches
        .into_iter()
        .skip(options.offset)
        .take(options.limit)
        .map(|key| {
            let entry = entry(tree, key);
            SearchResult {
                path: path(tree, entry),
                entry,
            }
        })
        .collect();
    Ok(SearchPage { total, hits })
}
