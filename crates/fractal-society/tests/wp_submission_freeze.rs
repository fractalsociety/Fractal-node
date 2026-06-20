use fractal_society::pkgs::submission_freeze::Submission;
use fractal_society::protocol::Hash;

fn submission(attempt: u32) -> Submission {
    Submission::new(
        Hash::new(b"agent"),
        Hash::new(b"protocol"),
        Hash::new(b"dataset"),
        Hash::new(b"environment"),
        attempt,
    )
}

#[test]
fn identical_inputs_produce_identical_manifest_hash() {
    let first = submission(1);
    let second = submission(1);

    assert_eq!(
        first.manifest_hash().unwrap(),
        second.manifest_hash().unwrap()
    );
}

#[test]
fn changing_any_hash_changes_manifest_hash() {
    let baseline = submission(1).manifest_hash().unwrap();
    let changed_agent = Submission::new(
        Hash::new(b"agent-v2"),
        Hash::new(b"protocol"),
        Hash::new(b"dataset"),
        Hash::new(b"environment"),
        1,
    )
    .manifest_hash()
    .unwrap();
    let changed_protocol = Submission::new(
        Hash::new(b"agent"),
        Hash::new(b"protocol-v2"),
        Hash::new(b"dataset"),
        Hash::new(b"environment"),
        1,
    )
    .manifest_hash()
    .unwrap();
    let changed_dataset = Submission::new(
        Hash::new(b"agent"),
        Hash::new(b"protocol"),
        Hash::new(b"dataset-v2"),
        Hash::new(b"environment"),
        1,
    )
    .manifest_hash()
    .unwrap();
    let changed_env = Submission::new(
        Hash::new(b"agent"),
        Hash::new(b"protocol"),
        Hash::new(b"dataset"),
        Hash::new(b"environment-v2"),
        1,
    )
    .manifest_hash()
    .unwrap();

    assert_ne!(baseline, changed_agent);
    assert_ne!(baseline, changed_protocol);
    assert_ne!(baseline, changed_dataset);
    assert_ne!(baseline, changed_env);
}

#[test]
fn changing_attempt_changes_manifest_hash() {
    assert_ne!(
        submission(1).manifest_hash().unwrap(),
        submission(2).manifest_hash().unwrap()
    );
}

#[test]
fn manifest_hash_is_deterministic() {
    let submission = submission(7);

    assert_eq!(
        submission.manifest_hash().unwrap(),
        submission.manifest_hash().unwrap()
    );
}
