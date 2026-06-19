//! Baseline-correctness verifier package.
//!
//! Re-verify the baseline-comparison arithmetic stored in a `Scorecard`
//! (`difference`, `percent_difference`, `is_better`).

use chrono::{DateTime, Utc};
use serde_json::json;

use crate::verifier::{BaselineResult, Scorecard, VerifierReport};

/// Verifier id for this package.
pub const VERIFIER_ID: &str = "baseline-correctness";

const VERIFIER_VERSION: &str = "0.1.0";
const ZERO_BASELINE_EPSILON: f64 = 1e-12;

/// Return a report; `passed` is true iff every baseline comparison matches the
/// scorecard's candidate/baseline arithmetic within `tolerance`.
pub fn verify(scorecard: &Scorecard, tolerance: f64) -> VerifierReport {
    if !tolerance.is_finite() || tolerance < 0.0 {
        return report(
            false,
            None,
            json!({
                "tolerance": tolerance,
                "checked_baselines": 0,
                "failed_baselines": 0,
                "failures": [],
            }),
            Vec::new(),
            vec!["tolerance must be finite and non-negative".to_string()],
        );
    }

    let mut failures = Vec::new();

    for (name, baseline) in &scorecard.baselines {
        match expected(baseline) {
            Some((expected_difference, expected_percent, expected_is_better)) => {
                let difference_error = (baseline.difference - expected_difference).abs();
                let percent_error = (baseline.percent_difference - expected_percent).abs();
                let is_better_matches = baseline.is_better == expected_is_better;
                if difference_error > tolerance || percent_error > tolerance || !is_better_matches {
                    failures.push(json!({
                        "baseline": name,
                        "stored_difference": baseline.difference,
                        "expected_difference": expected_difference,
                        "difference_error": difference_error,
                        "stored_percent_difference": baseline.percent_difference,
                        "expected_percent_difference": expected_percent,
                        "percent_error": percent_error,
                        "stored_is_better": baseline.is_better,
                        "expected_is_better": expected_is_better,
                    }));
                }
            }
            None => failures.push(json!({
                "baseline": name,
                "error": "baseline comparison contains non-finite values",
            })),
        }
    }

    let checked_baselines = scorecard.baselines.len() as u64;
    let failed_baselines = failures.len() as u64;
    let passed = failures.is_empty();
    let score = if checked_baselines == 0 {
        Some(1.0)
    } else {
        Some((checked_baselines.saturating_sub(failed_baselines)) as f64 / checked_baselines as f64)
    };

    report(
        passed,
        score,
        json!({
            "tolerance": tolerance,
            "checked_baselines": checked_baselines,
            "failed_baselines": failed_baselines,
            "failures": failures,
        }),
        Vec::new(),
        if passed {
            Vec::new()
        } else {
            vec!["baseline comparison arithmetic failed".to_string()]
        },
    )
}

fn expected(baseline: &BaselineResult) -> Option<(f64, f64, bool)> {
    if !baseline.candidate_value.is_finite()
        || !baseline.baseline_value.is_finite()
        || !baseline.difference.is_finite()
        || !baseline.percent_difference.is_finite()
    {
        return None;
    }

    let difference = baseline.candidate_value - baseline.baseline_value;
    let percent_difference = if baseline.baseline_value.abs() < ZERO_BASELINE_EPSILON {
        0.0
    } else {
        difference / baseline.baseline_value.abs() * 100.0
    };
    Some((difference, percent_difference, difference > 0.0))
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
