//! Scorecard-reproduction verifier package.
//!
//! Re-derives `total_pnl` from the last parseable evidence outcome and compares
//! it with `scorecard.primary_metrics["total_pnl"].value`.

use chrono::{DateTime, Utc};
use serde_json::json;

use crate::protocol::EvidenceBundle;
use crate::verifier::{Scorecard, VerifierReport};

/// Verifier id for this package.
pub const VERIFIER_ID: &str = "scorecard-reproduction";

const VERIFIER_VERSION: &str = "0.1.0";
const TOTAL_PNL_METRIC: &str = "total_pnl";

/// Return a report; `passed` is true iff the scorecard total PnL matches the evidence.
pub fn verify(evidence: &EvidenceBundle, scorecard: &Scorecard, tolerance: f64) -> VerifierReport {
    if !tolerance.is_finite() || tolerance < 0.0 {
        return report(
            false,
            None,
            json!({
                "metric": TOTAL_PNL_METRIC,
                "tolerance": tolerance,
                "evidence_total_pnl": null,
                "scorecard_total_pnl": null,
            }),
            Vec::new(),
            vec!["tolerance must be finite and non-negative".to_string()],
        );
    }

    let Some((step, evidence_total_pnl)) = last_total_pnl(evidence) else {
        return report(
            false,
            None,
            json!({
                "metric": TOTAL_PNL_METRIC,
                "tolerance": tolerance,
                "evidence_total_pnl": null,
                "scorecard_total_pnl": scorecard_total_pnl(scorecard),
            }),
            Vec::new(),
            vec!["no parseable total_pnl outcome found in evidence".to_string()],
        );
    };

    let Some(scorecard_total_pnl) = scorecard_total_pnl(scorecard) else {
        return report(
            false,
            None,
            json!({
                "metric": TOTAL_PNL_METRIC,
                "last_evidence_step": step,
                "tolerance": tolerance,
                "evidence_total_pnl": evidence_total_pnl,
                "scorecard_total_pnl": null,
            }),
            Vec::new(),
            vec!["scorecard primary_metrics.total_pnl is missing or non-finite".to_string()],
        );
    };

    let difference = (evidence_total_pnl - scorecard_total_pnl).abs();
    let passed = difference <= tolerance;
    report(
        passed,
        Some(if passed { 1.0 } else { 0.0 }),
        json!({
            "metric": TOTAL_PNL_METRIC,
            "last_evidence_step": step,
            "tolerance": tolerance,
            "evidence_total_pnl": evidence_total_pnl,
            "scorecard_total_pnl": scorecard_total_pnl,
            "difference": difference,
        }),
        Vec::new(),
        if passed {
            Vec::new()
        } else {
            vec!["scorecard total_pnl does not match evidence".to_string()]
        },
    )
}

fn last_total_pnl(evidence: &EvidenceBundle) -> Option<(u64, f64)> {
    evidence.decision_traces.iter().rev().find_map(|trace| {
        trace
            .outcome
            .get(TOTAL_PNL_METRIC)
            .and_then(serde_json::Value::as_f64)
            .filter(|value| value.is_finite())
            .map(|value| (trace.step, value))
    })
}

fn scorecard_total_pnl(scorecard: &Scorecard) -> Option<f64> {
    scorecard
        .primary_metrics
        .get(TOTAL_PNL_METRIC)
        .map(|metric| metric.value)
        .filter(|value| value.is_finite())
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
