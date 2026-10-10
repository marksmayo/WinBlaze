//! Allocation budgets exercise public operations without wall-clock flakiness.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
use winblaze_core::{
    DirectoryId, DirectoryRecord, FileId, FileRecord, SearchQuery, SortDirection, SortField,
};
use winblaze_index::{BufferedIndexTransaction, IndexCatalog, TreeIndex};

struct MeasuredAllocator;
thread_local! {
    static ENABLED: Cell<bool> = const { Cell::new(false) };
    static BYTES: Cell<usize> = const { Cell::new(0) };
    static LIVE: Cell<isize> = const { Cell::new(0) };
}
// SAFETY: every request is forwarded unchanged to System. The thread-local
// counters never allocate; measurement excludes fixture construction.
unsafe impl GlobalAlloc for MeasuredAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ENABLED.try_with(Cell::get).unwrap_or(false) {
            let _ = BYTES.try_with(|bytes| bytes.set(bytes.get().saturating_add(layout.size())));
            let _ = LIVE.try_with(|bytes| bytes.set(bytes.get() + layout.size() as isize));
        }
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, pointer: *mut u8, layout: Layout) {
        if ENABLED.try_with(Cell::get).unwrap_or(false) {
            let _ = LIVE.try_with(|bytes| bytes.set(bytes.get() - layout.size() as isize));
        }
        unsafe { System.dealloc(pointer, layout) }
    }
    unsafe fn realloc(&self, pointer: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        if ENABLED.try_with(Cell::get).unwrap_or(false) {
            let _ = BYTES.try_with(|bytes| bytes.set(bytes.get().saturating_add(size)));
            let _ = LIVE
                .try_with(|bytes| bytes.set(bytes.get() + size as isize - layout.size() as isize));
        }
        unsafe { System.realloc(pointer, layout, size) }
    }
}
#[global_allocator]
static ALLOCATOR: MeasuredAllocator = MeasuredAllocator;

fn measured<T>(operation: impl FnOnce() -> T) -> (T, usize) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ENABLED.set(false);
        }
    }
    BYTES.set(0);
    LIVE.set(0);
    ENABLED.set(true);
    let reset = Reset;
    let result = operation();
    drop(reset);
    (result, BYTES.get())
}

fn fixture(count: u64) -> BufferedIndexTransaction {
    let mut records = BufferedIndexTransaction::default();
    records.insert_directory(DirectoryRecord {
        id: DirectoryId(1),
        name: "root".into(),
        full_path: "C:\\root".into(),
        ..Default::default()
    });
    for id in (1..=count).rev() {
        records.insert_file(FileRecord {
            id: FileId(id),
            parent_directory_id: DirectoryId(1),
            name: format!("file-{id}.txt").into(),
            size_bytes: id,
            allocation_bytes: id,
            ..Default::default()
        });
    }
    records
}

#[test]
fn terminal_tree_publication_defers_file_display_storage() {
    let mut records = BufferedIndexTransaction::default();
    records.reserve(16_384, 1);
    records.insert_directory(DirectoryRecord {
        id: DirectoryId(1),
        name: "root".into(),
        full_path: "C:\\root".into(),
        ..Default::default()
    });
    for id in 1..=16_384 {
        records.insert_file(FileRecord {
            id: FileId(id),
            parent_directory_id: DirectoryId(1),
            name: "file-000-000-0000.bin".into(),
            allocation_bytes: id,
            ..Default::default()
        });
    }
    let (tree, bytes) = measured(|| TreeIndex::build(records));
    assert_eq!(tree.root().unwrap().1.file_count, 16_384);
    assert!(
        bytes < 4096,
        "terminal publication allocated {bytes} bytes of display storage"
    );
    let mut seen = Vec::new();
    tree.for_each_child(1, 0, 3, |entry| seen.push(entry.physical_bytes()));
    assert_eq!(seen, [16_384, 16_383, 16_382]);
    let (_, repeated_bytes) = measured(|| {
        tree.for_each_child(1, 0, 3, |_| {});
    });
    assert_eq!(repeated_bytes, 0);
}

