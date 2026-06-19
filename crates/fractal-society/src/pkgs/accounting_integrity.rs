//! Accounting-integrity verifier package.
//!
//! Re-verifies per-step trading accounting from public evidence. For each
//! decision trace whose outcome contains `equity`, `cash`, and
//! `position_notional`, the verifier checks that:
//!
//! `abs(equity - (cash + position_notional)) <= tolerance`.

use chrono::{DateTime, Utc};
use serde_json::json;

use crate::protocol::EvidenceBundle;
use crate::verifier::VerifierReport;

/// Verifier id for this package.
pub const VERIFIER_ID: &str = "accounting-integrity";

const VERIFIER_VERSION: &str = "0.1.0";

/// Return a report; `passed` is true iff every checked step reconciles.
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
        let has_equity = trace.outcome.get("equity").is_some();
        let has_cash = trace.outcome.get("cash").is_some();
        let has_position = trace.outcome.get("position_notional").is_some();
        if !(has_equity || has_cash || has_position) {
            continue;
        }
        if !(has_equity && has_cash && has_position) {
            failures.push(json!({
                "step": trace.step,
                "error": "partial accounting fields",
            }));
            continue;
        }

        let Some(equity) = number_field(&trace.outcome, "equity") else {
            failures.push(json!({ "step": trace.step, "error": "equity is not numeric" }));
            continue;
        };
        let Some(cash) = number_field(&trace.outcome, "cash") else {
            failures.push(json!({ "step": trace.step, "error": "cash is not numeric" }));
            continue;
        };
        let Some(position_notional) = number_field(&trace.outcome, "position_notional") else {
            failures.push(json!({
                "step": trace.step,
                "error": "position_notional is not numeric",
            }));
            continue;
        };

        checked_steps += 1;
        let expected = cash + position_notional;
        let diff = (equity - expected).abs();
        if diff > tolerance {
            failures.push(json!({
                "step": trace.step,
                "equity": equity,
                "cash": cash,
                "position_notional": position_notional,
                "expected_equity": expected,
                "difference": diff,
                "tolerance": tolerance,
            }));
        }
    }

    if checked_steps == 0 {
        warnings.push("no accounting outcomes found in evidence".to_string());
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
            vec!["accounting reconciliation failed".to_string()]
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
