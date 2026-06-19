use fractal_society::adapters::trading::{
    build_scorecard, BuyAndHoldBaseline, CashBaseline, TradingAdapter, TradingAgent, TradingConfig,
};
use fractal_society::kernel::{run, KernelConfig, RunOutcome};
use fractal_society::pkgs::baseline_correctness::{verify, VERIFIER_ID};
use fractal_society::simulation::Agent;
use fractal_society::verifier::Scorecard;

async fn run_outcome<A>(agent: A, seed: u64) -> RunOutcome
where
    A: Agent<TradingAdapter>,
{
    let tcfg = TradingConfig {
        max_steps: 12,
        ..TradingConfig::default()
    };
    let kcfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: 12,
    };
    run(TradingAdapter::new(seed, tcfg).unwrap(), agent, seed, &kcfg)
        .await
        .unwrap()
}

async fn scorecard_fixture() -> Scorecard {
    let candidate = run_outcome(TradingAgent::new(31), 31).await;
    let baselines = vec![
        (
            "cash".to_string(),
            run_outcome(CashBaseline::new(), 31).await,
        ),
        (
            "buy-and-hold".to_string(),
            run_outcome(BuyAndHoldBaseline::new(), 31).await,
        ),
    ];
    build_scorecard(
        &candidate,
        &baselines,
        &TradingConfig::default(),
        chrono::DateTime::from_timestamp(0, 0).unwrap(),
    )
}

#[tokio::test]
async fn passes_for_consistent_scorecard() {
    let scorecard = scorecard_fixture().await;
    let report = verify(&scorecard, 1e-9);

    assert!(report.passed, "{report:?}");
    assert_eq!(report.verifier_id, VERIFIER_ID);
    assert_eq!(report.execution_time_seconds, 0.0);
    assert!(report.errors.is_empty());
    assert_eq!(
        report.details["checked_baselines"].as_u64(),
        Some(scorecard.baselines.len() as u64)
    );
}

#[tokio::test]
async fn fails_when_difference_tampered() {
    let mut scorecard = scorecard_fixture().await;
    scorecard
        .baselines
        .get_mut("buy-and-hold")
        .expect("fixture should include buy-and-hold baseline")
        .difference += 10.0;

    let report = verify(&scorecard, 1e-9);

    assert!(!report.passed);
    assert!(!report.errors.is_empty());
    assert_eq!(report.details["failed_baselines"].as_u64(), Some(1));
}

#[tokio::test]
async fn report_identity_and_zero_time_are_stable() {
    let scorecard = scorecard_fixture().await;
    let report = verify(&scorecard, 1e-9);

    assert_eq!(VERIFIER_ID, "baseline-correctness");
    assert_eq!(report.verifier_id, VERIFIER_ID);
    assert_eq!(report.verifier_version, "0.1.0");
    assert_eq!(report.execution_time_seconds, 0.0);
    assert_eq!(
        report.timestamp,
        chrono::DateTime::from_timestamp(0, 0).unwrap()
    );
}
