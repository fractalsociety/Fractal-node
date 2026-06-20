//! Protocol validation package.
//!
//! Validates the required fields and finite policy values for a research
//! [`Protocol`](crate::protocol::Protocol).

use crate::protocol::Protocol;

/// Validate a protocol definition.
pub fn validate(protocol: &Protocol) -> std::result::Result<(), Vec<String>> {
    let mut errors = Vec::new();

    if protocol.primary_metrics.is_empty() {
        errors.push("primary_metrics must be non-empty".to_string());
    }
    if protocol
        .primary_metrics
        .iter()
        .any(|metric| metric.name.trim().is_empty())
    {
        errors.push("primary_metrics names must be non-empty".to_string());
    }
    if protocol.allowed_tools.is_empty() {
        errors.push("allowed_tools must be non-empty".to_string());
    }
    if protocol
        .allowed_tools
        .iter()
        .any(|tool| tool.trim().is_empty())
    {
        errors.push("allowed_tools entries must be non-empty".to_string());
    }
    if protocol.cost_model.fee_schedule.trim().is_empty() {
        errors.push("cost_model.fee_schedule must be non-empty".to_string());
    }
    if protocol.cost_model.slippage_model.trim().is_empty() {
        errors.push("cost_model.slippage_model must be non-empty".to_string());
    }

    validate_non_negative_finite(
        "safety_policy.max_drawdown",
        protocol.safety_policy.max_drawdown,
        &mut errors,
    );
    validate_non_negative_finite(
        "safety_policy.max_leverage",
        protocol.safety_policy.max_leverage,
        &mut errors,
    );

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

fn validate_non_negative_finite(field: &str, value: f64, errors: &mut Vec<String>) {
    if !value.is_finite() {
        errors.push(format!("{field} must be finite"));
    } else if value < 0.0 {
        errors.push(format!("{field} must be non-negative"));
    }
}
