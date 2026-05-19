#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ScanPipelineConfig {
    pub batch_size: usize,
    pub max_in_flight_events: usize,
    pub max_in_flight_bytes: u64,
}

impl Default for ScanPipelineConfig {
    fn default() -> Self {
        Self {
            batch_size: 64,
            max_in_flight_events: 128,
            max_in_flight_bytes: 8 * 1024 * 1024,
        }
    }
}

impl ScanPipelineConfig {
    pub fn should_apply_backpressure(&self, in_flight_events: usize, in_flight_bytes: u64) -> bool {
        in_flight_events >= self.max_in_flight_events || in_flight_bytes >= self.max_in_flight_bytes
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScanThroughputSample {
    pub files_seen: u64,
    pub bytes_seen: u64,
    pub elapsed_millis: u64,
}

impl ScanThroughputSample {
    pub fn files_per_second(self) -> u64 {
        if self.elapsed_millis == 0 {
            0
        } else {
            self.files_seen.saturating_mul(1000) / self.elapsed_millis
        }
    }

    pub fn bytes_per_second(self) -> u64 {
        if self.elapsed_millis == 0 {
            0
        } else {
            self.bytes_seen.saturating_mul(1000) / self.elapsed_millis
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ScanMemorySample {
    pub files_seen: u64,
    pub resident_bytes: u64,
    pub peak_bytes: u64,
}

impl ScanMemorySample {
    pub fn resident_bytes_per_file(self) -> u64 {
        if self.files_seen == 0 {
            0
        } else {
            self.resident_bytes / self.files_seen
        }
    }

    pub fn peak_bytes_per_file(self) -> u64 {
        if self.files_seen == 0 {
            0
        } else {
            self.peak_bytes / self.files_seen
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LargeUiScalePlan {
    pub files_seen: u64,
    pub scan_batch_size: usize,
    pub catalog_load_limit: usize,
    pub visible_row_limit: usize,
    pub treemap_tile_limit: usize,
}

impl LargeUiScalePlan {
    pub fn projected_scan_flushes(self) -> u64 {
        if self.files_seen == 0 {
            return 0;
        }
        let batch = self.scan_batch_size.max(1) as u64;
        self.files_seen.div_ceil(batch)
    }

    pub fn materialized_catalog_rows(self) -> usize {
        self.files_seen
            .min(self.catalog_load_limit as u64)
            .try_into()
            .unwrap_or(self.catalog_load_limit)
    }

    pub fn visible_tree_rows(self) -> usize {
        self.materialized_catalog_rows().min(self.visible_row_limit)
    }

    pub fn visible_treemap_tiles(self) -> usize {
        self.materialized_catalog_rows()
            .min(self.treemap_tile_limit)
    }

    pub fn catalog_rows_per_million_files(self) -> f64 {
        if self.files_seen == 0 {
            return 0.0;
        }
        self.materialized_catalog_rows() as f64 / (self.files_seen as f64 / 1_000_000.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_config_applies_backpressure_when_thresholds_are_hit() {
        let config = ScanPipelineConfig::default();
        assert!(!config.should_apply_backpressure(1, 1024));
        assert!(config.should_apply_backpressure(config.max_in_flight_events, 1024));
        assert!(config.should_apply_backpressure(1, config.max_in_flight_bytes));
    }

    #[test]
    fn throughput_sample_calculates_rates() {
        let sample = ScanThroughputSample {
            files_seen: 10,
            bytes_seen: 1_000,
            elapsed_millis: 500,
        };

        assert_eq!(sample.files_per_second(), 20);
        assert_eq!(sample.bytes_per_second(), 2_000);
    }

    #[test]
    fn memory_sample_calculates_per_file_overhead() {
        let sample = ScanMemorySample {
            files_seen: 4,
            resident_bytes: 2_000,
            peak_bytes: 3_000,
        };

        assert_eq!(sample.resident_bytes_per_file(), 500);
        assert_eq!(sample.peak_bytes_per_file(), 750);
    }

    #[test]
    fn large_ui_scale_plan_bounds_materialized_views_for_tens_of_millions() {
        let plan = LargeUiScalePlan {
            files_seen: 50_000_000,
            scan_batch_size: ScanPipelineConfig::default().batch_size,
            catalog_load_limit: 8_192,
            visible_row_limit: 256,
            treemap_tile_limit: 10,
        };

        assert_eq!(plan.projected_scan_flushes(), 781_250);
        assert_eq!(plan.materialized_catalog_rows(), 8_192);
        assert_eq!(plan.visible_tree_rows(), 256);
        assert_eq!(plan.visible_treemap_tiles(), 10);
        assert!(
            plan.catalog_rows_per_million_files() < 200.0,
            "catalog materialization must stay bounded as file count grows"
        );
    }
}
