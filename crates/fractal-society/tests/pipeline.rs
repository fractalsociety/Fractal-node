//! Package 67 — pipeline orchestrator: end-to-end run + determinism.

use std::sync::Arc;

use fractal_society::adapters::trading::{
    BuyAndHoldBaseline, CashBaseline, TradingAdapter, TradingAgent, TradingConfig,
};
use fractal_society::kernel::{run, KernelConfig, RunOutcome};
use fractal_society::pipeline::{run_pipeline, VerifierFn};
use fractal_society::pkgs::accounting_integrity;
use fractal_society::pkgs::cost_completeness;
use fractal_society::pkgs::pipeline_contract::PipelineStage;
use fractal_society::protocol::{EvidenceBundle, Hash};
use fractal_society::signing::AuthorSigner;
use fractal_society::simulation::Agent;

const SEED: u64 = 42;
const STEPS: u64 = 12;

fn tcfg() -> TradingConfig {
    TradingConfig {
        max_steps: STEPS,
        ..TradingConfig::default()
    }
}

fn kcfg() -> KernelConfig {
    KernelConfig {
        episodes: 1,
        max_steps_per_episode: STEPS,
    }
}

fn ts() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(0, 0).unwrap()
}

fn verifiers() -> Vec<VerifierFn> {
    vec![
        Arc::new(|ev: &EvidenceBundle| accounting_integrity::verify(ev, 1e-3)),
        Arc::new(|ev: &EvidenceBundle| cost_completeness::verify(ev, 1e-3)),
    ]
}

async fn baseline<A: Agent<TradingAdapter>>(name: &str, agent: A) -> (String, RunOutcome) {
    let outcome = run(
        TradingAdapter::new(SEED, tcfg()).unwrap(),
        agent,
        SEED,
        &kcfg(),
    )
    .await
    .unwrap();
    (name.to_string(), outcome)
}

#[tokio::test]
async fn orchestrator_runs_end_to_end() {
    let signer = AuthorSigner::from_seed(&[3u8; 32]);
    let pk = signer.public_key();
    let baselines = vec![
        baseline("cash", CashBaseline::new()).await,
        baseline("buy-and-hold", BuyAndHoldBaseline::new()).await,
    ];

    let result = run_pipeline(
        TradingAdapter::new(SEED, tcfg()).unwrap(),
        TradingAgent::new(SEED),
        SEED,
        kcfg(),
        tcfg(),
        baselines,
        verifiers(),
        &signer,
        ts(),
    )
    .await
    .unwrap();

    // Signed proof verifies against the author key.
    result.proof_manifest.verify_author(&pk).unwrap();
    // Both verifiers ran.
    assert_eq!(result.verifier_reports.len(), 2);
    // Stage reached at least Verify (Reward, if all verifiers pass and reward releases).
    assert!(
        (result.outcome.stage() as u8) >= (PipelineStage::Verify as u8),
        "pipeline did not reach Verify stage"
    );
    // Bundle has a real hash.
    assert!(result.bundle.bundle_hash().is_ok());
}

#[tokio::test]
async fn orchestrator_is_deterministic() {
    async fn run_once(signer: &AuthorSigner) -> (Hash, Hash) {
        let baselines = vec![baseline("cash", CashBaseline::new()).await];
        let result = run_pipeline(
            TradingAdapter::new(SEED, tcfg()).unwrap(),
            TradingAgent::new(SEED),
            SEED,
            kcfg(),
            tcfg(),
            baselines,
            verifiers(),
            signer,
            ts(),
        )
        .await
        .unwrap();
        (
            result.bundle.bundle_hash().unwrap(),
            Hash::of(&result.proof_manifest).unwrap(),
        )
    }

    let signer = AuthorSigner::from_seed(&[3u8; 32]);
    let first = run_once(&signer).await;
    let second = run_once(&signer).await;
    assert_eq!(first.0, second.0, "bundle hash must be deterministic");
    assert_eq!(
        first.1, second.1,
        "proof manifest hash must be deterministic"
    );
}
