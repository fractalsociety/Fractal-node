//! Review conflict-of-interest checker package.
//!
//! Applies local review eligibility rules for direct conflicts: a reviewer may
//! not review their own proof, and any declared direct financial interest blocks
//! the review.

use serde::{Deserialize, Serialize};

/// Review conflict request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewRequest {
    /// Reviewer identity.
    pub reviewer: String,
    /// Proof author identity.
    pub proof_author: String,
    /// Direct financial interests declared for this proof/review.
    pub financial_interests: Vec<String>,
}

/// Result of applying conflict-of-interest policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConflictOutcome {
    /// Review may proceed.
    Accept,
    /// Review must be rejected with a policy reason.
    Reject {
        /// Rejection reason.
        reason: String,
    },
}

/// Check a review request for direct conflicts.
pub fn check(request: &ReviewRequest) -> ConflictOutcome {
    if request.reviewer == request.proof_author {
        return ConflictOutcome::Reject {
            reason: "self-review is not allowed".to_string(),
        };
    }
    if !request.financial_interests.is_empty() {
        return ConflictOutcome::Reject {
            reason: "direct financial conflict declared".to_string(),
        };
    }
    ConflictOutcome::Accept
}
