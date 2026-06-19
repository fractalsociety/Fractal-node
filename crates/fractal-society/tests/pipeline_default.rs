//! Package 69 — canonical trading verifier pack + `run_pipeline_default`.

use fractal_society::adapters::trading::{
    CashBaseline, TradingAdapter, TradingAgent, TradingConfig,
};
use fractal_society::kernel::{run, KernelConfig};
use fractal_society::pipeline::{run_pipeline_default, trading_verifier_pack};
use fractal_society::signing::AuthorSigner;
use fractal_society::simulation::Agent;

const SEED: u64 = 11;
const STEPS: u64 = 12;

#[test]
fn canonical_pack_has_five_verifiers() {
    let pack = trading_verifier_pack(1e-3, vec!["place_order".to_string()]);
    assert_eq!(pack.len(), 5, "canonical pack has 5 verifiers");
}

#[tokio::test]
async fn run_pipeline_default_succeeds_end_to_end() {
    let signer = AuthorSigner::from_seed(&[9u8; 32]);
    let pk = signer.public_key();
    let tcfg = TradingConfig {
        max_steps: STEPS,
        ..TradingConfig::default()
    };
    let kcfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: STEPS,
    };
    let ts = chrono::DateTime::from_timestamp(0, 0).unwrap();

    let cash = run(
        TradingAdapter::new(SEED, tcfg.clone()).unwrap(),
        CashBaseline::new(),
        SEED,
        &kcfg,
    )
    .await
    .unwrap();
    let baselines = vec![("cash".to_string(), cash)];

    let result = run_pipeline_default(
        TradingAdapter::new(SEED, tcfg.clone()).unwrap(),
        TradingAgent::new(SEED),
        SEED,
        kcfg,
        tcfg,
        baselines,
        &signer,
        ts,
    )
    .await
    .unwrap();

    // All five canonical verifiers ran and passed.
    assert_eq!(result.verifier_reports.len(), 5);
    assert!(
        result.verifier_reports.iter().all(|report| report.passed),
        "all canonical verifiers must pass"
    );
    // Signed proof verifies and the run completed.
    result.proof_manifest.verify_author(&pk).unwrap();
    assert!(result.outcome.is_complete());
}
