use fractal_society::pkgs::appeals_flow::{Appeal, AppealState};

#[test]
fn legal_transitions_succeed() {
    let mut appeal = Appeal::file("appeal-1");

    assert_eq!(appeal.state, AppealState::Filed);
    appeal.begin_review().unwrap();
    assert_eq!(appeal.state, AppealState::UnderReview);
    appeal.resolve(true, "calculation error confirmed").unwrap();
    assert_eq!(appeal.state, AppealState::Resolved { upheld: true });
}

#[test]
fn resolving_from_filed_returns_error() {
    let mut appeal = Appeal::file("appeal-1");

    assert!(appeal.resolve(false, "too early").is_err());
    assert_eq!(appeal.state, AppealState::Filed);
    assert!(appeal.reason.is_none());
}

#[test]
fn reason_is_stored_on_resolve() {
    let mut appeal = Appeal::file("appeal-1");

    appeal.begin_review().unwrap();
    appeal.resolve(false, "insufficient evidence").unwrap();

    assert_eq!(appeal.reason.as_deref(), Some("insufficient evidence"));
}

#[test]
fn cannot_begin_review_after_resolution() {
    let mut appeal = Appeal::file("appeal-1");
    appeal.begin_review().unwrap();
    appeal.resolve(true, "upheld").unwrap();

    assert!(appeal.begin_review().is_err());
    assert_eq!(appeal.state, AppealState::Resolved { upheld: true });
}
