use fractal_society::adapters::trading::{TradingAdapter, TradingAgent, TradingConfig};
use fractal_society::kernel::{KernelConfig, run};
use fractal_society::pkgs::equity_curve::extract;

async fn clean_evidence() -> fractal_society::protocol::EvidenceBundle {
    let tcfg = TradingConfig {
        max_steps: 8,
        ..TradingConfig::default()
    };
    let kcfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: 8,
    };
    run(
        TradingAdapter::new(5555, tcfg).unwrap(),
        TradingAgent::new(5555),
        5555,
        &kcfg,
    )
    .await
    .unwrap()
    .evidence
}

#[tokio::test]
async fn known_evidence_extracts_expected_equity_series() {
    let evidence = clean_evidence().await;
    let expected = evidence
        .decision_traces
        .iter()
        .filter_map(|trace| trace.outcome.get("equity")?.as_f64())
        .filter(|equity| equity.is_finite())
        .collect::<Vec<_>>();

    assert_eq!(extract(&evidence), expected);
    assert!(!expected.is_empty());
}

#[tokio::test]
async fn steps_without_equity_are_skipped() {
    let mut evidence = clean_evidence().await;
    evidence.decision_traces[0].outcome = serde_json::json!({ "no_equity": true });
    let expected_len = evidence
        .decision_traces
        .iter()
        .filter(|trace| trace.outcome.get("equity").is_some())
        .count();

    let curve = extract(&evidence);

    assert_eq!(curve.len(), expected_len);
    assert!(curve.iter().all(|equity| equity.is_finite()));
}
