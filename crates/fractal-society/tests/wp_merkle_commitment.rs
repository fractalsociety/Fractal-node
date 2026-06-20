use fractal_society::adapters::trading::{TradingAdapter, TradingAgent, TradingConfig};
use fractal_society::kernel::{run, KernelConfig};
use fractal_society::pkgs::merkle_commitment::{prove, root, verify};
use fractal_society::protocol::{EvidenceBundle, Hash};

async fn clean_evidence() -> EvidenceBundle {
    let tcfg = TradingConfig {
        max_steps: 12,
        ..TradingConfig::default()
    };
    let kcfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: 12,
    };
    run(
        TradingAdapter::new(321, tcfg).unwrap(),
        TradingAgent::new(321),
        321,
        &kcfg,
    )
    .await
    .unwrap()
    .evidence
}

#[tokio::test]
async fn root_is_stable() {
    let evidence = clean_evidence().await;

    assert_eq!(root(&evidence), root(&evidence));
}

#[tokio::test]
async fn valid_proof_verifies() {
    let evidence = clean_evidence().await;
    let commitment = root(&evidence);
    let index = 3;
    let proof = prove(&evidence, index).expect("proof should exist for trace index");
    let leaf = &evidence.decision_traces[index].observation_hash;

    assert!(verify(leaf, &proof, &commitment));
}

#[tokio::test]
async fn wrong_leaf_rejected() {
    let evidence = clean_evidence().await;
    let commitment = root(&evidence);
    let proof = prove(&evidence, 2).expect("proof should exist for trace index");
    let wrong_leaf = Hash::new(b"different observation");

    assert!(!verify(&wrong_leaf, &proof, &commitment));
}

#[tokio::test]
async fn empty_evidence_root_is_fixed_empty_hash() {
    let mut evidence = clean_evidence().await;
    evidence.decision_traces.clear();

    assert_eq!(root(&evidence), Hash::new(b""));
    assert!(prove(&evidence, 0).is_none());
}
