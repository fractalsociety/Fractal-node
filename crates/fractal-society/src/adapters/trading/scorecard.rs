//! Trading scorecard builder (PHASE-04 gates P04-N10 and P04-N11).
//!
//! Builds a [`Scorecard`](crate::verifier::Scorecard) from a candidate run and a
//! set of baseline runs, labeling every cost/fill/tier assumption and attaching
//! the mandatory simulation disclaimer so a public scorecard cannot be mistaken
//! for live trading results. Deterministic: it takes an explicit timestamp and
//! derives every value from the supplied runs.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::adapters::trading::{TradingConfig, TradingOutcome};
use crate::kernel::RunOutcome;
use crate::protocol::EvidenceBundle;
use crate::verifier::{
    BaselineResult, CostAssumptions, MetricValue, ProofLevel, RiskMetrics, Scorecard,
    SimulationTier, VerifierSummary,
};

/// Build a trading [`Scorecard`] for `candidate` relative to `baselines`.
///
/// `baselines` is a list of `(name, run)` pairs (e.g. the four deterministic
/// baselines). `timestamp` must be a logical/run clock, not a wall clock, so the
/// scorecard stays reproducible.
pub fn build_scorecard(
    candidate: &RunOutcome,
    baselines: &[(String, RunOutcome)],
    config: &TradingConfig,
    timestamp: DateTime<Utc>,
) -> Scorecard {
    let curve = equity_curve(&candidate.evidence);
    let policy_violations = candidate
        .metrics
        .metrics
        .get("policy_violations")
        .copied()
        .unwrap_or(0.0) as u64;
    let candidate_return = candidate.metrics.primary_metric;

    let mut primary_metrics = HashMap::new();
    primary_metrics.insert(
        "net_return".to_string(),
        MetricValue {
            value: candidate_return,
            higher_is_better: true,
            unit: "fraction".to_string(),
        },
    );
    primary_metrics.insert(
        "total_pnl".to_string(),
        MetricValue {
            value: candidate
                .metrics
                .metrics
                .get("total_pnl")
                .copied()
                .unwrap_or(0.0),
            higher_is_better: true,
            unit: "USDC".to_string(),
        },
    );
    primary_metrics.insert(
        "fees".to_string(),
        MetricValue {
            value: candidate
                .metrics
                .metrics
                .get("fees")
                .copied()
                .unwrap_or(0.0),
            higher_is_better: false,
            unit: "USDC".to_string(),
        },
    );
    primary_metrics.insert(
        "policy_violations".to_string(),
        MetricValue {
            value: policy_violations as f64,
            higher_is_better: false,
            unit: "count".to_string(),
        },
    );

    let mut baseline_map = HashMap::new();
    for (name, run) in baselines {
        let baseline_return = run.metrics.primary_metric;
        let difference = candidate_return - baseline_return;
        let percent_difference = if baseline_return.abs() < 1e-12 {
            0.0
        } else {
            difference / baseline_return.abs() * 100.0
        };
        baseline_map.insert(
            name.clone(),
            BaselineResult {
                baseline_name: name.clone(),
                baseline_value: baseline_return,
                candidate_value: candidate_return,
                difference,
                percent_difference,
                is_better: difference > 0.0,
            },
        );
    }

    Scorecard {
        id: format!("scorecard-{}", candidate.evidence_hash.0),
        agent_id: candidate.manifest.agent_id.clone(),
        // RunManifest does not yet carry an agent version; placeholder until it does.
        agent_version: "0.1.0".to_string(),
        protocol_id: candidate.manifest.adapter_id.clone(),
        primary_metrics,
        baselines: baseline_map,
        risk_metrics: compute_risk_metrics(&curve, policy_violations),
        verifier_summary: VerifierSummary {
            total_verifiers: 0,
            verifiers_passed: 0,
            verifiers_failed: 0,
            required_passed: 0,
            required_total: 0,
        },
        simulation_tier: SimulationTier::S0,
        cost_assumptions: CostAssumptions {
            fee_model: format!("{} bps notional", config.fee_bps),
            latency_ms: 0,
            slippage_model: "fill-at-close (optimistic; no slippage or queue)".to_string(),
            starting_capital: config.initial_equity as u64,
        },
        confidence_intervals: HashMap::new(),
        proof_level: ProofLevel::Committed,
        limitations: vec![
            "Simulation results are not live trading results.".to_string(),
            "Tier S0: deterministic synthetic fixtures, not recorded market data.".to_string(),
            "funding rate accrual is not modeled.".to_string(),
            "Fill model is optimistic (fills at bar close; no slippage or queue position)."
                .to_string(),
            "Liquidation uses a flat equity floor, not a per-position maintenance-margin model."
                .to_string(),
            "No verifiers have been run; verifier_summary is zero.".to_string(),
            "Confidence intervals are not computed in this slice.".to_string(),
        ],
        disclaimer: "SIMULATION ONLY — tier S0 synthetic fixtures. Results are hypothetical, are not live trading results, and do not guarantee future performance. Costs, fills, funding, and liquidation follow the assumptions in cost_assumptions; see limitations.".to_string(),
        timestamp,
    }
}

/// Extract the per-step equity curve from a run's evidence (rejected steps are skipped).
fn equity_curve(evidence: &EvidenceBundle) -> Vec<f64> {
    evidence
        .decision_traces
        .iter()
        .filter_map(|trace| serde_json::from_value::<TradingOutcome>(trace.outcome.clone()).ok())
        .map(|outcome| outcome.equity)
        .collect()
}

/// Compute approximate risk metrics from an equity curve.
fn compute_risk_metrics(equity_curve: &[f64], policy_violations: u64) -> RiskMetrics {
    if equity_curve.len() < 2 {
        return RiskMetrics {
            max_drawdown: 0.0,
            volatility: 0.0,
            cvar_95: 0.0,
            worst_day: 0.0,
            policy_violations,
        };
    }
    let returns: Vec<f64> = equity_curve
        .windows(2)
        .map(|w| if w[0] != 0.0 { w[1] / w[0] - 1.0 } else { 0.0 })
        .collect();

    let mut peak = equity_curve[0];
    let mut max_drawdown = 0.0_f64;
    for &equity in equity_curve {
        peak = peak.max(equity);
        if peak > 0.0 {
            max_drawdown = max_drawdown.max((peak - equity) / peak);
        }
    }

    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / returns.len() as f64;
    let volatility = variance.sqrt();
    let worst_day = returns.iter().cloned().fold(f64::INFINITY, f64::min);

    let mut sorted = returns;
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let tail = ((sorted.len() as f64) * 0.05).ceil() as usize;
    let tail = tail.max(1);
    let cvar_95 = sorted[..tail].iter().sum::<f64>() / tail as f64;

    RiskMetrics {
        max_drawdown,
        volatility,
        cvar_95,
        worst_day,
        policy_violations,
    }
}
