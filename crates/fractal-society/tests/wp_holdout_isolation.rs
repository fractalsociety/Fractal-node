use fractal_society::adapters::trading::{TradingAdapter, TradingAgent, TradingConfig};
use fractal_society::kernel::{run, KernelConfig};
use fractal_society::pkgs::holdout_isolation::{verify, VERIFIER_ID};

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
        TradingAdapter::new(4242, tcfg).unwrap(),
        TradingAgent::new(4242),
        4242,
        &kcfg,
    )
    .await
    .unwrap()
    .evidence
}

#[tokio::test]
async fn clean_evidence_passes() {
    let evidence = clean_evidence().await;
    let report = verify(&evidence, &["private-holdout-42".to_string()]);

    assert!(report.passed, "{report:?}");
    assert_eq!(report.verifier_id, VERIFIER_ID);
    assert_eq!(report.execution_time_seconds, 0.0);
    assert!(report.errors.is_empty());
    assert_eq!(report.details["leak_count"].as_u64(), Some(0));
}

#[tokio::test]
async fn action_containing_private_id_fails() {
    let mut evidence = clean_evidence().await;
    evidence.decision_traces[0].action = serde_json::json!({
        "tool": "lookup",
        "dataset": "private-holdout-42",
    });

    let report = verify(&evidence, &["private-holdout-42".to_string()]);

    assert!(!report.passed);
    assert!(!report.errors.is_empty());
    assert_eq!(report.details["leak_count"].as_u64(), Some(1));
    assert_eq!(report.details["leaks"][0]["field"], "action");
}

#[tokio::test]
async fn report_identity_and_zero_time_are_stable() {
    let evidence = clean_evidence().await;
    let report = verify(&evidence, &[]);

    assert_eq!(VERIFIER_ID, "holdout-isolation");
    assert_eq!(report.verifier_id, VERIFIER_ID);
    assert_eq!(report.verifier_version, "0.1.0");
    assert_eq!(report.execution_time_seconds, 0.0);
    assert_eq!(
        report.timestamp,
        chrono::DateTime::from_timestamp(0, 0).unwrap()
    );
}
