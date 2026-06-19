use fractal_society::adapters::{ReferenceAdapter, ReferenceAgent};
use fractal_society::kernel::{run, KernelConfig};
use fractal_society::pkgs::reproducibility::{verify, VERIFIER_ID};
use fractal_society::protocol::Hash;

async fn reference_run() -> fractal_society::kernel::RunOutcome {
    let cfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: 15,
    };
    run(
        ReferenceAdapter::new(4, 15, 7),
        ReferenceAgent::new(4, 7),
        7,
        &cfg,
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn passes_when_replay_matches() {
    let original = reference_run().await;
    let report = verify(&original.evidence_hash, &original.manifest, || {
        (ReferenceAdapter::new(4, 15, 7), ReferenceAgent::new(4, 7))
    })
    .await;

    assert!(report.passed, "{report:?}");
    assert_eq!(report.verifier_id, VERIFIER_ID);
    assert_eq!(report.execution_time_seconds, 0.0);
    assert!(report.errors.is_empty());
    assert_eq!(report.details["matches"].as_bool(), Some(true));
}

#[tokio::test]
async fn fails_when_hash_differs() {
    let original = reference_run().await;
    let wrong_hash = Hash("00".repeat(32));
    let report = verify(&wrong_hash, &original.manifest, || {
        (ReferenceAdapter::new(4, 15, 7), ReferenceAgent::new(4, 7))
    })
    .await;

    assert!(!report.passed);
    assert!(!report.errors.is_empty());
    assert_eq!(report.details["matches"].as_bool(), Some(false));
    assert_eq!(
        report.details["original_hash"].as_str(),
        Some(wrong_hash.0.as_str())
    );
}

#[tokio::test]
async fn report_identity_and_zero_time_are_stable() {
    let original = reference_run().await;
    let report = verify(&original.evidence_hash, &original.manifest, || {
        (ReferenceAdapter::new(4, 15, 7), ReferenceAgent::new(4, 7))
    })
    .await;

    assert_eq!(VERIFIER_ID, "reproducibility");
    assert_eq!(report.verifier_id, VERIFIER_ID);
    assert_eq!(report.verifier_version, "0.1.0");
    assert_eq!(report.execution_time_seconds, 0.0);
    assert_eq!(
        report.timestamp,
        chrono::DateTime::from_timestamp(0, 0).unwrap()
    );
}
