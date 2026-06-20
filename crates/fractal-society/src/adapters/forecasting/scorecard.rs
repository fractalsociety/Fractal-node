//! Forecasting scorecard builder (AR-09).
//!
//! Builds a protocol [`Scorecard`] for a forecasting run, filling the
//! trading-flavored fields with neutral values and exposing forecasting metrics
//! (mean Brier score, forecast skill). This lets a forecasting run produce a
//! signed proof via the generic [`crate::pkgs::proof_manifest::build`] without
//! going through the trading-specific pipeline orchestrator.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::kernel::RunOutcome;
use crate::verifier::{
    BaselineResult, CostAssumptions, MetricValue, ProofLevel, RiskMetrics, Scorecard,
    SimulationTier, VerifierSummary,
};

/// Build a scorecard for a completed forecasting run.
pub fn build_forecasting_scorecard(run: &RunOutcome, timestamp: DateTime<Utc>) -> Scorecard {
    let primary_metrics: HashMap<String, MetricValue> = run
        .metrics
        .metrics
        .iter()
        .map(|(name, value)| {
            (
                name.clone(),
                MetricValue {
                    value: *value,
                    higher_is_better: name != "mean_brier",
                    unit: if name == "mean_brier" {
                        "brier".to_string()
                    } else {
                        "count".to_string()
                    },
                },
            )
        })
        .collect();

    Scorecard {
        id: format!("forecast-score-{}", run.manifest.run_id),
        agent_id: run.manifest.agent_id.clone(),
        agent_version: "1.0.0".to_string(),
        protocol_id: "forecasting-binary".to_string(),
        primary_metrics,
        baselines: HashMap::<String, BaselineResult>::new(),
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
            starting_capital: 0,
        },
        confidence_intervals: run.metrics.confidence_intervals.clone(),
        // Forecasting is deterministic and seeded, so it meets the
        // Reproducible proof level (P3).
        proof_level: ProofLevel::Reproducible,
        limitations: vec![
            "synthetic binary sequence generated from the run seed".to_string(),
            "single episode; no walk-forward holdout".to_string(),
        ],
        disclaimer: "Forecasting scorecard: lower mean Brier is better. Synthetic data only."
            .to_string(),
        timestamp,
    }
}
