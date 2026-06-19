//! Package 70 — runnable research-pipeline demo.
//!
//! Builds a synthetic trading run through the full pipeline and prints a public
//! proof card. The human-usable "it works" demo. Run with:
//!
//! ```sh
//! cargo run -p fractal-society --example run_research
//! ```

use fractal_society::adapters::trading::baselines::{
    BuyAndHoldBaseline, CashBaseline, MovingAverageBaseline, RandomBaseline,
};
use fractal_society::adapters::trading::{TradingAdapter, TradingAgent, TradingConfig};
use fractal_society::kernel::{run, KernelConfig, RunOutcome};
use fractal_society::pipeline::run_pipeline_default;
use fractal_society::pkgs::proof_card;
use fractal_society::signing::AuthorSigner;
use fractal_society::simulation::Agent;

const SEED: u64 = 42;
const STEPS: u64 = 30;

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

    // Baselines share the candidate's synthetic bars (via the shared SEED).
    let baselines = vec![
        baseline("cash", CashBaseline::new()).await,
        baseline("buy-and-hold", BuyAndHoldBaseline::new()).await,
        baseline("random", RandomBaseline::new(SEED)).await,
        baseline("moving-average", MovingAverageBaseline::new()).await,
    ];

    // Run the full pipeline: run -> score -> verify -> reward -> proof -> bundle.
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

    // Independently verify the signed proof before publishing the card.
    let public_key = signer.public_key();
    result
        .proof_manifest
        .verify_author(&public_key)
        .expect("signed proof must verify");

    let card = proof_card::build(&result.proof_manifest, &result.scorecard);

    let all_passed = result.verifier_reports.iter().all(|report| report.passed);
    println!("=== Fractal Society — research pipeline demo ===");
    println!("claim            : {}", card.claim);
    println!("proof level      : {}", card.proof_level);
    println!("simulation tier  : {:?}", card.simulation_tier);
    println!("net return       : {:.4}", card.net_return);
    println!("max drawdown     : {:.4}", card.max_drawdown);
    println!(
        "verifiers        : {} run, all passed = {}",
        result.verifier_reports.len(),
        all_passed
    );
    println!("reward released  : {}", result.outcome.reward_released);
    println!("pipeline complete: {}", result.outcome.is_complete());
    println!("proof hash       : {}", card.proof_hash.0);
    println!(
        "bundle hash      : {}",
        result.bundle.bundle_hash().expect("bundle hash").0
    );
    println!();
    println!("SIMULATION ONLY — results are hypothetical and are not live trading results.");
    println!("{}", card.disclaimer);
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
