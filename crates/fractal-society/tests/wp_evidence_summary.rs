use fractal_society::adapters::trading::{TradingAdapter, TradingAgent, TradingConfig};
use fractal_society::kernel::{KernelConfig, run};
use fractal_society::pkgs::evidence_summary::summarize;
use std::collections::HashMap;

async fn clean_evidence() -> fractal_society::protocol::EvidenceBundle {
    let tcfg = TradingConfig {
        max_steps: 10,
        ..TradingConfig::default()
    };
    let kcfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: 10,
    };
    run(
        TradingAdapter::new(4040, tcfg).unwrap(),
        TradingAgent::new(4040),
        4040,
        &kcfg,
    )
    .await
    .unwrap()
    .evidence
}

#[tokio::test]
async fn step_count_and_metric_snapshot_are_correct() {
    let evidence = clean_evidence().await;
    let summary = summarize(&evidence);

    assert_eq!(summary.step_count, evidence.decision_traces.len());
    assert_eq!(summary.metrics, evidence.metrics);
}

#[tokio::test]
async fn action_histogram_counts_known_run_actions() {
    let evidence = clean_evidence().await;
    let summary = summarize(&evidence);
    let mut expected = HashMap::new();
    for trace in &evidence.decision_traces {
        let action_type = trace
            .action
            .as_object()
            .and_then(|object| object.keys().next())
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        *expected.entry(action_type).or_insert(0) += 1;
    }
    let total_count = summary.action_type_counts.values().sum::<u64>();

    assert_eq!(total_count as usize, evidence.decision_traces.len());
    assert_eq!(summary.action_type_counts, expected);
}

#[tokio::test]
async fn unknown_action_shape_is_counted() {
    let mut evidence = clean_evidence().await;
    let before = summarize(&evidence)
        .action_type_counts
        .get("unknown")
        .copied()
        .unwrap_or(0);
    evidence.decision_traces[0].action = serde_json::Value::Null;
    let summary = summarize(&evidence);

    assert_eq!(
        summary.action_type_counts.get("unknown"),
        Some(&(before + 1))
    );
}
