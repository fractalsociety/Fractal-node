//! Followers must rebuild RPC tx index state (`mined_txs`, `eth_signed_raw`, hash maps) from synced
//! blocks — see `.cursor/scratchpad.md` Wallet infra / M4 polish.

use fractal_consensus::{eth_signed_raws_for_txs, execute_and_build_block};
use fractal_core::{NativeCall, Transaction, TxBody, VmKind};
use fractal_crypto::hash::keccak256;
use fractal_node::{NodeInner, HARDHAT_DEFAULT_SIGNER_0};

#[test]
fn apply_synced_block_fills_mined_txs_for_native_noop() {
    let mut n = NodeInner::devnet();
    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let mut scratch = n.state.clone();
    let block = execute_and_build_block(
        n.chain_id,
        1,
        n.view,
        n.head_hash,
        n.parent_qc_hash,
        [0u8; 32],
        1,
        n.gas_limit,
        &mut scratch,
        vec![tx],
        eth_signed_raws_for_txs(1),
    )
    .expect("block");

    n.apply_synced_block(&block).expect("apply");

    let ih = keccak256(&borsh::to_vec(&block.transactions[0]).expect("borsh"));
    assert!(
        n.mined_txs.contains_key(&ih),
        "follower should index mined tx by internal hash"
    );
    assert_eq!(n.height, 1);
}

#[test]
fn apply_synced_block_rejects_eth_raw_length_mismatch() {
    let mut n = NodeInner::devnet();
    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let mut scratch = n.state.clone();
    let mut block = execute_and_build_block(
        n.chain_id,
        1,
        n.view,
        n.head_hash,
        n.parent_qc_hash,
        [0u8; 32],
        1,
        n.gas_limit,
        &mut scratch,
        vec![tx],
        eth_signed_raws_for_txs(1),
    )
    .expect("block");
    block.eth_signed_raw.push(None);
    assert!(n.apply_synced_block(&block).is_err());
}
