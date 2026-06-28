use fractal_society::adapters::trading::{
    CashBaseline, TradingAdapter, TradingAgent, TradingConfig,
};
use fractal_society::kernel::{run, KernelConfig};
use fractal_society::offline_verify::{verify, VerifyVerdict};
use fractal_society::pipeline::{
    run_pipeline, run_pipeline_with_commitment, trading_verifier_pack,
};
use fractal_society::pkgs::chain_commitment::InMemoryCommitmentAdapter;
use fractal_society::signing::AuthorSigner;

const SEED: u64 = 84;
const STEPS: u64 = 12;

#[tokio::test]
async fn mock_adapter_populates_chain_reference_and_proof_still_verifies() {
    let signer = AuthorSigner::from_seed(&[84u8; 32]);
    let commitment = InMemoryCommitmentAdapter::new("mock-chain", 100);
    let result = run_with_commitment(&signer, Some(&commitment)).await;
    let chain_reference = result
        .proof_manifest
        .chain_reference
        .as_ref()
        .expect("commitment adapter should populate chain_reference");

    assert_eq!(chain_reference.network, "mock-chain");
    assert_eq!(chain_reference.block_number, 100);
    assert!(chain_reference.finalized);
    result
        .proof_manifest
        .verify_author(&signer.public_key())
        .unwrap();
    assert_eq!(
        verify(
            &result.bundle,
            &result.proof_manifest,
            &fractal_society::canonical::canonical_json(&result.scorecard).unwrap(),
            &signer.public_key()
        ),
        VerifyVerdict::Valid
    );
}

#[tokio::test]
async fn none_adapter_keeps_off_chain_behavior() {
    let signer = AuthorSigner::from_seed(&[85u8; 32]);
    let result = run_with_commitment(&signer, None).await;

    assert!(result.proof_manifest.chain_reference.is_none());
    result
        .proof_manifest
        .verify_author(&signer.public_key())
        .unwrap();
}

#[tokio::test]
async fn existing_run_pipeline_api_remains_off_chain() {
    let signer = AuthorSigner::from_seed(&[86u8; 32]);
    let (tcfg, kcfg, cash) = baseline().await;

    let result = run_pipeline(
        TradingAdapter::new(SEED, tcfg.clone()).unwrap(),
        TradingAgent::new(SEED),
        SEED,
        kcfg,
        tcfg,
        vec![("cash".to_string(), cash)],
        trading_verifier_pack(1e-3, vec!["place_order".to_string()]),
        &signer,
        chrono::DateTime::from_timestamp(0, 0).unwrap(),
    )
    .await
    .unwrap();

    assert!(result.proof_manifest.chain_reference.is_none());
}

async fn run_with_commitment(
    signer: &AuthorSigner,
    commitment: Option<&dyn fractal_society::pkgs::chain_commitment::CommitmentAdapter>,
) -> fractal_society::pipeline::PipelineResult {
    let (tcfg, kcfg, cash) = baseline().await;
    run_pipeline_with_commitment(
        TradingAdapter::new(SEED, tcfg.clone()).unwrap(),
        TradingAgent::new(SEED),
        SEED,
        kcfg,
        tcfg,
        vec![("cash".to_string(), cash)],
        trading_verifier_pack(1e-3, vec!["place_order".to_string()]),
        signer,
        chrono::DateTime::from_timestamp(0, 0).unwrap(),
        commitment,
    )
    .await
    .unwrap()
}

async fn baseline() -> (
    TradingConfig,
    KernelConfig,
    fractal_society::kernel::RunOutcome,
) {
    let tcfg = TradingConfig {
        max_steps: STEPS,
        ..TradingConfig::default()
    };
    let kcfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: STEPS,
    };
    let cash = run(
        TradingAdapter::new(SEED, tcfg.clone()).unwrap(),
        CashBaseline::new(),
        SEED,
        &kcfg,
    )
    .await
    .unwrap();
    (tcfg, kcfg, cash)
}
