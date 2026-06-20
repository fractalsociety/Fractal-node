//! Determinism-audit package.
//!
//! Diffs two evidence bundles step-by-step for action and outcome divergence.

use std::collections::HashMap;

use crate::protocol::EvidenceBundle;

/// Difference found between matching decision-trace steps.
#[derive(Debug, Clone, PartialEq)]
pub struct Divergence {
    /// Decision trace step where the divergence occurred.
    pub step: u64,
    /// Divergent field name.
    pub field: String,
    /// Value from the left/original evidence bundle.
    pub left: serde_json::Value,
    /// Value from the right/replayed evidence bundle.
    pub right: serde_json::Value,
}

/// Diff matching decision-trace steps by comparing their action and outcome JSON.
pub fn diff(left: &EvidenceBundle, right: &EvidenceBundle) -> Vec<Divergence> {
    let right_by_step: HashMap<u64, _> = right
        .decision_traces
        .iter()
        .map(|trace| (trace.step, trace))
        .collect();
    let mut divergences = Vec::new();

    for left_trace in &left.decision_traces {
        let Some(right_trace) = right_by_step.get(&left_trace.step) else {
            continue;
        };

        if left_trace.action != right_trace.action {
            divergences.push(Divergence {
                step: left_trace.step,
                field: "action".to_string(),
                left: left_trace.action.clone(),
                right: right_trace.action.clone(),
            });
        }
        if left_trace.outcome != right_trace.outcome {
            divergences.push(Divergence {
                step: left_trace.step,
                field: "outcome".to_string(),
                left: left_trace.outcome.clone(),
                right: right_trace.outcome.clone(),
            });
        }
    }

    divergences
}
