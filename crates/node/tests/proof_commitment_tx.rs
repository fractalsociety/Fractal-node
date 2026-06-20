use std::sync::Arc;

use fractal_core::{NativeCall, TxBody};
use fractal_node::{try_produce_one_tick, NodeInner, ProduceTickOutcome};
use fractal_rpc::ChainInteraction;
use tokio::sync::Mutex;

#[tokio::test]
async fn submit_proof_hash_mines_real_native_transaction() {
    let node = Arc::new(Mutex::new(NodeInner::devnet()));
    let proof_hash = [0x42u8; 32];

    let response = {
        let mut n = node.lock().await;
        n.submit_proof_hash(proof_hash).unwrap()
    };
    let tx_hash: [u8; 32] = hex::decode(response.transaction_hash.trim_start_matches("0x"))
        .unwrap()
        .try_into()
        .unwrap();

    assert!(matches!(
        try_produce_one_tick(&node).await,
        ProduceTickOutcome::Produced(1)
    ));

    let n = node.lock().await;
    let (block_number, _block_hash, tx_index) = n.mined_tx_info(&tx_hash).unwrap();
    assert_eq!(block_number, 1);
    assert_eq!(tx_index, 0);

    let block = n
        .block_by_hash(&n.block_hash_by_number(1).unwrap())
        .unwrap();
    assert_eq!(block.transactions.len(), 1);
    assert_eq!(block.transactions[0], n.tx_by_hash(&tx_hash).unwrap());
    assert!(matches!(
        &block.transactions[0].body,
        TxBody::Native(NativeCall::ProofCommitmentV1 { proof_hash: got }) if got == &proof_hash
    ));
}
