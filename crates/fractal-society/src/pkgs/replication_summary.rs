//! Replication-summary package.
//!
//! Aggregates replication records into simple pass/fail counts.

use crate::verifier::Replication;

/// Summary of a replication set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplicationSummary {
    /// Total replication records.
    pub total: usize,
    /// Replications marked successful.
    pub successful: usize,
    /// Replications marked failed.
    pub failed: usize,
    /// Whether at least one replication succeeded.
    pub any_success: bool,
}

/// Summarize replication records by their `success` field.
pub fn summarize(replications: &[Replication]) -> ReplicationSummary {
    let total = replications.len();
    let successful = replications
        .iter()
        .filter(|replication| replication.success)
        .count();
    let failed = total - successful;

    ReplicationSummary {
        total,
        successful,
        failed,
        any_success: successful > 0,
    }
}
