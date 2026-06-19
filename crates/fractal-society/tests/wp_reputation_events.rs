use fractal_society::pkgs::reputation_events::{from_review, from_verifier, ReputationKind};
use fractal_society::verifier::VerifierReport;

fn ts() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(0, 0).unwrap()
}

fn report(passed: bool) -> VerifierReport {
    VerifierReport {
        id: format!("report-{passed}"),
        verifier_id: "unit-verifier".to_string(),
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
        timestamp: ts(),
    }
}

#[test]
fn passing_report_yields_positive_verified_pass() {
    let event = from_verifier(&report(true), "agent-1", ts());

    assert_eq!(event.subject, "agent-1");
    assert_eq!(event.kind, ReputationKind::VerifiedPass);
    assert_eq!(event.delta, 1);
    assert_eq!(event.timestamp, ts());
}

#[test]
fn failing_report_yields_negative_verified_fail() {
    let event = from_verifier(&report(false), "agent-1", ts());

    assert_eq!(event.kind, ReputationKind::VerifiedFail);
    assert_eq!(event.delta, -1);
}

#[test]
fn approved_review_yields_positive_review_event() {
    let approved = from_review("review-subject", true, ts());
    let rejected = from_review("review-subject", false, ts());

    assert_eq!(approved.kind, ReputationKind::ReviewApproved);
    assert_eq!(approved.delta, 2);
    assert_eq!(rejected.kind, ReputationKind::ReviewRejected);
    assert_eq!(rejected.delta, -2);
}

#[test]
fn events_are_deterministic_given_inputs() {
    let a = from_verifier(&report(true), "agent-1", ts());
    let b = from_verifier(&report(true), "agent-1", ts());
    let c = from_review("review-subject", true, ts());
    let d = from_review("review-subject", true, ts());

    assert_eq!(a, b);
    assert_eq!(c, d);
    assert_ne!(a.id, c.id);
}
