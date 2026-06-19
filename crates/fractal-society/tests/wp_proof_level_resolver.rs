use std::collections::HashMap;

use fractal_society::pkgs::proof_level_resolver::resolve;
use fractal_society::protocol::{DecisionTrace, EvidenceBundle, Hash, RiskDecision};
use fractal_society::verifier::{
    ProofLevel, Replication, Review, ReviewConfidence, ReviewDecision,
};

fn ts() -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::from_timestamp(0, 0).unwrap()
}

fn evidence(trace_count: u64) -> EvidenceBundle {
    EvidenceBundle {
        id: "evidence-1".to_string(),
        run_id: "run-1".to_string(),
        decision_traces: (0..trace_count)
            .map(|step| DecisionTrace {
                step,
                observation_hash: Hash::new(format!("obs-{step}").as_bytes()),
                action: serde_json::json!({ "step": step }),
                risk_decision: RiskDecision::Approved,
                outcome: serde_json::json!({ "reward": 1.0 }),
                timestamp: ts(),
            })
            .collect(),
        metrics: HashMap::new(),
        verifier_reports: Vec::new(),
        timestamp: ts(),
    }
}

fn review(decision: ReviewDecision) -> Review {
    Review {
        id: "review-1".to_string(),
        proof_id: "proof-1".to_string(),
        reviewer: "reviewer-1".to_string(),
        decision,
        comments: "reviewed".to_string(),
        confidence: ReviewConfidence::High,
        coi_declarations: Vec::new(),
        timestamp: ts(),
    }
}

fn replication(success: bool) -> Replication {
    Replication {
        id: "replication-1".to_string(),
        original_proof_id: "proof-1".to_string(),
        replicator: "replicator-1".to_string(),
        success,
        differences: Vec::new(),
        tolerance: 0.0,
        actual_difference: Some(0.0),
        environment: "unit-test".to_string(),
        timestamp: ts(),
    }
}

#[test]
fn empty_inputs_resolve_private_draft() {
    assert_eq!(resolve(&evidence(0), &[], &[]), ProofLevel::PrivateDraft);
}

#[test]
fn approved_review_with_evidence_resolves_auditable() {
    assert_eq!(
        resolve(&evidence(1), &[review(ReviewDecision::Approve)], &[]),
        ProofLevel::Auditable
    );
}

#[test]
fn successful_replication_resolves_reproducible() {
    assert_eq!(
        resolve(
            &evidence(1),
            &[review(ReviewDecision::Approve)],
            &[replication(true)]
        ),
        ProofLevel::Reproducible
    );
}

#[test]
fn resolver_is_deterministic() {
    let evidence = evidence(2);
    let reviews = vec![
        review(ReviewDecision::Reject),
        review(ReviewDecision::Approve),
    ];
    let replications = vec![replication(false), replication(true)];

    assert_eq!(
        resolve(&evidence, &reviews, &replications),
        resolve(&evidence, &reviews, &replications)
    );
}
