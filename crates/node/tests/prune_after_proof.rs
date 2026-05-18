//! M11: prune execution history after masterchain accepts validity proofs (PRD §6.2.3).

use fractal_consensus::{
    eth_signed_raws_for_txs, execute_and_build_block, genesis_parent_qc, header_hash,
};
use fractal_core::{NativeCall, Transaction, TxBody, VmKind};
use fractal_crypto::hash::keccak256;
use fractal_node::prune::{max_proved_end_block, prune_execution_history};
use fractal_shard::{MasterchainBlockV1, ProofSubmissionV1, ShardAnchor};
use fractal_storage::{
    CF_BLOCKS, CF_NATIVE_EVENTS, CF_RECEIPTS, CF_STATE, CF_TX_INDEX, block_by_height_key,
    block_hash_to_height_key, evm_mpt_root_at_height_key, native_event_key, state_at_height_key,
};

#[test]
fn max_proved_end_block_from_submissions() {
    let mc = MasterchainBlockV1 {
        height: 1,
        shard_anchors: vec![],
        validity_proofs: vec![
            ProofSubmissionV1 {
                shard_id: 0,
                start_block: 1,
                end_block: 4,
                prover: [0u8; 20],
                lag_seconds: 0,
                proof_digest: [1u8; 32],
            },
            ProofSubmissionV1 {
                shard_id: 0,
                start_block: 2,
                end_block: 8,
                prover: [0u8; 20],
                lag_seconds: 0,
                proof_digest: [2u8; 32],
            },
        ],
        global_state_root: [0u8; 32],
        global_zk_root: [9u8; 32],
        cross_shard_messages: vec![],
    };
    assert_eq!(max_proved_end_block(&mc), Some(8));
}

#[test]
fn prune_after_proof_drops_checkpoint_blobs() {
    let dir = std::env::temp_dir().join(format!("fractal_prune_test_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let db = fractal_storage::FractalRocksDb::open(&dir).expect("open");
    for h in 1..=6u64 {
        db.put_proof_blob(0, 1, h, format!("proof-{h}").as_bytes())
            .expect("put");
    }
    let mut blocks = vec![];
    let (_, proofs, rocks_rows) =
        prune_execution_history(&mut blocks, &Some(db.clone()), 0, 1, 50, 4);
    assert_eq!(proofs, 4);
    assert_eq!(rocks_rows, 0);
    assert!(db.get_proof_blob(0, 1, 4).expect("get").is_some() == false);
    assert!(db.get_proof_blob(0, 1, 5).expect("get5").is_some());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn prune_after_proof_drops_execution_rocksdb_rows() {
    let dir = std::env::temp_dir().join(format!("fractal_prune_exec_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let db = fractal_storage::FractalRocksDb::open(&dir).expect("open");

    let node = fractal_node::NodeInner::devnet();
    let tx = Transaction {
        signer: fractal_node::HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let mut scratch = node.state.clone();
    let block = execute_and_build_block(
        node.chain_id,
        node.shard_id,
        1,
        node.view,
        node.head_hash,
        genesis_parent_qc(),
        vec![],
        node.validators.expected_proposer(node.view),
        1,
        node.gas_limit,
        &mut scratch,
        vec![tx.clone()],
        eth_signed_raws_for_txs(1),
        None,
    )
    .expect("block");
    let tx_hash = keccak256(&borsh::to_vec(&tx).expect("tx borsh"));
    let block_hash = header_hash(&block.header).expect("header hash");
    db.persist_block_commit_v1(&block, &scratch, &[tx_hash], 1)
        .expect("persist");

    assert!(
        db.get_raw(CF_BLOCKS, &block_by_height_key(1))
            .expect("block")
            .is_some()
    );
    assert!(
        db.get_raw(CF_BLOCKS, &block_hash_to_height_key(&block_hash))
            .expect("block hash")
            .is_some()
    );
    assert!(
        db.get_raw(CF_TX_INDEX, &tx_hash)
            .expect("tx index")
            .is_some()
    );
    assert!(
        db.get_raw(CF_RECEIPTS, &tx_hash)
            .expect("receipt")
            .is_some()
    );
    assert!(
        db.get_raw(CF_NATIVE_EVENTS, &native_event_key(1, 0))
            .expect("native event")
            .is_some()
    );
    assert!(
        db.get_raw(CF_STATE, &state_at_height_key(1))
            .expect("state")
            .is_some()
    );
    assert!(
        db.get_raw(CF_STATE, &evm_mpt_root_at_height_key(1))
            .expect("mpt root")
            .is_some()
    );

    let mut blocks = vec![block];
    let (mem_blocks, proofs, rocks_rows) =
        prune_execution_history(&mut blocks, &Some(db.clone()), 0, 1, 50, 1);
    assert_eq!(mem_blocks, 1);
    assert_eq!(proofs, 0);
    assert_eq!(rocks_rows, 7);

    assert!(
        db.get_raw(CF_BLOCKS, &block_by_height_key(1))
            .expect("block")
            .is_none()
    );
    assert!(
        db.get_raw(CF_BLOCKS, &block_hash_to_height_key(&block_hash))
            .expect("block hash")
            .is_none()
    );
    assert!(
        db.get_raw(CF_TX_INDEX, &tx_hash)
            .expect("tx index")
            .is_none()
    );
    assert!(
        db.get_raw(CF_RECEIPTS, &tx_hash)
            .expect("receipt")
            .is_none()
    );
    assert!(
        db.get_raw(CF_NATIVE_EVENTS, &native_event_key(1, 0))
            .expect("native event")
            .is_none()
    );
    assert!(
        db.get_raw(CF_STATE, &state_at_height_key(1))
            .expect("state")
            .is_none()
    );
    assert!(
        db.get_raw(CF_STATE, &evm_mpt_root_at_height_key(1))
            .expect("mpt root")
            .is_none()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn prune_skips_when_zk_root_zero() {
    let mc = MasterchainBlockV1 {
        height: 1,
        shard_anchors: vec![ShardAnchor {
            shard_id: 0,
            block_height: 4,
            state_root: [0u8; 32],
            witness_commitment: [0u8; 32],
        }],
        validity_proofs: vec![],
        global_state_root: [0u8; 32],
        global_zk_root: [0u8; 32],
        cross_shard_messages: vec![],
    };
    assert_eq!(max_proved_end_block(&mc), None);
}
