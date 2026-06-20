use fractal_society::pkgs::run_bundle::RunBundle;
use fractal_society::protocol::Hash;

fn bundle(agent_id: &str) -> RunBundle {
    RunBundle::new(
        Hash::new(b"run-manifest"),
        Hash::new(b"evidence"),
        Hash::new(b"scorecard"),
        Hash::new(b"proof"),
        agent_id,
    )
}

#[test]
fn bundle_hash_is_deterministic() {
    let bundle = bundle("agent-1");

    assert_eq!(bundle.bundle_hash().unwrap(), bundle.bundle_hash().unwrap());
}

#[test]
fn identical_inputs_produce_identical_hash() {
    let first = bundle("agent-1");
    let second = bundle("agent-1");

    assert_eq!(first.bundle_hash().unwrap(), second.bundle_hash().unwrap());
}

#[test]
fn changing_any_hash_changes_bundle_hash() {
    let baseline = bundle("agent-1").bundle_hash().unwrap();
    let changed_run_manifest = RunBundle::new(
        Hash::new(b"run-manifest-v2"),
        Hash::new(b"evidence"),
        Hash::new(b"scorecard"),
        Hash::new(b"proof"),
        "agent-1",
    )
    .bundle_hash()
    .unwrap();
    let changed_evidence = RunBundle::new(
        Hash::new(b"run-manifest"),
        Hash::new(b"evidence-v2"),
        Hash::new(b"scorecard"),
        Hash::new(b"proof"),
        "agent-1",
    )
    .bundle_hash()
    .unwrap();
    let changed_scorecard = RunBundle::new(
        Hash::new(b"run-manifest"),
        Hash::new(b"evidence"),
        Hash::new(b"scorecard-v2"),
        Hash::new(b"proof"),
        "agent-1",
    )
    .bundle_hash()
    .unwrap();
    let changed_proof = RunBundle::new(
        Hash::new(b"run-manifest"),
        Hash::new(b"evidence"),
        Hash::new(b"scorecard"),
        Hash::new(b"proof-v2"),
        "agent-1",
    )
    .bundle_hash()
    .unwrap();

    assert_ne!(baseline, changed_run_manifest);
    assert_ne!(baseline, changed_evidence);
    assert_ne!(baseline, changed_scorecard);
    assert_ne!(baseline, changed_proof);
}

#[test]
fn changing_agent_id_changes_bundle_hash() {
    assert_ne!(
        bundle("agent-1").bundle_hash().unwrap(),
        bundle("agent-2").bundle_hash().unwrap()
    );
}
