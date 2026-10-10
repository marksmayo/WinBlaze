use winblaze_index::{
    import::read_csv,
    tree_search::{search, SearchOptions},
};
#[test]
fn searches_beyond_the_preview_cap_and_pages_all_matches() {
    let mut csv = String::from(
        "Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\n",
    );
    for ordinal in 0..9000 {
        csv.push_str(&format!(
            "Z:\\offline\\file{ordinal:05}.bin,File,7,4096,0,0,1767225600\n"
        ));
    }
    let tree = read_csv(csv.as_bytes()).unwrap();
    let query = SearchOptions {
        pattern: "file08999".into(),
        ..Default::default()
    };
    let page = search(&tree, &query).unwrap();
    assert_eq!(page.total, 1);
    assert!(page.hits[0].path.ends_with("file08999.bin"));
    let query = SearchOptions {
        offset: 8998,
        limit: 2,
        ..Default::default()
    };
    let page = search(&tree, &query).unwrap();
    assert_eq!(page.total, 9001);
    assert_eq!(page.hits.len(), 2);
    assert_ne!(page.hits[0].path, page.hits[1].path);
}

#[test]
fn regex_search_matches_unicode_names_and_reports_invalid_patterns() {
    let csv = "Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\nZ:\\offline\\日本.BIN,File,7,4096,0,0,\nZ:\\offline\\other.txt,File,8,8192,0,0,\n";
    let tree = read_csv(csv.as_bytes()).unwrap();
    let query = SearchOptions {
        pattern: r"日本\.(bin|txt)$".into(),
        mode: winblaze_index::tree_search::TextMode::Regex,
        ..Default::default()
    };
    let page = search(&tree, &query).unwrap();
    assert_eq!(page.total, 1);
    assert!(page.hits[0].path.ends_with("日本.BIN"));
    let invalid = SearchOptions {
        pattern: "[".into(),
        ..query
    };
    assert_eq!(
        search(&tree, &invalid).unwrap_err().kind(),
        std::io::ErrorKind::InvalidInput
    );
}

#[test]
fn combines_logical_allocated_extension_and_utc_bounds_without_counting_unknown_dates() {
    let csv = "Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\nZ:\\offline\\match.bin,File,7,4096,0,0,1767225600\nZ:\\offline\\too_allocated.bin,File,7,8192,0,0,1767225600\nZ:\\offline\\unknown.bin,File,7,4096,0,0,\nZ:\\offline\\other.txt,File,7,4096,0,0,1767225600\n";
    let tree = read_csv(csv.as_bytes()).unwrap();
    let query = SearchOptions {
        min_logical: Some(7),
        max_logical: Some(7),
        min_allocated: Some(4096),
        max_allocated: Some(4096),
        extensions: vec!["*.BIN".into()],
        modified_after: Some(1767225600),
        modified_before: Some(1767312000),
        include_directories: false,
        ..Default::default()
    };
    let page = search(&tree, &query).unwrap();
    assert_eq!(page.total, 1);
    assert!(page.hits[0].path.ends_with("match.bin"));
    let reversed = SearchOptions {
        min_allocated: Some(8192),
        ..query
    };
    assert_eq!(
        search(&tree, &reversed).unwrap_err().kind(),
        std::io::ErrorKind::InvalidInput
    );
}

#[test]
fn sorts_complete_matches_by_allocated_size_with_stable_pages_in_both_directions() {
    let csv = "Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\nZ:\\offline\\a.bin,File,7,4096,0,0,\nZ:\\offline\\b.bin,File,8,8192,0,0,\n";
    let tree = read_csv(csv.as_bytes()).unwrap();
    let query = SearchOptions {
        include_directories: false,
        sort: winblaze_index::tree_search::SearchSort::Allocated,
        descending: true,
        limit: 1,
        ..Default::default()
    };
    assert!(search(&tree, &query).unwrap().hits[0]
        .path
        .ends_with("b.bin"));
    assert!(search(
        &tree,
        &SearchOptions {
            offset: 1,
            ..query.clone()
        }
    )
    .unwrap()
    .hits[0]
        .path
        .ends_with("a.bin"));
    assert!(search(
        &tree,
        &SearchOptions {
            descending: false,
            ..query
        }
    )
    .unwrap()
    .hits[0]
        .path
        .ends_with("a.bin"));
}
