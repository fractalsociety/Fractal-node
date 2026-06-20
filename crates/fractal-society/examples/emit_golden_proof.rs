//! Emit a golden proof fixture for the TS offline-verifier conformance test
//! (package 78). Runs the full pipeline and prints a JSON object with the
//! `bundle`, `manifest`, `scorecard`, and `public_key` (hex). Capture stdout
//! into `packages/society-schema/test/golden_proof.json`:
//!
//!   cargo run -p fractal-society --example emit_golden_proof

use serde::Serialize;

use fractal_society::adapters::trading::baselines::{
    BuyAndHoldBaseline, CashBaseline, MovingAverageBaseline, RandomBaseline,
};
use fractal_society::adapters::trading::{TradingAdapter, TradingAgent, TradingConfig};
use fractal_society::kernel::{run, KernelConfig, RunOutcome};
use fractal_society::pipeline::run_pipeline_default;
use fractal_society::pkgs::run_bundle::RunBundle;
use fractal_society::protocol::ProofManifest;
use fractal_society::signing::AuthorSigner;
use fractal_society::simulation::Agent;
use fractal_society::verifier::Scorecard;

const SEED: u64 = 42;
const STEPS: u64 = 20;

#[derive(Serialize)]
struct GoldenProof<'a> {
    /// Portable run bundle.
    bundle: &'a RunBundle,
    /// Signed proof manifest.
    manifest: &'a ProofManifest,
    /// Scorecard (hashed opaquely by the verifier).
    scorecard: &'a Scorecard,
    /// Author Ed25519 public key (64 hex chars).
    public_key: String,
}

#[tokio::main]
async fn main() {
    let tcfg = TradingConfig {
        max_steps: STEPS,
        ..TradingConfig::default()
    };
    let kcfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: STEPS,
    };
    let ts = chrono::DateTime::from_timestamp(0, 0).unwrap();
    let signer = AuthorSigner::from_seed(&[7u8; 32]);

    let baselines = vec![
        baseline("cash", CashBaseline::new()).await,
        baseline("buy-and-hold", BuyAndHoldBaseline::new()).await,
        baseline("random", RandomBaseline::new(SEED)).await,
        baseline("moving-average", MovingAverageBaseline::new()).await,
    ];

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
    .expect("pipeline must complete");

    let fixture = GoldenProof {
        bundle: &result.bundle,
        manifest: &result.proof_manifest,
        scorecard: &result.scorecard,
        public_key: hex::encode(signer.public_key()),
    };
    println!("{}", serde_json::to_string_pretty(&fixture).unwrap());
}

async fn baseline<A: Agent<TradingAdapter>>(name: &str, agent: A) -> (String, RunOutcome) {
    let tcfg = TradingConfig {
        max_steps: STEPS,
        ..TradingConfig::default()
    };
    let kcfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: STEPS,
    };
    let outcome = run(TradingAdapter::new(SEED, tcfg).unwrap(), agent, SEED, &kcfg)
        .await
        .unwrap();
    (name.to_string(), outcome)
}
