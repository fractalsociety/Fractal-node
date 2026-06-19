//! Reputation-event derivation package.
//!
//! Converts verifier and review outcomes into deterministic local
//! reputation-event records. This package intentionally owns the event types
//! locally so it can land independently of the future reputation subsystem.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::protocol::Hash;
use crate::verifier::VerifierReport;

/// Type of reputation event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReputationKind {
    /// A verifier report passed.
    VerifiedPass,
    /// A verifier report failed.
    VerifiedFail,
    /// A review approved the subject.
    ReviewApproved,
    /// A review rejected the subject.
    ReviewRejected,
}

/// Deterministic reputation event derived from verification or review.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReputationEvent {
    /// Stable event identifier.
    pub id: String,
    /// Subject receiving the reputation delta.
    pub subject: String,
    /// Event category.
    pub kind: ReputationKind,
    /// Reputation delta applied by this event.
    pub delta: i64,
    /// Hash reference to the evidence used to derive this event.
    pub evidence_ref: Hash,
    /// Logical timestamp supplied by the caller.
    pub timestamp: DateTime<Utc>,
}

/// Build a reputation event from a verifier report.
pub fn from_verifier(
    report: &VerifierReport,
    subject: &str,
    timestamp: DateTime<Utc>,
) -> ReputationEvent {
    let (kind, delta) = if report.passed {
        (ReputationKind::VerifiedPass, 1)
    } else {
        (ReputationKind::VerifiedFail, -1)
    };
    let evidence_ref = Hash::of(report).unwrap_or_else(|_| Hash::new(report.id.as_bytes()));
    event(subject, kind, delta, evidence_ref, timestamp)
}

/// Build a reputation event from a review outcome.
pub fn from_review(subject: &str, approved: bool, timestamp: DateTime<Utc>) -> ReputationEvent {
    let (kind, delta) = if approved {
        (ReputationKind::ReviewApproved, 2)
    } else {
        (ReputationKind::ReviewRejected, -2)
    };
    let evidence_ref = Hash::new(format!("review:{subject}:{approved}:{timestamp:?}").as_bytes());
    event(subject, kind, delta, evidence_ref, timestamp)
}

fn event(
    subject: &str,
    kind: ReputationKind,
    delta: i64,
    evidence_ref: Hash,
    timestamp: DateTime<Utc>,
) -> ReputationEvent {
    let id = Hash::new(
        format!(
            "reputation:{subject}:{kind:?}:{delta}:{}:{}",
            evidence_ref.0,
            timestamp.to_rfc3339()
        )
        .as_bytes(),
    )
    .0;
    ReputationEvent {
        id,
        subject: subject.to_string(),
        kind,
        delta,
        evidence_ref,
        timestamp,
    }
}
