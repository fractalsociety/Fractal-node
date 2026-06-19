//! Package 68 — full vertical-slice end-to-end test (the "it works" proof).
//!
//! Runs the whole pipeline on synthetic trading data with all four baselines and
//! the core verifier set, and asserts the proof is signed, every verifier
//! passes, and the pipeline outcome is complete (reward released).

use std::sync::Arc;

use fractal_society::adapters::trading::{
    BuyAndHoldBaseline, CashBaseline, MovingAverageBaseline, RandomBaseline, TradingAdapter,
    TradingAgent, TradingConfig,
};
use fractal_society::kernel::{run, KernelConfig, RunOutcome};
use fractal_society::pipeline::{run_pipeline, VerifierFn};
use fractal_society::pkgs::{
    accounting_integrity, cost_completeness, risk_policy, temporal_leakage,
};
use fractal_society::protocol::EvidenceBundle;
use fractal_society::signing::AuthorSigner;
use fractal_society::simulation::Agent;

const SEED: u64 = 7;
const STEPS: u64 = 20;

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
async fn pipeline_vertical_slice_complete() {
    let signer = AuthorSigner::from_seed(&[5u8; 32]);
    let pk = signer.public_key();
    let ts = chrono::DateTime::from_timestamp(0, 0).unwrap();

    let baselines = vec![
        baseline("cash", CashBaseline::new()).await,
        baseline("buy-and-hold", BuyAndHoldBaseline::new()).await,
        baseline("random", RandomBaseline::new(SEED)).await,
        baseline("moving-average", MovingAverageBaseline::new()).await,
    ];

    let verifiers: Vec<VerifierFn> = vec![
        Arc::new(|ev: &EvidenceBundle| accounting_integrity::verify(ev, 1e-3)),
        Arc::new(|ev: &EvidenceBundle| cost_completeness::verify(ev, 1e-3)),
        Arc::new(|ev: &EvidenceBundle| risk_policy::verify(ev)),
        Arc::new(|ev: &EvidenceBundle| temporal_leakage::verify(ev)),
    ];

    let result = run_pipeline(
        TradingAdapter::new(SEED, tcfg()).unwrap(),
        TradingAgent::new(SEED),
        SEED,
        kcfg(),
        tcfg(),
        baselines,
        verifiers,
        &signer,
        ts,
    )
    .await
    .unwrap();

    // Evidence was produced.
    assert!(
        !result.run.evidence.decision_traces.is_empty(),
        "evidence must be non-empty"
    );
    // Scorecard compared the candidate against all four baselines.
    assert_eq!(
        result.scorecard.baselines.len(),
        4,
        "scorecard must compare 4 baselines"
    );
    // Every verifier passed.
    assert!(
        result.verifier_reports.iter().all(|report| report.passed),
        "all verifiers must pass"
    );
    // The signed proof verifies against the author key.
    result
        .proof_manifest
        .verify_author(&pk)
        .expect("signed proof must verify");
    // The pipeline reached a complete state (reward released, all verifiers passed).
    assert!(
        result.outcome.is_complete(),
        "pipeline outcome must be complete"
    );
}
