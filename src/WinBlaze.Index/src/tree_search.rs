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
        TreeEntry::Directory { record, .. } => tree.directory_path(record).into_owned(),
        TreeEntry::File(file) => tree.file_display_path(file),
    }
}
#[cfg(test)]
thread_local! {
    /// Derived file paths built for matching, so tests can pin the cost.
    static DERIVED_FILE_PATHS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}
/// `path` for matching: stored paths are borrowed and derived file paths are
/// built in `scratch`, so filtering allocates nothing per candidate.
fn path_text<'s>(tree: &'s TreeIndex, entry: TreeEntry<'s>, scratch: &'s mut String) -> &'s str {
    match entry {
        TreeEntry::Directory { record, .. } if !record.full_path.is_empty() => &record.full_path,
        TreeEntry::Directory { record, .. } => {
            scratch.clear();
            tree.push_directory_path(record, scratch);
            scratch
        }
        TreeEntry::File(file) if !file.full_path.is_empty() => &file.full_path,
        TreeEntry::File(file) => {
            #[cfg(test)]
            DERIVED_FILE_PATHS.with(|count| count.set(count.get() + 1));
            scratch.clear();
            // Same join as TreeIndex::file_display_path, into the reused buffer.
            match tree.directory_full_path(file.parent_directory_id.0) {
                Some(parent) => winblaze_core::push_join_path(scratch, &parent, &file.name),
                None => scratch.push_str(&file.name),
            }
            scratch
        }
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
/// Pages ending within this many results are selected in one streaming pass.
const STREAMED_PAGE_LIMIT: usize = 1024;

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
        if text.is_ascii() && pattern.is_ascii() {
            // ASCII lowercasing is byte-wise, so this matches the general path
            // below without allocating a lowercase copy per candidate.
            let (text, pattern) = (text.as_bytes(), pattern.as_bytes());
            return match options.mode {
                TextMode::Exact => text.eq_ignore_ascii_case(pattern),
                TextMode::Prefix => text
                    .get(..pattern.len())
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case(pattern)),
                _ => {
                    pattern.is_empty()
                        || text
                            .windows(pattern.len())
                            .any(|window| window.eq_ignore_ascii_case(pattern))
                }
            };
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
    let mut scratch = String::new();
    // A derived file path is `parent + '\' + name` (no separator when the
    // parent is empty or already ends in one). A Contains pattern without `\`
    // cannot match across that separator, nor can lowercasing create one, so
    // the path matches exactly when the name or the parent's path does. Each
    // directory's answer is computed once instead of rebuilding and rescanning
    // the parent prefix for every file under it.
    let per_parent =
        regex.is_none() && options.mode == TextMode::Contains && !pattern.contains('\\');
    // 0 = not computed yet, 1 = no match, 2 = match.
    let mut parent_matches: Vec<u8> = Vec::new();
    let mut matches_filters = |key: (bool, usize)| {
        if (key.0 && !options.include_directories) || (!key.0 && !options.include_files) {
            return false;
        }
        let item = entry(tree, key);
        let (logical, modified) = match item {
            TreeEntry::Directory { rollup, .. } => (rollup.logical_bytes, rollup.modified_utc_max),
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
                        extensions.iter().any(|value| {
                            if ext.is_ascii() {
                                ext.eq_ignore_ascii_case(value)
                            } else {
                                ext.to_lowercase() == *value
                            }
                        })
                    })))
        {
            return false;
        }
        if pattern.is_empty() || matches_text(name(item)) {
            return true;
        }
        match item {
            TreeEntry::File(file) if per_parent && file.full_path.is_empty() => {
                // Unresolved parent: the display path is the bare name.
                let Some(index) = tree.directory_index(file.parent_directory_id.0) else {
                    return false;
                };
                if parent_matches.is_empty() {
                    parent_matches = vec![0; tree.directories().len()];
                }
                if parent_matches[index] == 0 {
                    let parent = tree.directory_path(&tree.directories()[index]);
                    parent_matches[index] = if matches_text(&parent) { 2 } else { 1 };
                }
                parent_matches[index] == 2
            }
            _ => matches_text(path_text(tree, item, &mut scratch)),
        }
    };
    let key_name = |key: (bool, usize)| -> &str {
        if key.0 {
            &tree.directories()[key.1].name
        } else {
            &tree.files()[key.1].name
        }
    };
    let compare = |&left: &(bool, usize), &right: &(bool, usize)| {
        let logical = |entry| match entry {
            TreeEntry::Directory { rollup, .. } => rollup.logical_bytes,
            TreeEntry::File(file) => file.size_bytes,
        };
        let modified = |entry| match entry {
            TreeEntry::Directory { rollup, .. } => rollup.modified_utc_max,
            TreeEntry::File(file) => file.modified_utc(),
        };
        // Name order (the default) needs no rollup lookup per comparison.
        let order = match options.sort {
            SearchSort::Name => std::cmp::Ordering::Equal,
            SearchSort::Kind => left.0.cmp(&right.0),
            sort => {
                let (left_entry, right_entry) = (entry(tree, left), entry(tree, right));
                match sort {
                    SearchSort::Logical => logical(left_entry).cmp(&logical(right_entry)),
                    SearchSort::Allocated => left_entry
                        .physical_bytes()
                        .cmp(&right_entry.physical_bytes()),
                    _ => modified(left_entry).cmp(&modified(right_entry)),
                }
            }
        }
        .then_with(|| cmp_lowercase(key_name(left), key_name(right)))
        .then_with(|| left.cmp(&right));
        if options.descending {
            order.reverse()
        } else {
            order
        }
    };
    let wanted = options.offset.saturating_add(options.limit);
    if options.limit != 0 && wanted <= STREAMED_PAGE_LIMIT {
        // A small page (the post-scan preview asks for 50 of every entry):
        // keep only the best `wanted` keys while counting matches, so most
        // candidates cost one comparison and no key vector is collected.
        // The order is total (ties break on the key), so this equals sorting
        // every match and slicing.
        let mut total = 0;
        let mut best: Vec<(bool, usize)> = Vec::with_capacity(wanted.min(4096) + 1);
        for key in keys {
            if !matches_filters(key) {
                continue;
            }
            total += 1;
            if best.len() == wanted && compare(&key, &best[wanted - 1]).is_ge() {
                continue;
            }
            let position = best
                .binary_search_by(|kept| compare(kept, &key))
                .unwrap_or_else(|position| position);
            best.insert(position, key);
            best.truncate(wanted);
        }
        let hits = best
            .into_iter()
            .skip(options.offset)
            .map(|key| {
                let entry = entry(tree, key);
                SearchResult {
                    path: path(tree, entry),
                    entry,
                }
            })
            .collect();
        return Ok(SearchPage { total, hits });
    }
    let mut matches: Vec<_> = keys.filter(|&key| matches_filters(key)).collect();
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

