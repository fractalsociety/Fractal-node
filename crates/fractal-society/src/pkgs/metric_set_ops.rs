//! Metric-set operations package.
//!
//! Combine/merge multiple `MetricSet`s (union of metric maps, averaged primary
//! metric) for cross-run aggregation.

use std::collections::HashMap;

use crate::simulation::MetricSet;

/// Merge metric sets by unioning metric maps and averaging primary metrics.
///
/// Duplicate metric keys are resolved by taking the last value in `sets`.
pub fn merge(sets: &[MetricSet]) -> MetricSet {
    let mut metrics = HashMap::new();
    let mut confidence_intervals = HashMap::new();
    let mut primary_sum = 0.0_f64;

    for set in sets {
        primary_sum += set.primary_metric;
        for (key, value) in &set.metrics {
            metrics.insert(key.clone(), *value);
        }
        for (key, value) in &set.confidence_intervals {
            confidence_intervals.insert(key.clone(), *value);
        }
    }

    MetricSet {
        primary_metric: if sets.is_empty() {
            0.0
        } else {
            primary_sum / sets.len() as f64
        },
        metrics,
        confidence_intervals,
    }
}
