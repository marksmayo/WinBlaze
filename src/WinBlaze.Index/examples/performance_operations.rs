//! Reproducible public-operation timings. Fixture construction is timed separately.
use std::{hint::black_box, time::Instant};
use winblaze_core::{
    DirectoryId, DirectoryRecord, FileId, FileRecord, SearchQuery, SortDirection, SortField,
};
use winblaze_index::{BufferedIndexTransaction, IndexCatalog, TreeIndex};

fn fixture(count: u64) -> BufferedIndexTransaction {
    let mut records = BufferedIndexTransaction::default();
    records.insert_directory(DirectoryRecord {
        id: DirectoryId(1),
        name: "root".into(),
        full_path: "C:\\fixture".into(),
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
fn main() {
    let count: u64 = std::env::args()
        .nth(1)
        .and_then(|arg| arg.parse().ok())
        .unwrap_or(100_000);
    assert!(count >= 10);
    let setup = Instant::now();
    let records = fixture(count);
    let setup_ms = setup.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let catalog = IndexCatalog::from_transaction(&records);
    let catalog_ms = started.elapsed().as_secs_f64() * 1000.0;
    let started = Instant::now();
    let tree = TreeIndex::build(records);
    let tree_ms = started.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(tree.root().unwrap().1.file_count, count);
    let started = Instant::now();
    let mut csv = Vec::new();
    winblaze_index::export::write_csv(&tree, &mut csv).expect("CSV serialization");
    black_box(&csv);
    let csv_ms = started.elapsed().as_secs_f64() * 1000.0;
    // This fixture has no embedded newlines. Verify exported records and totals,
    // rather than assuming an arbitrary minimum serialized length per row.
    let text = std::str::from_utf8(&csv).expect("UTF-8 CSV");
    let mut rows = text.lines();
    assert_eq!(
        rows.next(),
        Some("Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds")
    );
    let total = count * (count + 1) / 2;
    assert_eq!(
        rows.next(),
        Some(format!("\"C:\\fixture\",Directory,{total},{total},{count},0,").as_str())
    );
    let mut file_rows = 0;
    let mut last = "";
    for row in rows {
        file_rows += 1;
        last = row;
    }
    assert_eq!(file_rows, count);
    assert_eq!(
        last,
        format!("\"C:\\fixture\\file-{count}.txt\",File,{count},{count},0,0,")
    );
    drop(csv);
    let mut results = Vec::new();
    for field in [SortField::Name, SortField::Path, SortField::SizeBytes] {
        let query = SearchQuery {
            include_files: true,
            limit: Some(10),
            sort_field: field,
            sort_direction: SortDirection::Descending,
            ..Default::default()
        };
        let started = Instant::now();
        let hits = catalog.search(&query);
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        assert_eq!(hits.len(), 10);
        if field == SortField::SizeBytes {
            assert_eq!(hits[0].size_bytes, count);
        }
        black_box(hits);
        results.push(ms);
    }
    let started = Instant::now();
    let mut largest = Vec::with_capacity(10);
    tree.for_each_largest_file(10, |file| largest.push(file.id.0));
    let largest_first_ms = started.elapsed().as_secs_f64() * 1000.0;
    assert_eq!(largest[0], count);
    let started = Instant::now();
    tree.for_each_largest_file(10, |file| {
        black_box(file.id);
    });
    let largest_cached_ms = started.elapsed().as_secs_f64() * 1000.0;
    println!("{{\"files\":{count},\"setup_ms\":{setup_ms},\"catalog_ms\":{catalog_ms},\"tree_ms\":{tree_ms},\"csv_buffer_ms\":{csv_ms},\"search_name_top10_ms\":{},\"search_path_top10_ms\":{},\"search_size_top10_ms\":{},\"largest_first_ms\":{largest_first_ms},\"largest_cached_ms\":{largest_cached_ms},\"verified\":true}}",results[0],results[1],results[2]);
}
