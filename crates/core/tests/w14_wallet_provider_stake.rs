//! Wallet §14.4: on-chain provider registration, stake, delayed unstake, and slashing.

use fractal_core::{
    Account, ExecError, NativeCall, ProviderRegistration, ProviderSlashRecord, State, Transaction,
    TxBody, VmKind, WALLET_PROVIDER_UNSTAKE_DELAY_MS,
};

const OWNER: fractal_core::Address = [0x11; 20];
const OTHER: fractal_core::Address = [0x22; 20];
const PROVIDER: [u8; 32] = [0x33; 32];
const CLASS_BROWSER: u8 = 0;

fn native_tx(signer: fractal_core::Address, nonce: u64, call: NativeCall) -> Transaction {
    Transaction {
        signer,
        nonce,
        vm: VmKind::Native,
        body: TxBody::Native(call),
    }
}

fn registration(bond: u128) -> ProviderRegistration {
    ProviderRegistration {
        provider_id: PROVIDER,
        owner: OWNER,
        public_key: [0x44; 32],
        encryption_pubkey: [0x55; 32],
        metadata_uri: "ipfs://provider".into(),
        endpoint_uri: "https://provider.example".into(),
        tool_classes: vec![CLASS_BROWSER],
        tee_attestation_hash: None,
        registration_bond: bond,
    }
}

fn state() -> State {
    let mut st = State::default();
    st.execution_timestamp_ms = 1_000;
    st.accounts.insert(
        OWNER,
        Account {
            nonce: 0,
            balance: 10_000,
        },
    );
    st.accounts.insert(
        OTHER,
        Account {
            nonce: 0,
            balance: 10_000,
        },
    );
    st
}

#[test]
fn provider_register_stake_unstake_finalize_round_trip() {
    let mut st = state();
    st.apply_transaction(&native_tx(
        OWNER,
        0,
        NativeCall::WalletRegisterProviderV1 {
            registration: registration(100),
        },
    ))
    .unwrap();
    st.apply_transaction(&native_tx(
        OWNER,
        1,
        NativeCall::WalletStakeForClassV1 {
            provider_id: PROVIDER,
            tool_class: CLASS_BROWSER,
            amount: 1_000,
        },
    ))
    .unwrap();
    let stake = st
        .wallet_provider_stakes
        .get(&(PROVIDER, CLASS_BROWSER))
        .unwrap();
    assert_eq!(stake.total, 1_000);
    assert_eq!(stake.available, 1_000);
    assert_eq!(st.accounts.get(&OWNER).unwrap().balance, 8_900);

    st.apply_transaction(&native_tx(
        OWNER,
        2,
        NativeCall::WalletProviderUnstakeRequestV1 {
            provider_id: PROVIDER,
            tool_class: CLASS_BROWSER,
            amount: 400,
        },
    ))
    .unwrap();
    let stake = st
        .wallet_provider_stakes
        .get(&(PROVIDER, CLASS_BROWSER))
        .unwrap();
    assert_eq!(stake.total, 1_000);
    assert_eq!(stake.available, 600);
    assert_eq!(stake.pending_unstake, 400);
    assert_eq!(st.wallet_provider_unstake_requests.len(), 1);

    let err = st
        .apply_transaction(&native_tx(
            OWNER,
            3,
            NativeCall::WalletProviderUnstakeFinalizeV1 { request_id: 1 },
        ))
        .unwrap_err();
    assert_eq!(err, ExecError::WalletProviderUnstakeNotMature);

    st.execution_timestamp_ms = 1_000 + WALLET_PROVIDER_UNSTAKE_DELAY_MS;
    st.apply_transaction(&native_tx(
        OWNER,
        3,
        NativeCall::WalletProviderUnstakeFinalizeV1 { request_id: 1 },
    ))
    .unwrap();
    let stake = st
        .wallet_provider_stakes
        .get(&(PROVIDER, CLASS_BROWSER))
        .unwrap();
    assert_eq!(stake.total, 600);
    assert_eq!(stake.available, 600);
    assert_eq!(stake.pending_unstake, 0);
    assert_eq!(st.accounts.get(&OWNER).unwrap().balance, 9_300);
}

#[test]
fn provider_slash_requires_evidence_and_burns_pending_unstake() {
    let mut st = state();
    st.apply_transaction(&native_tx(
        OWNER,
        0,
        NativeCall::WalletRegisterProviderV1 {
            registration: registration(0),
        },
    ))
    .unwrap();
    st.apply_transaction(&native_tx(
        OWNER,
        1,
        NativeCall::WalletStakeForClassV1 {
            provider_id: PROVIDER,
            tool_class: CLASS_BROWSER,
            amount: 1_000,
        },
    ))
    .unwrap();
    st.apply_transaction(&native_tx(
        OWNER,
        2,
        NativeCall::WalletProviderUnstakeRequestV1 {
            provider_id: PROVIDER,
            tool_class: CLASS_BROWSER,
            amount: 700,
        },
    ))
    .unwrap();
    let evidence = [0x99; 32];
    let slash = ProviderSlashRecord {
        tool_class: CLASS_BROWSER,
        amount: 500,
        reason_code: 2,
        evidence_hash: evidence,
        challenger: OTHER,
    };
    let err = st
        .apply_transaction(&native_tx(
            OTHER,
            0,
            NativeCall::WalletSlashProviderV1 {
                provider_id: PROVIDER,
                slash: slash.clone(),
            },
        ))
        .unwrap_err();
    assert_eq!(err, ExecError::MissingSlashingEvidence);

    st.apply_transaction(&native_tx(
        OTHER,
        0,
        NativeCall::CommitSlashingEvidence {
            evidence_hash: evidence,
        },
    ))
    .unwrap();
    st.apply_transaction(&native_tx(
        OTHER,
        1,
        NativeCall::WalletSlashProviderV1 {
            provider_id: PROVIDER,
            slash,
        },
    ))
    .unwrap();
    let stake = st
        .wallet_provider_stakes
        .get(&(PROVIDER, CLASS_BROWSER))
        .unwrap();
    assert_eq!(stake.total, 500);
    assert_eq!(stake.available, 0);
    assert_eq!(stake.pending_unstake, 500);
    assert_eq!(
        st.wallet_provider_unstake_requests.get(&1).unwrap().amount,
        500
    );
    assert_eq!(st.protocol_burned_wei, 500);
    assert_eq!(st.wallet_provider_slashes[0].burned_amount, 500);
}

#[test]
fn provider_deregister_requires_empty_stake_and_returns_bond() {
    let mut st = state();
    st.apply_transaction(&native_tx(
        OWNER,
        0,
        NativeCall::WalletRegisterProviderV1 {
            registration: registration(50),
        },
    ))
    .unwrap();
    st.apply_transaction(&native_tx(
        OWNER,
        1,
        NativeCall::WalletDeregisterProviderV1 {
            provider_id: PROVIDER,
        },
    ))
    .unwrap();
    assert!(!st.wallet_providers.contains_key(&PROVIDER));
    assert_eq!(st.accounts.get(&OWNER).unwrap().balance, 10_000);
}
