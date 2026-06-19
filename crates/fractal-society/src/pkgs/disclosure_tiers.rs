//! Disclosure-tier redaction package.
//!
//! Maps a `Visibility` tier to a redacted copy of an `EvidenceBundle` so public
//! disclosure can prove commitments without leaking raw traces.

use crate::protocol::{EvidenceBundle, RiskDecision, Visibility};

/// Return a redacted copy of `evidence` appropriate for `tier`.
pub fn redact(evidence: &EvidenceBundle, tier: Visibility) -> EvidenceBundle {
    match tier {
        Visibility::Private => {
            let mut redacted = evidence.clone();
            redacted.decision_traces.clear();
            redacted
        }
        Visibility::CommittedPrivate | Visibility::ReviewerAccess => {
            let mut redacted = evidence.clone();
            for trace in &mut redacted.decision_traces {
                trace.action = serde_json::Value::Null;
                trace.outcome = serde_json::Value::Null;
                trace.risk_decision = RiskDecision::Approved;
            }
            redacted
        }
        Visibility::PartialPublic => {
            let mut redacted = evidence.clone();
            for trace in &mut redacted.decision_traces {
                trace.outcome = serde_json::Value::Null;
            }
            redacted
        }
        Visibility::Open => evidence.clone(),
    }
}
