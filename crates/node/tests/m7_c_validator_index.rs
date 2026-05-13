//! `FRACTAL_VALIDATOR_INDEX` gate (`docs/prd.md` §7 M7-c).
//!
//! These tests drive `try_produce_one_tick` directly so they don't depend on a
//! tokio interval and stay deterministic. Behavior under test:
//! - Singleton + index 0: leader at every view; ticks produce blocks.
//! - BFT-7 + index 0 at view 0: leader; produces.
//! - BFT-7 + index 3 at view 0: not leader; skips without draining the mempool.
//! - BFT-7 + index 3 at view 3 (after manually advancing): leader; produces.
//! - `producer_loop` started for a non-leader yields zero blocks within a
//!   short window (smoke that the gate also closes off the async path).

use std::sync::Arc;
use std::time::Duration;

use fractal_consensus::ValidatorSet;
use fractal_core::{NativeCall, Transaction, TxBody, VmKind};
use fractal_mempool::PooledTx;
use fractal_node::{
    producer_loop, try_produce_one_tick, NodeInner, ProduceTickOutcome, HARDHAT_DEFAULT_SIGNER_0,
};
use tokio::sync::Mutex;

fn push_one_native_noop(n: &mut NodeInner) {
    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    n.mempool.insert(PooledTx {
        tx,
        max_priority_fee_per_gas: 0,
        max_fee_per_gas: u128::MAX,
        eth_signed_raw: None,
    });
}

#[tokio::test]
async fn singleton_index_zero_produces_every_view() {
    let node = Arc::new(Mutex::new(NodeInner::devnet_with_validator_index(
        ValidatorSet::phase1_singleton(),
        0,
    )));
    for expected_height in 1u64..=3 {
        match try_produce_one_tick(&node).await {
            ProduceTickOutcome::Produced(h) => assert_eq!(h, expected_height),
            other => panic!("expected Produced({expected_height}); got {other:?}"),
        }
    }
}

#[tokio::test]
async fn bft7_non_leader_does_not_produce_and_does_not_drain_mempool() {
    let node = Arc::new(Mutex::new(NodeInner::devnet_with_validator_index(
        ValidatorSet::phase2_bft7_fixture(),
        3, // leader is index 0 at view 0
    )));
    {
        let mut n = node.lock().await;
        push_one_native_noop(&mut n);
        assert_eq!(n.mempool.len(), 1);
    }
    assert_eq!(try_produce_one_tick(&node).await, ProduceTickOutcome::NotMyTurn);

    let n = node.lock().await;
    assert_eq!(n.height, 0, "non-leader must not have produced");
    assert_eq!(
        n.mempool.len(),
        1,
        "mempool must NOT be drained on a non-leader tick"
    );
    assert_eq!(n.view, 0, "view must not advance without a block");
}

#[tokio::test]
async fn bft7_index_zero_produces_only_at_view_zero() {
    let node = Arc::new(Mutex::new(NodeInner::devnet_with_validator_index(
        ValidatorSet::phase2_bft7_fixture(),
        0,
    )));
    // view 0: index 0 is leader → produce.
    match try_produce_one_tick(&node).await {
        ProduceTickOutcome::Produced(h) => assert_eq!(h, 1),
        other => panic!("expected Produced(1); got {other:?}"),
    }
    // After producing, view advances to 1; index 0 is NOT leader for view 1..6.
    for _ in 0..6 {
        assert_eq!(try_produce_one_tick(&node).await, ProduceTickOutcome::NotMyTurn);
    }
    // Operator workaround for solo-binary BFT-7 stalls: hop view to 7 manually.
    {
        let mut n = node.lock().await;
        n.view = 7;
    }
    match try_produce_one_tick(&node).await {
        ProduceTickOutcome::Produced(h) => assert_eq!(h, 2),
        other => panic!("expected Produced(2) at view 7; got {other:?}"),
    }
}

#[tokio::test]
async fn bft7_index_three_takes_over_at_view_three() {
    let node = Arc::new(Mutex::new(NodeInner::devnet_with_validator_index(
        ValidatorSet::phase2_bft7_fixture(),
        3,
    )));
    // Skip ticks at view 0..2 (NotMyTurn). Then jump to view 3 and produce.
    for _ in 0..3 {
        assert_eq!(try_produce_one_tick(&node).await, ProduceTickOutcome::NotMyTurn);
    }
    {
        let mut n = node.lock().await;
        n.view = 3;
    }
    match try_produce_one_tick(&node).await {
        ProduceTickOutcome::Produced(h) => assert_eq!(h, 1),
        other => panic!("expected Produced(1) at view 3 for index 3; got {other:?}"),
    }
}

#[tokio::test]
async fn producer_loop_async_path_also_respects_gate() {
    let node = Arc::new(Mutex::new(NodeInner::devnet_with_validator_index(
        ValidatorSet::phase2_bft7_fixture(),
        4, // never the leader for view 0
    )));
    let handle = tokio::spawn(producer_loop(node.clone()));
    // Sleep > 2 tick intervals (default 500ms each); confirm no block was produced.
    tokio::time::sleep(Duration::from_millis(1200)).await;
    handle.abort();
    let n = node.lock().await;
    assert_eq!(n.height, 0);
    assert_eq!(n.view, 0);
    assert!(n.blocks.is_empty());
}
