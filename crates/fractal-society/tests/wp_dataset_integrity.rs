use std::collections::HashMap;

use fractal_society::pkgs::dataset_integrity::{verify, VERIFIER_ID};
use fractal_society::protocol::{DataSource, DatasetManifest, Hash, Visibility};

fn valid_manifest() -> DatasetManifest {
    DatasetManifest {
        id: "dataset-1".to_string(),
        source: DataSource::Synthetic {
            generator: "unit-test".to_string(),
        },
        time_range: (
            chrono::DateTime::from_timestamp(0, 0).unwrap(),
            chrono::DateTime::from_timestamp(10, 0).unwrap(),
        ),
        schema_version: "1.0.0".to_string(),
        missingness: HashMap::new(),
        transformations: vec!["none".to_string()],
        content_hash: Hash("a".repeat(64)),
        visibility: Visibility::Private,
    }
}

#[test]
fn passes_for_valid_manifest() {
    let report = verify(&valid_manifest());

    assert!(report.passed, "{report:?}");
    assert_eq!(report.verifier_id, VERIFIER_ID);
    assert_eq!(report.execution_time_seconds, 0.0);
    assert!(report.errors.is_empty());
}

#[test]
fn fails_for_bad_hash() {
    let mut manifest = valid_manifest();
    manifest.content_hash = Hash("zz".to_string());

    let report = verify(&manifest);

    assert!(!report.passed);
    assert!(!report.errors.is_empty());
}

#[test]
fn fails_for_empty_id() {
    let mut manifest = valid_manifest();
    manifest.id.clear();

    let report = verify(&manifest);

    assert!(!report.passed);
    assert!(report
        .errors
        .iter()
        .any(|error| error.contains("dataset id")));
}

#[test]
fn report_identity_and_zero_time_are_stable() {
    let report = verify(&valid_manifest());

    assert_eq!(VERIFIER_ID, "dataset-integrity");
    assert_eq!(report.verifier_id, VERIFIER_ID);
    assert_eq!(report.verifier_version, "0.1.0");
    assert_eq!(report.execution_time_seconds, 0.0);
    assert_eq!(
        report.timestamp,
        chrono::DateTime::from_timestamp(0, 0).unwrap()
    );
}
