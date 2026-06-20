use std::collections::HashMap;

use fractal_society::pkgs::overfit_detector::assess;
use fractal_society::verifier::{
    CostAssumptions, MetricValue, ProofLevel, RiskMetrics, Scorecard, SimulationTier,
    VerifierSummary,
};

fn scorecard(net_return: f64) -> Scorecard {
    let mut primary_metrics = HashMap::new();
    primary_metrics.insert(
        "net_return".to_string(),
        MetricValue {
            value: net_return,
            higher_is_better: true,
            unit: "fraction".to_string(),
        },
    );

    Scorecard {
        id: "scorecard".to_string(),
        agent_id: "agent".to_string(),
        agent_version: "0.1.0".to_string(),
        protocol_id: "protocol".to_string(),
        primary_metrics,
        baselines: HashMap::new(),
        risk_metrics: RiskMetrics {
            max_drawdown: 0.0,
            volatility: 0.0,
            cvar_95: 0.0,
            worst_day: 0.0,
            policy_violations: 0,
        },
        verifier_summary: VerifierSummary {
            total_verifiers: 0,
            verifiers_passed: 0,
            verifiers_failed: 0,
            required_passed: 0,
            required_total: 0,
        },
        simulation_tier: SimulationTier::S0,
        cost_assumptions: CostAssumptions {
            fee_model: "none".to_string(),
            latency_ms: 0,
            slippage_model: "none".to_string(),
            starting_capital: 100_000,
        },
        confidence_intervals: HashMap::new(),
        proof_level: ProofLevel::Committed,
        limitations: Vec::new(),
        disclaimer: "SIMULATION ONLY".to_string(),
        timestamp: chrono::DateTime::from_timestamp(0, 0).unwrap(),
    }
}

#[test]
fn small_gap_is_not_overfit() {
    let assessment = assess(&scorecard(0.12), &scorecard(0.10), 0.05);

    assert!(!assessment.overfit);
    assert_eq!(assessment.train_return, 0.12);
    assert_eq!(assessment.eval_return, 0.10);
}

#[test]
fn large_gap_is_overfit() {
    let assessment = assess(&scorecard(0.30), &scorecard(0.10), 0.05);

    assert!(assessment.overfit);
}

#[test]
fn gap_is_train_minus_eval() {
    let assessment = assess(&scorecard(0.30), &scorecard(0.10), 0.05);

    assert!((assessment.gap - 0.20).abs() < 1e-12);
}
