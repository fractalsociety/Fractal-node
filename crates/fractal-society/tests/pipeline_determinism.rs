//! Package 72 — pipeline determinism + replay.
//!
//! Proves the integrated pipeline is fully deterministic (identical inputs ->
//! identical evidence/scorecard/proof/bundle hashes), varies with the seed, and
//! that a reproduced proof is trustlessly verifiable offline (replay integrity).

use fractal_society::adapters::trading::{
    CashBaseline, TradingAdapter, TradingAgent, TradingConfig,
};
use fractal_society::kernel::{run, KernelConfig};
use fractal_society::offline_verify::{verify, VerifyVerdict};
use fractal_society::pipeline::run_pipeline_default;
use fractal_society::pkgs::run_bundle::RunBundle;
use fractal_society::protocol::{Hash, ProofManifest};
use fractal_society::signing::AuthorSigner;
use fractal_society::verifier::Scorecard;

const STEPS: u64 = 12;

/// Hashes captured from one pipeline run, used for determinism comparisons.
struct ProofHashes {
    /// Candidate evidence hash.
    evidence: Hash,
    /// Scorecard canonical hash.
    scorecard: Hash,
    /// Proof manifest canonical hash.
    proof: Hash,
    /// Run bundle hash.
    bundle: Hash,
}

#[allow(clippy::type_complexity)]
async fn run_once(seed: u64) -> (ProofHashes, RunBundle, ProofManifest, Scorecard, [u8; 32]) {
    let tcfg = TradingConfig {
        max_steps: STEPS,
        ..TradingConfig::default()
    };
    let kcfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: STEPS,
    };
    // The signer is fixed across runs; only the pipeline `seed` varies.
    let signer = AuthorSigner::from_seed(&[6u8; 32]);

    let cash = run(
        TradingAdapter::new(seed, tcfg.clone()).unwrap(),
        CashBaseline::new(),
        seed,
        &kcfg,
    )
    .await
    .unwrap();
    let result = run_pipeline_default(
        TradingAdapter::new(seed, tcfg.clone()).unwrap(),
        TradingAgent::new(seed),
        seed,
        kcfg,
        tcfg,
        vec![("cash".to_string(), cash)],
        &signer,
        chrono::DateTime::from_timestamp(0, 0).unwrap(),
    )
    .await
    .unwrap();

    let hashes = ProofHashes {
        evidence: result.run.evidence_hash.clone(),
        scorecard: Hash::of(&result.scorecard).unwrap(),
        proof: Hash::of(&result.proof_manifest).unwrap(),
        bundle: result.bundle.bundle_hash().unwrap(),
    };
    (
        hashes,
        result.bundle,
        result.proof_manifest,
        result.scorecard,
        signer.public_key(),
    )
}

#[tokio::test]
async fn pipeline_is_deterministic() {
    let (first, _, _, _, _) = run_once(42).await;
    let (second, _, _, _, _) = run_once(42).await;

    assert_eq!(
        first.evidence, second.evidence,
        "evidence hash must be deterministic"
    );
    assert_eq!(
        first.scorecard, second.scorecard,
        "scorecard hash must be deterministic"
    );
    assert_eq!(
        first.proof, second.proof,
        "proof manifest hash must be deterministic"
    );
    assert_eq!(
        first.bundle, second.bundle,
        "bundle hash must be deterministic"
    );
}

#[tokio::test]
async fn pipeline_varies_by_seed() {
    let (low, _, _, _, _) = run_once(42).await;
    let (high, _, _, _, _) = run_once(43).await;

    assert_ne!(
        low.bundle, high.bundle,
        "bundle hash must vary with the seed"
    );
    assert_ne!(
        low.proof, high.proof,
        "proof manifest hash must vary with the seed"
    );
}

#[tokio::test]
async fn reproduced_proof_is_offline_verifiable() {
    // Re-running (replaying) from the same seed reproduces a proof that
    // verifies trustlessly via offline_verify — no host or re-run required.
    let (_, bundle, manifest, scorecard, public_key) = run_once(42).await;
    let scorecard_bytes = fractal_society::canonical::canonical_json(&scorecard).unwrap();
    assert_eq!(
        verify(&bundle, &manifest, &scorecard_bytes, &public_key),
        VerifyVerdict::Valid,
        "a reproduced proof must be offline-verifiable"
    );
}
