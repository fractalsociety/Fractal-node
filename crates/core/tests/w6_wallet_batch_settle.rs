//! Wallet-native §16.3 `WalletBatchSettleV1` (distinct from M3 `SettleBatch`).

use ed25519_dalek::SigningKey;
use fractal_core::{
    apply_block, Account, NativeCall, State, Transaction, TxBody, VmKind,
    WalletToolBatchSettlePayload, WEI_PER_FRAC,
};
use fractal_wallet::{
    prepare_wallet_batch_receipts, sign_wallet_tool_batch, tool_receipt_root, MeteringRecord,
    ToolReceipt, ToolReceiptBody, ToolClass,
};
use rand::rngs::OsRng;

fn sample_receipt(
    provider_sk: &SigningKey,
    agent_sk: &SigningKey,
    intent_id: [u8; 32],
    cost: u128,
) -> ToolReceipt {
    let provider_pk = provider_sk.verifying_key().to_bytes();
    let body = ToolReceiptBody {
        intent_id,
        task_id: 1,
        agent_session: agent_sk.verifying_key().to_bytes(),
        provider_id: fractal_wallet::provider_id_from_public_key(&provider_pk),
        tool_class: ToolClass::Browser,
        payload_commitment: [0x11; 32],
        output_commitment: [0x22; 32],
        output_pointer: "da://out/1".into(),
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
    let r = ToolReceipt::sign_new(body, provider_sk).unwrap();
    let ack = r.sign_agent_ack(agent_sk).unwrap();
    r.with_agent_ack(ack)
}

#[test]
fn wallet_batch_settle_v1_records_root_and_pays_provider() {
    let provider_sk = SigningKey::generate(&mut OsRng);
    let provider_pk = provider_sk.verifying_key().to_bytes();
    let agent_sk = SigningKey::generate(&mut OsRng);

    let r1 = sample_receipt(&provider_sk, &agent_sk, [0xaau8; 32], 3 * WEI_PER_FRAC);
    let r2 = sample_receipt(&provider_sk, &agent_sk, [0xbbu8; 32], 2 * WEI_PER_FRAC);
    let total = 5 * WEI_PER_FRAC;
    let provider_id = fractal_wallet::provider_id_from_public_key(&provider_pk);
    let (root, blobs) =
        prepare_wallet_batch_receipts(&[r1.clone(), r2.clone()], provider_id, ToolClass::Browser, total)
            .unwrap();
    assert_eq!(root, tool_receipt_root(&[r1.clone(), r2.clone()]));

    let payout_to = [0x42u8; 20];
    let batch_id = [0x99u8; 32];
    let (_, batch_sig) = sign_wallet_tool_batch(
        &provider_sk,
        batch_id,
        root,
        total,
        2,
        payout_to,
    )
    .unwrap();

    let relayer = [0x01u8; 20];
    let mut state = State::default();
    state.accounts.insert(
        relayer,
        Account {
            nonce: 0,
            balance: 10 * WEI_PER_FRAC,
        },
    );

    let payload = WalletToolBatchSettlePayload {
        batch_id,
        provider_id,
        provider_public_key: provider_pk,
        tool_class: ToolClass::Browser as u8,
        receipt_root: root,
        total_cost: total,
        payout_to,
        receipts_borsh: blobs,
        submitted_at: 100,
        provider_batch_sig: batch_sig,
    };

    apply_block(
        &mut state,
        &[Transaction {
            signer: relayer,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::WalletBatchSettleV1(payload)),
        }],
    )
    .unwrap();

    assert!(state.wallet_tool_batches.contains_key(&batch_id));
    assert!(state
        .wallet_settled_tool_receipt_ids
        .contains_key(&r1.receipt_id));
    assert_eq!(
        state.accounts.get(&relayer).unwrap().balance,
        5 * WEI_PER_FRAC
    );
    assert_eq!(
        state.accounts.get(&payout_to).unwrap().balance,
        5 * WEI_PER_FRAC
    );
}

#[test]
fn wallet_batch_settle_rejects_duplicate_receipt_id() {
    let provider_sk = SigningKey::generate(&mut OsRng);
    let provider_pk = provider_sk.verifying_key().to_bytes();
    let agent_sk = SigningKey::generate(&mut OsRng);
    let intent = [0xccu8; 32];
    let r = sample_receipt(&provider_sk, &agent_sk, intent, WEI_PER_FRAC);
    let provider_id = fractal_wallet::provider_id_from_public_key(&provider_pk);
    let (root, blobs) = prepare_wallet_batch_receipts(
        &[r.clone()],
        provider_id,
        ToolClass::Browser,
        WEI_PER_FRAC,
    )
    .unwrap();

    let relayer = [0x02u8; 20];
    let payout_to = [0x43u8; 20];
    let batch_id = [0x88u8; 32];
    let (_, sig) = sign_wallet_tool_batch(
        &provider_sk,
        batch_id,
        root,
        WEI_PER_FRAC,
        1,
        payout_to,
    )
    .unwrap();

    let mut state = State::default();
    state.accounts.insert(
        relayer,
        Account {
            nonce: 0,
            balance: 10 * WEI_PER_FRAC,
        },
    );

    let payload = WalletToolBatchSettlePayload {
        batch_id,
        provider_id,
        provider_public_key: provider_pk,
        tool_class: ToolClass::Browser as u8,
        receipt_root: root,
        total_cost: WEI_PER_FRAC,
        payout_to,
        receipts_borsh: blobs,
        submitted_at: 1,
        provider_batch_sig: sig,
    };
    apply_block(
        &mut state,
        &[Transaction {
            signer: relayer,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::WalletBatchSettleV1(payload.clone())),
        }],
    )
    .unwrap();

    let batch_id2 = [0x77u8; 32];
    let (_, sig2) = sign_wallet_tool_batch(
        &provider_sk,
        batch_id2,
        root,
        WEI_PER_FRAC,
        1,
        payout_to,
    )
    .unwrap();
    let err = apply_block(
        &mut state,
        &[Transaction {
            signer: relayer,
            nonce: 1,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::WalletBatchSettleV1(WalletToolBatchSettlePayload {
                batch_id: batch_id2,
                provider_batch_sig: sig2,
                ..payload
            })),
        }],
    )
    .unwrap_err();
    assert!(matches!(
        err,
        fractal_core::ExecError::WalletToolReceiptAlreadySettled
    ));
}