/// Compares the `char::to_lowercase` sequences of two names; ASCII names
/// compare byte-wise, which orders identically.
fn cmp_lowercase(left: &str, right: &str) -> std::cmp::Ordering {
    if left.is_ascii() && right.is_ascii() {
        left.bytes()
            .map(|byte| byte.to_ascii_lowercase())
            .cmp(right.bytes().map(|byte| byte.to_ascii_lowercase()))
    } else {
        left.chars()
            .flat_map(char::to_lowercase)
            .cmp(right.chars().flat_map(char::to_lowercase))
    }
}

#[cfg(test)]
mod tests {
    use super::{search, SearchOptions, TextMode};
    use crate::{BufferedIndexTransaction, TreeEntry, TreeIndex};
    use winblaze_core::{DirectoryId, DirectoryRecord, FileId, FileRecord};

    fn directory(id: u64, parent: Option<u64>, name: &str, full_path: &str) -> DirectoryRecord {
        DirectoryRecord {
            id: DirectoryId(id),
            parent_directory_id: parent.map(DirectoryId),
            name: name.into(),
            full_path: full_path.into(),
        }
    }

    fn file(id: u64, parent: u64, name: &str, full_path: &str) -> FileRecord {
        FileRecord {
            id: FileId(id),
            parent_directory_id: DirectoryId(parent),
            name: name.into(),
            full_path: full_path.into(),
            size_bytes: id,
            allocation_bytes: id * 4096,
            ..Default::default()
        }
    }

