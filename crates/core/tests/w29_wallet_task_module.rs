//! §14.5 / §29: native task lifecycle (`WalletPostTaskV1` … `WalletFinalizeTaskV1`).

use fractal_core::{
    apply_block, Account, ExecError, NativeCall, State, Transaction, TxBody, VmKind,
    HARDHAT_DEFAULT_SIGNER_0, HARDHAT_DEFAULT_SIGNER_1,
};
use fractal_core::WALLET_TASK_FINALIZED;

fn fund_two() -> State {
    let mut state = State::default();
    state.execution_timestamp_ms = 1_000_000;
    state.accounts.insert(
        HARDHAT_DEFAULT_SIGNER_0,
        Account {
            nonce: 0,
            balance: 10_000_000,
        },
    );
    state.accounts.insert(
        HARDHAT_DEFAULT_SIGNER_1,
        Account {
            nonce: 0,
            balance: 1_000_000,
        },
    );
    state
}

#[test]
fn task_full_lifecycle_pays_checkout_signer() {
    let mut state = fund_two();
    let agent_pk = [0xabu8; 32];
    let root = [0x11u8; 32];
    let bal0_before = state.accounts.get(&HARDHAT_DEFAULT_SIGNER_0).unwrap().balance;
    let txs = [
        Transaction {
            signer: HARDHAT_DEFAULT_SIGNER_0,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::WalletPostTaskV1 {
                metadata_uri: "ipfs://task-meta".into(),
                bounty_budget: 100,
                tool_budget: 50,
                verifier_budget: 25,
            }),
        },
        Transaction {
            signer: HARDHAT_DEFAULT_SIGNER_1,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::WalletCheckoutTaskV1 {
                task_id: 1,
                agent_session: agent_pk,
                expiry_ms: 2_000_000,
            }),
        },
        Transaction {
            signer: HARDHAT_DEFAULT_SIGNER_1,
            nonce: 1,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::WalletRenewCheckoutV1 {
                task_id: 1,
                evidence_uri: "progress:10%".into(),
                new_expiry_ms: 3_000_000,
            }),
        },
        Transaction {
            signer: HARDHAT_DEFAULT_SIGNER_1,
            nonce: 2,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::WalletSubmitTaskV1 {
                task_id: 1,
                artifact_pointer: "da://artifact".into(),
                tool_receipt_root: root,
            }),
        },
        Transaction {
            signer: HARDHAT_DEFAULT_SIGNER_0,
            nonce: 1,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::WalletVerifyTaskV1 {
                task_id: 1,
                verifier_sig: [0u8; 64],
                score: 99,
            }),
        },
        Transaction {
            signer: HARDHAT_DEFAULT_SIGNER_1,
            nonce: 3,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::WalletFinalizeTaskV1 { task_id: 1 }),
        },
    ];
    apply_block(&mut state, &txs).unwrap();

    let row = state.wallet_tasks.get(&1).unwrap();
    assert_eq!(row.status, WALLET_TASK_FINALIZED);
    assert_eq!(row.escrow_wei, 0);
    assert_eq!(row.verifier_score, 99);
    assert_eq!(row.tool_receipt_root, root);

    let paid = bal0_before.saturating_sub(175);
    let agent_bal = state.accounts.get(&HARDHAT_DEFAULT_SIGNER_1).unwrap().balance;
    assert_eq!(agent_bal, 1_000_000 + 175);

    let owner_bal = state.accounts.get(&HARDHAT_DEFAULT_SIGNER_0).unwrap().balance;
    assert_eq!(owner_bal, paid);
}

#[test]
fn task_verify_rejected_for_checkout_signer() {
    let mut state = fund_two();
    apply_block(
        &mut state,
        &[
            Transaction {
                signer: HARDHAT_DEFAULT_SIGNER_0,
                nonce: 0,
                vm: VmKind::Native,
                body: TxBody::Native(NativeCall::WalletPostTaskV1 {
                    metadata_uri: String::new(),
                    bounty_budget: 1,
                    tool_budget: 0,
                    verifier_budget: 0,
                }),
            },
            Transaction {
                signer: HARDHAT_DEFAULT_SIGNER_1,
                nonce: 0,
                vm: VmKind::Native,
                body: TxBody::Native(NativeCall::WalletCheckoutTaskV1 {
                    task_id: 1,
                    agent_session: [1u8; 32],
                    expiry_ms: 9_000_000,
                }),
            },
            Transaction {
                signer: HARDHAT_DEFAULT_SIGNER_1,
                nonce: 1,
                vm: VmKind::Native,
                body: TxBody::Native(NativeCall::WalletSubmitTaskV1 {
                    task_id: 1,
                    artifact_pointer: "x".into(),
                    tool_receipt_root: [2u8; 32],
                }),
            },
        ],
    )
    .unwrap();

    let err = state
        .apply_transaction(&Transaction {
            signer: HARDHAT_DEFAULT_SIGNER_1,
            nonce: 2,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::WalletVerifyTaskV1 {
                task_id: 1,
                verifier_sig: [0u8; 64],
                score: 1,
            }),
        })
        .unwrap_err();
    assert_eq!(err, ExecError::NotAuthorized);
}

#[test]
fn task_submit_after_expiry_fails() {
    let mut state = fund_two();
    apply_block(
        &mut state,
        &[
            Transaction {
                signer: HARDHAT_DEFAULT_SIGNER_0,
                nonce: 0,
                vm: VmKind::Native,
                body: TxBody::Native(NativeCall::WalletPostTaskV1 {
                    metadata_uri: String::new(),
                    bounty_budget: 1,
                    tool_budget: 0,
                    verifier_budget: 0,
                }),
            },
            Transaction {
                signer: HARDHAT_DEFAULT_SIGNER_1,
                nonce: 0,
                vm: VmKind::Native,
                body: TxBody::Native(NativeCall::WalletCheckoutTaskV1 {
                    task_id: 1,
                    agent_session: [1u8; 32],
                    expiry_ms: 500_000,
                }),
            },
        ],
    )
    .unwrap();

    state.execution_timestamp_ms = 600_000;
    let err = state
        .apply_transaction(&Transaction {
            signer: HARDHAT_DEFAULT_SIGNER_1,
            nonce: 1,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::WalletSubmitTaskV1 {
                task_id: 1,
                artifact_pointer: "late".into(),
                tool_receipt_root: [0u8; 32],
            }),
        })
        .unwrap_err();
    assert_eq!(err, ExecError::WalletTaskState);
}
