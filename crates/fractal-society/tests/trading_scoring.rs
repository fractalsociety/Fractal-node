use fractal_society::adapters::trading::{
    TradingAction, TradingAdapter, TradingConfig, TradingOutcome,
};
use fractal_society::simulation::{DomainAdapter, RunTrace};

#[tokio::test]
async fn score_counts_rejections_and_uses_last_outcome_total_pnl() {
    let mut adapter = TradingAdapter::new(42, TradingConfig::default()).unwrap();
    adapter.reset().await.unwrap();

    let known_outcome = TradingOutcome {
        reward: 0.0,
        equity: 100_500.0,
        cash: 100_500.0,
        position_notional: 0.0,
        total_pnl: 500.0,
        realized_pnl: 0.0,
        unrealized_pnl: 500.0,
        fees: 0.0,
        step: 1,
        fills: Vec::new(),
        liquidated: false,
        terminal: false,
    };
    let ts = chrono::DateTime::from_timestamp(0, 0).unwrap();
    let mut trace = RunTrace::new("score-test");
    trace.record_step(
        0,
        serde_json::json!({}),
        serde_json::to_value(TradingAction::Hold).unwrap(),
        serde_json::json!({"rejected":"daily loss stop active"}),
        ts,
    );
    trace.record_step(
        1,
        serde_json::json!({}),
        serde_json::to_value(TradingAction::Hold).unwrap(),
        serde_json::to_value(&known_outcome).unwrap(),
        ts,
    );

    let metrics = adapter.score(&trace).await.unwrap();
    assert_eq!(metrics.metrics["policy_violations"], 1.0);
    assert_eq!(metrics.metrics["total_pnl"], 500.0);
    assert!((metrics.primary_metric - 500.0 / 100_000.0).abs() < 1e-9);
}
