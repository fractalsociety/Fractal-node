//! M11: tier-1 validity proof submission + Plonky2 `globalZkRoot` on masterchain seal.

use std::sync::Arc;

use fractal_core::{NativeCall, Transaction, TxBody, VmKind};
use fractal_mempool::PooledTx;
use fractal_node::{
    try_produce_one_tick, NodeInner, ProduceTickOutcome, HARDHAT_DEFAULT_SIGNER_0,
};
use fractal_proof_aggregator::proof_submission_from_checkpoint_digest;
use tokio::sync::Mutex;

fn push_noop(n: &mut NodeInner, nonce: u64) {
    n.mempool.insert(PooledTx {
        tx: Transaction {
            signer: HARDHAT_DEFAULT_SIGNER_0,
            nonce,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        },
        max_priority_fee_per_gas: 0,
        max_fee_per_gas: u128::MAX,
        eth_signed_raw: None,
    });
}

#[tokio::test]
async fn validity_proof_sets_global_zk_root_on_anchor() {
    let node = Arc::new(Mutex::new(NodeInner::devnet()));
    {
        let mut n = node.lock().await;
        n.anchor_interval = 2;
        let sub = proof_submission_from_checkpoint_digest(
            0,
            1,
            2,
            [0xde; 20],
            [0x42; 32],
            0,
        );
        n.masterchain_ledger
            .submit_validity_proof(sub)
            .expect("accept proof before anchor");
        push_noop(&mut n, 0);
    }
    assert!(matches!(
        try_produce_one_tick(&node).await,
        ProduceTickOutcome::Produced(1)
    ));
    {
        let mut n = node.lock().await;
        push_noop(&mut n, 1);
    }
    assert!(matches!(
        try_produce_one_tick(&node).await,
        ProduceTickOutcome::Produced(2)
    ));
    let n = node.lock().await;
    let head = n.masterchain_ledger.head().expect("masterchain head");
    assert_eq!(head.validity_proofs.len(), 1);
    assert_ne!(head.global_zk_root, [0u8; 32]);
    assert_eq!(
        n.masterchain_ledger.global_zk_root(),
        Some(head.global_zk_root)
    );
    let bundle = n
        .masterchain_ledger
        .plonky2_bundle()
        .expect("Plonky2 SNARK bundle");
    assert!(!bundle.snark_bytes.is_empty());
    bundle.verify().expect("verify Plonky2 SNARK");
}
