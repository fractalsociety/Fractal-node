//! M10 exit: deterministic proposal ordering + optimistic rollback (PRD §7.9.4, §7.9.1).

use std::sync::Arc;

use fractal_consensus::ConsensusMode;
use fractal_core::{Account, NativeCall, Transaction, TxBody, VmKind};
use fractal_mempool::PooledTx;
use fractal_node::{
    try_produce_one_tick, NodeInner, ProduceTickOutcome, HARDHAT_DEFAULT_SIGNER_0,
};
use tokio::sync::Mutex;

fn funded_signer(seed: u8) -> [u8; 20] {
    let mut s = [0u8; 20];
    s[19] = seed;
    s
}

#[tokio::test]
async fn hyperbft_block_orders_by_mempool_priority_not_insertion_order() {
    let node = Arc::new(Mutex::new(NodeInner::devnet()));
    let low = funded_signer(1);
    let high = funded_signer(2);
    {
        let mut n = node.lock().await;
        n.consensus_mode = ConsensusMode::HyperBft;
        for (signer, bal) in [(low, 100), (high, 100)] {
            n.state.accounts.insert(
                signer,
                Account {
                    nonce: 0,
                    balance: bal,
                },
            );
        }
        // Insert low priority first, high second — block must still lead with high.
        n.mempool.insert(PooledTx {
            tx: Transaction {
                signer: low,
                nonce: 0,
                vm: VmKind::Native,
                body: TxBody::Native(NativeCall::NoOp),
            },
            max_priority_fee_per_gas: 1,
            max_fee_per_gas: u128::MAX,
            eth_signed_raw: None,
        });
        n.mempool.insert(PooledTx {
            tx: Transaction {
                signer: high,
                nonce: 0,
                vm: VmKind::Native,
                body: TxBody::Native(NativeCall::NoOp),
            },
            max_priority_fee_per_gas: 100,
            max_fee_per_gas: u128::MAX,
            eth_signed_raw: None,
        });
    }

    let mut produced = false;
    for _ in 0..12 {
        match try_produce_one_tick(&node).await {
            ProduceTickOutcome::Produced(h) if h >= 1 => {
                produced = true;
                break;
            }
            _ => {}
        }
    }
    assert!(produced, "expected a committed block");

    let n = node.lock().await;
    let block = n.blocks.iter().find(|b| b.header.height == 1).expect("h=1");
    assert_eq!(block.transactions.len(), 2);
    assert_eq!(
        block.transactions[0].signer, high,
        "higher priority fee must be sequenced first (no post-proposal reorder)"
    );
    assert_eq!(block.transactions[1].signer, low);
}

#[tokio::test]
async fn hyperbft_rollback_on_failed_proposal_leaves_committed_state() {
    let node = Arc::new(Mutex::new(NodeInner::devnet()));
    {
        let mut n = node.lock().await;
        n.consensus_mode = ConsensusMode::HyperBft;
        n.state.accounts.insert(
            HARDHAT_DEFAULT_SIGNER_0,
            Account {
                nonce: 0,
                balance: 1_000,
            },
        );
        n.mempool.insert(PooledTx {
            tx: Transaction {
                signer: HARDHAT_DEFAULT_SIGNER_0,
                nonce: 99,
                vm: VmKind::Native,
                body: TxBody::Native(NativeCall::NoOp),
            },
            max_priority_fee_per_gas: 0,
            max_fee_per_gas: u128::MAX,
            eth_signed_raw: None,
        });
    }

    let committed_before = node.lock().await.state.clone();
    let mut outcome = ProduceTickOutcome::NotMyTurn;
    for _ in 0..8 {
        outcome = try_produce_one_tick(&node).await;
        if matches!(outcome, ProduceTickOutcome::BuildFailed) {
            break;
        }
    }
    assert!(
        matches!(outcome, ProduceTickOutcome::BuildFailed),
        "bad nonce should fail speculative build, got {outcome:?}"
    );
    let n = node.lock().await;
    assert_eq!(n.state, committed_before);
    assert_eq!(n.height, 0);
}
