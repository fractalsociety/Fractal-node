use fractal_society::pkgs::replication_summary::{ReplicationSummary, summarize};
use fractal_society::verifier::Replication;

fn replication(id: &str, success: bool) -> Replication {
    Replication {
        id: id.to_string(),
        original_proof_id: "proof-1".to_string(),
        replicator: format!("replicator-{id}"),
        success,
        differences: Vec::new(),
        tolerance: 0.01,
        actual_difference: Some(0.0),
        environment: "test".to_string(),
        timestamp: chrono::DateTime::from_timestamp(0, 0).unwrap(),
    }
}

#[test]
fn counts_by_success() {
    let replications = vec![
        replication("1", true),
        replication("2", false),
        replication("3", true),
    ];

    assert_eq!(
        summarize(&replications),
        ReplicationSummary {
            total: 3,
            successful: 2,
            failed: 1,
            any_success: true,
        }
    );
}

#[test]
fn any_success_true_iff_at_least_one_success() {
    let all_failed = vec![replication("1", false), replication("2", false)];
    let mixed = vec![replication("1", false), replication("2", true)];

    assert!(!summarize(&all_failed).any_success);
    assert!(summarize(&mixed).any_success);
}

#[test]
fn empty_summary_is_all_zero() {
    assert_eq!(
        summarize(&[]),
        ReplicationSummary {
            total: 0,
            successful: 0,
            failed: 0,
            any_success: false,
        }
    );
}
