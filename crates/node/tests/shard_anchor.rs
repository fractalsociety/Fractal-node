//! Shard anchor emission + masterchain ledger (PRD §7.10, M10).

use std::sync::Arc;

use fractal_core::{NativeCall, OnChainTaskReceipt, Transaction, TxBody, VmKind};
use fractal_crypto::hash::keccak256;
use fractal_masterchain::ledger::MasterchainLedger;
use fractal_mempool::PooledTx;
use fractal_node::{HARDHAT_DEFAULT_SIGNER_0, NodeInner, ProduceTickOutcome, try_produce_one_tick};
use fractal_shard::{
    CrossShardMessageV1, ShardAnchor, ShardTopology, masterchain_block_from_anchors_and_messages,
};
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

fn receipt_payload(receipt_id: [u8; 32]) -> (OnChainTaskReceipt, Vec<u8>, [u8; 32]) {
    let receipt = OnChainTaskReceipt {
        receipt_id,
        job_id: [0x45; 32],
        requester: [0x46; 20],
        worker: 1,
        verifier: 2,
        artifact_root: [0x47; 32],
        output_hash: [0x48; 32],
        score: 99,
        payout_amount: 0,
        verifier_fee: 0,
        protocol_fee: 0,
        final_status: 1,
        finalized_at: 7,
        schema_version: 1,
        tool_class: 3,
    };
    let payload = borsh::to_vec(&NativeCall::SettleReceipt(receipt.clone())).expect("payload");
    let payload_hash = keccak256(&payload);
    (receipt, payload, payload_hash)
}

fn receipt_payload_with_worker(
    receipt_id: [u8; 32],
    worker: u64,
) -> (OnChainTaskReceipt, Vec<u8>, [u8; 32]) {
    let mut receipt = receipt_payload(receipt_id).0;
    receipt.worker = worker;
    receipt.output_hash = [worker as u8; 32];
    let payload = borsh::to_vec(&NativeCall::SettleReceipt(receipt.clone())).expect("payload");
    let payload_hash = keccak256(&payload);
    (receipt, payload, payload_hash)
}

#[tokio::test]
async fn anchor_emitted_at_interval_on_produce() {
    let node = Arc::new(Mutex::new(NodeInner::devnet()));
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
        let mut n = node.lock().await;
        push_noop(&mut n, 1);
    }
    assert!(matches!(
        try_produce_one_tick(&node).await,
        ProduceTickOutcome::Produced(2)
    ));
    let n = node.lock().await;
    assert_eq!(n.masterchain_ledger.masterchain_height, 1);
    let anchor = n
        .masterchain_ledger
        .anchor_for_shard(0)
        .expect("anchor at height 2");
    assert_eq!(anchor.block_height, 2);
}

#[test]
fn destination_shard_delivers_masterchain_messages_idempotently() {
    let mut shard1 = NodeInner::devnet();
    shard1.shard_topology = ShardTopology { shard_count: 2 };
    shard1.shard_id = 1;

    let (receipt, payload, payload_hash) = receipt_payload([0x44; 32]);
    let to_shard1 = CrossShardMessageV1 {
        from_shard: 0,
        to_shard: 1,
        payload_hash,
        payload,
    };
    let noop_payload = borsh::to_vec(&NativeCall::NoOp).expect("noop payload");
    let to_shard0 = CrossShardMessageV1 {
        from_shard: 1,
        to_shard: 0,
        payload_hash: keccak256(&noop_payload),
        payload: noop_payload,
    };
    let block = masterchain_block_from_anchors_and_messages(
        7,
        vec![],
        vec![],
        [0u8; 32],
        vec![to_shard0, to_shard1.clone()],
    );

    assert_eq!(shard1.apply_masterchain_cross_shard_deliveries(&block), 1);
    assert_eq!(shard1.apply_masterchain_cross_shard_deliveries(&block), 0);
    assert_eq!(shard1.delivered_cross_shard_messages.len(), 1);
    assert_eq!(
        shard1.delivered_cross_shard_messages[0].masterchain_height,
        7
    );
    assert_eq!(shard1.delivered_cross_shard_messages[0].message, to_shard1);
    assert_eq!(
        shard1.state.receipts.get(&receipt.receipt_id),
        Some(&receipt),
        "destination shard should execute borsh NativeCall payload exactly once"
    );

    let rpc = shard1.delivered_cross_shard_messages_json();
    assert_eq!(rpc["shardId"], "0x1");
    assert_eq!(rpc["count"], 1);
    assert_eq!(rpc["messages"][0]["masterchainHeight"], "0x7");
    assert_eq!(rpc["messages"][0]["toShard"], "0x1");
}

