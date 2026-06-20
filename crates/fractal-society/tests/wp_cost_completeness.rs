use fractal_society::adapters::trading::{TradingAdapter, TradingAgent, TradingConfig};
use fractal_society::kernel::{run, KernelConfig};
use fractal_society::pkgs::cost_completeness::{verify, VERIFIER_ID};

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
async fn passes_for_clean_run() {
    let evidence = clean_evidence().await;
    let report = verify(&evidence, 1e-5);

    assert!(report.passed, "{report:?}");
    assert_eq!(report.verifier_id, VERIFIER_ID);
    assert_eq!(report.execution_time_seconds, 0.0);
    assert!(report.errors.is_empty());
    assert!(report.details["checked_steps"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn fails_when_fees_omitted() {
    let mut evidence = clean_evidence().await;
    let outcome = evidence
        .decision_traces
        .iter_mut()
        .find(|trace| {
            trace.outcome["fees"]
                .as_f64()
                .is_some_and(|fees| fees > 0.0)
        })
        .expect("clean trading run should contain a fee-paying outcome");
    outcome.outcome["fees"] = serde_json::json!(0.0);

    let report = verify(&evidence, 1e-5);

    assert!(!report.passed);
    assert!(!report.errors.is_empty());
    assert_eq!(report.details["failed_steps"].as_u64(), Some(1));
}

#[tokio::test]
async fn report_identity_and_zero_time_are_stable() {
    let evidence = clean_evidence().await;
    let report = verify(&evidence, 1e-5);

    assert_eq!(VERIFIER_ID, "cost-completeness");
    assert_eq!(report.verifier_id, VERIFIER_ID);
    assert_eq!(report.verifier_version, "0.1.0");
    assert_eq!(report.execution_time_seconds, 0.0);
    assert_eq!(
        report.timestamp,
        chrono::DateTime::from_timestamp(0, 0).unwrap()
    );
}
