use fractal_society::pkgs::verifier_summary::summarize;
use fractal_society::verifier::VerifierReport;

fn report(id: &str, passed: bool) -> VerifierReport {
    VerifierReport {
        id: id.to_string(),
        verifier_id: id.to_string(),
        verifier_version: "0.1.0".to_string(),
        passed,
        score: Some(if passed { 1.0 } else { 0.0 }),
        details: serde_json::json!({ "passed": passed }),
        warnings: Vec::new(),
        errors: if passed {
            Vec::new()
        } else {
            vec!["failed".to_string()]
        },
        execution_time_seconds: 0.0,
        timestamp: chrono::DateTime::from_timestamp(0, 0).unwrap(),
    }
}

#[test]
fn all_pass_has_zero_failed() {
    let summary = summarize(&[report("a", true), report("b", true)]);

    assert_eq!(summary.total_verifiers, 2);
    assert_eq!(summary.verifiers_passed, 2);
    assert_eq!(summary.verifiers_failed, 0);
    assert_eq!(summary.required_total, 2);
    assert_eq!(summary.required_passed, 2);
}

#[test]
fn mixed_reports_count_correctly() {
    let summary = summarize(&[report("a", true), report("b", false), report("c", true)]);

    assert_eq!(summary.total_verifiers, 3);
    assert_eq!(summary.verifiers_passed, 2);
    assert_eq!(summary.verifiers_failed, 1);
    assert_eq!(summary.required_total, 3);
    assert_eq!(summary.required_passed, 2);
}

#[test]
fn empty_reports_are_all_zero() {
    let summary = summarize(&[]);

    assert_eq!(summary.total_verifiers, 0);
    assert_eq!(summary.verifiers_passed, 0);
    assert_eq!(summary.verifiers_failed, 0);
    assert_eq!(summary.required_total, 0);
    assert_eq!(summary.required_passed, 0);
}
