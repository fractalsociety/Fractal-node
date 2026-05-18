//! HyperBFT pipelined produce path (`docs/prd.md` §7.9).

use std::sync::Arc;

use fractal_consensus::ConsensusMode;
use fractal_core::{NativeCall, Transaction, TxBody, VmKind};
use fractal_mempool::PooledTx;
use fractal_node::{
    try_produce_one_tick, NodeInner, ProduceTickOutcome, HARDHAT_DEFAULT_SIGNER_0,
};
use fractal_shard::ShardTopology;
use tokio::sync::Mutex;

#[tokio::test]
async fn hyperbft_mode_produces_with_70ms_cadence_config() {
    let node = Arc::new(Mutex::new(NodeInner::devnet()));
    {
        let mut n = node.lock().await;
        n.consensus_mode = ConsensusMode::HyperBft;
        n.hyperbft_config.target_block_time_ms = 70;
        n.mempool.insert(PooledTx {
            tx: Transaction {
                signer: HARDHAT_DEFAULT_SIGNER_0,
                nonce: 0,
                vm: VmKind::Native,
                body: TxBody::Native(NativeCall::NoOp),
            },
            max_priority_fee_per_gas: 0,
            max_fee_per_gas: u128::MAX,
            eth_signed_raw: None,
        });
    }
    assert_eq!(node.lock().await.effective_block_cadence_ms(), 70);
    assert!(matches!(
        try_produce_one_tick(&node).await,
        ProduceTickOutcome::Pipelined(1)
    ));
    let mut committed = false;
    for _ in 0..6 {
        if matches!(
            try_produce_one_tick(&node).await,
            ProduceTickOutcome::Produced(1)
        ) {
            committed = true;
            break;
        }
    }
    assert!(committed, "pipeline should commit block 1 within a few ticks");
    assert_eq!(node.lock().await.height, 1);
}

#[tokio::test]
async fn multi_shard_topology_defaults_to_hyperbft() {
    let mut n = NodeInner::devnet();
    n.shard_topology = ShardTopology { shard_count: 2 };
    n.shard_id = 0;
    n.consensus_mode = ConsensusMode::parse_env(None, true);
    assert_eq!(n.consensus_mode, ConsensusMode::HyperBft);
    assert_eq!(n.effective_block_cadence_ms(), 70);
}
