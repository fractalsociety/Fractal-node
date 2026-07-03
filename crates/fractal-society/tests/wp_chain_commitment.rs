use fractal_society::pkgs::chain_commitment::{
    submit_rlmf_attestation, CommitmentAdapter, InMemoryCommitmentAdapter,
    RLMF_ATTESTATION_SCHEMA_V1,
};
use fractal_society::protocol::Hash;

#[test]
fn submit_returns_configured_network_and_finalized_reference() {
    let adapter = InMemoryCommitmentAdapter::new("localnet", 42);
    let proof_hash = Hash::new(b"proof");

    let reference = adapter.submit(&proof_hash).unwrap();

    assert_eq!(reference.network, "localnet");
    assert_eq!(reference.block_number, 42);
    assert!(reference.finalized);
    assert_eq!(reference.transaction_hash.len(), 64);
}

#[test]
fn two_submits_produce_distinct_increasing_blocks() {
    let adapter = InMemoryCommitmentAdapter::new("localnet", 7);
    let proof_hash = Hash::new(b"proof");

    let first = adapter.submit(&proof_hash).unwrap();
    let second = adapter.submit(&proof_hash).unwrap();

    assert_eq!(first.block_number, 7);
    assert_eq!(second.block_number, 8);
    assert_ne!(first.transaction_hash, second.transaction_hash);
}

#[test]
fn deterministic_for_same_starting_block() {
    let proof_hash = Hash::new(b"proof");
    let first_adapter = InMemoryCommitmentAdapter::new("localnet", 100);
    let second_adapter = InMemoryCommitmentAdapter::new("localnet", 100);

    let first = first_adapter.submit(&proof_hash).unwrap();
    let second = second_adapter.submit(&proof_hash).unwrap();

    assert_eq!(first.network, second.network);
    assert_eq!(first.block_number, second.block_number);
    assert_eq!(first.transaction_hash, second.transaction_hash);
    assert_eq!(first.finalized, second.finalized);
}

#[test]
fn rlmf_attestation_submission_uses_commitment_root_when_available() {
    let commitment_hash = Hash::new(b"rlmf commitment");
    let commitment_root = Hash::new(b"rlmf root");
    let submission = fractal_society::pkgs::chain_commitment::RlmfAttestationSubmission::new(
        "rlmf-attest-1",
        "adapter-failure-calibration-v1",
        "fractalwork",
        &commitment_hash.0,
        Some(&commitment_root.0),
    )
    .unwrap();
    let adapter = InMemoryCommitmentAdapter::new("fractalchain2-local", 300);

    let receipt = submit_rlmf_attestation(&adapter, submission).unwrap();

    assert_eq!(receipt.submission.schema, RLMF_ATTESTATION_SCHEMA_V1);
    assert_eq!(receipt.submitted_hash, commitment_root);
    assert_eq!(receipt.chain_reference.network, "fractalchain2-local");
    assert_eq!(receipt.chain_reference.block_number, 300);
    assert!(receipt.chain_reference.finalized);
}

#[test]
fn rlmf_attestation_submission_rejects_invalid_hashes() {
    let err = fractal_society::pkgs::chain_commitment::RlmfAttestationSubmission::new(
        "rlmf-attest-2",
        "adapter-failure-calibration-v2",
        "fractalwork",
        "not-a-hash",
        None::<&str>,
    )
    .unwrap_err();

    assert!(err.to_string().contains("Hash must be 64 hex characters"));
}
