//! Evidence-summary package.
//!
//! Builds compact evidence summaries with metrics and action-type counts.

use std::collections::HashMap;

use crate::protocol::EvidenceBundle;

/// Compact summary of an evidence bundle.
#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceSummary {
    /// Number of decision trace steps.
    pub step_count: usize,
    /// Snapshot of run metrics.
    pub metrics: HashMap<String, f64>,
    /// Counts keyed by each action JSON object's top-level variant name.
    pub action_type_counts: HashMap<String, u64>,
}

/// Summarize an evidence bundle without mutating it.
pub fn summarize(evidence: &EvidenceBundle) -> EvidenceSummary {
    let mut action_type_counts = HashMap::new();
    for trace in &evidence.decision_traces {
        let action_type = action_type(&trace.action);
        *action_type_counts.entry(action_type).or_insert(0) += 1;
    }

    EvidenceSummary {
        step_count: evidence.decision_traces.len(),
        metrics: evidence.metrics.clone(),
        action_type_counts,
    }
}

fn action_type(action: &serde_json::Value) -> String {
    action
        .as_object()
        .and_then(|object| object.keys().next())
        .cloned()
        .unwrap_or_else(|| "unknown".to_string())
}
