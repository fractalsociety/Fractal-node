use fractal_society::adapters::{ReferenceAdapter, ReferenceAgent};
use fractal_society::kernel::{run, KernelConfig};
use fractal_society::pkgs::temporal_leakage::{verify, VERIFIER_ID};
use fractal_society::protocol::EvidenceBundle;

async fn clean_evidence() -> EvidenceBundle {
    let cfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: 8,
    };
    run(
        ReferenceAdapter::new(4, 8, 14),
        ReferenceAgent::new(4, 14),
        14,
        &cfg,
    )
    .await
    .unwrap()
    .evidence
}

#[tokio::test]
async fn passes_for_monotonic() {
    let evidence = clean_evidence().await;
    let report = verify(&evidence);

    assert!(report.passed, "{report:?}");
    assert_eq!(report.verifier_id, VERIFIER_ID);
    assert_eq!(report.execution_time_seconds, 0.0);
    assert!(report.errors.is_empty());
}

#[tokio::test]
async fn fails_for_regressed_step() {
    let mut evidence = clean_evidence().await;
    evidence.decision_traces.swap(1, 2);

    let report = verify(&evidence);

    assert!(!report.passed);
    assert!(!report.errors.is_empty());
}

#[tokio::test]
async fn fails_for_duplicate_step() {
    let mut evidence = clean_evidence().await;
    evidence.decision_traces[1].step = evidence.decision_traces[0].step;

    let report = verify(&evidence);

    assert!(!report.passed);
    assert!(report
        .errors
        .iter()
        .any(|error| error.contains("non-monotonic")));
}

#[tokio::test]
async fn report_identity_and_zero_time_are_stable() {
    let evidence = clean_evidence().await;
    let report = verify(&evidence);

    assert_eq!(VERIFIER_ID, "temporal-leakage");
    assert_eq!(report.verifier_id, VERIFIER_ID);
    assert_eq!(report.verifier_version, "0.1.0");
    assert_eq!(report.execution_time_seconds, 0.0);
    assert_eq!(
        report.timestamp,
        chrono::DateTime::from_timestamp(0, 0).unwrap()
    );
}
