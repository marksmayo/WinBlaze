use std::ffi::c_void;
use winblaze_native::{
    api::{WbCStringView, WbCsvFilter, WbTreeNode},
    bridge::{
        wb_export_csv, wb_export_csv_filtered, wb_export_tsv, wb_import_csv, wb_import_csv_options,
        wb_import_scan, wb_tree_root,
    },
};
fn view(s: &str) -> WbCStringView {
    WbCStringView {
        ptr: s.as_ptr().cast(),
        len: s.len(),
    }
}
extern "C" fn count(node: *const WbTreeNode, out: *mut c_void) {
    unsafe {
        *out.cast::<u64>() = (*node).file_count;
    }
}
#[test]
fn imports_an_offline_scan_atomically_and_keeps_it_after_a_failed_import() {
    let dir = std::env::temp_dir().join(format!("winblaze-import-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let input = dir.join("input.csv");
    let output = dir.join("out.csv");
    let csv="Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\nZ:\\offline,Directory,7,4096,1,0,\nZ:\\offline\\a.bin,File,7,4096,0,0,\n";
    std::fs::write(&input, csv).unwrap();
    assert_eq!(wb_import_csv(view(input.to_str().unwrap())), 0);
    let mut files = 0_u64;
    assert_eq!(
        wb_tree_root(Some(count), (&mut files as *mut u64).cast()),
        1
    );
    assert_eq!(files, 1);
    std::fs::write(&input, "not a scan").unwrap();
    assert_eq!(wb_import_csv(view(input.to_str().unwrap())), 2);
    assert_eq!(wb_export_csv(view(output.to_str().unwrap())), 0);
    assert!(std::fs::read_to_string(&output).unwrap().contains("a.bin"));
    let filter = WbCsvFilter {
        patterns: view("*.BIN;*.txt"),
        min_bytes: 7,
        max_bytes: 7,
        flags: 3,
        ..Default::default()
    };
    assert_eq!(
        wb_export_csv_filtered(view(output.to_str().unwrap()), filter),
        0
    );
    assert_eq!(std::fs::read_to_string(&output).unwrap().lines().count(), 2);
    std::fs::write(&output, "keep").unwrap();
    assert_eq!(
        wb_export_csv_filtered(
            view(output.to_str().unwrap()),
            WbCsvFilter {
                min_bytes: 8,
                ..filter
            }
        ),
        1
    );
    assert_eq!(std::fs::read_to_string(&output).unwrap(), "keep");
    std::fs::write(&input, "File Name,Size,Allocated,Modified,Attributes,Files,Folders\nZ:\\offline\\a.bin,7,4096,2026/01/01 00:00:00,32,0,0\n").unwrap();
    assert_eq!(wb_import_csv_options(view(input.to_str().unwrap()), 1), 0);
    assert_eq!(wb_export_csv(view(output.to_str().unwrap())), 0);
    assert!(std::fs::read_to_string(&output)
        .unwrap()
        .contains("1767225600"));
    assert_eq!(wb_import_csv_options(view(input.to_str().unwrap()), 2), 1);
    assert_eq!(wb_export_csv(view(output.to_str().unwrap())), 0);
    assert!(std::fs::read_to_string(&output)
        .unwrap()
        .contains("1767225600"));
    let json = dir.join("scan.json");
    std::fs::write(&json, r#"{"schema_version":1,"entries":[{"Path":"Z:\\offline\\json.bin","Kind":"File","LogicalBytes":9,"AllocatedBytes":4096,"Files":0,"Directories":0,"ModifiedUnixSeconds":null}]}"#).unwrap();
    assert_eq!(wb_import_scan(view(json.to_str().unwrap()), 0, 0), 0);
    let tsv = dir.join("scan.tsv");
    assert_eq!(wb_export_tsv(view(tsv.to_str().unwrap())), 0);
    assert!(std::fs::read_to_string(&tsv)
        .unwrap()
        .starts_with("Path\tKind"));
    assert_eq!(wb_import_scan(view(tsv.to_str().unwrap()), 0, 0), 0);
    assert_eq!(wb_export_csv(view(output.to_str().unwrap())), 0);
    assert!(std::fs::read_to_string(&output)
        .unwrap()
        .contains("json.bin"));
    std::fs::write(&json, "{}").unwrap();
    assert_eq!(wb_import_scan(view(json.to_str().unwrap()), 0, 0), 2);
    assert_eq!(wb_export_csv(view(output.to_str().unwrap())), 0);
    assert!(std::fs::read_to_string(&output)
        .unwrap()
        .contains("json.bin"));
    let dump = dir.join("offline.MFT");
    let mut record = [0_u8; 1024];
    record[..4].copy_from_slice(b"FILE");
    record[4..6].copy_from_slice(&0x30_u16.to_le_bytes());
    record[6..8].copy_from_slice(&3_u16.to_le_bytes());
    record[0x14..0x16].copy_from_slice(&0x38_u16.to_le_bytes());
    record[0x16..0x18].copy_from_slice(&3_u16.to_le_bytes());
    record[0x1c..0x20].copy_from_slice(&1024_u32.to_le_bytes());
    record[0x2c..0x30].copy_from_slice(&5_u32.to_le_bytes());
    record[0x30..0x36].copy_from_slice(&[0xab, 0xcd, 0, 0, 0, 0]);
    record[0x38..0x3c].copy_from_slice(&0x30_u32.to_le_bytes());
    record[0x3c..0x40].copy_from_slice(&90_u32.to_le_bytes());
    record[0x48..0x4c].copy_from_slice(&66_u32.to_le_bytes());
    record[0x4c..0x4e].copy_from_slice(&24_u16.to_le_bytes());
    record[0x50..0x58].copy_from_slice(&5_u64.to_le_bytes());
    record[0x92..0x96].copy_from_slice(&u32::MAX.to_le_bytes());
    record[510..512].copy_from_slice(&[0xab, 0xcd]);
    record[1022..1024].copy_from_slice(&[0xab, 0xcd]);
    std::fs::write(&dump, record).unwrap();
    assert_eq!(wb_import_scan(view(dump.to_str().unwrap()), 0, 0), 0);
    assert_eq!(wb_export_csv(view(output.to_str().unwrap())), 0);
    assert!(std::fs::read_to_string(&output).unwrap().contains("MFT:\\"));
    std::fs::write(&dump, &record[..1023]).unwrap();
    assert_eq!(wb_import_scan(view(dump.to_str().unwrap()), 0, 0), 2);
    std::fs::remove_dir_all(dir).unwrap();
}
