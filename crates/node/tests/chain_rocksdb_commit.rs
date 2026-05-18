//! Node writes typed block / tx index / receipt / state rows when `chain_store` is set.

use fractal_consensus::{eth_signed_raws_for_txs, execute_and_build_block, genesis_parent_qc};
use fractal_core::{NativeCall, Transaction, TxBody, VmKind};
use fractal_node::{NodeInner, HARDHAT_DEFAULT_SIGNER_0};
use fractal_storage::{block_by_height_key, FractalRocksDb, StoredBlockV1, CF_BLOCKS};

#[test]
fn apply_synced_block_persists_to_chain_rocksdb_when_configured() {
    let dir = std::env::temp_dir().join(format!(
        "fractal_node_chain_rocks_{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    let db = FractalRocksDb::open(&dir).expect("open db");

    let mut node = NodeInner::devnet();
    node.chain_store = Some(db);

    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let mut scratch = node.state.clone();
    let gq = genesis_parent_qc();
    let block = execute_and_build_block(
            node.chain_id,
            node.shard_id,
            1,
        node.view,
        node.head_hash,
        gq,
        vec![],
        node.validators.expected_proposer(node.view),
        1,
        node.gas_limit,
        &mut scratch,
        vec![tx],
        eth_signed_raws_for_txs(1),
        None,
    )
    .expect("block");

    node.apply_synced_block(&block).expect("apply");

    let db = node.chain_store.as_ref().expect("store");
    let raw = db
        .get_raw(CF_BLOCKS, &block_by_height_key(1))
        .expect("get")
        .expect("block bytes");
    let got: StoredBlockV1 = borsh::from_slice(&raw).expect("decode");
    assert_eq!(got.block.header.height, 1);
    let _ = std::fs::remove_dir_all(&dir);
}
