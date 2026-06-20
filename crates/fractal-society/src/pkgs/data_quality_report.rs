//! Data-quality reporting package.
//!
//! Summarize data quality from an observed sequence + expected range:
//! completeness fraction, gap count, missing count (distinct from gap_detection
//! which only returns the gap list).

use std::collections::HashSet;

/// Summary of sequence completeness over an expected integer range.
#[derive(Debug, Clone, PartialEq)]
pub struct DataQualityReport {
    /// Unique observed values inside the expected range.
    pub observed: usize,
    /// Expected count in the range.
    pub expected: usize,
    /// `observed / expected`, or `1.0` for an empty expected range.
    pub completeness: f64,
    /// Number of contiguous missing runs.
    pub gap_count: usize,
    /// Count of missing expected values.
    pub missing_count: i64,
}

/// Build a data-quality report for `observed` over the inclusive expected range.
pub fn report(observed: &[i64], expected_min: i64, expected_max: i64) -> DataQualityReport {
    let expected = expected_count(expected_min, expected_max);
    if expected == 0 {
        return DataQualityReport {
            observed: 0,
            expected: 0,
            completeness: 1.0,
            gap_count: 0,
            missing_count: 0,
        };
    }

    let observed_in_range: HashSet<i64> = observed
        .iter()
        .copied()
        .filter(|value| *value >= expected_min && *value <= expected_max)
        .collect();
    let observed_count = observed_in_range.len();
    let missing_count = expected as i64 - observed_count as i64;

    DataQualityReport {
        observed: observed_count,
        expected,
        completeness: observed_count as f64 / expected as f64,
        gap_count: gap_count(&observed_in_range, expected_min, expected_max),
        missing_count,
    }
}

fn expected_count(expected_min: i64, expected_max: i64) -> usize {
    if expected_max < expected_min {
        0
    } else {
        expected_max
            .checked_sub(expected_min)
            .and_then(|diff| diff.checked_add(1))
            .and_then(|count| usize::try_from(count).ok())
            .unwrap_or(usize::MAX)
    }
}

fn gap_count(observed: &HashSet<i64>, expected_min: i64, expected_max: i64) -> usize {
    let mut gaps = 0_usize;
    let mut in_gap = false;

    for value in expected_min..=expected_max {
        if observed.contains(&value) {
            in_gap = false;
        } else if !in_gap {
            gaps += 1;
            in_gap = true;
        }
    }

    gaps
}