    /// A tree that exercises every path shape the search can see: a root
    /// ending in `\`, stored and derived directory paths, a parent with no
    /// trailing separator, files with stored paths, files whose parent is
    /// missing, and Unicode names whose lowercase form changes length or
    /// depends on context (final sigma, dotted capital I, Kelvin sign).
    fn tricky_tree() -> TreeIndex {
        let mut tx = BufferedIndexTransaction::default();
        tx.insert_directory(directory(5, None, r"C:\", r"C:\"));
        tx.insert_directory(directory(10, Some(5), "Users", ""));
        tx.insert_directory(directory(11, Some(10), "ΟΔΟΣ", ""));
        tx.insert_directory(directory(12, Some(11), "İstanbul", ""));
        tx.insert_directory(directory(13, Some(10), "\u{212A}elvin Docs", ""));
        tx.insert_directory(directory(14, Some(5), "Stored", r"C:\Stored"));
        tx.insert_directory(directory(15, Some(14), "deep.folder", ""));
        tx.insert_directory(directory(16, None, "Z:", "Z:"));
        tx.insert_directory(directory(17, Some(16), "net", ""));
        let names = [
            "report.TXT",
            "Σ.dat",
            "aΣ",
            "ΣΑΣ.bin",
            "kelvin.log",
            "users-notes.md",
            "istanbul.jpg",
            "İ.png",
            "docs",
            "C.txt",
            "plain",
            "émile.doc",
            "folder.txt",
            "a filename longer than the inline capacity.bin",
        ];
        let parents = [5, 10, 11, 12, 13, 14, 15, 16, 17];
        let mut id = 100;
        for parent in parents {
            for name in names {
                tx.insert_file(file(id, parent, name, ""));
                id += 1;
            }
        }
        // Stored paths (older snapshots) and a parent that never resolved.
        tx.insert_file(file(id, 14, "kept.txt", r"C:\Stored\elsewhere\kept.txt"));
        tx.insert_file(file(id + 1, 999, "orphan Users.txt", ""));
        tx.insert_file(file(id + 2, 999, "orphan.txt", ""));
        TreeIndex::build(tx)
    }

    /// The original matcher: lowercase name or full display path contains
    /// the lowercase pattern. The search must agree with it exactly.
    fn reference_contains(tree: &TreeIndex, pattern: &str) -> Vec<String> {
        let pattern = pattern.to_lowercase();
        let mut hits = Vec::new();
        let mut consider = |entry: TreeEntry<'_>| {
            let path = super::path(tree, entry);
            if pattern.is_empty()
                || super::name(entry).to_lowercase().contains(&pattern)
                || path.to_lowercase().contains(&pattern)
            {
                hits.push(path);
            }
        };
        for index in 0..tree.directories().len() {
            consider(super::entry(tree, (true, index)));
        }
        for index in 0..tree.files().len() {
            consider(super::entry(tree, (false, index)));
        }
        hits.sort();
        hits
    }

    fn search_paths(tree: &TreeIndex, pattern: &str, mode: TextMode) -> (usize, Vec<String>) {
        let page = search(
            tree,
            &SearchOptions {
                pattern: pattern.into(),
                mode,
                limit: usize::MAX,
                ..Default::default()
            },
        )
        .unwrap();
        let mut paths: Vec<_> = page.hits.into_iter().map(|hit| hit.path).collect();
        paths.sort();
        (page.total, paths)
    }

    const PATTERNS: &[&str] = &[
        "",
        "c",
        "C:",
        "users",
        "USERS",
        "ers",
        "s\\",
        "\\",
        "c:\\",
        "users\\όδ",
        "όδος",
        "οδος",
        "οδοσ",
        "ς",
        "σ",
        "σ.dat",
        "aς",
        "ς\\",
        "i̇stanbul",
        "istanbul",
        "kelvin",
        "\u{212A}",
        "k",
        "docs",
        "deep",
        ".folder",
        "folder",
        "stored",
        "elsewhere",
        "z:",
        "net",
        "orphan",
        "é",
        "inline capacity",
        "txt",
        "missing",
        "Users\\ΟΔΟΣ\\İstanbul",
    ];

    #[test]
    fn contains_search_matches_the_full_path_reference_for_every_shape() {
        let tree = tricky_tree();
        for pattern in PATTERNS {
            let expected = reference_contains(&tree, pattern);
            let (total, got) = search_paths(&tree, pattern, TextMode::Contains);
            assert_eq!(got, expected, "pattern {pattern:?}");
            assert_eq!(total, expected.len(), "pattern {pattern:?}");
        }
    }

    #[test]
    fn contains_search_matches_the_reference_on_seeded_random_trees() {
        // Pieces chosen for case folding and separator edge cases.
        const PIECES: &[&str] = &[
            "a", "A", "Σ", "σ", "ς", "İ", "i", "\u{307}", "K", "\u{212A}", "k", ".", " ", "é", "É",
            "ß", "x", "\\", ":",
        ];
        struct Rng(u64);
        impl Rng {
            fn below(&mut self, bound: usize) -> usize {
                self.0 ^= self.0 << 13;
                self.0 ^= self.0 >> 7;
                self.0 ^= self.0 << 17;
                (self.0 % bound as u64) as usize
            }
            fn text(&mut self, max: usize) -> String {
                let length = 1 + self.below(max);
                (0..length)
                    .map(|_| PIECES[self.below(PIECES.len())])
                    .collect()
            }
        }
        let mut rng = Rng(0x2545_F491_4F6C_DD1D);
        for _round in 0..40 {
            let mut tx = BufferedIndexTransaction::default();
            tx.insert_directory(directory(5, None, r"C:\", r"C:\"));
            let mut directory_ids = vec![5u64];
            for id in 10..30u64 {
                let parent = directory_ids[rng.below(directory_ids.len())];
                let name = rng.text(4).replace('\\', "_");
                tx.insert_directory(directory(id, Some(parent), &name, ""));
                directory_ids.push(id);
            }
            for id in 100..300u64 {
                let parent = if rng.below(20) == 0 {
                    9999
                } else {
                    directory_ids[rng.below(directory_ids.len())]
                };
                let name = rng.text(5);
                tx.insert_file(file(id, parent, &name, ""));
            }
            let tree = TreeIndex::build(tx);
            for _ in 0..25 {
                let pattern = rng.text(3).replace('\\', "");
                let expected = reference_contains(&tree, &pattern);
                let (total, got) = search_paths(&tree, &pattern, TextMode::Contains);
                assert_eq!(got, expected, "pattern {pattern:?}");
                assert_eq!(total, expected.len());
            }
        }
    }

    #[test]
    fn exact_prefix_and_regex_modes_still_check_the_full_path() {
        let tree = tricky_tree();
        let (_, exact) = search_paths(&tree, r"c:\users\docs", TextMode::Exact);
        assert_eq!(exact, vec![r"C:\Users\docs".to_string()]);
        let (_, prefix) = search_paths(&tree, r"c:\stored\deep", TextMode::Prefix);
        assert_eq!(prefix.len(), 1 + 14, "{prefix:?}");
        let (_, regex) = search_paths(&tree, r"users\\ΟΔΟΣ\\[^\\]+\.dat$", TextMode::Regex);
        assert_eq!(regex, vec![r"C:\Users\ΟΔΟΣ\Σ.dat".to_string()]);
        let (_, prefix_name) = search_paths(&tree, "report", TextMode::Prefix);
        assert_eq!(prefix_name.len(), 9);
    }

    #[test]
    fn contains_search_checks_each_parent_path_once_instead_of_per_file() {
        let mut tx = BufferedIndexTransaction::default();
        tx.insert_directory(directory(5, None, r"C:\", r"C:\"));
        tx.insert_directory(directory(10, Some(5), "data", ""));
        for id in 0..2000 {
            tx.insert_file(file(100 + id, 10, &format!("item{id:04}.bin"), ""));
        }
        let tree = TreeIndex::build(tx);
        super::DERIVED_FILE_PATHS.with(|count| count.set(0));
        let (total, _) = search_paths(&tree, "nothing-matches", TextMode::Contains);
        assert_eq!(total, 0);
        let built = super::DERIVED_FILE_PATHS.with(|count| count.get());
        assert_eq!(built, 0, "file paths were materialized per candidate");
        let (total, _) = search_paths(&tree, "DATA", TextMode::Contains);
        assert_eq!(total, 2001);
        // A pattern spanning a separator still needs the full path.
        let (total, _) = search_paths(&tree, r"data\item0001", TextMode::Contains);
        assert_eq!(total, 1);
    }

    #[test]
    fn ascii_fast_path_orders_like_unicode_lowercase() {
        let names = [
            "",
            "a",
            "A",
            "ab",
            "aB",
            "Ab",
            "b",
            "Z",
            "_x",
            "a.txt",
            "A.TXT",
            "readme",
            "\u{212A}elvin",
            "kelvin",
            "\u{c9}t\u{e9}",
            "\u{e9}t\u{e9}",
            "zz\u{e9}",
            "Zz",
        ];
        for left in names {
            for right in names {
                let unicode = left
                    .chars()
                    .flat_map(char::to_lowercase)
                    .cmp(right.chars().flat_map(char::to_lowercase));
                assert_eq!(
                    super::cmp_lowercase(left, right),
                    unicode,
                    "{left:?} vs {right:?}"
                );
            }
        }
    }
}
