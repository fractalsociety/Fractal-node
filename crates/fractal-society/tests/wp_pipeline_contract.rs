use fractal_society::pkgs::pipeline_contract::{validate, PipelineOutcome, PipelineStage};
use fractal_society::protocol::Hash;
use fractal_society::verifier::VerifierReport;

fn report(passed: bool) -> VerifierReport {
    VerifierReport {
        id: "report".to_string(),
        verifier_id: "verifier".to_string(),
        verifier_version: "0.1.0".to_string(),
        passed,
        score: None,
        details: serde_json::Value::Null,
        warnings: Vec::new(),
        errors: Vec::new(),
        execution_time_seconds: 0.0,
        timestamp: chrono::DateTime::from_timestamp(0, 0).unwrap(),
    }
}

fn outcome() -> PipelineOutcome {
    PipelineOutcome {
        evidence_hash: Hash::new(b"evidence"),
        scorecard_hash: Hash::new(b"scorecard"),
        verifier_reports: Vec::new(),
        committed: false,
        reward_released: false,
    }
}

#[test]
fn stage_ladder_is_correct() {
    let mut run = outcome();
    run.evidence_hash = Hash(String::new());
    assert_eq!(run.stage(), PipelineStage::Run);

    let mut score = outcome();
    assert_eq!(score.stage(), PipelineStage::Score);

    score.verifier_reports.push(report(true));
    assert_eq!(score.stage(), PipelineStage::Verify);

    score.committed = true;
    assert_eq!(score.stage(), PipelineStage::Commit);

    score.reward_released = true;
    assert_eq!(score.stage(), PipelineStage::Reward);
}

#[test]
fn is_complete_requires_reward_and_all_verifiers_passed() {
    let mut complete = outcome();
    complete.verifier_reports.push(report(true));
    complete.committed = true;
    complete.reward_released = true;
    assert!(complete.is_complete());

    let mut unrewarded = complete.clone();
    unrewarded.reward_released = false;
    assert!(!unrewarded.is_complete());

    let mut failed = complete;
    failed.verifier_reports.push(report(false));
    assert!(!failed.is_complete());
}

#[test]
fn validate_rejects_empty_or_zero_evidence_hash() {
    let mut empty = outcome();
    empty.evidence_hash = Hash(String::new());
    assert!(validate(&empty).is_err());

    let mut zero = outcome();
    zero.evidence_hash = Hash("0".repeat(64));
    assert!(validate(&zero).is_err());
}

#[test]
fn validate_accepts_well_formed_outcome() {
    let mut valid = outcome();
    valid.verifier_reports.push(report(true));
    valid.committed = true;
    valid.reward_released = true;

    assert!(validate(&valid).is_ok());
}
