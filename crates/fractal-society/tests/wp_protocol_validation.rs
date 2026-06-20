use std::collections::HashMap;

use chrono::{Duration, Utc};
use fractal_society::pkgs::protocol_validation::validate;
use fractal_society::protocol::{
    CostModel, DatasetBoundaries, MetricDefinition, MetricType, Protocol, SafetyPolicy, WindowSpec,
};

fn window(start: i64, end: i64) -> WindowSpec {
    let epoch = chrono::DateTime::<Utc>::from_timestamp(0, 0).unwrap();
    WindowSpec {
        start: epoch + Duration::seconds(start),
        end: epoch + Duration::seconds(end),
        seed: 42,
    }
}

fn valid_protocol() -> Protocol {
    Protocol {
        id: "protocol-1".to_string(),
        version: "1.0.0".to_string(),
        agent_versions: HashMap::new(),
        allowed_tools: vec!["place_order".to_string()],
        dataset_boundaries: DatasetBoundaries {
            development: window(0, 10),
            validation: window(10, 20),
            evaluation: window(20, 30),
        },
        primary_metrics: vec![MetricDefinition {
            name: "net_return".to_string(),
            higher_is_better: true,
            metric_type: MetricType::Percentage,
        }],
        cost_model: CostModel {
            fee_schedule: "5bps".to_string(),
            latency_ms: 0,
            slippage_model: "fixed".to_string(),
        },
        safety_policy: SafetyPolicy {
            max_drawdown: 0.2,
            max_leverage: 2.0,
            policy_violations_eq_zero: true,
        },
        required_verifiers: vec![],
    }
}

#[test]
fn valid_protocol_passes() {
    assert_eq!(validate(&valid_protocol()), Ok(()));
}

#[test]
fn empty_primary_metrics_fails() {
    let mut protocol = valid_protocol();
    protocol.primary_metrics.clear();

    let errors = validate(&protocol).unwrap_err();

    assert!(errors
        .iter()
        .any(|error| error == "primary_metrics must be non-empty"));
}

#[test]
fn empty_allowed_tools_fails() {
    let mut protocol = valid_protocol();
    protocol.allowed_tools.clear();

    let errors = validate(&protocol).unwrap_err();

    assert!(errors
        .iter()
        .any(|error| error == "allowed_tools must be non-empty"));
}

#[test]
fn empty_fee_schedule_fails() {
    let mut protocol = valid_protocol();
    protocol.cost_model.fee_schedule.clear();

    let errors = validate(&protocol).unwrap_err();

    assert!(errors
        .iter()
        .any(|error| error == "cost_model.fee_schedule must be non-empty"));
}

#[test]
fn negative_max_leverage_fails() {
    let mut protocol = valid_protocol();
    protocol.safety_policy.max_leverage = -1.0;

    let errors = validate(&protocol).unwrap_err();

    assert!(errors
        .iter()
        .any(|error| error == "safety_policy.max_leverage must be non-negative"));
}

#[test]
fn non_finite_safety_policy_fails() {
    let mut protocol = valid_protocol();
    protocol.safety_policy.max_drawdown = f64::NAN;
    protocol.safety_policy.max_leverage = f64::INFINITY;

    let errors = validate(&protocol).unwrap_err();

    assert!(errors
        .iter()
        .any(|error| error == "safety_policy.max_drawdown must be finite"));
    assert!(errors
        .iter()
        .any(|error| error == "safety_policy.max_leverage must be finite"));
}
