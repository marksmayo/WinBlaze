use winblaze_index::import::read_csv;
#[test]
fn largest_file_pages_are_complete_and_stable_across_the_cache_boundary() {
    let mut csv = String::from(
        "Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\n",
    );
    for i in 0..10051 {
        csv.push_str(&format!("Z:\\offline\\{i}.bin,File,7,4096,0,0,\n"));
    }
    let tree = read_csv(csv.as_bytes()).unwrap();
    let mut ids = Vec::new();
    for offset in (0..10051).step_by(50) {
        tree.for_each_largest_file_page(offset, 50, |file| ids.push(file.id.0));
    }
    assert_eq!(ids, (1..=10051).collect::<Vec<_>>());
    let mut beyond = 0;
    tree.for_each_largest_file_page(usize::MAX, 50, |_| beyond += 1);
    assert_eq!(beyond, 0);
    tree.for_each_largest_file_page(0, 0, |_| beyond += 1);
    assert_eq!(beyond, 0);
}
