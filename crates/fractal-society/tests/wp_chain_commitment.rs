use fractal_society::pkgs::chain_commitment::{CommitmentAdapter, InMemoryCommitmentAdapter};
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
