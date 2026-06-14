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
    assert_eq!(
        state.accounts.get(&HARDHAT_DEFAULT_SIGNER_0).unwrap().nonce,
        1
    );
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
mod wallet_task_receipt_tool_fixture {
    use ed25519_dalek::SigningKey;
    use fractal_wallet::{
        provider_id_from_public_key, MeteringRecord, ToolClass, ToolReceipt, ToolReceiptBody,
    };

    pub fn sign_one(
        agent: &SigningKey,
        provider: &SigningKey,
        intent_byte: u8,
        task_id: u64,
        cost: u128,
    ) -> ToolReceipt {
        let provider_pk = provider.verifying_key().to_bytes();
        let body = ToolReceiptBody {
            intent_id: [intent_byte; 32],
            task_id,
            agent_session: agent.verifying_key().to_bytes(),
            provider_id: provider_id_from_public_key(&provider_pk),
            tool_class: ToolClass::Browser,
            payload_commitment: [0xabu8; 32],
            output_commitment: [0xcdu8; 32],
            output_pointer: "da://x".into(),
            metering: MeteringRecord {
                input_tokens: 0,
                output_tokens: 0,
                wall_duration_ms: 0,
                bytes_metered: 0,
            },
            cost,
            started_at: 1,
            completed_at: 2,
            attestation: None,
        };
        ToolReceipt::sign_new(body, provider).unwrap()
    }
}

#[cfg(feature = "wallet")]
#[test]
fn wallet_task_receipt_anchor_witness_verified() {
    use ed25519_dalek::SigningKey;
    use fractal_core::wallet_anchor;
    use fractal_wallet::build_task_receipt;
    use rand::rngs::OsRng;

    let mut rng = OsRng;
    let agent = SigningKey::generate(&mut rng);
    let prov = SigningKey::generate(&mut rng);
    let agent_pk = agent.verifying_key().to_bytes();

    let mut state = funded_state();
    let r = wallet_task_receipt_tool_fixture::sign_one(&agent, &prov, 0x55, 100, 5);
    let receipts = vec![r];
    let root = fractal_wallet::tool_receipt_root(&receipts);
    let tr = build_task_receipt(
        100,
        agent_pk,
        [4u8; 32],
        "da://x".into(),
        &receipts,
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
    use ed25519_dalek::SigningKey;
    use fractal_wallet::build_task_receipt;
    use rand::rngs::OsRng;

    let mut rng = OsRng;
    let agent = SigningKey::generate(&mut rng);
    let prov = SigningKey::generate(&mut rng);
    let agent_pk = agent.verifying_key().to_bytes();

    let mut state = funded_state();
    let r = wallet_task_receipt_tool_fixture::sign_one(&agent, &prov, 0x66, 101, 3);
    let receipts = vec![r];
    let root = fractal_wallet::tool_receipt_root(&receipts);
    let tr = build_task_receipt(
        101,
        agent_pk,
        [4u8; 32],
        "da://y".into(),
        &receipts,
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
