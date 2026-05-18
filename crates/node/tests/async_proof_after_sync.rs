//! Async STWO checkpoint proving runs **off** the HotStuff-2 commit path (`docs/prd.md` §7.8).
//!
//! After a block is applied (`apply_synced_block` — follower-style replay), the node enqueues a
//! [`CheckpointJob`] with `try_send` (bounded channel, default capacity 64). A background task
//! runs `build_persisted_checkpoint_proof` inside `spawn_blocking` so block production / BFT
//! logic is not held on the STWO CPU path.
//!
//! **Not covered here:** there is no explicit “spare CPU” scheduler — work runs when the async
//! worker pulls from the channel (Tokio blocking pool). Optional `FRACTAL_PROOF_ROCKSDB_PATH` /
//! `FRACTAL_PROOF_ARTIFACT_DIR` persist artifacts; **chain state** stays in-process (`State` +
//! `blocks`), not in RocksDB.

use fractal_consensus::{
    eth_signed_raws_for_txs, execute_and_build_block, genesis_parent_qc, header_hash,
};
use fractal_core::{NativeCall, Transaction, TxBody, VmKind};
use fractal_node::NodeInner;
use fractal_node::HARDHAT_DEFAULT_SIGNER_0;
use fractal_proof_condenser::{
    spawn_async_proof_condenser, ProofArtifactRegistry, ProofPersistenceConfig,
};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn checkpoint_proof_recorded_after_follower_sync() {
    let reg = Arc::new(ProofArtifactRegistry::new(ProofPersistenceConfig::default()));
    let (proof_tx, proof_rx) = tokio::sync::mpsc::channel(64);
    let _worker = spawn_async_proof_condenser(proof_rx, Some(reg.clone()), None);

    let mut n = NodeInner::devnet();
    n.set_proof_job_tx(Some(proof_tx));
    n.proof_artifact_registry = Some(reg.clone());

    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let mut scratch = n.state.clone();
    let gq = genesis_parent_qc();
    let block = execute_and_build_block(
        n.chain_id,
        0,
        1,
        n.view,
        n.head_hash,
        gq,
        vec![],
        n.validators.expected_proposer(n.view),
        1,
        n.gas_limit,
        &mut scratch,
        vec![tx],
        eth_signed_raws_for_txs(1),
        None,
    )
    .expect("block");

    n.apply_synced_block(&block).expect("apply");
    assert_eq!(n.height, 1, "BFT/sync path should advance head before async proof completes");

    let deadline = tokio::time::Instant::now() + Duration::from_secs(120);
    loop {
        if let Some(p) = reg.get(1) {
            assert_eq!(p.height, 1);
            assert_eq!(p.chain_id, n.chain_id);
            assert_eq!(
                p.header_hash,
                header_hash(&block.header).expect("header hash"),
                "proof job should bind to the committed header"
            );
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "timeout waiting for async checkpoint proof (STWO or stub fallback) at height=1"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}
