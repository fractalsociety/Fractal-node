use fractal_society::adapters::trading::{TradingAdapter, TradingAgent, TradingConfig};
use fractal_society::kernel::{run, KernelConfig};
use fractal_society::pkgs::risk_policy::{verify, VERIFIER_ID};

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
async fn passes_when_consistent() {
    let evidence = clean_evidence().await;
    let rejected_steps = evidence
        .decision_traces
        .iter()
        .filter(|trace| trace.outcome.get("rejected").is_some())
        .count() as u64;

    let report = verify(&evidence);

    assert!(report.passed, "{report:?}");
    assert_eq!(report.verifier_id, VERIFIER_ID);
    assert_eq!(report.execution_time_seconds, 0.0);
    assert!(report.errors.is_empty());
    assert_eq!(
        report.details["rejected_steps"].as_u64(),
        Some(rejected_steps)
    );
    assert_eq!(
        report.details["reported_policy_violations"].as_u64(),
        Some(rejected_steps)
    );
}

#[tokio::test]
async fn fails_when_count_mismatched() {
    let mut evidence = clean_evidence().await;
    let reported = *evidence
        .metrics
        .get("policy_violations")
        .expect("clean trading run should report policy violations");
    let tampered = reported + 1.0;
    evidence
        .metrics
        .insert("policy_violations".to_string(), tampered);

    let report = verify(&evidence);

    assert!(!report.passed);
    assert!(!report.errors.is_empty());
    assert_eq!(
        report.details["reported_policy_violations"].as_u64(),
        Some(tampered as u64)
    );
}

#[tokio::test]
async fn report_identity_and_zero_time_are_stable() {
    let evidence = clean_evidence().await;
    let report = verify(&evidence);

    assert_eq!(VERIFIER_ID, "risk-policy");
    assert_eq!(report.verifier_id, VERIFIER_ID);
    assert_eq!(report.verifier_version, "0.1.0");
    assert_eq!(report.execution_time_seconds, 0.0);
    assert_eq!(
        report.timestamp,
        chrono::DateTime::from_timestamp(0, 0).unwrap()
    );
}
