use fractal_society::adapters::trading::{
    CashBaseline, TradingAdapter, TradingAgent, TradingConfig,
};
use fractal_society::kernel::{run, KernelConfig};
use fractal_society::offline_verify::VerifyVerdict;
use fractal_society::persistence::artifact_store::{ArtifactStore, FileArtifactStore};
use fractal_society::persistence::event_log::{EventLog, FileEventLog};
use fractal_society::persistence::{load_proof, PersistedPipelineArtifacts};
use fractal_society::pipeline::{run_pipeline_persisted, trading_verifier_pack};
use fractal_society::signing::AuthorSigner;

const SEED: u64 = 81;
const STEPS: u64 = 12;

#[tokio::test]
async fn persisted_pipeline_loads_and_verifies_offline() {
    let (root, mut store, mut event_log) = stores("valid");
    let signer = AuthorSigner::from_seed(&[81u8; 32]);
    let result = run_persisted(&mut store, &mut event_log, &signer).await;

    let loaded = load_proof(&store, &result.bundle, &signer.public_key()).unwrap();

    assert_eq!(loaded.verdict, VerifyVerdict::Valid);
    assert_eq!(loaded.bundle, result.bundle);
    assert_eq!(loaded.manifest.claim_id, result.proof_manifest.claim_id);
    assert_eq!(loaded.scorecard.id, result.scorecard.id);
    assert_eq!(loaded.evidence.id, result.run.evidence.id);
    assert_eq!(event_log.replay().unwrap().len(), 4);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn tampered_persisted_scorecard_is_rejected_at_load() {
    let (root, mut store, mut event_log) = stores("tampered");
    let signer = AuthorSigner::from_seed(&[82u8; 32]);
    let result = run_persisted(&mut store, &mut event_log, &signer).await;
    let hashes = persisted_hashes(&result).unwrap();
    let mut tampered = result.scorecard.clone();
    tampered.id = "tampered-scorecard".to_string();
    std::fs::write(
        root.join("artifacts").join(&hashes.scorecard_hash.0),
        fractal_society::canonical::canonical_json(&tampered).unwrap(),
    )
    .unwrap();

    // Fix 1 (bytes-verified loading): the tampered bytes no longer hash to their
    // content-addressed key, so load_proof rejects them at the storage layer
    // (a stronger guarantee than returning an Invalid verdict after the fact).
    let loaded = load_proof(&store, &result.bundle, &signer.public_key());
    assert!(
        loaded.is_err(),
        "tampering persisted scorecard bytes must be detected at load"
    );
    let _ = std::fs::remove_dir_all(root);
}

async fn run_persisted(
    store: &mut dyn ArtifactStore,
    event_log: &mut dyn EventLog,
    signer: &AuthorSigner,
) -> fractal_society::pipeline::PipelineResult {
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

    run_pipeline_persisted(
        TradingAdapter::new(SEED, tcfg.clone()).unwrap(),
        TradingAgent::new(SEED),
        SEED,
        kcfg,
        tcfg,
        vec![("cash".to_string(), cash)],
        trading_verifier_pack(1e-3, vec!["place_order".to_string()]),
        signer,
        chrono::DateTime::from_timestamp(0, 0).unwrap(),
        store,
        event_log,
    )
    .await
    .unwrap()
}

fn persisted_hashes(
    result: &fractal_society::pipeline::PipelineResult,
) -> fractal_society::Result<PersistedPipelineArtifacts> {
    Ok(PersistedPipelineArtifacts {
        evidence_hash: result.bundle.evidence_hash.clone(),
        scorecard_hash: result.bundle.scorecard_hash.clone(),
        proof_hash: result.bundle.proof_hash.clone(),
        bundle_hash: result.bundle.bundle_hash()?,
    })
}

fn stores(label: &str) -> (std::path::PathBuf, FileArtifactStore, FileEventLog) {
    let root = std::env::temp_dir().join(format!(
        "fractal_society_wp_pipeline_persistence_{label}_{}",
        std::process::id()
    ));
    let artifacts = root.join("artifacts");
    let events = root.join("events.jsonl");
    (
        root,
        FileArtifactStore::new(artifacts),
        FileEventLog::new(events),
    )
}
