//! CodSpeed/divan benchmarks for the platform-independent index and core
//! rollup paths: tree build, catalog search, complete-index search, largest
//! files, CSV/JSON export and import, directory aggregation and record diffs.
use divan::{black_box, Bencher};
use winblaze_core::{
    aggregate_directory_records, diff_file_records, DirectoryId, DirectoryRecord, FileId,
    FileRecord, FileTimestamps, SearchQuery, SortDirection, SortField,
};
use winblaze_index::{
    export::{write_csv, write_json},
    import::{read_csv, read_json},
    tree_search::{search, SearchOptions, SearchSort, TextMode},
    BufferedIndexTransaction, IndexCatalog, TreeIndex,
};

fn main() {
    divan::main();
}

const SIZES: &[u64] = &[1_000, 10_000];
const EXTENSIONS: &[&str] = &["txt", "bin", "dll", "exe", "jpg", "log", "rs", "json"];
const DIRECTORY_FANOUT: u64 = 8;

/// A deterministic two-level tree: one root, `DIRECTORY_FANOUT` top-level
/// directories each holding `DIRECTORY_FANOUT` subdirectories, with files
/// spread across every directory.
fn fixture_records(file_count: u64) -> (Vec<DirectoryRecord>, Vec<FileRecord>) {
    let mut directories = vec![DirectoryRecord {
        id: DirectoryId(1),
        name: "fixture".into(),
        full_path: "C:\\fixture".into(),
        ..Default::default()
    }];
    let mut next_id = 2;
    for top in 0..DIRECTORY_FANOUT {
        let top_id = next_id;
        next_id += 1;
        let top_path = format!("C:\\fixture\\dir-{top}");
        directories.push(DirectoryRecord {
            id: DirectoryId(top_id),
            parent_directory_id: Some(DirectoryId(1)),
            name: format!("dir-{top}"),
            full_path: top_path.clone(),
            ..Default::default()
        });
        for sub in 0..DIRECTORY_FANOUT {
            directories.push(DirectoryRecord {
                id: DirectoryId(next_id),
                parent_directory_id: Some(DirectoryId(top_id)),
                name: format!("sub-{sub}"),
                full_path: format!("{top_path}\\sub-{sub}"),
                ..Default::default()
            });
            next_id += 1;
        }
    }
    let directory_count = directories.len() as u64;
    let files = (1..=file_count)
        .map(|id| {
            let extension = EXTENSIONS[(id % EXTENSIONS.len() as u64) as usize];
            let size = (id.wrapping_mul(2_654_435_761) % 1_000_000) + 1;
            FileRecord {
                id: FileId(id),
                parent_directory_id: DirectoryId(1 + id % directory_count),
                name: format!("file-{id:06}.{extension}").into(),
                size_bytes: size,
                allocation_bytes: size.div_ceil(4096) * 4096,
                timestamps: FileTimestamps::new(
                    Some(1_700_000_000),
                    Some(1_700_000_000 + (id as i64 % 10_000_000)),
                    None,
                ),
                ..Default::default()
            }
        })
        .collect();
    (directories, files)
}

fn fixture_transaction(file_count: u64) -> BufferedIndexTransaction {
    let (directories, files) = fixture_records(file_count);
    let mut transaction = BufferedIndexTransaction::default();
    for directory in directories {
        transaction.insert_directory(directory);
    }
    for file in files {
        transaction.insert_file(file);
    }
    transaction
}

fn fixture_tree(file_count: u64) -> TreeIndex {
    TreeIndex::build(fixture_transaction(file_count))
}

fn csv_bytes(tree: &TreeIndex) -> Vec<u8> {
    let mut output = Vec::new();
    write_csv(tree, &mut output).expect("CSV serialization");
    output
}

fn json_bytes(tree: &TreeIndex) -> Vec<u8> {
    let mut output = Vec::new();
    write_json(tree, &mut output).expect("JSON serialization");
    output
}

mod tree {
    use super::*;

    #[divan::bench(args = SIZES)]
    fn build(bencher: Bencher, file_count: u64) {
        bencher
            .with_inputs(|| fixture_transaction(file_count))
            .bench_values(|transaction| black_box(TreeIndex::build(transaction)));
    }

    #[divan::bench(args = SIZES)]
    fn largest_files_first_query(bencher: Bencher, file_count: u64) {
        bencher
            .with_inputs(|| fixture_tree(file_count))
            .bench_refs(|tree| {
                let mut total = 0u64;
                tree.for_each_largest_file(100, |file| total += file.allocation_bytes);
                black_box(total)
            });
    }

