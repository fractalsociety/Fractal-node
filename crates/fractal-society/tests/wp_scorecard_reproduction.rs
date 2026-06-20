use fractal_society::adapters::trading::{
    build_scorecard, TradingAdapter, TradingAgent, TradingConfig,
};
use fractal_society::kernel::{run, KernelConfig, RunOutcome};
use fractal_society::pkgs::scorecard_reproduction::{verify, VERIFIER_ID};

async fn candidate_run() -> (RunOutcome, TradingConfig) {
    let tcfg = TradingConfig {
        max_steps: 12,
        ..TradingConfig::default()
    };
    let kcfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: 12,
    };
    let run = run(
        TradingAdapter::new(789, tcfg.clone()).unwrap(),
        TradingAgent::new(789),
        789,
        &kcfg,
    )
    .await
    .unwrap();
    (run, tcfg)
}

#[tokio::test]
async fn passes_when_consistent() {
    let (candidate, tcfg) = candidate_run().await;
    let timestamp = chrono::DateTime::from_timestamp(0, 0).unwrap();
    let scorecard = build_scorecard(&candidate, &[], &tcfg, timestamp);

    let report = verify(&candidate.evidence, &scorecard, 1e-6);

    assert!(report.passed, "{report:?}");
    assert_eq!(report.verifier_id, VERIFIER_ID);
    assert_eq!(report.execution_time_seconds, 0.0);
    assert!(report.errors.is_empty());
    assert_eq!(report.details["metric"], "total_pnl");
}

#[tokio::test]
async fn fails_when_mismatched() {
    let (candidate, tcfg) = candidate_run().await;
    let timestamp = chrono::DateTime::from_timestamp(0, 0).unwrap();
    let mut scorecard = build_scorecard(&candidate, &[], &tcfg, timestamp);
    scorecard
        .primary_metrics
        .get_mut("total_pnl")
        .expect("scorecard should include total_pnl")
        .value += 10.0;

    let report = verify(&candidate.evidence, &scorecard, 1e-6);

    assert!(!report.passed);
    assert!(!report.errors.is_empty());
    assert!(report.details["difference"]
        .as_f64()
        .is_some_and(|difference| difference > 1.0));
}

#[tokio::test]
async fn report_identity_and_zero_time_are_stable() {
    let (candidate, tcfg) = candidate_run().await;
    let timestamp = chrono::DateTime::from_timestamp(0, 0).unwrap();
    let scorecard = build_scorecard(&candidate, &[], &tcfg, timestamp);
    let report = verify(&candidate.evidence, &scorecard, 1e-6);

    assert_eq!(VERIFIER_ID, "scorecard-reproduction");
    assert_eq!(report.verifier_id, VERIFIER_ID);
    assert_eq!(report.verifier_version, "0.1.0");
    assert_eq!(report.execution_time_seconds, 0.0);
    assert_eq!(
        report.timestamp,
        chrono::DateTime::from_timestamp(0, 0).unwrap()
    );
}
