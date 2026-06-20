#![cfg(feature = "live-chain")]

use std::sync::Mutex;

use ethers::utils::id;
use fractal_society::chain::evm_adapter::{
    batch_root_from_hash, submit_batch_root_calldata, EvmBatchRoot, EvmCommitmentAdapter,
    EvmCommitmentReceipt, EvmSettlementClient, SUBMIT_BATCH_ROOT_METHOD,
};
use fractal_society::pkgs::chain_commitment::CommitmentAdapter;
use fractal_society::protocol::Hash;

#[derive(Debug, Default)]
struct MockEvmClient {
    roots: Mutex<Vec<EvmBatchRoot>>,
}

impl MockEvmClient {
    fn roots(&self) -> Vec<EvmBatchRoot> {
        self.roots.lock().unwrap().clone()
    }
}

impl EvmSettlementClient for MockEvmClient {
    fn submit_batch_root(
        &self,
        root: EvmBatchRoot,
    ) -> fractal_society::Result<EvmCommitmentReceipt> {
        self.roots.lock().unwrap().push(root);
        Ok(EvmCommitmentReceipt {
            network: "anvil-31337".to_string(),
            transaction_hash: "0xabc123".to_string(),
            block_number: 7,
            finalized: true,
        })
    }
}

impl EvmSettlementClient for &MockEvmClient {
    fn submit_batch_root(
        &self,
        root: EvmBatchRoot,
    ) -> fractal_society::Result<EvmCommitmentReceipt> {
        (*self).submit_batch_root(root)
    }
}

#[test]
fn submit_returns_chain_reference_from_evm_receipt() {
    let proof_hash = Hash::new(b"proof");
    let adapter = EvmCommitmentAdapter::new(MockEvmClient::default());

    let reference = adapter.submit(&proof_hash).unwrap();

    assert_eq!(reference.network, "anvil-31337");
    assert_eq!(reference.transaction_hash, "0xabc123");
    assert_eq!(reference.block_number, 7);
    assert!(reference.finalized);
}

#[test]
fn submit_calls_submit_batch_root_with_proof_hash_bytes32() {
    let proof_hash = Hash::new(b"proof");
    let expected_root = batch_root_from_hash(&proof_hash).unwrap();
    let client = MockEvmClient::default();
    let adapter = EvmCommitmentAdapter::new(&client);

    adapter.submit(&proof_hash).unwrap();

    assert_eq!(client.roots(), vec![expected_root]);
}

#[test]
fn calldata_uses_batch_settlement_submit_batch_root_signature() {
    let proof_hash = Hash::new(b"proof");
    let root = batch_root_from_hash(&proof_hash).unwrap();

    let calldata = submit_batch_root_calldata(&proof_hash).unwrap();

    assert_eq!(&calldata[..4], &id("submitBatchRoot(bytes32)")[..4]);
    assert_eq!(&calldata[4..], root.0);
    assert_eq!(SUBMIT_BATCH_ROOT_METHOD, "submitBatchRoot");
}
