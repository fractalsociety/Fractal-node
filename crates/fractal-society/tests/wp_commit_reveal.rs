use fractal_society::pkgs::commit_reveal::{commit, reveal};
use serde_json::json;

#[test]
fn reveal_accepts_matching_commitment() {
    let value = json!({
        "skill": "repo-map",
        "version": "1.0.0",
        "checks": ["layout", "tests"]
    });
    let claimed = commit(&value);

    assert!(reveal(&value, &claimed));
}

#[test]
fn reveal_rejects_tampered_value() {
    let value = json!({"reward": 100, "recipient": "alice"});
    let tampered = json!({"reward": 101, "recipient": "alice"});
    let claimed = commit(&value);

    assert!(!reveal(&tampered, &claimed));
}

#[test]
fn commit_is_deterministic_and_canonical() {
    let first = json!({"b": 2, "a": {"z": 1, "y": 0}});
    let second = json!({"a": {"y": 0, "z": 1}, "b": 2});

    assert_eq!(commit(&first), commit(&first));
    assert_eq!(commit(&first), commit(&second));
}