#[test]
fn complete_csv_rows_are_encoded_with_bounded_writer_calls() {
    struct Output {
        bytes: Vec<u8>,
        calls: usize,
    }
    impl std::io::Write for Output {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.calls += 1;
            self.bytes.extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let tree = TreeIndex::build(fixture(1000));
    let mut output = Output {
        bytes: Vec::new(),
        calls: 0,
    };
    winblaze_index::export::write_csv(&tree, &mut output).unwrap();
    let text = std::str::from_utf8(&output.bytes).unwrap();
    assert_eq!(text.lines().count(), 1002);
    assert!(text.contains("file-1000.txt\",File,1000,1000,0,0,"));
    assert!(
        output.calls <= 1002,
        "CSV used {} writer calls",
        output.calls
    );
}

#[test]
fn short_file_names_are_owned_without_individual_heap_allocations() {
    let mut files = Vec::with_capacity(16_384);
    let (_, bytes) = measured(|| {
        for _ in 0..16_384 {
            files.push(FileRecord {
                name: "file-000-000-0000.bin".into(),
                ..Default::default()
            });
        }
    });
    assert!(files
        .iter()
        .all(|file| file.name.as_ref() == "file-000-000-0000.bin"));
    assert_eq!(bytes, 0, "short names allocated {bytes} bytes");
}

#[test]
fn published_file_catalog_retains_paths_and_timestamps_within_record_budget() {
    const COUNT: usize = 16_384;
    const NAME: &str = "example.txt";
    const PATH: &str = "C:\\fixture\\example.txt";
    const TIMES: [Option<i64>; 4] = [None, Some(i64::MIN), Some(0), Some(i64::MAX)];
    let (files, _) = measured(|| {
        let mut transaction = BufferedIndexTransaction::default();
        transaction.reserve(COUNT, 0);
        for id in 1..=COUNT as u64 {
            transaction.insert_file(FileRecord {
                id: FileId(id),
                name: NAME.into(),
                full_path: PATH.into(),
                timestamps: winblaze_core::FileTimestamps::new(
                    TIMES[(id as usize - 1) % 4],
                    TIMES[((id as usize - 1) / 4) % 4],
                    TIMES[((id as usize - 1) / 16) % 4],
                ),
                ..Default::default()
            });
        }
        transaction.into_record_vecs().2
    });
    assert_eq!(files.len(), COUNT);
    for (position, file) in files.iter().enumerate() {
        assert_eq!(file.id.0, position as u64 + 1);
        assert_eq!(file.name.as_ref(), NAME);
        assert_eq!(&*file.full_path, PATH);
        assert_eq!(file.created_utc(), TIMES[position % 4]);
        assert_eq!(file.modified_utc(), TIMES[(position / 4) % 4]);
        assert_eq!(file.accessed_utc(), TIMES[(position / 16) % 4]);
    }
    // Immutable names and paths need no spare-capacity words. Payload strings
    // are counted separately; every name, path and timestamp remains intact.
    let budget = COUNT * (104 + NAME.len() + PATH.len()) + 4096;
    let retained = LIVE.get();
    assert!(
        retained <= budget as isize,
        "published catalog retained {retained} bytes; budget {budget}"
    );
}

#[test]
fn building_sorted_tree_does_not_duplicate_the_record_array() {
    let records = fixture(20_000);
    let (tree, bytes) = measured(|| TreeIndex::build(records));
    assert_eq!(tree.root().unwrap().1.file_count, 20_000);
    assert_eq!(tree.root().unwrap().1.logical_bytes, 200_010_000);
    assert!(tree.files().iter().map(|file| file.id.0).eq(1..=20_000));
    assert!(
        bytes < 4_000_000,
        "tree construction allocated {bytes} bytes"
    );
}

#[test]
fn tree_construction_keeps_only_one_file_sized_child_array() {
    let count = 100_000;
    let mut records = BufferedIndexTransaction::default();
    records.reserve(count, 1);
    records.insert_directory(DirectoryRecord {
        id: DirectoryId(1),
        name: "root".into(),
        full_path: r"C:\root".into(),
        ..Default::default()
    });
    for id in 1..=count as u64 {
        records.insert_file(FileRecord {
            id: FileId(id),
            parent_directory_id: DirectoryId(1),
            name: format!("file-{id}.txt").into(),
            size_bytes: id,
            allocation_bytes: id,
            ..Default::default()
        });
    }
    let (tree, bytes) = measured(|| TreeIndex::build(records));
    assert_eq!(tree.root().unwrap().1.file_count, count as u64);
    assert_eq!(tree.root().unwrap().1.logical_bytes, 5_000_050_000);
    let mut previous = u64::MAX;
    let mut visited = 0;
    tree.for_each_child(1, 0, count, |entry| {
        assert!(entry.physical_bytes() < previous);
        previous = entry.physical_bytes();
        visited += 1;
    });
    assert_eq!(visited, count);
    assert!(
        bytes <= count * std::mem::size_of::<u32>() + 4096,
        "tree construction allocated {bytes} bytes for {count} files"
    );
}

#[test]
fn early_worker_batches_keep_dense_catalog_lookup_memory_bounded() {
    const COUNT: usize = 16_384;
    let (records, _) = measured(|| {
        let mut records = BufferedIndexTransaction::default();
        // A later worker's batch can arrive before the lower-ID batches.
        for id in (1537..=2048).chain(1..=1536).chain(2049..=COUNT as u64) {
            records.insert_file(FileRecord {
                id: FileId(id),
                size_bytes: id,
                ..Default::default()
            });
        }
        records
    });
    let retained = LIVE.get();
    let budget = COUNT * std::mem::size_of::<FileRecord>() + COUNT * 8 + 8192;
    assert!(
        retained <= budget as isize,
        "out-of-order dense catalog retained {retained} bytes; budget {budget}"
    );
    let files = records.snapshot_files();
    assert_eq!(files.len(), COUNT);
    for (position, file) in files.iter().enumerate() {
        assert_eq!(file.id.0, position as u64 + 1);
        assert_eq!(file.size_bytes, file.id.0);
    }
}

#[test]
fn large_catalog_growth_bounds_cumulative_record_allocation() {
    const COUNT: usize = 100_000;
    let (records, bytes) = measured(|| {
        let mut records = BufferedIndexTransaction::default();
        for id in 1..=COUNT as u64 {
            records.insert_file(FileRecord {
                id: FileId(id),
                size_bytes: id,
                ..Default::default()
            });
        }
        records
    });
    let budget = COUNT * std::mem::size_of::<FileRecord>() * 3;
    assert!(
        bytes <= budget,
        "catalog growth allocated {bytes} bytes cumulatively; budget {budget}"
    );
    let files = records.snapshot_files();
    assert_eq!(files.len(), COUNT);
    for (position, file) in files.iter().enumerate() {
        assert_eq!(file.id.0, position as u64 + 1);
        assert_eq!(file.size_bytes, file.id.0);
    }
}

#[test]
fn limited_search_materializes_only_returned_records() {
    let records = fixture(20_000);
    let catalog = IndexCatalog::from_transaction(&records);
    let query = SearchQuery {
        include_files: true,
        limit: Some(10),
        sort_field: SortField::SizeBytes,
        sort_direction: SortDirection::Descending,
        ..Default::default()
    };
    let (hits, bytes) = measured(|| catalog.search(&query));
    assert_eq!(hits.len(), 10);
    assert!(hits
        .iter()
        .map(|hit| hit.size_bytes)
        .eq((19_991..=20_000).rev()));
    assert!(bytes < 2_000_000, "limited search allocated {bytes} bytes");
}

#[test]
fn csv_export_streams_paths_without_per_record_allocations() {
    let tree = TreeIndex::build(fixture(20_000));
    let (result, bytes) =
        measured(|| winblaze_index::export::write_csv(&tree, &mut std::io::sink()));
    result.unwrap();
    assert!(bytes < 4096, "streaming CSV allocated {bytes} bytes");
}

#[test]
fn zero_result_limit_skips_catalog_work() {
    let catalog = IndexCatalog::from_transaction(&fixture(20_000));
    let query = SearchQuery {
        include_files: true,
        limit: Some(0),
        pattern: Some("file".into()),
        ..Default::default()
    };
    let (hits, bytes) = measured(|| catalog.search(&query));
    assert!(hits.is_empty());
    assert_eq!(bytes, 0, "zero-limit search must not allocate");
}

#[test]
fn largest_file_cache_retains_only_its_bounded_result_order() {
    let tree = TreeIndex::build(fixture(100_000));
    let mut ids = Vec::with_capacity(10);
    measured(|| tree.for_each_largest_file(10, |file| ids.push(file.id.0)));
    assert!(ids.into_iter().eq((99_991..=100_000).rev()));
    let retained = LIVE.get();
    assert!(
        retained < 70_000,
        "largest-file cache retained {retained} bytes"
    );
}

#[test]
fn limited_text_sort_avoids_keys_for_every_match_and_preserves_order() {
    let catalog = IndexCatalog::from_transaction(&fixture(20_000));
    for field in [SortField::Name, SortField::Path] {
        for direction in [SortDirection::Ascending, SortDirection::Descending] {
            let mut query = SearchQuery {
                include_files: true,
                sort_field: field,
                sort_direction: direction,
                ..Default::default()
            };
            let mut expected = catalog.search(&query);
            expected.truncate(10);
            query.limit = Some(10);
            let (hits, bytes) = measured(|| catalog.search(&query));
            assert_eq!(hits, expected);
            assert!(
                bytes < 1_500_000,
                "limited textual sort allocated {bytes} bytes"
            );
        }
    }
}

#[test]
fn search_without_selected_record_types_does_no_work() {
    let catalog = IndexCatalog::from_transaction(&fixture(20_000));
    let query = SearchQuery {
        include_files: false,
        include_directories: false,
        pattern: Some("file".into()),
        extensions: vec!["TXT".into()],
        ..Default::default()
    };
    let (hits, bytes) = measured(|| catalog.search(&query));
    assert!(hits.is_empty());
    assert_eq!(bytes, 0);
}

#[test]
fn limited_text_sort_preserves_case_ties_and_unicode() {
    let mut records = fixture(24);
    let names = [
        "a.txt",
        "A.TXT",
        "\u{00e4}.txt",
        "\u{00c4}.txt",
        "z.txt",
        "Z.TXT",
    ];
    for (i, mut file) in records.snapshot_files().into_iter().enumerate() {
        file.name = names[i % names.len()].into();
        file.full_path = format!("C:\\root\\{}", file.name).into();
        records.insert_file(file);
    }
    let catalog = IndexCatalog::from_transaction(&records);
    for field in [SortField::Name, SortField::Path] {
        for direction in [SortDirection::Ascending, SortDirection::Descending] {
            let mut query = SearchQuery {
                include_files: true,
                include_directories: true,
                sort_field: field,
                sort_direction: direction,
                ..Default::default()
            };
            let all = catalog.search(&query);
            for limit in [1, 3, 10, 24] {
                query.limit = Some(limit);
                assert_eq!(catalog.search(&query), all[..limit].to_vec());
            }
        }
    }
}
#[test]
fn json_export_streams_reconstructed_paths_without_per_file_allocations() {
    let tree = TreeIndex::build(fixture(20_000));
    let (result, bytes) =
        measured(|| winblaze_index::export::write_json(&tree, &mut std::io::sink()));
    result.unwrap();
    assert!(bytes < 4096, "streaming JSON allocated {bytes} bytes");
}
#[test]
fn ascii_filtered_export_has_no_per_file_matching_allocations() {
    let tree = TreeIndex::build(fixture(20_000));
    let filter = winblaze_index::export::ExportFilter {
        patterns: vec!["*.TXT".into(), "*.LOG".into()],
        ..Default::default()
    };
    let (result, bytes) = measured(|| {
        winblaze_index::export::write_csv_filtered(&tree, &filter, &mut std::io::sink())
    });
    result.unwrap();
    assert!(bytes < 4096, "filtered CSV allocated {bytes} bytes");
}
#[test]
fn tree_build_uses_compact_parent_indices_and_reuses_child_cursors() {
    let records = fixture(20_000);
    let (tree, bytes) = measured(|| TreeIndex::build(records));
    assert_eq!(tree.root().unwrap().1.file_count, 20_000);
    assert!(bytes < 3_210_000, "tree build allocated {bytes} bytes");
}
#[test]
fn csv_import_avoids_repeated_ancestor_and_record_copies() {
    let mut csv = String::from(
        "Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\r\n",
    );
    for id in 0..5000 {
        use std::fmt::Write;
        writeln!(
            csv,
            "\"C:\\root\\nested\\very-long-folder-name\\more\\file-{id:05}.txt\",File,1,1,0,0,"
        )
        .unwrap();
    }
    let (result, bytes) = measured(|| winblaze_index::import::read_csv(csv.as_bytes()));
    let tree = result.unwrap();
    assert_eq!(tree.files().len(), 5000);
    assert_eq!(tree.root().unwrap().1.logical_bytes, 5000);
    assert!(bytes < 5_200_000, "CSV import allocated {bytes} bytes");
}
#[test]
fn no_match_search_borrows_filters_and_allocates_no_result_storage() {
    let catalog = IndexCatalog::from_transaction(&fixture(20_000));
    let query = SearchQuery {
        include_files: true,
        pattern: Some("no-such-pattern".into()),
        extensions: vec![" .TXT ".into()],
        ..Default::default()
    };
    let (hits, bytes) = measured(|| catalog.search(&query));
    assert!(hits.is_empty());
    assert_eq!(bytes, 0, "empty search allocated {bytes} bytes");
}
#[test]
fn foreign_csv_import_moves_columns_and_uses_fixed_date_fields() {
    let mut csv = String::from(
        "Name,Files,Folders,Logical Size,Physical Size,Attributes,Last Change,Flags,Level\r\n",
    );
    for id in 0..2000 {
        use std::fmt::Write;
        writeln!(
            csv,
            "C:\\root\\file-{id:05}.txt,0,0,1,1,32,2026-01-01T00:00:00Z,0x0,1"
        )
        .unwrap();
    }
    let (result, bytes) = measured(|| winblaze_index::import::read_csv(csv.as_bytes()));
    let tree = result.unwrap();
    assert_eq!(tree.files().len(), 2000);
    assert!(tree.files().iter().all(|f| f.modified_utc().is_some()));
    assert!(bytes < 2_700_000, "foreign CSV allocated {bytes} bytes");
}
#[test]
fn derived_paths_allocate_the_exact_required_capacity() {
    let parent = "p".repeat(100);
    let name = "n".repeat(100);
    let (path, bytes) = measured(|| winblaze_core::join_path(&parent, &name));
    assert_eq!(path, format!("{parent}\\{name}"));
    assert_eq!(bytes, 201, "path joining allocated {bytes} bytes");
}
#[test]
fn directory_tree_reuses_count_arrays_as_fill_cursors() {
    let mut records = BufferedIndexTransaction::default();
    for id in 1..=5000 {
        records.insert_directory(DirectoryRecord {
            id: DirectoryId(id),
            parent_directory_id: (id > 1).then_some(DirectoryId(1)),
            name: id.to_string(),
            ..Default::default()
        });
    }
    let (tree, bytes) = measured(|| TreeIndex::build(records));
    assert_eq!(tree.child_count(1), Some(4999));
    assert!(bytes < 1_200_000, "directory tree allocated {bytes} bytes");
}
#[test]
fn identical_unique_file_snapshots_skip_diff_lookup_maps() {
    let records = fixture(20_000).snapshot_files();
    let (changes, bytes) = measured(|| winblaze_core::diff_file_records(&records, &records));
    assert!(changes.changes.is_empty());
    assert_eq!(bytes, 0, "unchanged diff allocated {bytes} bytes");
}
#[test]
fn unchanged_incremental_apply_keeps_history_without_lookup_maps() {
    let mut transaction = fixture(20_000);
    let records = transaction.snapshot_files();
    let (changes, bytes) = measured(|| transaction.apply_incremental_files(&records, &records));
    assert!(changes.changes.is_empty());
    assert_eq!(transaction.file_change_sets().len(), 1);
    assert_eq!(transaction.snapshot_files(), records);
    assert!(bytes < 4096, "unchanged apply allocated {bytes} bytes");
}
#[test]
fn unicode_filtered_export_compiles_patterns_once_and_folds_names_once() {
    let mut records = fixture(20_000);
    for mut file in records.snapshot_files() {
        file.name = format!("Ω-{}.txt", file.id.0).into();
        records.insert_file(file);
    }
    let tree = TreeIndex::build(records);
    let filter = winblaze_index::export::ExportFilter {
        patterns: vec!["*.LOG".into(), "Ω-*.TXT".into()],
        ..Default::default()
    };
    let (result, bytes) = measured(|| {
        winblaze_index::export::write_csv_filtered(&tree, &filter, &mut std::io::sink())
    });
    result.unwrap();
    assert!(
        bytes < 2_300_000,
        "Unicode filtered export allocated {bytes} bytes"
    );
}
#[test]
fn impossible_search_bounds_skip_even_filter_preparation() {
    let catalog = IndexCatalog::from_transaction(&fixture(20_000));
    let mut query = SearchQuery {
        include_files: true,
        extensions: (0..12).map(|id| format!("ext{id}")).collect(),
        ..Default::default()
    };
    query.size.min_bytes = Some(10);
    query.size.max_bytes = Some(1);
    let (hits, bytes) = measured(|| catalog.search(&query));
    assert!(hits.is_empty());
    assert_eq!(bytes, 0);
    query.size = Default::default();
    query.modified.modified_after_utc = Some(10);
    query.modified.modified_before_utc = Some(10);
    let (hits, bytes) = measured(|| catalog.search(&query));
    assert!(hits.is_empty());
    assert_eq!(bytes, 0);
}
#[test]
fn long_extension_lists_preserve_matching_and_empty_extension_rules() {
    let catalog = IndexCatalog::from_transaction(&fixture(20_000));
    let mut query = SearchQuery {
        include_files: true,
        limit: Some(1),
        sort_field: SortField::SizeBytes,
        sort_direction: SortDirection::Descending,
        extensions: (0..12).map(|id| format!("unused{id}")).collect(),
        ..Default::default()
    };
    query.extensions.extend([" . ".into(), " .TXT ".into()]);
    let hits = catalog.search(&query);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].size_bytes, 20_000);
}

