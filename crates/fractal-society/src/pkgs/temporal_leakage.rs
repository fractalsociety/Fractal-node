//! Temporal-leakage verifier package.
//!
//! Detects non-monotonic or duplicate decision-trace step indices. A valid
//! evidence bundle must have strictly increasing steps, which rules out simple
//! tampering and look-ahead ordering leaks.

use chrono::{DateTime, Utc};
use serde_json::json;

use crate::protocol::EvidenceBundle;
use crate::verifier::VerifierReport;

/// Verifier id for this package.
pub const VERIFIER_ID: &str = "temporal-leakage";

const VERIFIER_VERSION: &str = "0.1.0";

/// Verify that decision-trace steps are strictly increasing.
pub fn verify(evidence: &EvidenceBundle) -> VerifierReport {
    let mut errors = Vec::new();
    let mut previous = None;

    for trace in &evidence.decision_traces {
        if let Some(prev) = previous {
            if trace.step <= prev {
                errors.push(format!(
                    "non-monotonic step: previous={prev}, current={}",
                    trace.step
                ));
            }
        }
        previous = Some(trace.step);
    }

    let passed = errors.is_empty();
    report(
        passed,
        Some(if passed { 1.0 } else { 0.0 }),
        json!({
            "trace_count": evidence.decision_traces.len(),
            "violations": errors.len(),
        }),
        Vec::new(),
        errors,
    )
}

fn report(
    passed: bool,
    score: Option<f64>,
    details: serde_json::Value,
    warnings: Vec<String>,
    errors: Vec<String>,
) -> VerifierReport {
    VerifierReport {
        id: format!("{VERIFIER_ID}-report"),
        verifier_id: VERIFIER_ID.to_string(),
        verifier_version: VERIFIER_VERSION.to_string(),
        passed,
        score,
        details,
        warnings,
        errors,
        execution_time_seconds: 0.0,
        timestamp: DateTime::<Utc>::from_timestamp(0, 0).expect("epoch timestamp is valid"),
    }
}
