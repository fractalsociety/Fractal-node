//! W6-d: `NativeCall::WalletTaskReceiptAnchorV1` (`docs/wallet.md` §9.2).

use fractal_core::{
    apply_block, Account, NativeCall, State, Transaction, TxBody, VmKind, HARDHAT_DEFAULT_SIGNER_0,
};

fn funded_state() -> State {
    let mut state = State::default();
    state.accounts.insert(
        HARDHAT_DEFAULT_SIGNER_0,
        Account {
            nonce: 0,
            balance: 1_000_000,
        },
    );
    state
}

#[test]
fn wallet_task_receipt_anchor_stores_commitment() {
    let mut state = funded_state();
    let commitment = [7u8; 32];
    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::WalletTaskReceiptAnchorV1 {
            commitment,
            receipt_witness: vec![],
        }),
    };
    apply_block(&mut state, std::slice::from_ref(&tx)).unwrap();
    assert_eq!(
        state.wallet_task_receipt_anchors.get(&commitment),
        Some(&HARDHAT_DEFAULT_SIGNER_0)
    );
    assert_eq!(state.accounts.get(&HARDHAT_DEFAULT_SIGNER_0).unwrap().nonce, 1);
}

#[test]
fn wallet_task_receipt_anchor_duplicate_rejected() {
    let mut state = funded_state();
    let commitment = [9u8; 32];
    let body = TxBody::Native(NativeCall::WalletTaskReceiptAnchorV1 {
        commitment,
        receipt_witness: vec![],
    });
    let tx0 = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: body.clone(),
    };
    apply_block(&mut state, &[tx0]).unwrap();
    let tx1 = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 1,
        vm: VmKind::Native,
        body,
    };
    let err = apply_block(&mut state, &[tx1]).unwrap_err();
    assert_eq!(
        err.to_string(),
        fractal_core::ExecError::DuplicateWalletAnchor.to_string()
    );
}

#[cfg(not(feature = "wallet"))]
#[test]
fn wallet_task_receipt_anchor_non_empty_witness_requires_wallet_feature() {
    let mut state = funded_state();
    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::WalletTaskReceiptAnchorV1 {
            commitment: [1u8; 32],
            receipt_witness: vec![1, 2, 3],
        }),
    };
    let err = apply_block(&mut state, std::slice::from_ref(&tx)).unwrap_err();
    assert_eq!(
        err.to_string(),
        fractal_core::ExecError::WalletFeatureDisabled.to_string()
    );
}

#[cfg(feature = "wallet")]
#[test]
fn wallet_task_receipt_anchor_witness_verified() {
    use fractal_core::wallet_anchor;
    use fractal_wallet::{build_task_receipt, ToolReceiptSummary};

    let mut state = funded_state();
    let summaries = vec![ToolReceiptSummary {
        receipt_id: [1u8; 32],
        intent_id: [2u8; 32],
        task_id: 100,
        cost: 5,
    }];
    let root = fractal_wallet::tool_receipt_root(&summaries);
    let tr = build_task_receipt(
        100,
        [3u8; 32],
        [4u8; 32],
        "da://x".into(),
        &summaries,
        5,
        root,
    )
    .unwrap();
    let witness = borsh::to_vec(&tr).unwrap();
    let commitment = wallet_anchor::task_receipt_commitment(&tr).unwrap();

    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::WalletTaskReceiptAnchorV1 {
            commitment,
            receipt_witness: witness,
        }),
    };
    apply_block(&mut state, std::slice::from_ref(&tx)).unwrap();
    assert_eq!(
        state.wallet_task_receipt_anchors.get(&commitment),
        Some(&HARDHAT_DEFAULT_SIGNER_0)
    );
}

#[cfg(feature = "wallet")]
#[test]
fn wallet_task_receipt_anchor_witness_mismatch() {
    use fractal_wallet::{build_task_receipt, ToolReceiptSummary};

    let mut state = funded_state();
    let summaries = vec![ToolReceiptSummary {
        receipt_id: [1u8; 32],
        intent_id: [2u8; 32],
        task_id: 101,
        cost: 3,
    }];
    let root = fractal_wallet::tool_receipt_root(&summaries);
    let tr = build_task_receipt(
        101,
        [3u8; 32],
        [4u8; 32],
        "da://y".into(),
        &summaries,
        3,
        root,
    )
    .unwrap();
    let witness = borsh::to_vec(&tr).unwrap();
    let wrong_commitment = [0xffu8; 32];

    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::WalletTaskReceiptAnchorV1 {
            commitment: wrong_commitment,
            receipt_witness: witness,
        }),
    };
    let err = apply_block(&mut state, std::slice::from_ref(&tx)).unwrap_err();
    assert_eq!(
        err.to_string(),
        fractal_core::ExecError::WalletCommitmentMismatch.to_string()
    );
}
