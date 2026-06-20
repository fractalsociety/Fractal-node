//! Replication-check package.
//!
//! Re-derives replication success from `actual_difference` and `tolerance`.

use crate::verifier::Replication;

/// Re-derived replication classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplicationClass {
    /// Actual difference is finite and within tolerance.
    Success,
    /// Actual difference is finite and exceeds tolerance.
    Fail,
    /// Actual difference or tolerance is missing or non-finite.
    Indeterminate,
}

/// Classify a replication from its tolerance and actual difference.
pub fn classify(replication: &Replication) -> ReplicationClass {
    let Some(actual_difference) = replication.actual_difference else {
        return ReplicationClass::Indeterminate;
    };
    if !actual_difference.is_finite() || !replication.tolerance.is_finite() {
        return ReplicationClass::Indeterminate;
    }
    if actual_difference <= replication.tolerance {
        ReplicationClass::Success
    } else {
        ReplicationClass::Fail
    }
}

/// Return true iff a replication has a finite actual difference within tolerance.
pub fn within_tolerance(replication: &Replication) -> bool {
    classify(replication) == ReplicationClass::Success
}
