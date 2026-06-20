//! AR-09: forecasting (non-trading) domain adapter.
//!
//! Proves the generic kernel runs a second domain end-to-end: the forecasting
//! adapter drives `kernel::run`, produces a signed proof via the generic
//! `proof_manifest::build`, is deterministic across reruns, and its source
//! contains no trading code.

use std::path::PathBuf;

use fractal_society::adapters::forecasting::{
    build_forecasting_scorecard, ForecastingAdapter, ForecastingAgent,
};
use fractal_society::kernel::{run, KernelConfig};
use fractal_society::pkgs::proof_manifest;
use fractal_society::signing::AuthorSigner;

const SEED: u64 = 42;
const STEPS: u64 = 20;

#[tokio::test]
async fn forecasting_run_is_deterministic() {
    let kcfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: STEPS,
    };

    let run1 = run(
        ForecastingAdapter::new(STEPS, SEED),
        ForecastingAgent::new(SEED),
        SEED,
        &kcfg,
    )
    .await
    .unwrap();
    let run2 = run(
        ForecastingAdapter::new(STEPS, SEED),
        ForecastingAgent::new(SEED),
        SEED,
        &kcfg,
    )
    .await
    .unwrap();

    assert_eq!(
        run1.evidence_hash, run2.evidence_hash,
        "same seed must yield byte-identical evidence hash"
    );
    assert_eq!(run1.manifest.adapter_id, "forecasting-binary");
    // Forecasting metrics were computed.
    assert!(run1.metrics.metrics.contains_key("mean_brier"));
    assert!(run1.evidence.decision_traces.len() == STEPS as usize);
}

#[tokio::test]
async fn forecasting_run_produces_signed_proof() {
    let kcfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: STEPS,
    };
    let signer = AuthorSigner::from_seed(&[7u8; 32]);
    let pk = signer.public_key();
    let ts = chrono::DateTime::from_timestamp(0, 0).unwrap();

    let outcome = run(
        ForecastingAdapter::new(STEPS, SEED),
        ForecastingAgent::new(SEED),
        SEED,
        &kcfg,
    )
    .await
    .unwrap();

    let scorecard = build_forecasting_scorecard(&outcome, ts);
    let manifest = proof_manifest::build(&outcome, &scorecard, &signer, ts).unwrap();

    manifest.verify_author(&pk).unwrap();
    assert_eq!(manifest.trace_merkle_root, outcome.evidence_hash);
}

#[test]
fn forecasting_module_has_no_trading_code() {
    // AR-09 architecture boundary: the forecasting adapter must not import or
    // reference trading-domain code.
    let banned = &[
        "adapters::trading",
        "hyperliquid",
        "TradingAdapter",
        "MarketBar",
        "place_order",
        "PlaceOrder",
    ];
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for rel in [
        "src/adapters/forecasting/mod.rs",
        "src/adapters/forecasting/types.rs",
        "src/adapters/forecasting/adapter.rs",
        "src/adapters/forecasting/scorecard.rs",
    ] {
        let src =
            std::fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"));
        let lower = src.to_ascii_lowercase();
        for token in banned {
            assert!(
                !lower.contains(&token.to_ascii_lowercase()),
                "banned trading token '{}' found in {}",
                token,
                rel
            );
        }
    }
}
