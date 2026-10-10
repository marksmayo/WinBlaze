//! Hypothetical monthly tier costs from allocated bytes and UTC modification age.
use crate::TreeIndex;
use std::io;

#[derive(Clone, Debug)]
pub struct StoragePolicy {
    pub now_utc: i64,
    pub warm_after_days: u64,
    pub cold_after_days: u64,
    /// Prices in the user's currency per GiB per month, hot/warm/cold.
    pub monthly_prices: [f64; 3],
}
#[derive(Default, Debug)]
pub struct StorageEstimate {
    pub allocated_bytes: [u64; 3],
    pub files: [u64; 3],
    pub unknown_dates: u64,
    pub monthly_cost: f64,
    pub all_hot_cost: f64,
}
pub fn estimate(tree: &TreeIndex, policy: &StoragePolicy) -> io::Result<StorageEstimate> {
    if policy.warm_after_days > policy.cold_after_days
        || policy
            .monthly_prices
            .iter()
            .any(|price| !price.is_finite() || *price < 0.0)
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Invalid tier thresholds or prices",
        ));
    }
    let mut result = StorageEstimate::default();
    for file in tree.files() {
        let tier = if let Some(modified) = file.modified_utc() {
            let age = policy
                .now_utc
                .saturating_sub(crate::export::unix_seconds(modified))
                .max(0) as u64
                / 86400;
            if age >= policy.cold_after_days {
                2
            } else if age >= policy.warm_after_days {
                1
            } else {
                0
            }
        } else {
            result.unknown_dates += 1;
            0
        };
        result.allocated_bytes[tier] = result.allocated_bytes[tier]
            .checked_add(file.allocation_bytes)
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "Allocated byte total overflow")
            })?;
        result.files[tier] += 1;
    }
    for tier in 0..3 {
        let gib = result.allocated_bytes[tier] as f64 / 1073741824.0;
        result.monthly_cost += gib * policy.monthly_prices[tier];
        result.all_hot_cost += gib * policy.monthly_prices[0];
    }
    if !result.monthly_cost.is_finite() || !result.all_hot_cost.is_finite() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Projected price overflow",
        ));
    }
    Ok(result)
}
