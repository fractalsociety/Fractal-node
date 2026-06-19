//! Package 71 — offline trustless verification.

use fractal_society::adapters::trading::{
    CashBaseline, TradingAdapter, TradingAgent, TradingConfig,
};
use fractal_society::kernel::{run, KernelConfig};
use fractal_society::offline_verify::{verify, VerifyVerdict};
use fractal_society::pipeline::run_pipeline_default;
use fractal_society::pkgs::run_bundle::RunBundle;
use fractal_society::protocol::ProofManifest;
use fractal_society::signing::AuthorSigner;
use fractal_society::verifier::Scorecard;

const SEED: u64 = 8;
const STEPS: u64 = 12;

async fn fixture() -> (RunBundle, ProofManifest, Scorecard, [u8; 32]) {
    let tcfg = TradingConfig {
        max_steps: STEPS,
        ..TradingConfig::default()
    };
    let kcfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: STEPS,
    };
    let signer = AuthorSigner::from_seed(&[4u8; 32]);

    let cash = run(
        TradingAdapter::new(SEED, tcfg.clone()).unwrap(),
        CashBaseline::new(),
        SEED,
        &kcfg,
    )
    .await
    .unwrap();
    let result = run_pipeline_default(
        TradingAdapter::new(SEED, tcfg.clone()).unwrap(),
        TradingAgent::new(SEED),
        SEED,
        kcfg,
        tcfg,
        vec![("cash".to_string(), cash)],
        &signer,
        chrono::DateTime::from_timestamp(0, 0).unwrap(),
    )
    .await
    .unwrap();

    (
        result.bundle,
        result.proof_manifest,
        result.scorecard,
        signer.public_key(),
    )
}

#[tokio::test]
async fn valid_proof_verifies_offline() {
    let (bundle, manifest, scorecard, public_key) = fixture().await;
    assert_eq!(
        verify(&bundle, &manifest, &scorecard, &public_key),
        VerifyVerdict::Valid,
        "a genuine proof must verify offline without re-running"
    );
}

#[tokio::test]
async fn tampered_scorecard_is_rejected() {
    let (bundle, manifest, mut scorecard, public_key) = fixture().await;
    scorecard.id = "tampered".to_string();
    let verdict = verify(&bundle, &manifest, &scorecard, &public_key);
    assert!(
        matches!(verdict, VerifyVerdict::Invalid { .. }),
        "a tampered scorecard must be rejected"
    );
}

#[tokio::test]
async fn wrong_public_key_is_rejected() {
    let (bundle, manifest, scorecard, _public_key) = fixture().await;
    let wrong = AuthorSigner::from_seed(&[99u8; 32]).public_key();
    let verdict = verify(&bundle, &manifest, &scorecard, &wrong);
    assert!(
        matches!(verdict, VerifyVerdict::Invalid { .. }),
        "a proof must not verify under the wrong author key"
    );
}
