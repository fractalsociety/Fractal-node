use fractal_society::pkgs::replication_check::{classify, within_tolerance, ReplicationClass};
use fractal_society::verifier::Replication;

fn replication(tolerance: f64, actual_difference: Option<f64>) -> Replication {
    Replication {
        id: "replication-1".to_string(),
        original_proof_id: "proof-1".to_string(),
        replicator: "alice".to_string(),
        success: false,
        differences: Vec::new(),
        tolerance,
        actual_difference,
        environment: "test".to_string(),
        timestamp: chrono::DateTime::from_timestamp(0, 0).unwrap(),
    }
}

#[test]
fn finite_difference_within_tolerance_succeeds() {
    let replication = replication(0.01, Some(0.01));

    assert_eq!(classify(&replication), ReplicationClass::Success);
    assert!(within_tolerance(&replication));
}

#[test]
fn difference_above_tolerance_fails() {
    let replication = replication(0.01, Some(0.02));

    assert_eq!(classify(&replication), ReplicationClass::Fail);
    assert!(!within_tolerance(&replication));
}

#[test]
fn missing_difference_is_indeterminate() {
    let replication = replication(0.01, None);

    assert_eq!(classify(&replication), ReplicationClass::Indeterminate);
    assert!(!within_tolerance(&replication));
}

#[test]
fn non_finite_values_are_indeterminate() {
    let actual_nan = replication(0.01, Some(f64::NAN));
    let tolerance_nan = replication(f64::NAN, Some(0.0));

    assert_eq!(classify(&actual_nan), ReplicationClass::Indeterminate);
    assert_eq!(classify(&tolerance_nan), ReplicationClass::Indeterminate);
    assert!(!within_tolerance(&actual_nan));
    assert!(!within_tolerance(&tolerance_nan));
}
