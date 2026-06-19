//! Verifier-summary aggregation package.
//!
//! Aggregates verifier reports into the count summary used by scorecards and
//! public proof surfaces.

use crate::verifier::{VerifierReport, VerifierSummary};

/// Aggregate verifier reports into a summary.
pub fn summarize(reports: &[VerifierReport]) -> VerifierSummary {
    let total = reports.len() as u64;
    let passed = reports.iter().filter(|report| report.passed).count() as u64;
    VerifierSummary {
        total_verifiers: total,
        verifiers_passed: passed,
        verifiers_failed: total - passed,
        required_passed: passed,
        required_total: total,
    }
}
