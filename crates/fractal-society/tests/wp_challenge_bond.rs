use fractal_society::pkgs::challenge_bond::{BondState, ChallengeBond};

#[test]
fn post_creates_posted_bond() {
    let bond = ChallengeBond::post("alice", 100);

    assert_eq!(bond.poster, "alice");
    assert_eq!(bond.amount, 100);
    assert_eq!(bond.state, BondState::Posted);
}

#[test]
fn slash_transitions_posted_to_slashed() {
    let mut bond = ChallengeBond::post("alice", 100);

    bond.slash().unwrap();

    assert_eq!(bond.state, BondState::Slashed);
}

#[test]
fn release_transitions_posted_to_released() {
    let mut bond = ChallengeBond::post("alice", 100);

    bond.release().unwrap();

    assert_eq!(bond.state, BondState::Released);
}

#[test]
fn double_settle_returns_error_and_keeps_state() {
    let mut bond = ChallengeBond::post("alice", 100);
    bond.release().unwrap();

    assert!(bond.slash().is_err());
    assert_eq!(bond.state, BondState::Released);
}
