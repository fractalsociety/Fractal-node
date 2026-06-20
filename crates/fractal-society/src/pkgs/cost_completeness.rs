//! Cost-completeness verifier package.
//!
//! Re-verifies the per-step PnL invariant:
//!
//! `abs(total_pnl - (realized_pnl + unrealized_pnl - fees)) <= tolerance`.

use chrono::{DateTime, Utc};
use serde_json::json;

use crate::protocol::EvidenceBundle;
use crate::verifier::VerifierReport;

/// Verifier id for this package.
pub const VERIFIER_ID: &str = "cost-completeness";

const VERIFIER_VERSION: &str = "0.1.0";

/// Return a report; `passed` is true iff every checked step includes complete costs.
pub fn verify(evidence: &EvidenceBundle, tolerance: f64) -> VerifierReport {
    let mut checked_steps = 0_u64;
    let mut failures = Vec::new();
    let mut warnings = Vec::new();

    if !tolerance.is_finite() || tolerance < 0.0 {
        return report(
            false,
            None,
            json!({
                "tolerance": tolerance,
                "checked_steps": checked_steps,
                "failed_steps": 0,
                "failures": failures,
            }),
            warnings,
            vec!["tolerance must be finite and non-negative".to_string()],
        );
    }

    for trace in &evidence.decision_traces {
        let has_total = trace.outcome.get("total_pnl").is_some();
        let has_realized = trace.outcome.get("realized_pnl").is_some();
        let has_unrealized = trace.outcome.get("unrealized_pnl").is_some();
        let has_fees = trace.outcome.get("fees").is_some();
        if !(has_total || has_realized || has_unrealized || has_fees) {
            continue;
        }
        if !(has_total && has_realized && has_unrealized && has_fees) {
            failures.push(json!({
                "step": trace.step,
                "error": "partial pnl fields",
            }));
            continue;
        }

        let Some(total_pnl) = number_field(&trace.outcome, "total_pnl") else {
            failures.push(json!({ "step": trace.step, "error": "total_pnl is not numeric" }));
            continue;
        };
        let Some(realized_pnl) = number_field(&trace.outcome, "realized_pnl") else {
            failures.push(json!({ "step": trace.step, "error": "realized_pnl is not numeric" }));
            continue;
        };
        let Some(unrealized_pnl) = number_field(&trace.outcome, "unrealized_pnl") else {
            failures.push(json!({ "step": trace.step, "error": "unrealized_pnl is not numeric" }));
            continue;
        };
        let Some(fees) = number_field(&trace.outcome, "fees") else {
            failures.push(json!({ "step": trace.step, "error": "fees is not numeric" }));
            continue;
        };

        checked_steps += 1;
        let expected = realized_pnl + unrealized_pnl - fees;
        let diff = (total_pnl - expected).abs();
        if diff > tolerance {
            failures.push(json!({
                "step": trace.step,
                "total_pnl": total_pnl,
                "realized_pnl": realized_pnl,
                "unrealized_pnl": unrealized_pnl,
                "fees": fees,
                "expected_total_pnl": expected,
                "difference": diff,
                "tolerance": tolerance,
            }));
        }
    }

    if checked_steps == 0 {
        warnings.push("no pnl outcomes found in evidence".to_string());
    }

    let failed_steps = failures.len() as u64;
    let passed = failures.is_empty();
    let score = if checked_steps == 0 {
        Some(1.0)
    } else {
        Some((checked_steps.saturating_sub(failed_steps)) as f64 / checked_steps as f64)
    };

    report(
        passed,
        score,
        json!({
            "tolerance": tolerance,
            "checked_steps": checked_steps,
            "failed_steps": failed_steps,
            "failures": failures,
        }),
        warnings,
        if passed {
            Vec::new()
        } else {
            vec!["cost completeness check failed".to_string()]
        },
    )
}

fn number_field(value: &serde_json::Value, key: &str) -> Option<f64> {
    value.get(key).and_then(serde_json::Value::as_f64)
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
