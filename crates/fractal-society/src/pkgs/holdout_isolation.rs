//! Holdout-isolation verifier package.
//!
//! Verifies that private holdout identifiers do not appear in public evidence.

use chrono::{DateTime, Utc};
use serde_json::json;

use crate::protocol::EvidenceBundle;
use crate::verifier::VerifierReport;

/// Verifier id for this package.
pub const VERIFIER_ID: &str = "holdout-isolation";

const VERIFIER_VERSION: &str = "0.1.0";

/// Return a report; `passed` is true iff no private identifier appears in the evidence.
pub fn verify(evidence: &EvidenceBundle, private_ids: &[String]) -> VerifierReport {
    let private_ids: Vec<&str> = private_ids
        .iter()
        .map(String::as_str)
        .filter(|id| !id.is_empty())
        .collect();
    let mut leaks = Vec::new();

    for trace in &evidence.decision_traces {
        collect_json_leaks(
            trace.step,
            "action",
            &trace.action,
            &private_ids,
            &mut leaks,
        );
        collect_json_leaks(
            trace.step,
            "outcome",
            &trace.outcome,
            &private_ids,
            &mut leaks,
        );
        for private_id in &private_ids {
            if trace.observation_hash.0.contains(private_id) {
                leaks.push(json!({
                    "step": trace.step,
                    "field": "observation_hash",
                    "private_id": private_id,
                }));
            }
        }
    }

    let leak_count = leaks.len() as u64;
    let passed = leaks.is_empty();
    report(
        passed,
        Some(if passed { 1.0 } else { 0.0 }),
        json!({
            "private_id_count": private_ids.len(),
            "leak_count": leak_count,
            "leaks": leaks,
        }),
        Vec::new(),
        if passed {
            Vec::new()
        } else {
            vec!["private holdout identifier leaked into evidence".to_string()]
        },
    )
}

fn collect_json_leaks(
    step: u64,
    field: &str,
    value: &serde_json::Value,
    private_ids: &[&str],
    leaks: &mut Vec<serde_json::Value>,
) {
    match value {
        serde_json::Value::String(text) => {
            for private_id in private_ids {
                if text.contains(private_id) {
                    leaks.push(json!({
                        "step": step,
                        "field": field,
                        "private_id": private_id,
                    }));
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_json_leaks(step, field, item, private_ids, leaks);
            }
        }
        serde_json::Value::Object(map) => {
            for (key, item) in map {
                for private_id in private_ids {
                    if key.contains(private_id) {
                        leaks.push(json!({
                            "step": step,
                            "field": field,
                            "private_id": private_id,
                        }));
                    }
                }
                collect_json_leaks(step, field, item, private_ids, leaks);
            }
        }
        serde_json::Value::Null | serde_json::Value::Bool(_) | serde_json::Value::Number(_) => {}
    }
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
