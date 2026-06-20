use fractal_society::adapters::trading::{TradingAdapter, TradingAgent, TradingConfig};
use fractal_society::kernel::{run, KernelConfig};
use fractal_society::pkgs::determinism_audit::diff;

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
        TradingAdapter::new(3838, tcfg).unwrap(),
        TradingAgent::new(3838),
        3838,
        &kcfg,
    )
    .await
    .unwrap()
    .evidence
}

#[tokio::test]
async fn identical_bundles_have_no_divergences() {
    let evidence = clean_evidence().await;

    assert!(diff(&evidence, &evidence).is_empty());
}

#[tokio::test]
async fn one_mutated_action_field_emits_one_divergence() {
    let left = clean_evidence().await;
    let mut right = left.clone();
    let step = right.decision_traces[0].step;
    right.decision_traces[0].action = serde_json::json!({ "tool": "tampered" });

    let divergences = diff(&left, &right);

    assert_eq!(divergences.len(), 1);
    assert_eq!(divergences[0].step, step);
    assert_eq!(divergences[0].field, "action");
    assert_eq!(
        divergences[0].right,
        serde_json::json!({ "tool": "tampered" })
    );
}

#[tokio::test]
async fn one_mutated_outcome_field_emits_one_divergence() {
    let left = clean_evidence().await;
    let mut right = left.clone();
    let step = right.decision_traces[0].step;
    right.decision_traces[0].outcome = serde_json::json!({ "tampered": true });

    let divergences = diff(&left, &right);

    assert_eq!(divergences.len(), 1);
    assert_eq!(divergences[0].step, step);
    assert_eq!(divergences[0].field, "outcome");
    assert_eq!(
        divergences[0].right,
        serde_json::json!({ "tampered": true })
    );
}
