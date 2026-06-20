//! Sandbox-policy verifier package.
//!
//! Verifies that recorded action tool usage stays within a declared allowlist.

use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde_json::json;

use crate::protocol::EvidenceBundle;
use crate::verifier::VerifierReport;

/// Verifier id for this package.
pub const VERIFIER_ID: &str = "sandbox-policy";

const VERIFIER_VERSION: &str = "0.1.0";

/// Return a report; `passed` is true iff every recorded tool is allowed.
pub fn verify(evidence: &EvidenceBundle, allowed_tools: &[String]) -> VerifierReport {
    let allowed: HashSet<&str> = allowed_tools.iter().map(String::as_str).collect();
    let mut checked_tool_uses = 0_u64;
    let mut failures = Vec::new();

    for trace in &evidence.decision_traces {
        let Some(tool) = trace.action.get("tool").and_then(serde_json::Value::as_str) else {
            continue;
        };
        checked_tool_uses += 1;

        if !allowed.contains(tool) {
            failures.push(json!({
                "step": trace.step,
                "tool": tool,
                "error": "tool not allowed",
            }));
        }
    }

    let failed_tool_uses = failures.len() as u64;
    let passed = failures.is_empty();
    report(
        passed,
        Some(if checked_tool_uses == 0 {
            1.0
        } else {
            (checked_tool_uses.saturating_sub(failed_tool_uses)) as f64 / checked_tool_uses as f64
        }),
        json!({
            "allowed_tools": allowed_tools,
            "checked_tool_uses": checked_tool_uses,
            "failed_tool_uses": failed_tool_uses,
            "failures": failures,
        }),
        Vec::new(),
        if passed {
            Vec::new()
        } else {
            vec!["sandbox policy check failed".to_string()]
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
