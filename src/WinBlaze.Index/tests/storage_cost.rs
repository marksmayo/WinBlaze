use winblaze_index::{
    import::read_csv,
    storage_cost::{estimate, StoragePolicy},
};
#[test]
fn estimates_tiers_from_allocated_bytes_and_explicit_utc_age_thresholds() {
    let csv = "Path,Kind,LogicalBytes,AllocatedBytes,Files,Directories,ModifiedUnixSeconds\n\
Z:\\root\\hot.bin,File,1,1073741824,0,0,1999999999\n\
Z:\\root\\warm.bin,File,1,2147483648,0,0,1996544000\n\
Z:\\root\\cold.bin,File,1,3221225472,0,0,1982720000\n\
Z:\\root\\unknown.bin,File,1,1073741824,0,0,\n";
    let tree = read_csv(csv.as_bytes()).unwrap();
    let policy = StoragePolicy {
        now_utc: 2000000000,
        warm_after_days: 30,
        cold_after_days: 180,
        monthly_prices: [0.1, 0.03, 0.01],
    };
    let result = estimate(&tree, &policy).unwrap();
    assert_eq!(result.allocated_bytes, [2147483648, 2147483648, 3221225472]);
    assert_eq!(result.files, [2, 1, 1]);
    assert_eq!(result.unknown_dates, 1);
    assert!((result.monthly_cost - 0.29).abs() < 0.000001);
    assert!((result.all_hot_cost - 0.7).abs() < 0.000001);
    assert!(estimate(
        &tree,
        &StoragePolicy {
            cold_after_days: 10,
            ..policy.clone()
        }
    )
    .is_err());
    assert!(estimate(
        &tree,
        &StoragePolicy {
            monthly_prices: [f64::NAN, 0.03, 0.01],
            ..policy
        }
    )
    .is_err());
}
