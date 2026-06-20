use fractal_society::pkgs::review_conflicts::{check, ConflictOutcome, ReviewRequest};

fn request(reviewer: &str, author: &str, interests: Vec<&str>) -> ReviewRequest {
    ReviewRequest {
        reviewer: reviewer.to_string(),
        proof_author: author.to_string(),
        financial_interests: interests.into_iter().map(str::to_string).collect(),
    }
}

#[test]
fn rejects_self_review() {
    let outcome = check(&request("alice", "alice", Vec::new()));

    assert!(matches!(outcome, ConflictOutcome::Reject { .. }));
    if let ConflictOutcome::Reject { reason } = outcome {
        assert!(reason.contains("self-review"));
    }
}

#[test]
fn rejects_financial_conflict() {
    let outcome = check(&request("alice", "bob", vec!["investor"]));

    assert!(matches!(outcome, ConflictOutcome::Reject { .. }));
    if let ConflictOutcome::Reject { reason } = outcome {
        assert!(reason.contains("financial"));
    }
}

#[test]
fn accepts_unconflicted_review() {
    assert_eq!(
        check(&request("alice", "bob", Vec::new())),
        ConflictOutcome::Accept
    );
}

#[test]
fn check_is_deterministic() {
    let req = request("alice", "bob", vec!["grant"]);

    assert_eq!(check(&req), check(&req));
}
