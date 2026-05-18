//! STWO tier-1 → Plonky2 tier-2 pipeline (in-process, stub digest path for CI speed).

use fractal_consensus::ValidatorSet;
use fractal_core::{NativeCall, Transaction, TxBody, VmKind};
use fractal_mempool::PooledTx;
use fractal_node::{HARDHAT_DEFAULT_SIGNER_0, NodeInner, ProduceTickOutcome, try_produce_one_tick};
use fractal_proof_aggregator::VerifiedStwoStatementV1;
use fractal_proof_condenser::{checkpoint_job_from_block, checkpoint_job_from_block_range};
use std::sync::Arc;
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
async fn stwo_range_digest_to_plonky2_on_anchor() {
    let node = Arc::new(Mutex::new(NodeInner::devnet_with_validators(
        ValidatorSet::phase1_singleton(),
    )));
    {
        let mut n = node.lock().await;
        n.anchor_interval = 3;
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
    {
        let n = node.lock().await;
        let job = checkpoint_job_from_block_range(n.chain_id, &n.blocks).expect("range job");
        assert_eq!(job.start_block, 1);
        assert_eq!(job.end_block, 2);
        let digest = job.stwo_commitment_stub();
        drop(n);
        let mut n = node.lock().await;
        let stmt = VerifiedStwoStatementV1::from_checkpoint_job(n.shard_id, &job, digest);
        n.record_verified_stwo_for_masterchain(stmt)
            .expect("buffer verified range stwo");
        push_noop(&mut n, 2);
    }
    assert!(matches!(
        try_produce_one_tick(&node).await,
        ProduceTickOutcome::Produced(3)
    ));
    let n = node.lock().await;
    let head = n.masterchain_ledger.head().expect("masterchain");
    assert_eq!(head.validity_proofs.len(), 1);
    assert_eq!(head.validity_proofs[0].start_block, 1);
    assert_eq!(head.validity_proofs[0].end_block, 2);
    assert_ne!(head.global_zk_root, [0u8; 32]);
    n.masterchain_ledger
        .plonky2_bundle()
        .expect("bundle")
        .verify()
        .expect("plonky2 verify");
    assert_eq!(
        n.masterchain_ledger
            .plonky2_bundle()
            .expect("bundle")
            .statement
            .verified_stwo_statements
            .len(),
        1
    );
}

#[tokio::test]
async fn stwo_digest_to_plonky2_on_anchor() {
    let node = Arc::new(Mutex::new(NodeInner::devnet_with_validators(
        ValidatorSet::phase1_singleton(),
    )));
    {
        let mut n = node.lock().await;
        n.anchor_interval = 2;
        push_noop(&mut n, 0);
    }
    assert!(matches!(
        try_produce_one_tick(&node).await,
        ProduceTickOutcome::Produced(1)
    ));
    {
        let n = node.lock().await;
        let block = n.blocks.last().expect("block 1");
        let job = checkpoint_job_from_block(n.chain_id, block).expect("job");
        let digest = job.stwo_commitment_stub();
        drop(n);
        let mut n = node.lock().await;
        n.record_stwo_for_masterchain(&job, digest)
            .expect("buffer stwo");
        push_noop(&mut n, 1);
    }
    assert!(matches!(
        try_produce_one_tick(&node).await,
        ProduceTickOutcome::Produced(2)
    ));
    let n = node.lock().await;
    let head = n.masterchain_ledger.head().expect("masterchain");
    assert_eq!(head.validity_proofs.len(), 1);
    assert_ne!(head.global_zk_root, [0u8; 32]);
    let bundle = n.masterchain_ledger.plonky2_bundle().expect("bundle");
    bundle.verify().expect("plonky2 verify");
}
