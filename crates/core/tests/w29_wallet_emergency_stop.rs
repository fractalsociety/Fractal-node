//! §14.1 / §29: global governance `WalletEmergencyStopV1` gates new wallet activity.

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
fn emergency_stop_blocks_post_task_and_mints_reopen_after_disengage() {
    let mut state = fund_two();
    let stop = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::WalletEmergencyStopV1 { engage: true }),
    };
    apply_block(&mut state, &[stop]).unwrap();
    assert!(state.wallet_emergency_stop);

    let post = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 1,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::WalletPostTaskV1 {
            metadata_uri: "ipfs://x".into(),
            bounty_budget: 1,
            tool_budget: 0,
            verifier_budget: 0,
        }),
    };
    let err = apply_block(&mut state, &[post]).unwrap_err();
    assert_eq!(err, ExecError::WalletEmergencyStopActive);

    let release = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 1,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::WalletEmergencyStopV1 { engage: false }),
    };
    apply_block(&mut state, &[release]).unwrap();
    assert!(!state.wallet_emergency_stop);

    let post2 = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 2,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::WalletPostTaskV1 {
            metadata_uri: "ipfs://y".into(),
            bounty_budget: 2,
            tool_budget: 0,
            verifier_budget: 0,
        }),
    };
    apply_block(&mut state, &[post2]).unwrap();
    assert_eq!(state.next_wallet_task_id, 2);
}

#[test]
fn emergency_stop_requires_governance_when_set() {
    let mut state = fund_two();
    state.governance = HARDHAT_DEFAULT_SIGNER_1;
    let bad = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::WalletEmergencyStopV1 { engage: true }),
    };
    let err = apply_block(&mut state, &[bad]).unwrap_err();
    assert_eq!(err, ExecError::NotAuthorized);
}

#[test]
fn finalize_task_allowed_while_emergency_stop_engaged() {
    let mut state = fund_two();
    let agent_pk = [0xabu8; 32];
    let root = [0x11u8; 32];
    let setup = [
        Transaction {
            signer: HARDHAT_DEFAULT_SIGNER_0,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::WalletPostTaskV1 {
                metadata_uri: "ipfs://task".into(),
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
            body: TxBody::Native(NativeCall::WalletSubmitTaskV1 {
                task_id: 1,
                artifact_pointer: "da://a".into(),
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
                score: 88,
            }),
        },
    ];
    apply_block(&mut state, &setup).unwrap();

    let engage = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 2,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::WalletEmergencyStopV1 { engage: true }),
    };
    apply_block(&mut state, &[engage]).unwrap();

    let fin = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_1,
        nonce: 2,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::WalletFinalizeTaskV1 { task_id: 1 }),
    };
    apply_block(&mut state, &[fin]).unwrap();
    let row = state.wallet_tasks.get(&1).unwrap();
    assert_eq!(row.status, WALLET_TASK_FINALIZED);
    assert_eq!(row.escrow_wei, 0);
}
