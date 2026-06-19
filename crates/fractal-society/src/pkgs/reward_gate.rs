//! Reward-gate decision package.
//!
//! Decide whether a reward may release given the set of verifier pass-states and
//! the review/challenge window status (this package defines the decision type
//! locally).

use crate::verifier::VerifierReport;

/// Decision returned by the reward gate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RewardDecision {
    /// Release the reward.
    Release,
    /// Withhold the reward with deterministic explanatory reasons.
    Withhold {
        /// Reasons the reward cannot release.
        reasons: Vec<String>,
    },
}

/// Evaluate whether a reward may release.
///
/// Release requires a closed challenge window, every supplied verifier report to
/// pass, and at least `min_required_pass` passing verifier reports.
pub fn evaluate(
    verifier_reports: &[VerifierReport],
    challenge_window_open: bool,
    min_required_pass: usize,
) -> RewardDecision {
    let mut reasons = Vec::new();

    if challenge_window_open {
        reasons.push("challenge window is open".to_string());
    }

    let passed = verifier_reports
        .iter()
        .filter(|report| report.passed)
        .count();
    let failed: Vec<&str> = verifier_reports
        .iter()
        .filter(|report| !report.passed)
        .map(|report| report.verifier_id.as_str())
        .collect();

    if !failed.is_empty() {
        reasons.push(format!("failed verifiers: {}", failed.join(", ")));
    }

    if passed < min_required_pass {
        reasons.push(format!(
            "too few passing verifiers: required {min_required_pass}, got {passed}",
        ));
    }

    if reasons.is_empty() {
        RewardDecision::Release
    } else {
        RewardDecision::Withhold { reasons }
    }
}
