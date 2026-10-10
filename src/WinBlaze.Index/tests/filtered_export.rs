use winblaze_index::{
    export::{write_csv_filtered, ExportFilter},
    import::read_csv,
};
#[test]
fn filtered_export_combines_wildcards_size_and_utc_date_bounds() {
    let csv="Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\nZ:\\offline\\report.TXT,File,7,4096,0,0,123\nZ:\\offline\\data.bin,File,9,4096,0,0,124\nZ:\\offline\\tiny.bin,File,1,4096,0,0,124\nZ:\\offline\\old.txt,File,7,4096,0,0,122\nZ:\\offline\\unknown.txt,File,7,4096,0,0,\n";
    let tree = read_csv(csv.as_bytes()).unwrap();
    let filter = ExportFilter {
        patterns: vec!["*.txt".into(), "data.?in".into()],
        min_bytes: Some(7),
        max_bytes: Some(9),
        modified_after_utc: Some(123),
        modified_before_utc: Some(124),
    };
    let mut out = Vec::new();
    write_csv_filtered(&tree, &filter, &mut out).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert_eq!(text.lines().count(), 3);
    assert!(text.contains("report.TXT"));
    assert!(text.contains("data.bin"));
    assert!(!text.contains("Directory"));
}
#[test]
fn invalid_filter_bounds_do_not_write_and_wildcards_backtrack_correctly() {
    let tree=read_csv(b"Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\nZ:\\offline\\ababc.TXT,File,7,9,0,0,123\n".as_slice()).unwrap();
    for filter in [
        ExportFilter {
            min_bytes: Some(9),
            max_bytes: Some(7),
            ..Default::default()
        },
        ExportFilter {
            modified_after_utc: Some(5),
            modified_before_utc: Some(4),
            ..Default::default()
        },
        ExportFilter {
            patterns: vec!["".into()],
            ..Default::default()
        },
    ] {
        let mut out = Vec::new();
        assert!(write_csv_filtered(&tree, &filter, &mut out).is_err());
        assert!(out.is_empty());
    }
    for (pattern, count) in [
        ("a*a?c.*", 2),
        ("a*wrong", 1),
        ("*", 2),
        ("abc?", 1),
        ("?????.txt", 2),
    ] {
        let mut out = Vec::new();
        write_csv_filtered(
            &tree,
            &ExportFilter {
                patterns: vec![pattern.into()],
                ..Default::default()
            },
            &mut out,
        )
        .unwrap();
        assert_eq!(String::from_utf8(out).unwrap().lines().count(), count);
    }
}
