use fractal_society::adapters::trading::{TradingAdapter, TradingAgent, TradingConfig};
use fractal_society::kernel::{run, KernelConfig};
use fractal_society::pkgs::disclosure_tiers::redact;
use fractal_society::protocol::Visibility;

async fn clean_evidence() -> fractal_society::protocol::EvidenceBundle {
    let tcfg = TradingConfig {
        max_steps: 12,
        ..TradingConfig::default()
    };
    let kcfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: 12,
    };
    run(
        TradingAdapter::new(123, tcfg).unwrap(),
        TradingAgent::new(123),
        123,
        &kcfg,
    )
    .await
    .unwrap()
    .evidence
}

#[tokio::test]
async fn committed_private_hides_raw() {
    let evidence = clean_evidence().await;
    let redacted = redact(&evidence, Visibility::CommittedPrivate);

    assert_eq!(
        redacted.decision_traces.len(),
        evidence.decision_traces.len()
    );
    for (original, trace) in evidence
        .decision_traces
        .iter()
        .zip(redacted.decision_traces.iter())
    {
        assert_eq!(trace.step, original.step);
        assert_eq!(trace.observation_hash, original.observation_hash);
        assert_eq!(trace.timestamp, original.timestamp);
        assert!(trace.action.is_null());
        assert!(trace.outcome.is_null());
    }
}

#[tokio::test]
async fn private_empties_traces() {
    let evidence = clean_evidence().await;
    let redacted = redact(&evidence, Visibility::Private);

    assert!(!evidence.decision_traces.is_empty());
    assert!(redacted.decision_traces.is_empty());
}

#[tokio::test]
async fn open_is_identity() {
    let evidence = clean_evidence().await;
    let redacted = redact(&evidence, Visibility::Open);

    assert_eq!(
        serde_json::to_value(&redacted).unwrap(),
        serde_json::to_value(&evidence).unwrap()
    );
}

#[tokio::test]
async fn id_and_run_id_preserved() {
    let evidence = clean_evidence().await;

    for tier in [
        Visibility::Private,
        Visibility::CommittedPrivate,
        Visibility::ReviewerAccess,
        Visibility::PartialPublic,
        Visibility::Open,
    ] {
        let redacted = redact(&evidence, tier);
        assert_eq!(redacted.id, evidence.id);
        assert_eq!(redacted.run_id, evidence.run_id);
    }
}