#[test]
fn duplicate_ids_retain_diff_semantics_instead_of_using_unchanged_fast_path() {
    let mut first = FileRecord {
        id: FileId(1),
        name: "first".into(),
        ..Default::default()
    };
    let second = FileRecord {
        name: "second".into(),
        ..first.clone()
    };
    first.full_path = "first".into();
    let records = [first, second];
    let changes = winblaze_core::diff_file_records(&records, &records);
    assert_eq!(changes.changes.len(), 1);
    assert_eq!(
        changes.changes[0].kind,
        winblaze_core::FileChangeKind::Moved
    );
}

#[test]
fn overwrite_source_path_checks_do_not_materialize_every_catalog_path() {
    let tree = TreeIndex::build(fixture(20_000));
    let (found, bytes) = measured(|| tree.contains_file_display_path("C:\\exports\\output.csv"));
    assert!(!found);
    assert!(bytes < 4096, "source guard allocated {bytes} bytes");
    assert!(tree.contains_file_display_path("c:\\ROOT\\FILE-100.TXT"));
    assert!(tree.contains_file_display_path(r"\\?\C:\root\file-100.txt"));
}

#[test]
fn source_path_matching_preserves_legacy_unicode_and_prefix_rules() {
    for parent in [
        "",
        r"C:\root",
        r"C:\root\",
        r"\\?\",
        r"\\?\C:\root",
        "C:\\\u{130}\\\u{3a3}",
    ] {
        for name in ["file.txt", "FILE.TXT", r"\\?\file", "\u{130}\u{3a3}.txt"] {
            for stored in ["", r"\\?\C:\root\file.txt", "C:\\\u{130}\\\u{3a3}.txt"] {
                let mut records = BufferedIndexTransaction::default();
                records.insert_directory(DirectoryRecord {
                    id: DirectoryId(1),
                    full_path: parent.into(),
                    ..Default::default()
                });
                records.insert_file(FileRecord {
                    id: FileId(1),
                    parent_directory_id: DirectoryId(1),
                    name: name.into(),
                    full_path: stored.into(),
                    ..Default::default()
                });
                let tree = TreeIndex::build(records);
                let display = tree.file_display_path(&tree.files()[0]);
                for target in [
                    display.clone(),
                    display.to_lowercase(),
                    display.to_uppercase(),
                    format!(r"\\?\{display}"),
                    format!("{display}a"),
                    r"C:\missing".into(),
                ] {
                    let expected = display.trim_start_matches(r"\\?\").to_lowercase()
                        == target.trim_start_matches(r"\\?\").to_lowercase();
                    assert_eq!(
                        tree.contains_file_display_path(&target),
                        expected,
                        "{parent:?} {name:?} {stored:?} {target:?}"
                    );
                }
            }
        }
    }
}
