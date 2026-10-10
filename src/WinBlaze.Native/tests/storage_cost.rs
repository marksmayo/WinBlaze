use winblaze_native::{
    api::{WbCStringView, WbStoragePolicy},
    bridge::{wb_import_csv, wb_storage_cost},
};
fn view(s: &str) -> WbCStringView {
    WbCStringView {
        ptr: s.as_ptr().cast(),
        len: s.len(),
    }
}
#[test]
fn native_cost_estimates_use_the_published_catalog_and_reject_invalid_prices() {
    let input = std::env::temp_dir().join(format!("wb-cost-{}.csv", std::process::id()));
    std::fs::write(&input,"Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\nZ:\\offline\\cold.bin,File,1,1073741824,0,0,1982720000\n").unwrap();
    assert_eq!(wb_import_csv(view(&input.to_string_lossy())), 0);
    let policy = WbStoragePolicy {
        now_utc: 2000000000,
        warm_after_days: 30,
        cold_after_days: 180,
        monthly_prices: [0.1, 0.03, 0.01],
    };
    let result = wb_storage_cost(policy);
    assert_eq!(result.status, 0);
    assert_eq!(result.files, [0, 0, 1]);
    assert!((result.monthly_cost - 0.01).abs() < 1e-9);
    assert_eq!(
        wb_storage_cost(WbStoragePolicy {
            monthly_prices: [-1.0, 0.0, 0.0],
            ..policy
        })
        .status,
        1
    );
    std::fs::remove_file(input).unwrap();
}
