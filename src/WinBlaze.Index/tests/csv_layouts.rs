//! Alternative CSV layouts: the same complete catalog with fewer bytes.
use winblaze_core::{DirectoryId, DirectoryRecord, FileAttributes, FileId, FileRecord};
use winblaze_index::export::{write_csv_with_layout, write_paths, CsvLayout};
use winblaze_index::{BufferedIndexTransaction, IndexTransaction, TreeIndex};

const HEADER: &str =
    "Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\r\n";

fn fixture() -> TreeIndex {
    let mut transaction = BufferedIndexTransaction::default();
    let directory = |id: u64, parent: Option<u64>, name: &str, path: &str| DirectoryRecord {
        id: DirectoryId(id),
        parent_directory_id: parent.map(DirectoryId),
        name: name.into(),
        full_path: path.into(),
    };
    transaction.upsert_directory(&directory(5, None, "C:\\", "C:\\"));
    transaction.upsert_directory(&directory(6, Some(5), "Users", "C:\\Users"));
    transaction.upsert_directory(&directory(7, Some(6), "a,b", "C:\\Users\\a,b"));
    let file = |id: u64, parent: u64, name: &str, size: u64| FileRecord {
        id: FileId(id),
        parent_directory_id: DirectoryId(parent),
        name: name.into(),
        full_path: String::new().into(),
        size_bytes: size,
        allocation_bytes: size,
        attributes: FileAttributes::ARCHIVE,
        timestamps: winblaze_core::FileTimestamps::new(None, Some(116444737230000000), None),
    };
    transaction.upsert_file(&file(1, 5, "pagefile.sys", 100));
    transaction.upsert_file(&file(2, 6, "note \"one\".txt", 10));
    transaction.upsert_file(&file(3, 7, "plain.bin", 1));
    transaction.upsert_file(&file(4, 7, "big.bin", 50));
    TreeIndex::build(transaction)
}

fn export(layout: CsvLayout) -> String {
    let mut output = Vec::new();
    write_csv_with_layout(&fixture(), layout, &mut output).unwrap();
    String::from_utf8(output).unwrap()
}

#[test]
fn the_default_layout_is_unchanged() {
    let text = export(CsvLayout::default());
    let mut expected = String::from(HEADER);
    expected.push_str("\"C:\\\",Directory,161,161,4,2,123\r\n");
    expected.push_str("\"C:\\Users\",Directory,61,61,3,1,123\r\n");
    expected.push_str("\"C:\\Users\\a,b\",Directory,51,51,2,0,123\r\n");
    expected.push_str("\"C:\\pagefile.sys\",File,100,100,0,0,123\r\n");
    expected.push_str("\"C:\\Users\\note \"\"one\"\".txt\",File,10,10,0,0,123\r\n");
    expected.push_str("\"C:\\Users\\a,b\\plain.bin\",File,1,1,0,0,123\r\n");
    expected.push_str("\"C:\\Users\\a,b\\big.bin\",File,50,50,0,0,123\r\n");
    assert_eq!(text, expected);
}

#[test]
fn the_tree_layout_lists_each_directory_followed_by_its_files_by_name() {
    let text = export(CsvLayout {
        tree: true,
        ..CsvLayout::default()
    });
    let mut expected = String::from(HEADER);
    // The root row stays first with its totals, so readers of the first
    // two lines (the benchmark harness) see the same summary.
    expected.push_str("\"C:\\\",Directory,161,161,4,2,123\r\n");
    expected.push_str("\"pagefile.sys\",File,100,100,0,0,123\r\n");
    expected.push_str("\"C:\\Users\",Directory,61,61,3,1,123\r\n");
    expected.push_str("\"note \"\"one\"\".txt\",File,10,10,0,0,123\r\n");
    expected.push_str("\"C:\\Users\\a,b\",Directory,51,51,2,0,123\r\n");
    expected.push_str("\"plain.bin\",File,1,1,0,0,123\r\n");
    expected.push_str("\"big.bin\",File,50,50,0,0,123\r\n");
    assert_eq!(text, expected);
    assert!(text.len() < export(CsvLayout::default()).len());
}

#[test]
fn the_tree_layout_reopens_as_the_same_catalog() {
    let text = export(CsvLayout {
        tree: true,
        minimal_quoting: true,
        ..CsvLayout::default()
    });
    let reopened = winblaze_index::import::read_csv(text.as_bytes()).unwrap();
    let original = fixture();
    let paths = |tree: &TreeIndex| {
        let mut paths: Vec<String> = tree
            .files()
            .iter()
            .map(|file| tree.file_display_path(file))
            .collect();
        paths.sort();
        paths
    };
    assert_eq!(paths(&reopened), paths(&original));
    assert_eq!(reopened.directories().len(), 3);
    assert_eq!(
        reopened.root().unwrap().1.logical_bytes,
        original.root().unwrap().1.logical_bytes
    );
}

#[test]
fn the_files_only_layout_keeps_the_root_row_and_drops_other_directories() {
    let text = export(CsvLayout {
        files_only: true,
        ..CsvLayout::default()
    });
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines[1], "\"C:\\\",Directory,161,161,4,2,123");
    assert_eq!(lines.len(), 1 + 1 + 4);
    assert!(lines[2..].iter().all(|line| line.contains(",File,")));
    let reopened = winblaze_index::import::read_csv(text.as_bytes()).unwrap();
    assert_eq!(reopened.files().len(), 4);
}

#[test]
fn minimal_quoting_quotes_only_fields_that_need_it() {
    let text = export(CsvLayout {
        minimal_quoting: true,
        ..CsvLayout::default()
    });
    assert!(text.contains("\r\nC:\\pagefile.sys,File,100,100,0,0,123\r\n"));
    assert!(text.contains("\r\n\"C:\\Users\\a,b\",Directory,"));
    assert!(text.contains("\r\n\"C:\\Users\\note \"\"one\"\".txt\",File,"));
    assert!(text.contains("\r\n\"C:\\Users\\a,b\\big.bin\",File,"));
    let reopened = winblaze_index::import::read_csv(text.as_bytes()).unwrap();
    assert_eq!(reopened.files().len(), 4);
}

#[test]
fn the_paths_layout_writes_one_file_path_per_line() {
    let mut output = Vec::new();
    write_paths(&fixture(), &mut output).unwrap();
    let text = String::from_utf8(output).unwrap();
    assert_eq!(
        text,
        "C:\\pagefile.sys\r\nC:\\Users\\note \"one\".txt\r\nC:\\Users\\a,b\\plain.bin\r\nC:\\Users\\a,b\\big.bin\r\n"
    );
}
