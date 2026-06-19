use chrono::DateTime;
use fractal_society::adapters::trading::{
    build_scorecard, TradingAdapter, TradingAgent, TradingConfig,
};
use fractal_society::kernel::{run, KernelConfig, RunOutcome};
use fractal_society::pkgs::proof_manifest::build;
use fractal_society::protocol::{Hash, Visibility};
use fractal_society::signing::AuthorSigner;
use fractal_society::verifier::Scorecard;

async fn candidate_run() -> RunOutcome {
    let tcfg = TradingConfig {
        max_steps: 12,
        ..TradingConfig::default()
    };
    let kcfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: 12,
    };
    run(
        TradingAdapter::new(123, tcfg).unwrap(),
        TradingAgent::new(123),
        123,
        &kcfg,
    )
    .await
    .unwrap()
}

fn scorecard(run: &RunOutcome) -> Scorecard {
    build_scorecard(
        run,
        &[],
        &TradingConfig::default(),
        DateTime::from_timestamp(0, 0).unwrap(),
    )
}

#[tokio::test]
async fn build_signs_manifest_and_sets_committed_private_fields() {
    let run = candidate_run().await;
    let scorecard = scorecard(&run);
    let signer = AuthorSigner::from_seed(&[7u8; 32]);
    let timestamp = DateTime::from_timestamp(0, 0).unwrap();

    let manifest = build(&run, &scorecard, &signer, timestamp).unwrap();

    manifest.verify_author(&signer.public_key()).unwrap();
    assert_eq!(manifest.manifest_version, "1.0.0");
    assert_eq!(manifest.claim_id, run.manifest.run_id);
    assert_eq!(manifest.protocol_hash, Hash::of(&run.manifest).unwrap());
    assert_eq!(
        manifest.agent_hash,
        Hash::of(&run.manifest.agent_id).unwrap()
    );
    assert_eq!(manifest.dataset_hash, Hash::new(b"dataset"));
    assert_eq!(manifest.environment_hash, Hash::new(b"environment"));
    assert_eq!(manifest.trace_merkle_root, run.evidence_hash);
    assert_eq!(manifest.verifier_set_hash, Hash::new(b"verifiers"));
    assert_eq!(manifest.scorecard_hash, Hash::of(&scorecard).unwrap());
    assert_eq!(manifest.disclosure, Visibility::CommittedPrivate);
    assert_eq!(manifest.platform_attestation, None);
    assert!(manifest.chain_reference.is_none());
    assert_eq!(manifest.timestamp, timestamp);
    assert!(!manifest.author_signature.is_empty());
}

#[tokio::test]
async fn mutating_claim_id_breaks_author_signature() {
    let run = candidate_run().await;
    let scorecard = scorecard(&run);
    let signer = AuthorSigner::from_seed(&[7u8; 32]);
    let mut manifest = build(
        &run,
        &scorecard,
        &signer,
        DateTime::from_timestamp(0, 0).unwrap(),
    )
    .unwrap();

    manifest.verify_author(&signer.public_key()).unwrap();
    manifest.claim_id = "tampered-claim".to_string();

    assert!(manifest.verify_author(&signer.public_key()).is_err());
}
