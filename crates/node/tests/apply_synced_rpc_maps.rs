//! Followers must rebuild RPC tx index state (`mined_txs`, `eth_signed_raw`, hash maps) from synced
//! blocks — see `.cursor/scratchpad.md` Wallet infra / M4 polish.

use fractal_consensus::{eth_signed_raws_for_txs, execute_and_build_block};
use fractal_core::{NativeCall, Transaction, TxBody, VmKind};
use fractal_crypto::hash::keccak256;
use fractal_node::{NodeInner, SyncApplyError, HARDHAT_DEFAULT_SIGNER_0};

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
        n.validators.expected_proposer(n.view),
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
        n.validators.expected_proposer(n.view),
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

#[test]
fn apply_synced_block_rejects_bad_parent_qc_hash() {
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
        [0u8; 32],
        n.validators.expected_proposer(n.view),
        1,
        n.gas_limit,
        &mut scratch,
        vec![tx],
        eth_signed_raws_for_txs(1),
    )
    .expect("block");
    assert!(matches!(
        n.apply_synced_block(&block),
        Err(SyncApplyError::ParentQcHash)
    ));
}

#[test]
fn apply_synced_block_rejects_invalid_proposer() {
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
        [0xfe; 32],
        1,
        n.gas_limit,
        &mut scratch,
        vec![tx],
        eth_signed_raws_for_txs(1),
    )
    .expect("block");
    assert!(matches!(
        n.apply_synced_block(&block),
        Err(SyncApplyError::InvalidProposer)
    ));
}

#[test]
fn devnet_with_bft7_fixture_has_seven_validators() {
    let n = NodeInner::devnet_with_validators(fractal_consensus::ValidatorSet::phase2_bft7_fixture());
    assert_eq!(n.validators.len(), 7);
}
