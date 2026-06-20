//! Equity-curve extraction package.
//!
//! Extracts per-step equity values from evidence outcomes.

use crate::protocol::EvidenceBundle;

/// Extract finite numeric `equity` fields from decision-trace outcomes.
pub fn extract(evidence: &EvidenceBundle) -> Vec<f64> {
    evidence
        .decision_traces
        .iter()
        .filter_map(|trace| trace.outcome.get("equity")?.as_f64())
        .filter(|equity| equity.is_finite())
        .collect()
}