    #[divan::bench(args = SIZES)]
    fn apply_file_changes(bencher: Bencher, file_count: u64) {
        bencher
            .with_inputs(|| {
                let tree = fixture_tree(file_count);
                let next = tree.max_file_id().unwrap() + 1;
                let added: Vec<FileRecord> = (next..next + 100)
                    .map(|id| FileRecord {
                        id: FileId(id),
                        parent_directory_id: DirectoryId(2),
                        name: format!("added-{id}.txt").into(),
                        size_bytes: id,
                        allocation_bytes: 4096,
                        ..Default::default()
                    })
                    .collect();
                let modified: Vec<FileRecord> = (1..=100)
                    .map(|id| {
                        let mut file = tree.file_by_id(id).unwrap().clone();
                        file.size_bytes += 1;
                        file
                    })
                    .collect();
                (tree, added, modified)
            })
            .bench_values(|(mut tree, added, modified)| {
                black_box(tree.apply_file_changes(added, modified).unwrap());
                tree
            });
    }
}

mod catalog {
    use super::*;

    #[divan::bench(args = SIZES)]
    fn from_transaction(bencher: Bencher, file_count: u64) {
        bencher
            .with_inputs(|| fixture_transaction(file_count))
            .bench_refs(|transaction| black_box(IndexCatalog::from_transaction(transaction)));
    }

    fn top10(bencher: Bencher, file_count: u64, sort_field: SortField) {
        let catalog = IndexCatalog::from_transaction(&fixture_transaction(file_count));
        let query = SearchQuery {
            include_files: true,
            limit: Some(10),
            sort_field,
            sort_direction: SortDirection::Descending,
            ..Default::default()
        };
        bencher.bench(|| black_box(catalog.search(black_box(&query))));
    }

    #[divan::bench(args = SIZES)]
    fn search_top10_by_name(bencher: Bencher, file_count: u64) {
        top10(bencher, file_count, SortField::Name);
    }

    #[divan::bench(args = SIZES)]
    fn search_top10_by_size(bencher: Bencher, file_count: u64) {
        top10(bencher, file_count, SortField::SizeBytes);
    }
}

mod tree_search {
    use super::*;

    fn run(bencher: Bencher, file_count: u64, options: SearchOptions) {
        let tree = fixture_tree(file_count);
        bencher.bench(|| black_box(search(&tree, black_box(&options)).unwrap().total));
    }

    #[divan::bench(args = SIZES)]
    fn contains(bencher: Bencher, file_count: u64) {
        let options = SearchOptions {
            pattern: "file-00".into(),
            limit: 100,
            ..Default::default()
        };
        run(bencher, file_count, options);
    }

    #[divan::bench(args = SIZES)]
    fn regex(bencher: Bencher, file_count: u64) {
        let options = SearchOptions {
            pattern: r"file-\d+7\.(rs|json)$".into(),
            mode: TextMode::Regex,
            limit: 100,
            ..Default::default()
        };
        run(bencher, file_count, options);
    }

    #[divan::bench(args = SIZES)]
    fn filtered_by_size_and_extension(bencher: Bencher, file_count: u64) {
        let options = SearchOptions {
            sort: SearchSort::Allocated,
            descending: true,
            min_logical: Some(100_000),
            extensions: vec!["bin".into(), "dll".into()],
            limit: 100,
            ..Default::default()
        };
        run(bencher, file_count, options);
    }
}

mod export {
    use super::*;

    #[divan::bench(args = SIZES)]
    fn csv(bencher: Bencher, file_count: u64) {
        let tree = fixture_tree(file_count);
        bencher.bench(|| black_box(csv_bytes(&tree)));
    }

    #[divan::bench(args = SIZES)]
    fn json(bencher: Bencher, file_count: u64) {
        let tree = fixture_tree(file_count);
        bencher.bench(|| black_box(json_bytes(&tree)));
    }
}

mod import {
    use super::*;

    #[divan::bench(args = SIZES)]
    fn csv(bencher: Bencher, file_count: u64) {
        let input = csv_bytes(&fixture_tree(file_count));
        bencher.bench(|| black_box(read_csv(black_box(input.as_slice())).unwrap()));
    }

    #[divan::bench(args = SIZES)]
    fn json(bencher: Bencher, file_count: u64) {
        let input = json_bytes(&fixture_tree(file_count));
        bencher.bench(|| black_box(read_json(black_box(input.as_slice())).unwrap()));
    }
}

mod core {
    use super::*;

    #[divan::bench(args = SIZES)]
    fn aggregate_directories(bencher: Bencher, file_count: u64) {
        let (directories, files) = fixture_records(file_count);
        bencher.bench(|| black_box(aggregate_directory_records(&directories, &files)));
    }

    #[divan::bench(args = SIZES)]
    fn diff_records(bencher: Bencher, file_count: u64) {
        let (_, previous) = fixture_records(file_count);
        // Modify every 10th record, drop the last 1% and add as many new ones.
        let removed = file_count / 100;
        let mut current: Vec<FileRecord> = previous[..previous.len() - removed as usize]
            .iter()
            .map(|file| {
                let mut file = file.clone();
                if file.id.0 % 10 == 0 {
                    file.size_bytes += 1;
                }
                file
            })
            .collect();
        current.extend((0..removed).map(|offset| FileRecord {
            id: FileId(file_count + 1 + offset),
            parent_directory_id: DirectoryId(1),
            name: format!("new-{offset}.txt").into(),
            ..Default::default()
        }));
        bencher.bench(|| black_box(diff_file_records(&previous, &current)));
    }
}
