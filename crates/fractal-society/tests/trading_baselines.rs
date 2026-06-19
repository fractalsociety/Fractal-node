//! PHASE-04 gate P04-N08: baselines run through the kernel, reproduce from
//! identical construction, and produce distinct runs.

use std::collections::HashSet;

use fractal_society::adapters::trading::{
    BuyAndHoldBaseline, CashBaseline, MovingAverageBaseline, RandomBaseline, TradingConfig,
};
use fractal_society::kernel::{run, KernelConfig};
use fractal_society::protocol::Hash;
use fractal_society::simulation::Agent;

async fn run_hash<A: Agent<fractal_society::adapters::trading::TradingAdapter>>(
    agent: A,
    seed: u64,
) -> Hash {
    let tcfg = TradingConfig {
        max_steps: 15,
        ..TradingConfig::default()
    };
    let kcfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: 15,
    };
    run(
        fractal_society::adapters::trading::TradingAdapter::new(seed, tcfg).unwrap(),
        agent,
        seed,
        &kcfg,
    )
    .await
    .unwrap()
    .evidence_hash
}

#[tokio::test]
async fn cash_baseline_never_trades() {
    let tcfg = TradingConfig {
        max_steps: 20,
        ..TradingConfig::default()
    };
    let kcfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: 20,
    };
    let outcome = run(
        fractal_society::adapters::trading::TradingAdapter::new(5, tcfg).unwrap(),
        CashBaseline::new(),
        5,
        &kcfg,
    )
    .await
    .unwrap();
    assert_eq!(outcome.metrics.metrics["fees"], 0.0);
    assert_eq!(outcome.metrics.metrics["total_pnl"], 0.0);
    assert!(outcome.metrics.primary_metric.abs() < 1e-9);
}

#[tokio::test]
async fn each_baseline_reproduces_from_identical_construction() {
    assert_eq!(
        run_hash(CashBaseline::new(), 11).await,
        run_hash(CashBaseline::new(), 11).await
    );
    assert_eq!(
        run_hash(BuyAndHoldBaseline::new(), 11).await,
        run_hash(BuyAndHoldBaseline::new(), 11).await
    );
    assert_eq!(
        run_hash(RandomBaseline::new(99), 11).await,
        run_hash(RandomBaseline::new(99), 11).await
    );
    assert_eq!(
        run_hash(MovingAverageBaseline::new(), 11).await,
        run_hash(MovingAverageBaseline::new(), 11).await
    );
}

#[tokio::test]
async fn baselines_produce_distinct_runs() {
    let cash = run_hash(CashBaseline::new(), 23).await;
    let buy_hold = run_hash(BuyAndHoldBaseline::new(), 23).await;
    let random = run_hash(RandomBaseline::new(7), 23).await;
    let moving_average = run_hash(MovingAverageBaseline::new(), 23).await;
    let unique: HashSet<&Hash> = [&cash, &buy_hold, &random, &moving_average]
        .into_iter()
        .collect();
    assert!(
        unique.len() >= 3,
        "expected at least 3 distinct baseline runs, got {}",
        unique.len()
    );
    assert_ne!(cash, buy_hold, "cash and buy-and-hold must differ");
}
