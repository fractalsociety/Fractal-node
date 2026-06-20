use fractal_society::pkgs::season_state_machine::{
    close, finalize, freeze, new_season, open, SeasonState,
};
use fractal_society::protocol::Hash;

#[test]
fn legal_transitions_succeed() {
    let mut season = new_season("s1", Hash::new(b"rules"));

    open(&mut season).unwrap();
    assert_eq!(season.state, SeasonState::Open);
    freeze(&mut season).unwrap();
    assert_eq!(season.state, SeasonState::Frozen);
    finalize(&mut season).unwrap();
    assert_eq!(season.state, SeasonState::Final);
    close(&mut season).unwrap();
    assert_eq!(season.state, SeasonState::Closed);
}

#[test]
fn illegal_transition_returns_error() {
    let mut season = new_season("s1", Hash::new(b"rules"));

    assert!(freeze(&mut season).is_err());
    assert_eq!(season.state, SeasonState::Draft);
}

#[test]
fn rules_freeze_after_open_and_hash_is_unchanged() {
    let rules_hash = Hash::new(b"rules-v1");
    let mut season = new_season("s1", rules_hash.clone());

    assert!(!season.rules_frozen);
    open(&mut season).unwrap();

    assert!(season.rules_frozen);
    assert_eq!(season.rules_hash, rules_hash);
}

#[test]
fn full_lifecycle_reaches_closed() {
    let mut season = new_season("s1", Hash::new(b"rules"));

    open(&mut season).unwrap();
    freeze(&mut season).unwrap();
    finalize(&mut season).unwrap();
    close(&mut season).unwrap();

    assert_eq!(season.state, SeasonState::Closed);
    assert!(season.rules_frozen);
}
