//! Measure the public catalog APIs separately from scanning and GUI rendering.
use std::ffi::c_void;
use std::time::Instant;
use winblaze_native::api::{
    WbCStringView, WbCsvFilter, WbDuplicateRow, WbExtensionStat, WbSearchFilter, WbStoragePolicy,
    WbTreeNode,
};
use winblaze_native::bridge::{
    wb_duplicate_results, wb_export_csv, wb_export_csv_filtered, wb_export_csv_metadata,
    wb_export_json, wb_export_json_metadata, wb_export_tsv, wb_import_scan,
    wb_index_snapshot_extension_stats, wb_scan_roots, wb_storage_cost, wb_tree_largest_files_page,
    wb_tree_root, wb_tree_search,
};

#[derive(Default)]
struct Root {
    path: String,
    files: u64,
    bytes: u64,
}
extern "C" fn root(node: *const WbTreeNode, context: *mut c_void) {
    // SAFETY: the synchronous API supplies a live node and our Root context.
    let (node, out) = unsafe { (&*node, &mut *context.cast::<Root>()) };
    // SAFETY: name is borrowed and valid for this callback.
    let name = unsafe { std::slice::from_raw_parts(node.name.ptr.cast(), node.name.len) };
    out.path = String::from_utf8_lossy(name).into_owned();
    out.files = node.file_count;
    out.bytes = node.logical_bytes;
}
extern "C" fn extension(stat: *const WbExtensionStat, context: *mut c_void) {
    // SAFETY: the synchronous callback borrows a live stat and count context.
    let (stat, count) = unsafe { (&*stat, &mut *context.cast::<u64>()) };
    *count += stat.files;
}
extern "C" fn largest(node: *const WbTreeNode, context: *mut c_void) {
    // SAFETY: synchronous callbacks borrow a live node, UTF-8 name, and our output vector.
    unsafe {
        let node = &*node;
        let name = std::slice::from_raw_parts(node.name.ptr.cast(), node.name.len);
        (*context.cast::<Vec<(String, u64, u64)>>()).push((
            String::from_utf8_lossy(name).into_owned(),
            node.physical_bytes,
            node.logical_bytes,
        ));
    }
}
fn borrowed_text(value: WbCStringView) -> String {
    if value.len == 0 {
        return String::new();
    }
    // SAFETY: synchronous native callbacks borrow a live UTF-8 view.
    String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(value.ptr.cast(), value.len) })
        .into_owned()
}
extern "C" fn duplicate(row: *const WbDuplicateRow, context: *mut c_void) {
    // SAFETY: synchronous native callback and our owned output vector.
    let (row, output) = unsafe { (&*row, &mut *context.cast::<Vec<String>>()) };
    if row.is_error != 0 {
        return;
    }
    output.push(format!(
        "{},\"{}\",{},verified,{}\r\n",
        row.group,
        borrowed_text(row.path).replace('"', "\"\""),
        row.logical_bytes,
        borrowed_text(row.digest)
    ));
}
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let import_operations = [
        "csv-import",
        "csv-import-utc",
        "csv-import-json",
        "json-import",
        "mft-import",
        "tsv-import",
        "multiple-roots",
    ];
    let is_import = args
        .get(1)
        .is_some_and(|value| import_operations.contains(&value.as_str()));
    if !((args.len() == 4 && !is_import) || (args.len() == 5 && is_import))
        || ![
            "csv-import",
            "csv-import-utc",
            "csv-import-json",
            "json-import",
            "mft-import",
            "tsv-import",
            "multiple-roots",
            "tsv-export",
            "detailed-json",
            "detailed-csv",
            "regex-search",
            "allocated-search",
            "storage-cost",
            "duplicates-sha256",
            "duplicates-md5",
            "duplicates-sha1",
            "duplicates-sha512",
            "largest-files",
            "advanced-filter",
            "cache-load",
            "csv-export",
            "json-export",
            "filtered-export",
            "extensions",
        ]
        .contains(&args[1].as_str())
    {
        eprintln!("usage: feature_catalog <cache-load|csv-export|json-export|filtered-export|extensions> <expected-root> <output.csv>");
        std::process::exit(2);
    }
    let started = Instant::now();
    let mut catalog = Root::default();
    if args[1] == "multiple-roots" {
        let result = wb_scan_roots(WbCStringView {
            ptr: args[3].as_ptr().cast(),
            len: args[3].len(),
        });
        if result.status != 0 || result.warnings != 0 {
            eprintln!("Combined root scan failed or reported unavailable metadata");
            std::process::exit(1);
        }
    }
    if is_import
        && args[1] != "multiple-roots"
        && wb_import_scan(
            WbCStringView {
                ptr: args[3].as_ptr().cast(),
                len: args[3].len(),
            },
            match args[1].as_str() {
                "json-import" => 3,
                "mft-import" => 4,
                "tsv-import" => 2,
                _ => 1,
            },
            u32::from(args[1] == "csv-import-utc"),
        ) != 0
    {
        eprintln!("External scan import failed");
        std::process::exit(1);
    }
    if wb_tree_root(Some(root), (&mut catalog as *mut Root).cast()) == 0
        || !catalog
            .path
            .replace('/', "\\")
            .eq_ignore_ascii_case(&args[2].replace('/', "\\"))
    {
        eprintln!("cache root differs from the fixture; seed it with scan_export first");
        std::process::exit(1);
    }
    let load_ms = started.elapsed().as_secs_f64() * 1000.0;
    let operation = Instant::now();
    let mut extension_files = 0_u64;
    if [
        "csv-export",
        "csv-import",
        "csv-import-utc",
        "csv-import-json",
        "json-import",
        "mft-import",
        "tsv-import",
        "multiple-roots",
        "tsv-export",
        "detailed-json",
        "detailed-csv",
        "json-export",
        "filtered-export",
        "advanced-filter",
    ]
    .contains(&args[1].as_str())
    {
        let destination = args.last().unwrap();
        let path = WbCStringView {
            ptr: destination.as_ptr().cast(),
            len: destination.len(),
        };
        let filter = b"*.bin";
        let result = match args[1].as_str() {
            "json-export" | "csv-import-json" => wb_export_json(path),
            "detailed-json" => wb_export_json_metadata(path),
            "detailed-csv" => wb_export_csv_metadata(path),
            "tsv-export" => wb_export_tsv(path),
            "filtered-export" => wb_export_csv_filtered(
                path,
                WbCsvFilter {
                    patterns: WbCStringView {
                        ptr: filter.as_ptr().cast(),
                        len: filter.len(),
                    },
                    ..Default::default()
                },
            ),
            "advanced-filter" => {
                let patterns = b"*.bin;unique-???.txt";
                wb_export_csv_filtered(
                    path,
                    WbCsvFilter {
                        patterns: WbCStringView {
                            ptr: patterns.as_ptr().cast(),
                            len: patterns.len(),
                        },
                        min_bytes: 1_048_576,
                        max_bytes: 1_048_576,
                        modified_after_utc: 1_767_225_600,
                        modified_before_utc: 4_102_444_799,
                        flags: 15,
                    },
                )
            }
            _ => wb_export_csv(path),
        };
        if result != 0 {
            eprintln!("Catalog export failed");
            std::process::exit(1);
        }
    } else if ["largest-files", "regex-search", "allocated-search"].contains(&args[1].as_str()) {
        let mut files = Vec::<(String, u64, u64)>::new();
        if args[1] == "largest-files" {
            for offset in (0..catalog.files).step_by(50) {
                wb_tree_largest_files_page(
                    offset,
                    50,
                    Some(largest),
                    (&mut files as *mut Vec<(String, u64, u64)>).cast(),
                );
            }
        } else {
            let pattern = if args[1] == "regex-search" {
                r"\.bin$"
            } else {
                ""
            };
            let result = wb_tree_search(
                WbSearchFilter {
                    pattern: WbCStringView {
                        ptr: pattern.as_ptr().cast(),
                        len: pattern.len(),
                    },
                    limit: catalog.files,
                    flags: 64 | if args[1] == "allocated-search" { 4 } else { 0 },
                    min_allocated: 1,
                    mode: if args[1] == "regex-search" { 3 } else { 0 },
                    sort: 2,
                    ..Default::default()
                },
                Some(largest),
                (&mut files as *mut Vec<(String, u64, u64)>).cast(),
            );
            if result.status != 0 || result.emitted != result.total {
                eprintln!("Complete catalog search failed");
                std::process::exit(1);
            }
        }
        let mut text = String::from("Path,AllocatedBytes,LogicalBytes\r\n");
        for (name, allocated, logical) in files {
            text.push_str(&format!(
                "\"{}\",{allocated},{logical}\r\n",
                name.replace('"', "\"\"")
            ));
        }
        std::fs::write(&args[3], text).expect("write largest-file report");
    } else if args[1].starts_with("duplicates-") {
        let algorithm = match args[1].as_str() {
            "duplicates-sha256" => 1,
            "duplicates-md5" => 2,
            "duplicates-sha1" => 3,
            _ => 4,
        };
        let mut rows = Vec::<String>::new();
        let result = wb_duplicate_results(
            algorithm,
            Some(duplicate),
            (&mut rows as *mut Vec<String>).cast(),
        );
        if result.status != 0 || result.errors != 0 {
            eprintln!("Duplicate verification did not complete without errors");
            std::process::exit(1);
        }
        let mut csv = String::from("Group,Path,LogicalBytes,Status,Digest\r\n");
        for row in rows {
            csv.push_str(&row);
        }
        std::fs::write(&args[3], csv).expect("write verified duplicate/hash report");
    } else if args[1] == "storage-cost" {
        let result = wb_storage_cost(WbStoragePolicy {
            now_utc: 4_102_444_800,
            warm_after_days: 0,
            cold_after_days: 0,
            monthly_prices: [1.0, 0.5, 0.25],
        });
        if result.status != 0 {
            eprintln!("Storage estimate failed");
            std::process::exit(1);
        }
        std::fs::write(&args[3],format!(
            "{{\"files\":{:?},\"allocated_bytes\":{:?},\"unknown_dates\":{},\"monthly_cost\":{},\"all_hot_cost\":{}}}",
            result.files,result.allocated_bytes,result.unknown_dates,result.monthly_cost,result.all_hot_cost)).expect("write storage estimate");
    } else if args[1] == "extensions" {
        wb_index_snapshot_extension_stats(
            Some(extension),
            (&mut extension_files as *mut u64).cast(),
        );
    }
    println!("{{\"files\":{},\"logical_bytes\":{},\"load_ms\":{},\"operation_ms\":{},\"extension_files\":{}}}",
        catalog.files, catalog.bytes, load_ms, operation.elapsed().as_secs_f64() * 1000.0, extension_files);
}
