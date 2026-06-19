//! Proof-level resolver package.
//!
//! Derives the public [`ProofLevel`](crate::verifier::ProofLevel) from evidence,
//! review outcomes, and replication records. The level is computed from records
//! rather than chosen by the author.

use crate::protocol::EvidenceBundle;
use crate::verifier::{ProofLevel, Replication, Review, ReviewDecision};

/// Resolve the maximum proof level reached by the supplied records.
pub fn resolve(
    evidence: &EvidenceBundle,
    reviews: &[Review],
    replications: &[Replication],
) -> ProofLevel {
    if evidence.decision_traces.is_empty() {
        return ProofLevel::PrivateDraft;
    }

    let mut level = ProofLevel::Committed;
    if reviews
        .iter()
        .any(|review| matches!(review.decision, ReviewDecision::Approve))
    {
        level = ProofLevel::Auditable;
    }
    if replications.iter().any(|replication| replication.success) {
        level = ProofLevel::Reproducible;
    }
    level
}
