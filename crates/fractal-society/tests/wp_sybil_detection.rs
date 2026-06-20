use fractal_society::pkgs::sybil_detection::{analyze, ReviewRecord, SuspiciousPattern};

fn review(reviewer: &str, subject: &str) -> ReviewRecord {
    ReviewRecord {
        reviewer: reviewer.to_string(),
        subject: subject.to_string(),
    }
}

#[test]
fn self_review_is_flagged() {
    let patterns = analyze(&[review("alice", "alice")]);

    assert_eq!(
        patterns,
        vec![SuspiciousPattern::SelfReview {
            reviewer: "alice".to_string()
        }]
    );
}

#[test]
fn duplicate_review_is_flagged() {
    let patterns = analyze(&[review("alice", "proof-1"), review("alice", "proof-1")]);

    assert_eq!(
        patterns,
        vec![SuspiciousPattern::DuplicateReview {
            reviewer: "alice".to_string(),
            subject: "proof-1".to_string()
        }]
    );
}

#[test]
fn circular_review_is_flagged() {
    let patterns = analyze(&[review("alice", "bob"), review("bob", "alice")]);

    assert_eq!(
        patterns,
        vec![SuspiciousPattern::CircularReview {
            cycle: vec!["bob".to_string(), "alice".to_string()]
        }]
    );
}

#[test]
fn clean_set_is_empty() {
    let patterns = analyze(&[
        review("alice", "proof-1"),
        review("bob", "proof-2"),
        review("carol", "proof-3"),
    ]);

    assert!(patterns.is_empty());
}
