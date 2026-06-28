use fractal_society::pkgs::review_aggregation::{aggregate, Consensus};
use fractal_society::verifier::{Review, ReviewConfidence, ReviewDecision};

fn review(id: &str, decision: ReviewDecision) -> Review {
    Review {
        id: id.to_string(),
        proof_id: "proof-1".to_string(),
        reviewer: format!("reviewer-{id}"),
        decision,
        comments: String::new(),
        confidence: ReviewConfidence::High,
        coi_declarations: Vec::new(),
        timestamp: chrono::DateTime::from_timestamp(0, 0).unwrap(),
    }
}

#[test]
fn majority_approve_returns_approved() {
    let reviews = vec![
        review("1", ReviewDecision::Approve),
        review("2", ReviewDecision::Approve),
        review("3", ReviewDecision::Reject),
    ];

    assert_eq!(aggregate(&reviews, 3), Consensus::Approved);
}

#[test]
fn tie_returns_rejected() {
    let reviews = vec![
        review("1", ReviewDecision::Approve),
        review("2", ReviewDecision::Reject),
    ];

    assert_eq!(aggregate(&reviews, 2), Consensus::Rejected);
}

#[test]
fn below_quorum_returns_no_quorum() {
    let reviews = vec![review("1", ReviewDecision::Approve)];

    assert_eq!(aggregate(&reviews, 2), Consensus::NoQuorum);
}

#[test]
fn request_changes_counts_as_rejection() {
    let reviews = vec![
        review("1", ReviewDecision::Approve),
        review("2", ReviewDecision::RequestChanges),
        review("3", ReviewDecision::Abstain),
    ];

    assert_eq!(aggregate(&reviews, 3), Consensus::Rejected);
}
