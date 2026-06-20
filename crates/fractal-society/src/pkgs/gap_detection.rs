//! Data-gap detection package.
//!
//! Detect gaps in an ordered integer sequence (e.g. bar timestamps / sequence
//! numbers) and emit explicit `DataGap` records (never silently interpolate).

/// Explicit record of missing integer values between two observed sequence
/// values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataGap {
    /// Observed value immediately before the gap.
    pub after: i64,
    /// Observed value immediately after the gap.
    pub before: i64,
    /// Count of missing integer values.
    pub missing_count: i64,
}

/// Detect gaps after sorting a copy of `sequence` in ascending order.
pub fn detect(sequence: &[i64]) -> Vec<DataGap> {
    let mut sorted = sequence.to_vec();
    sorted.sort_unstable();

    sorted
        .windows(2)
        .filter_map(|pair| {
            let after = pair[0];
            let before = pair[1];
            let diff = before - after;
            (diff > 1).then_some(DataGap {
                after,
                before,
                missing_count: diff - 1,
            })
        })
        .collect()
}
