use fractal_society::adapters::trading::{TradingAdapter, TradingAgent, TradingConfig};
use fractal_society::kernel::{run, KernelConfig};
use fractal_society::pkgs::sandbox_policy::{verify, VERIFIER_ID};

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
        TradingAdapter::new(1313, tcfg).unwrap(),
        TradingAgent::new(1313),
        1313,
        &kcfg,
    )
    .await
    .unwrap()
    .evidence
}

#[tokio::test]
async fn passes_when_no_tool_use() {
    let evidence = clean_evidence().await;
    let report = verify(&evidence, &[]);

    assert!(report.passed, "{report:?}");
    assert_eq!(report.verifier_id, VERIFIER_ID);
    assert_eq!(report.execution_time_seconds, 0.0);
    assert_eq!(report.details["checked_tool_uses"].as_u64(), Some(0));
}

#[tokio::test]
async fn passes_when_tool_allowed() {
    let mut evidence = clean_evidence().await;
    evidence.decision_traces[0].action = serde_json::json!({ "tool": "calc" });

    let report = verify(&evidence, &["calc".to_string()]);

    assert!(report.passed, "{report:?}");
    assert!(report.errors.is_empty());
    assert_eq!(report.details["checked_tool_uses"].as_u64(), Some(1));
}

#[tokio::test]
async fn fails_when_tool_not_allowed() {
    let mut evidence = clean_evidence().await;
    evidence.decision_traces[0].action = serde_json::json!({ "tool": "shell" });

    let report = verify(&evidence, &["calc".to_string()]);

    assert!(!report.passed);
    assert!(!report.errors.is_empty());
    assert_eq!(report.details["failed_tool_uses"].as_u64(), Some(1));
}

#[tokio::test]
async fn report_identity_and_zero_time_are_stable() {
    let evidence = clean_evidence().await;
    let report = verify(&evidence, &[]);

    assert_eq!(VERIFIER_ID, "sandbox-policy");
    assert_eq!(report.verifier_id, VERIFIER_ID);
    assert_eq!(report.verifier_version, "0.1.0");
    assert_eq!(report.execution_time_seconds, 0.0);
    assert_eq!(
        report.timestamp,
        chrono::DateTime::from_timestamp(0, 0).unwrap()
    );
}
