//! PHASE-04 gates P04-N10 / P04-N11: the trading scorecard labels assumptions,
//! carries the simulation disclaimer, compares baselines, and is deterministic.

use fractal_society::adapters::trading::{
    build_scorecard, BuyAndHoldBaseline, CashBaseline, MovingAverageBaseline, RandomBaseline,
    TradingAgent, TradingConfig,
};
use fractal_society::kernel::{run, KernelConfig, RunOutcome};
use fractal_society::simulation::Agent;
use fractal_society::verifier::{ProofLevel, SimulationTier};

async fn run_outcome<A: Agent<fractal_society::adapters::trading::TradingAdapter>>(
    agent: A,
    seed: u64,
) -> RunOutcome {
    let tcfg = TradingConfig {
        max_steps: 20,
        ..TradingConfig::default()
    };
    let kcfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: 20,
    };
    run(
        fractal_society::adapters::trading::TradingAdapter::new(seed, tcfg).unwrap(),
        agent,
        seed,
        &kcfg,
    )
    .await
    .unwrap()
}

async fn fixture() -> (RunOutcome, Vec<(String, RunOutcome)>) {
    let candidate = run_outcome(TradingAgent::new(31), 31).await;
    let baselines = vec![
        (
            "cash".to_string(),
            run_outcome(CashBaseline::new(), 31).await,
        ),
        (
            "buy-and-hold".to_string(),
            run_outcome(BuyAndHoldBaseline::new(), 31).await,
        ),
        (
            "random".to_string(),
            run_outcome(RandomBaseline::new(31), 31).await,
        ),
        (
            "moving-average".to_string(),
            run_outcome(MovingAverageBaseline::new(), 31).await,
        ),
    ];
    (candidate, baselines)
}

#[tokio::test]
async fn scorecard_labels_assumptions_disclaimer_and_baselines() {
    let (candidate, baselines) = fixture().await;
    let ts = chrono::DateTime::from_timestamp(0, 0).unwrap();
    let sc = build_scorecard(&candidate, &baselines, &TradingConfig::default(), ts);

    assert_eq!(sc.simulation_tier, SimulationTier::S0);
    assert_eq!(sc.proof_level, ProofLevel::Committed);
    assert_eq!(sc.cost_assumptions.starting_capital, 100_000);
    assert!(sc.cost_assumptions.fee_model.contains("5 bps"));
    assert!(sc.disclaimer.contains("SIMULATION"));
    assert!(sc.limitations.iter().any(|l| l.contains("funding")));
    assert!(sc.primary_metrics.contains_key("net_return"));
    assert!(sc.primary_metrics.contains_key("total_pnl"));
    assert_eq!(sc.baselines.len(), 4);
    assert!(sc.baselines["cash"].baseline_value.abs() < 1e-9);
    assert_eq!(sc.verifier_summary.total_verifiers, 0);
    assert!(sc.id.starts_with("scorecard-"));
    assert!(sc.risk_metrics.max_drawdown >= 0.0);
    assert!(sc.risk_metrics.volatility >= 0.0);
    assert_eq!(
        sc.risk_metrics.policy_violations,
        candidate.metrics.metrics["policy_violations"] as u64
    );
}

#[tokio::test]
async fn scorecard_is_deterministic() {
    let (candidate, baselines) = fixture().await;
    let ts = chrono::DateTime::from_timestamp(0, 0).unwrap();
    let a = build_scorecard(&candidate, &baselines, &TradingConfig::default(), ts);
    let b = build_scorecard(&candidate, &baselines, &TradingConfig::default(), ts);
    assert_eq!(
        serde_json::to_value(&a).unwrap(),
        serde_json::to_value(&b).unwrap()
    );
}
