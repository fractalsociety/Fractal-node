//! Review-aggregation package.
//!
//! Aggregates review records into a quorum-gated consensus decision.

use crate::verifier::{Review, ReviewDecision};

/// Consensus decision from a review set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Consensus {
    /// Approvals are a strict majority of non-abstaining reject votes.
    Approved,
    /// Rejections tie or outnumber approvals once quorum is met.
    Rejected,
    /// Not enough review records to meet quorum.
    NoQuorum,
}

/// Aggregate reviews with a quorum; ties resolve to rejected.
pub fn aggregate(reviews: &[Review], quorum: usize) -> Consensus {
    if reviews.len() < quorum {
        return Consensus::NoQuorum;
    }

    let mut approvals = 0usize;
    let mut rejections = 0usize;
    for review in reviews {
        match review.decision {
            ReviewDecision::Approve => approvals += 1,
            ReviewDecision::RequestChanges | ReviewDecision::Reject => rejections += 1,
            ReviewDecision::Abstain => {}
        }
    }

    if approvals > rejections {
        Consensus::Approved
    } else {
        Consensus::Rejected
    }
}
