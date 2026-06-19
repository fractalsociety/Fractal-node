//! Reviewer-grant package.
//!
//! Issue, revoke, and validate reviewer access grants over a proof using logical
//! (non-wall-clock) time.

/// Logical access grant for a reviewer over a proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewerGrant {
    /// Proof identifier covered by the grant.
    pub proof_id: String,
    /// Reviewer identity receiving access.
    pub reviewer: String,
    /// Logical grant timestamp.
    pub granted_at: u64,
    /// Logical expiry timestamp.
    pub expires_at: u64,
    /// Whether the grant has been revoked.
    pub revoked: bool,
}

/// Issue a reviewer grant using caller-supplied logical time.
pub fn issue(proof_id: &str, reviewer: &str, granted_at: u64, ttl_seconds: u64) -> ReviewerGrant {
    ReviewerGrant {
        proof_id: proof_id.to_string(),
        reviewer: reviewer.to_string(),
        granted_at,
        expires_at: granted_at.saturating_add(ttl_seconds),
        revoked: false,
    }
}

/// Revoke a reviewer grant.
pub fn revoke(grant: &mut ReviewerGrant) {
    grant.revoked = true;
}

/// Return true when the grant is not revoked and `now` is before expiry.
pub fn is_valid(grant: &ReviewerGrant, now: u64) -> bool {
    !grant.revoked && now < grant.expires_at
}
