//! PRD §12 mainnet permissionless economics: registry, redelegate, burn, governance params.

use fractal_core::{
    finalize_block_hooks, Account, BlockFinalizeContext, ChainEconomicsParams, ExecError,
    NativeCall, State, Transaction, TxBody, VmKind, DEVNET_FAUCET_TREASURY,
    HARDHAT_DEFAULT_SIGNER_0, MAINNET_MIN_VALIDATOR_STAKE_WEI, MAINNET_UNBONDING_PERIOD_MS,
    WEI_PER_FRAC,
};

fn native_tx(signer: fractal_core::Address, nonce: u64, call: NativeCall) -> Transaction {
    Transaction {
        signer,
        nonce,
        vm: VmKind::Native,
        body: TxBody::Native(call),
    }
}

#[test]
fn mainnet_profile_defaults() {
    let p = ChainEconomicsParams::mainnet();
    assert_eq!(p.min_validator_stake_wei, MAINNET_MIN_VALIDATOR_STAKE_WEI);
    assert_eq!(p.unbonding_period_ms, MAINNET_UNBONDING_PERIOD_MS);
    assert!(p.permissionless_validator_entry);
    assert!(p.evm_base_fee_burn);
}

#[test]
fn register_validator_requires_min_stake_and_permissionless_flag() {
    let mut st = State::default();
    st.chain_economics = ChainEconomicsParams::mainnet();
    st.accounts.insert(
        HARDHAT_DEFAULT_SIGNER_0,
        Account {
            nonce: 0,
            balance: MAINNET_MIN_VALIDATOR_STAKE_WEI + WEI_PER_FRAC,
        },
    );
    let fp = [0xabu8; 32];
    let pk = [0x11u8; 48];

    let err = st
        .apply_transaction(&native_tx(
            HARDHAT_DEFAULT_SIGNER_0,
            0,
            NativeCall::RegisterValidator {
                validator_fingerprint: fp,
                bls_pubkey: pk,
            },
        ))
        .unwrap_err();
    assert_eq!(err, ExecError::BelowMinValidatorStake);

    st.apply_transaction(&native_tx(
        HARDHAT_DEFAULT_SIGNER_0,
        0,
        NativeCall::Delegate {
            validator_fingerprint: fp,
            amount: MAINNET_MIN_VALIDATOR_STAKE_WEI,
        },
    ))
    .unwrap();

    st.apply_transaction(&native_tx(
        HARDHAT_DEFAULT_SIGNER_0,
        1,
        NativeCall::RegisterValidator {
            validator_fingerprint: fp,
            bls_pubkey: pk,
        },
    ))
    .unwrap();
    assert!(st.validator_registry.contains_key(&fp));
}

#[test]
fn redelegate_moves_shares_between_fingerprints() {
    let mut st = State::default();
    st.accounts.insert(
        HARDHAT_DEFAULT_SIGNER_0,
        Account {
            nonce: 0,
            balance: 2 * WEI_PER_FRAC,
        },
    );
    let fp_a = [1u8; 32];
    let fp_b = [2u8; 32];

    st.apply_transaction(&native_tx(
        HARDHAT_DEFAULT_SIGNER_0,
        0,
        NativeCall::Delegate {
            validator_fingerprint: fp_a,
            amount: WEI_PER_FRAC,
        },
    ))
    .unwrap();
    st.apply_transaction(&native_tx(
        HARDHAT_DEFAULT_SIGNER_0,
        1,
        NativeCall::Redelegate {
            from_validator_fingerprint: fp_a,
            to_validator_fingerprint: fp_b,
            amount: 500_000_000_000_000_000,
        },
    ))
    .unwrap();

    assert_eq!(st.consensus_stake_total_for_fingerprint(&fp_a), 500_000_000_000_000_000);
    assert_eq!(st.consensus_stake_total_for_fingerprint(&fp_b), 500_000_000_000_000_000);
}

#[test]
fn evm_base_fee_burn_debits_treasury() {
    let mut st = State::default();
    st.chain_economics = ChainEconomicsParams::mainnet();
    st.accounts.insert(
        DEVNET_FAUCET_TREASURY,
        Account {
            nonce: 0,
            balance: 10_000,
        },
    );
    let ctx = BlockFinalizeContext {
        block_timestamp_ms: 1,
        unbonding_period_ms: MAINNET_UNBONDING_PERIOD_MS,
        proposer: [0u8; 32],
        parent_qc_signer_indices: &[],
        validator_fingerprints: &[],
        treasury: DEVNET_FAUCET_TREASURY,
        block_reward_wei: 0,
        base_fee_per_gas: 10,
        evm_gas_used: 100,
    };
    finalize_block_hooks(&mut st, &ctx).unwrap();
    assert_eq!(st.protocol_burned_wei, 1000);
    assert_eq!(
        st.accounts.get(&DEVNET_FAUCET_TREASURY).unwrap().balance,
        9000
    );
}
