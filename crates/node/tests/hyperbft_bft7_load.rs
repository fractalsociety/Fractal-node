//! BFT-7 + HyperBFT 70 ms load: deterministic ordering under pipelined produce (`docs/prd.md` §7.9).

use std::sync::Arc;

use fractal_consensus::ValidatorSet;
use fractal_core::{NativeCall, Transaction, TxBody, VmKind};
use fractal_mempool::PooledTx;
use fractal_node::{
    try_produce_one_tick, NodeInner, ProduceTickOutcome, HARDHAT_DEFAULT_SIGNER_0,
};
use tokio::sync::Mutex;

#[tokio::test]
async fn hyperbft_bft7_seventy_ms_pipeline_load() {
    let node = Arc::new(Mutex::new(NodeInner::devnet_with_validator_index(
        ValidatorSet::phase2_bft7_fixture(),
        0,
    )));
    {
        let mut n = node.lock().await;
        n.consensus_mode = fractal_consensus::ConsensusMode::HyperBft;
        n.hyperbft_config.target_block_time_ms = 70;
        assert_eq!(n.effective_block_cadence_ms(), 70);
        for nonce in 0u64..8 {
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
    }

    let mut committed_heights = Vec::new();
    for _ in 0..120 {
        {
            let mut n = node.lock().await;
            n.inject_quorum_votes_for_pipeline_or_tip();
            let set_size = n.validators.len() as u64;
            let idx = n.validator_index as u64;
            let rem = n.view % set_size;
            if rem != idx {
                n.view += (set_size + idx - rem) % set_size;
            }
        }
        match try_produce_one_tick(&node).await {
            ProduceTickOutcome::Produced(h) => {
                if committed_heights.last().copied() != Some(h) {
                    committed_heights.push(h);
                }
            }
            ProduceTickOutcome::Pipelined(_) | ProduceTickOutcome::NotMyTurn => {}
            ProduceTickOutcome::AwaitingParentQc => {}
            other => panic!("unexpected tick outcome: {other:?}"),
        }
        let h = node.lock().await.height;
        if h >= 5 {
            break;
        }
    }

    assert!(
        committed_heights.len() >= 3,
        "expected at least 3 committed blocks, got {committed_heights:?}"
    );
    for w in committed_heights.windows(2) {
        assert_eq!(w[0] + 1, w[1], "heights must advance by 1");
    }

    let n = node.lock().await;
    let mut nonces: Vec<u64> = n
        .blocks
        .iter()
        .flat_map(|b| b.transactions.iter().map(|t| t.nonce))
        .collect();
    nonces.sort_unstable();
    assert_eq!(nonces, (0..nonces.len() as u64).collect::<Vec<_>>());
}
