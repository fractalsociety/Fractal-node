use fractal_society::pkgs::reward_gate::{evaluate, RewardDecision};
use fractal_society::verifier::VerifierReport;

fn report(id: &str, passed: bool) -> VerifierReport {
    VerifierReport {
        id: format!("{id}-report"),
        verifier_id: id.to_string(),
        verifier_version: "0.1.0".to_string(),
        passed,
        score: Some(if passed { 1.0 } else { 0.0 }),
        details: serde_json::json!({}),
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
fn all_pass_and_window_closed_releases() {
    let reports = [
        report("accounting-integrity", true),
        report("risk-policy", true),
    ];

    let decision = evaluate(&reports, false, 2);

    assert_eq!(decision, RewardDecision::Release);
}

#[test]
fn one_fail_withholds() {
    let reports = [
        report("accounting-integrity", true),
        report("risk-policy", false),
    ];

    let decision = evaluate(&reports, false, 1);

    match decision {
        RewardDecision::Withhold { reasons } => {
            assert!(reasons.iter().any(|reason| reason.contains("risk-policy")));
        }
        RewardDecision::Release => panic!("failed verifier must withhold reward"),
    }
}

#[test]
fn window_open_withholds() {
    let reports = [
        report("accounting-integrity", true),
        report("risk-policy", true),
    ];

    let decision = evaluate(&reports, true, 2);

    match decision {
        RewardDecision::Withhold { reasons } => {
            assert!(reasons
                .iter()
                .any(|reason| reason.contains("challenge window")));
        }
        RewardDecision::Release => panic!("open challenge window must withhold reward"),
    }
}

#[test]
fn too_few_pass_withholds() {
    let reports = [report("accounting-integrity", true)];

    let decision = evaluate(&reports, false, 2);

    match decision {
        RewardDecision::Withhold { reasons } => {
            assert!(reasons
                .iter()
                .any(|reason| reason.contains("too few passing verifiers")));
        }
        RewardDecision::Release => panic!("insufficient passing verifiers must withhold reward"),
    }
}
