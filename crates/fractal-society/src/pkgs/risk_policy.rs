//! Risk-policy verifier package.
//!
//! Re-verifies that the run-level `policy_violations` metric matches the count
//! of rejected decision traces in the public evidence.

use chrono::{DateTime, Utc};
use serde_json::json;

use crate::protocol::EvidenceBundle;
use crate::verifier::VerifierReport;

/// Verifier id for this package.
pub const VERIFIER_ID: &str = "risk-policy";

const VERIFIER_VERSION: &str = "0.1.0";
const POLICY_VIOLATIONS_METRIC: &str = "policy_violations";

/// Return a report; `passed` is true iff the reported policy-violation metric
/// exactly matches the number of rejected decision traces.
pub fn verify(evidence: &EvidenceBundle) -> VerifierReport {
    let rejected_steps = evidence
        .decision_traces
        .iter()
        .filter(|trace| trace.outcome.get("rejected").is_some())
        .count() as u64;

    let Some(reported) = evidence.metrics.get(POLICY_VIOLATIONS_METRIC).copied() else {
        return report(
            false,
            None,
            json!({
                "metric": POLICY_VIOLATIONS_METRIC,
                "rejected_steps": rejected_steps,
                "reported_policy_violations": null,
            }),
            Vec::new(),
            vec!["missing policy_violations metric".to_string()],
        );
    };

    if !reported.is_finite() || reported < 0.0 || reported.fract() != 0.0 {
        return report(
            false,
            None,
            json!({
                "metric": POLICY_VIOLATIONS_METRIC,
                "rejected_steps": rejected_steps,
                "reported_policy_violations": reported,
            }),
            Vec::new(),
            vec!["policy_violations metric must be a finite non-negative integer".to_string()],
        );
    }

    let reported_steps = reported as u64;
    let passed = reported_steps == rejected_steps;
    report(
        passed,
        Some(if passed { 1.0 } else { 0.0 }),
        json!({
            "metric": POLICY_VIOLATIONS_METRIC,
            "rejected_steps": rejected_steps,
            "reported_policy_violations": reported_steps,
        }),
        Vec::new(),
        if passed {
            Vec::new()
        } else {
            vec!["policy violation count mismatch".to_string()]
        },
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
