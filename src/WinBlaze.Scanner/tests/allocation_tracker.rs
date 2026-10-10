use winblaze_scanner::allocation::AllocationTracker;
#[test]
fn descending_references_at_u64_limit_preserve_every_identity() {
    let mut tracker = AllocationTracker::default();
    for offset in 0..20_000 {
        let id = u128::from(u64::MAX - offset);
        assert_eq!(tracker.claim((7, id), 4096), 4096);
    }
    for offset in (0..20_000).rev() {
        assert_eq!(tracker.claim((7, u128::from(u64::MAX - offset)), 4096), 0);
    }
}

#[test]
fn growing_identity_ranges_and_outliers_match_exact_accounting_after_volume_promotion() {
    let mut tracker = AllocationTracker::default();
    let mut expected = std::collections::HashSet::new();
    for round in 0..3 {
        for id in 0..25_000u128 {
            let identity = match round {
                0 => (7, (3 << 48) + id),
                1 => (8, u128::from(u64::MAX) - id),
                _ => (7 + (id % 3) as u64, id * 1024),
            };
            assert_eq!(tracker.claim(identity, 0), 0);
            let bytes = if expected.insert(identity) { 4096 } else { 0 };
            assert_eq!(tracker.claim(identity, 4096), bytes);
            assert_eq!(tracker.claim(identity, 8192), 0);
        }
    }
    for identity in expected {
        assert_eq!(tracker.claim(identity, 16384), 0);
        let wide = (identity.0, identity.1 + (1u128 << 64));
        assert_eq!(tracker.claim(wide, 8192), 8192);
        assert_eq!(tracker.claim(wide, 8192), 0);
    }
}
#[test]
fn physical_storage_claims_preserve_volume_and_full_128_bit_identity() {
    let mut tracker = AllocationTracker::default();
    assert_eq!(tracker.claim((1, 1), 4096), 4096);
    assert_eq!(tracker.claim((1, 1), 4096), 0);
    assert_eq!(tracker.claim((2, 1), 4096), 4096);
    assert_eq!(tracker.claim((1, 1), 4096), 0);
    assert_eq!(tracker.claim((1, 1 + (1u128 << 64)), 8192), 8192);
    assert_eq!(tracker.claim((1, 9), 0), 0);
    assert_eq!(tracker.claim((1, 9), 4096), 4096);
}
#[test]
fn simultaneous_hardlink_claims_allocate_storage_exactly_once() {
    let tracker = winblaze_scanner::allocation::ConcurrentAllocationTracker::default();
    let claimed: u64 = std::thread::scope(|scope| {
        (0..20)
            .map(|_| {
                scope.spawn(|| {
                    (1..=10_000)
                        .map(|id| tracker.claim((7, id), 4096))
                        .sum::<u64>()
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .sum()
    });
    assert_eq!(claimed, 10_000 * 4096);
}

#[test]
fn interleaved_volume_and_wide_identity_claims_remain_unique_across_workers() {
    let tracker = winblaze_scanner::allocation::ConcurrentAllocationTracker::default();
    let claimed: u64 = std::thread::scope(|scope| {
        (0..8)
            .map(|worker| {
                let tracker = &tracker;
                scope.spawn(move || {
                    let mut bytes = 0;
                    for id in 0..512 {
                        for offset in 0..2 {
                            let volume = 7 + (worker + offset) % 2;
                            bytes += tracker.claim((volume, id), 4096);
                            bytes += tracker.claim((volume, id + (1u128 << 64)), 8192);
                        }
                    }
                    bytes
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .sum()
    });
    assert_eq!(claimed, 512 * 2 * (4096 + 8192));
}
