use std::sync::Arc;

use fractal_consensus::{BlockPayload, CircuitVersion, ExecutionFeatureSetV1, ZoneProofUpdateV1};
use fractal_core::{NativeCall, Transaction, TxBody, VmKind};
use fractal_mempool::PooledTx;
use fractal_node::{
    try_produce_one_tick, BlockPayloadMode, NodeInner, ProduceTickOutcome,
    HARDHAT_DEFAULT_SIGNER_0, HARDHAT_DEFAULT_SIGNER_1,
};
use fractal_rpc::ChainInteraction;
use tokio::sync::Mutex;

fn push_tx(n: &mut NodeInner, tx: Transaction) {
    n.mempool.insert(PooledTx {
        tx,
        max_priority_fee_per_gas: 1,
        max_fee_per_gas: u128::MAX,
        eth_signed_raw: None,
    });
}

fn proof_commitment_tx(nonce: u64, proof_hash: [u8; 32]) -> Transaction {
    Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::ProofCommitmentV1 { proof_hash }),
    }
}

fn noop_tx(nonce: u64) -> Transaction {
    Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    }
}

fn transfer_tx(nonce: u64) -> Transaction {
    Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce,
        vm: VmKind::Evm,
        body: TxBody::Transfer {
            to: HARDHAT_DEFAULT_SIGNER_1,
            amount: 1,
        },
    }
}

fn proof_update(zone_id: u64, height: u64, byte: u8) -> ZoneProofUpdateV1 {
    ZoneProofUpdateV1 {
        zone_id,
        height,
        parent_root: [1u8; 32],
        new_root: [2u8; 32],
        tx_root: [3u8; 32],
        da_root: [4u8; 32],
        message_root: [5u8; 32],
        forced_inclusion_root: [6u8; 32],
        circuit_version: CircuitVersion::NativeStateTransitionV1,
        feature_set: ExecutionFeatureSetV1::empty(),
        proof_digest: [byte; 32],
    }
}

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

#[tokio::test]
async fn block_payload_mode_defaults_to_legacy_and_is_queryable() {
    let n = NodeInner::devnet();

    assert_eq!(n.block_payload_mode(), BlockPayloadMode::Legacy);
    assert_eq!(n.chain_config().block_payload_mode, "legacy");
}

#[test]
fn block_payload_mode_parses_supported_env_values() {
    assert_eq!(
        BlockPayloadMode::parse("legacy"),
        Some(BlockPayloadMode::Legacy)
    );
    assert_eq!(
        BlockPayloadMode::parse("proof_ingestion"),
        Some(BlockPayloadMode::ProofIngestion)
    );
    assert_eq!(
        BlockPayloadMode::parse("proof-ingestion"),
        Some(BlockPayloadMode::ProofIngestion)
    );
    assert_eq!(
        BlockPayloadMode::parse("mixed"),
        Some(BlockPayloadMode::Mixed)
    );
    assert_eq!(BlockPayloadMode::parse("full"), None);
}

#[tokio::test]
async fn proof_ingestion_mode_proposes_only_proof_compat_transactions() {
    let node = Arc::new(Mutex::new(NodeInner::devnet()));
    let proof_hash = [0x51u8; 32];
    {
        let mut n = node.lock().await;
        n.set_block_payload_mode(BlockPayloadMode::ProofIngestion);
        push_tx(&mut n, noop_tx(1));
        push_tx(&mut n, proof_commitment_tx(0, proof_hash));
    }

    assert_eq!(
        try_produce_one_tick(&node).await,
        ProduceTickOutcome::Produced(1)
    );

    let n = node.lock().await;
    assert_eq!(n.mempool.len(), 1);
    let block = n
        .block_by_hash(&n.block_hash_by_number(1).unwrap())
        .unwrap();
    assert_eq!(block.transactions, vec![proof_commitment_tx(0, proof_hash)]);
}

#[tokio::test]
async fn submit_proof_update_is_included_without_transaction_gas() {
    let node = Arc::new(Mutex::new(NodeInner::devnet()));
    let update = proof_update(7, 99, 0xaa);
    let expected_root = BlockPayload::ProofUpdates(vec![update.clone()])
        .payload_root()
        .unwrap();
    {
        let mut n = node.lock().await;
        n.set_block_payload_mode(BlockPayloadMode::ProofIngestion);
        let update_hash = n.submit_proof_update(update.clone(), 9).unwrap();
        assert!(update_hash != [0u8; 32]);
        assert_eq!(n.proof_pool.len(), 1);
        assert_eq!(n.mempool.len(), 0);
    }

    assert_eq!(
        try_produce_one_tick(&node).await,
        ProduceTickOutcome::Produced(1)
    );

    let n = node.lock().await;
    assert_eq!(n.proof_pool.len(), 0);
    assert_eq!(n.mempool.len(), 0);
    let block = n
        .block_by_hash(&n.block_hash_by_number(1).unwrap())
        .unwrap();
    assert!(block.transactions.is_empty());
    assert_eq!(block.header.gas_used, 0);
    assert_ne!(block.header.extra, expected_root);
    assert_eq!(block.header.da_bytes, block.da_sidecar.original_len);
    assert_eq!(
        block.header.da_share_count as usize,
        block.da_sidecar.shares.len()
    );
    fractal_consensus::verify_da_sidecar(&block.header, &block.da_sidecar).unwrap();
    let da_payload = fractal_consensus::reconstruct_da_payload(&block.da_sidecar).unwrap();
    assert_eq!(
        da_payload,
        borsh::to_vec(&BlockPayload::ProofUpdates(vec![update])).unwrap()
    );
}

#[tokio::test]
async fn mixed_mode_proposes_shared_state_and_proof_compat_transactions() {
    let node = Arc::new(Mutex::new(NodeInner::devnet()));
    let proof_hash = [0x62u8; 32];
    {
        let mut n = node.lock().await;
        n.set_block_payload_mode(BlockPayloadMode::Mixed);
        push_tx(&mut n, noop_tx(2));
        push_tx(&mut n, transfer_tx(1));
        push_tx(&mut n, proof_commitment_tx(0, proof_hash));
    }

    assert_eq!(
        try_produce_one_tick(&node).await,
        ProduceTickOutcome::Produced(1)
    );

    let n = node.lock().await;
    assert_eq!(n.mempool.len(), 1);
    let block = n
        .block_by_hash(&n.block_hash_by_number(1).unwrap())
        .unwrap();
    assert_eq!(block.transactions.len(), 2);
    assert_eq!(block.transactions[0], proof_commitment_tx(0, proof_hash));
    assert_eq!(block.transactions[1], transfer_tx(1));
}
