//! PRD §12.3: `DELEGATE`, commission, reward compounding, `WITHDRAW_REWARDS`.

use fractal_core::{
    finalize_block_hooks, Account, BlockFinalizeContext, ExecError, NativeCall, State, Transaction,
    TxBody, VmKind, DEVNET_FAUCET_TREASURY, HARDHAT_DEFAULT_SIGNER_0, HARDHAT_DEFAULT_SIGNER_1,
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
fn delegate_bonds_to_consensus_stake_shares() {
    let mut st = State::default();
    st.accounts.insert(
        HARDHAT_DEFAULT_SIGNER_0,
        Account {
            nonce: 0,
            balance: 500,
        },
    );
    let fp = [0xabu8; 32];

    st.apply_transaction(&native_tx(
        HARDHAT_DEFAULT_SIGNER_0,
        0,
        NativeCall::Delegate {
            validator_fingerprint: fp,
            amount: 300,
        },
    ))
    .unwrap();

    assert_eq!(st.consensus_stake_total_for_fingerprint(&fp), 300);
    assert_eq!(
        st.consensus_stake_shares
            .get(&(HARDHAT_DEFAULT_SIGNER_0, fp))
            .copied(),
        Some(300)
    );
    assert_eq!(
        st.accounts.get(&HARDHAT_DEFAULT_SIGNER_0).unwrap().balance,
        200
    );
}

#[test]
fn operator_sets_commission_and_non_operator_rejected() {
    let mut st = State::default();
    st.accounts.insert(
        HARDHAT_DEFAULT_SIGNER_0,
        Account {
            nonce: 0,
            balance: 1_000,
        },
    );
    st.accounts.insert(
        HARDHAT_DEFAULT_SIGNER_1,
        Account {
            nonce: 0,
            balance: 1_000,
        },
    );
    let fp = [0xceu8; 32];

    st.apply_transaction(&native_tx(
        HARDHAT_DEFAULT_SIGNER_0,
        0,
        NativeCall::Delegate {
            validator_fingerprint: fp,
            amount: 600,
        },
    ))
    .unwrap();
    st.apply_transaction(&native_tx(
        HARDHAT_DEFAULT_SIGNER_1,
        0,
        NativeCall::Delegate {
            validator_fingerprint: fp,
            amount: 400,
        },
    ))
    .unwrap();

    st.apply_transaction(&native_tx(
        HARDHAT_DEFAULT_SIGNER_0,
        1,
        NativeCall::SetValidatorCommission {
            validator_fingerprint: fp,
            commission_bps: 500,
        },
    ))
    .unwrap();
    assert_eq!(st.consensus_commission_bps.get(&fp).copied(), Some(500));

    let err = st
        .apply_transaction(&native_tx(
            HARDHAT_DEFAULT_SIGNER_1,
            1,
            NativeCall::SetValidatorCommission {
                validator_fingerprint: fp,
                commission_bps: 100,
            },
        ))
        .unwrap_err();
    assert_eq!(err, ExecError::NotAuthorized);
}

#[test]
fn block_reward_commission_and_withdraw_rewards() {
    let mut st = State::default();
    st.accounts.insert(
        HARDHAT_DEFAULT_SIGNER_0,
        Account {
            nonce: 0,
            balance: 1_000,
        },
    );
    st.accounts.insert(
        HARDHAT_DEFAULT_SIGNER_1,
        Account {
            nonce: 0,
            balance: 500,
        },
    );
    st.accounts.insert(
        DEVNET_FAUCET_TREASURY,
        Account {
            nonce: 0,
            balance: 10_000,
        },
    );
    let fp = [0xabu8; 32];

    st.apply_transaction(&native_tx(
        HARDHAT_DEFAULT_SIGNER_0,
        0,
        NativeCall::Delegate {
            validator_fingerprint: fp,
            amount: 600,
        },
    ))
    .unwrap();
    st.apply_transaction(&native_tx(
        HARDHAT_DEFAULT_SIGNER_1,
        0,
        NativeCall::Delegate {
            validator_fingerprint: fp,
            amount: 400,
        },
    ))
    .unwrap();
    st.apply_transaction(&native_tx(
        HARDHAT_DEFAULT_SIGNER_0,
        1,
        NativeCall::SetValidatorCommission {
            validator_fingerprint: fp,
            commission_bps: 1_000,
        },
    ))
    .unwrap();

    let ctx = BlockFinalizeContext {
        block_timestamp_ms: 1,
        unbonding_period_ms: 1,
        proposer: fp,
        parent_qc_signer_indices: &[],
        validator_fingerprints: &[fp],
        treasury: DEVNET_FAUCET_TREASURY,
        block_reward_wei: 1_000,
        base_fee_per_gas: 0,
        evm_gas_used: 0,
    };
    finalize_block_hooks(&mut st, &ctx).unwrap();

    assert_eq!(
        st.consensus_reward_credits
            .get(&(HARDHAT_DEFAULT_SIGNER_0, fp))
            .copied(),
        Some(100)
    );
    assert!(st.consensus_stake_total_for_fingerprint(&fp) > 1000);

    st.apply_transaction(&native_tx(
        HARDHAT_DEFAULT_SIGNER_0,
        2,
        NativeCall::WithdrawRewards {
            validator_fingerprint: fp,
        },
    ))
    .unwrap();
    assert_eq!(
        st.accounts.get(&HARDHAT_DEFAULT_SIGNER_0).unwrap().balance,
        500,
        "400 liquid after delegate + 100 commission withdrawn"
    );
    assert!(!st.consensus_reward_credits.contains_key(&(HARDHAT_DEFAULT_SIGNER_0, fp)));
}
