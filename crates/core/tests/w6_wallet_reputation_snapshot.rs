//! W6 / §10.4: `NativeCall::WalletReputationSnapshotV1` (`docs/wallet.md` §17).

use fractal_core::{
    apply_block, Account, NativeCall, State, Transaction, TxBody, VmKind, HARDHAT_DEFAULT_SIGNER_0,
};
use fractal_crypto::hash::keccak256;
use fractal_wallet::{
    compute_reputation_score_milli, ReputationLedgerSummary, ReputationParams, SettlementEvent,
    ToolClass,
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
fn wallet_reputation_snapshot_stores_score_and_commitment() {
    let mut state = funded_state();
    let provider_id = [0xabu8; 32];
    let summary = ReputationLedgerSummary {
        tool_class: ToolClass::Browser,
        successful: vec![SettlementEvent {
            settled_at_ms: 1_000,
            weight: 1,
        }],
        failed_settlements: 0,
        slashing_events: 0,
        first_seen_ms: 500,
        now_ms: 2_000,
        available_stake: 10,
        distinct_client_count: 1,
    };
    let summary_borsh = borsh::to_vec(&summary).unwrap();
    let expected_score =
        compute_reputation_score_milli(&summary, &ReputationParams::default());
    let expected_commitment = keccak256(&summary_borsh);

    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::WalletReputationSnapshotV1 {
            provider_id,
            tool_class: ToolClass::Browser as u8,
            summary_borsh: summary_borsh.clone(),
        }),
    };
    apply_block(&mut state, std::slice::from_ref(&tx)).unwrap();

    assert_eq!(
        state.wallet_reputation_score_milli(&provider_id, ToolClass::Browser as u8),
        Some(expected_score)
    );
    assert_eq!(
        state
            .wallet_reputation_ledger_commitment
            .get(&(provider_id, ToolClass::Browser as u8)),
        Some(&expected_commitment)
    );
    assert_eq!(state.accounts.get(&HARDHAT_DEFAULT_SIGNER_0).unwrap().nonce, 1);
}

#[test]
fn wallet_reputation_snapshot_tool_class_mismatch_rejected() {
    let mut state = funded_state();
    let provider_id = [1u8; 32];
    let summary = ReputationLedgerSummary {
        tool_class: ToolClass::Browser,
        successful: vec![],
        failed_settlements: 0,
        slashing_events: 0,
        first_seen_ms: 0,
        now_ms: 0,
        available_stake: 0,
        distinct_client_count: 0,
    };
    let summary_borsh = borsh::to_vec(&summary).unwrap();
    let tx = Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::WalletReputationSnapshotV1 {
            provider_id,
            tool_class: ToolClass::LlmInference as u8,
            summary_borsh,
        }),
    };
    let err = apply_block(&mut state, std::slice::from_ref(&tx)).unwrap_err();
    assert_eq!(
        err.to_string(),
        fractal_core::ExecError::InvalidShape.to_string()
    );
}
