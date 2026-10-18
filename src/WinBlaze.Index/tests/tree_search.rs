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

#[test]
fn paged_search_matches_slices_of_the_full_order_for_every_sort() {
    use winblaze_core::{DirectoryId, DirectoryRecord, FileId, FileRecord, FileTimestamps};
    use winblaze_index::tree_search::SearchSort;
    let mut tx = winblaze_index::BufferedIndexTransaction::default();
    tx.insert_directory(DirectoryRecord {
        id: DirectoryId(1),
        name: "root".into(),
        full_path: "Z:/root".into(),
        ..Default::default()
    });
    for d in 2..=40u64 {
        tx.insert_directory(DirectoryRecord {
            id: DirectoryId(d),
            parent_directory_id: Some(DirectoryId(1 + d % 3)),
            name: ["Docs", "docs", "Ünï", "b", "B"][(d % 5) as usize].to_string(),
            full_path: format!("Z:/root/d{d}"),
            ..Default::default()
        });
    }
    // Repeated names, sizes and times force the tie-break on every sort.
    let names = [
        "a.TXT",
        "A.txt",
        "b.bin",
        "ß.dat",
        "z",
        "Z",
        "émile.doc",
        "a.txt",
    ];
    for id in 1..=3000u64 {
        tx.insert_file(FileRecord {
            id: FileId(id),
            parent_directory_id: DirectoryId(2 + id % 39),
            name: names[(id * 7 % 8) as usize].into(),
            size_bytes: id % 13,
            allocation_bytes: (id % 5) * 4096,
            timestamps: FileTimestamps::new(None, (id % 4 != 0).then_some((id % 9) as i64), None),
            ..Default::default()
        });
    }
    let tree = winblaze_index::TreeIndex::build(tx);
    let key = |hit: &winblaze_index::tree_search::SearchResult| hit.path.clone();
    for sort in [
        SearchSort::Name,
        SearchSort::Logical,
        SearchSort::Allocated,
        SearchSort::Kind,
        SearchSort::Modified,
    ] {
        for descending in [false, true] {
            let options = SearchOptions {
                sort,
                descending,
                include_directories: true,
                include_files: true,
                offset: 0,
                limit: usize::MAX,
                ..Default::default()
            };
            let full = search(&tree, &options).unwrap();
            let full: Vec<_> = full
                .hits
                .iter()
                .map(|hit| (key(hit), hit.entry.physical_bytes()))
                .collect();
            assert_eq!(full.len(), 3040);
            for (offset, limit) in [
                (0, 1),
                (0, 50),
                (7, 50),
                (0, 1024),
                (1000, 24),
                (3031, 50),
                (0, 3040),
            ] {
                let page = search(
                    &tree,
                    &SearchOptions {
                        offset,
                        limit,
                        ..options.clone()
                    },
                )
                .unwrap();
                assert_eq!(page.total, 3040);
                let got: Vec<_> = page
                    .hits
                    .iter()
                    .map(|hit| (key(hit), hit.entry.physical_bytes()))
                    .collect();
                let end = (offset + limit).min(full.len());
                assert_eq!(
                    got,
                    full[offset.min(end)..end],
                    "{sort:?} desc={descending} {offset}+{limit}"
                );
            }
        }
    }
}
