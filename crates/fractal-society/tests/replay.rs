//! Gate P02-N10: a completed run exports its manifest and replays identically
//! on a fresh adapter instance.

use fractal_society::adapters::{ReferenceAdapter, ReferenceAgent};
use fractal_society::kernel::{self, KernelConfig};

#[tokio::test]
async fn replay_reproduces_identical_evidence_hash() {
    let cfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: 15,
    };
    let original = kernel::run(
        ReferenceAdapter::new(4, 15, 7),
        ReferenceAgent::new(4, 7),
        7,
        &cfg,
    )
    .await
    .unwrap();

    // Reconstruct fresh instances and replay from the frozen manifest.
    let replayed = kernel::replay(
        ReferenceAdapter::new(4, 15, 7),
        ReferenceAgent::new(4, 7),
        &original.manifest,
    )
    .await
    .unwrap();

    assert_eq!(original.evidence_hash, replayed.evidence_hash);
    assert_eq!(original.evidence.id, replayed.evidence.id);
    assert_eq!(
        original.evidence.decision_traces.len(),
        replayed.evidence.decision_traces.len()
    );
}

#[tokio::test]
async fn manifest_content_hash_is_stable() {
    use fractal_society::kernel::KernelConfig;
    let cfg = KernelConfig::default();
    let out = kernel::run(
        ReferenceAdapter::new(4, 20, 55),
        ReferenceAgent::new(4, 55),
        55,
        &cfg,
    )
    .await
    .unwrap();
    let h1 = out.manifest.content_hash().unwrap();
    // Re-derive from the same manifest fields.
    let h2 = out.manifest.content_hash().unwrap();
    assert_eq!(h1, h2);
}