#[test]
fn cross_shard_message_seals_orders_and_executes_on_destination_shard() {
    let mut shard0 = NodeInner::devnet();
    shard0.shard_topology = ShardTopology { shard_count: 2 };
    shard0.shard_id = 0;
    let mut shard1 = NodeInner::devnet();
    shard1.shard_topology = ShardTopology { shard_count: 2 };
    shard1.shard_id = 1;

    let (receipt, payload, payload_hash) = receipt_payload([0x55; 32]);
    let msg = CrossShardMessageV1 {
        from_shard: shard0.shard_id,
        to_shard: shard1.shard_id,
        payload_hash,
        payload,
    };

    let mut ledger = MasterchainLedger::default();
    ledger
        .ingest_shard_anchor(ShardAnchor {
            shard_id: 0,
            block_height: 10,
            state_root: [0x10; 32],
            witness_commitment: [0x11; 32],
        })
        .expect("anchor 0");
    ledger
        .ingest_shard_anchor(ShardAnchor {
            shard_id: 1,
            block_height: 10,
            state_root: [0x20; 32],
            witness_commitment: [0x21; 32],
        })
        .expect("anchor 1");
    ledger.submit_cross_shard_message(msg.clone());

    let mc = ledger.seal_round([0u8; 20]).expect("seal").expect("block");
    assert_eq!(mc.cross_shard_messages, vec![msg]);
    assert_eq!(shard1.apply_masterchain_cross_shard_deliveries(&mc), 1);
    assert_eq!(
        shard1.state.receipts.get(&receipt.receipt_id),
        Some(&receipt)
    );
    assert_eq!(shard1.apply_masterchain_cross_shard_deliveries(&mc), 0);
    assert_eq!(shard1.state.receipts.len(), 1);
}

#[test]
fn cross_shard_conflicting_receipts_resolve_by_masterchain_order() {
    let mut destination = NodeInner::devnet();
    destination.shard_topology = ShardTopology { shard_count: 3 };
    destination.shard_id = 2;

    let receipt_id = [0x66; 32];
    let (winner, winner_payload, winner_hash) = receipt_payload_with_worker(receipt_id, 10);
    let (loser, loser_payload, loser_hash) = receipt_payload_with_worker(receipt_id, 20);
    assert_ne!(winner, loser);
    assert_ne!(winner_hash, loser_hash);

    let later_submitted_but_ordered_first = CrossShardMessageV1 {
        from_shard: 0,
        to_shard: destination.shard_id,
        payload_hash: winner_hash,
        payload: winner_payload,
    };
    let earlier_submitted_but_ordered_second = CrossShardMessageV1 {
        from_shard: 1,
        to_shard: destination.shard_id,
        payload_hash: loser_hash,
        payload: loser_payload,
    };

    let mut ledger = MasterchainLedger::default();
    for shard_id in 0..3 {
        ledger
            .ingest_shard_anchor(ShardAnchor {
                shard_id,
                block_height: 10,
                state_root: [0x10 + shard_id as u8; 32],
                witness_commitment: [0x20 + shard_id as u8; 32],
            })
            .expect("anchor");
    }
    ledger.submit_cross_shard_message(earlier_submitted_but_ordered_second.clone());
    ledger.submit_cross_shard_message(later_submitted_but_ordered_first.clone());

    let mc = ledger.seal_round([0u8; 20]).expect("seal").expect("block");
    assert_eq!(
        mc.cross_shard_messages,
        vec![
            later_submitted_but_ordered_first.clone(),
            earlier_submitted_but_ordered_second
        ],
        "masterchain order, not arrival order, decides destination execution order"
    );

    assert_eq!(destination.apply_masterchain_cross_shard_deliveries(&mc), 1);
    assert_eq!(destination.state.receipts.get(&receipt_id), Some(&winner));
    assert_ne!(destination.state.receipts.get(&receipt_id), Some(&loser));
    assert_eq!(destination.delivered_cross_shard_messages.len(), 1);
    assert_eq!(
        destination.delivered_cross_shard_messages[0].message_index,
        0
    );
    assert_eq!(
        destination.delivered_cross_shard_messages[0].message,
        later_submitted_but_ordered_first
    );
    assert_eq!(destination.apply_masterchain_cross_shard_deliveries(&mc), 0);
    assert_eq!(destination.state.receipts.get(&receipt_id), Some(&winner));
}
