//! PRD §12 / M7: fingerprint-keyed consensus stake + governance slash (native VM).

use borsh::to_vec;
use fractal_bft_wire::{ConsensusMisbehaviorEvidenceV1, ValidatorSet};
use fractal_consensus::{Vote, VoteSignBody};
use fractal_core::{
    permissionless_validator_entries, Account, ExecError, NativeCall, State, Transaction, TxBody,
    VmKind, HARDHAT_DEFAULT_SIGNER_0,
};

fn on_chain_validator_index(st: &State, fingerprint: [u8; 32]) -> u32 {
    permissionless_validator_entries(st)
        .iter()
        .position(|(fp, _)| *fp == fingerprint)
        .expect("registered validator") as u32
}
fn native_tx(signer: fractal_core::Address, nonce: u64, call: NativeCall) -> Transaction {
    Transaction {
        signer,
        nonce,
        vm: VmKind::Native,
        body: TxBody::Native(call),
    }
}

#[test]
fn deposit_and_withdraw_consensus_stake_round_trip() {
    let mut st = State::default();
    st.accounts.insert(
        HARDHAT_DEFAULT_SIGNER_0,
        Account {
            nonce: 0,
            balance: 1_000,
        },
    );
    let fp = [0xabu8; 32];
    assert_eq!(st.consensus_stake_total_for_fingerprint(&fp), 0);

    st.apply_transaction(&native_tx(
        HARDHAT_DEFAULT_SIGNER_0,
        0,
        NativeCall::DepositConsensusStake {
            validator_fingerprint: fp,
            amount: 400,
        },
    ))
    .unwrap();
    assert_eq!(st.consensus_stake_total_for_fingerprint(&fp), 400);
    assert_eq!(
        st.accounts.get(&HARDHAT_DEFAULT_SIGNER_0).unwrap().balance,
        600
    );

    st.apply_transaction(&native_tx(
        HARDHAT_DEFAULT_SIGNER_0,
        1,
        NativeCall::WithdrawConsensusStake {
            validator_fingerprint: fp,
            amount: 100,
        },
    ))
    .unwrap();
    assert_eq!(st.consensus_stake_total_for_fingerprint(&fp), 300);
    assert_eq!(
        st.accounts.get(&HARDHAT_DEFAULT_SIGNER_0).unwrap().balance,
        600,
        "withdraw queues unbonding; funds are not liquid until release"
    );
    assert_eq!(st.consensus_unbonding.len(), 1);
    assert_eq!(st.consensus_unbonding[0].amount, 100);
    assert_eq!(st.consensus_unbonding[0].release_ms, 0);

    use fractal_core::{finalize_block_hooks, BlockFinalizeContext};
    let period = 1000u64;
    let ctx = BlockFinalizeContext {
        block_timestamp_ms: 100,
        unbonding_period_ms: period,
        proposer: fp,
        parent_qc_signer_indices: &[],
        validator_fingerprints: std::slice::from_ref(&fp),
        treasury: fractal_core::DEVNET_FAUCET_TREASURY,
        block_reward_wei: 0,
        base_fee_per_gas: 0,
        evm_gas_used: 0,
    };
    finalize_block_hooks(&mut st, &ctx).unwrap();
    assert_eq!(st.consensus_unbonding[0].release_ms, 100 + period);

    let ctx2 = BlockFinalizeContext {
        block_timestamp_ms: 100 + period,
        unbonding_period_ms: period,
        proposer: fp,
        parent_qc_signer_indices: &[],
        validator_fingerprints: std::slice::from_ref(&fp),
        treasury: fractal_core::DEVNET_FAUCET_TREASURY,
        block_reward_wei: 0,
        base_fee_per_gas: 0,
        evm_gas_used: 0,
    };
    finalize_block_hooks(&mut st, &ctx2).unwrap();
    assert!(st.consensus_unbonding.is_empty());
    assert_eq!(
        st.accounts.get(&HARDHAT_DEFAULT_SIGNER_0).unwrap().balance,
        700
    );
}

#[test]
fn deposit_zero_rejected() {
    let mut st = State::default();
    st.accounts.insert(
        HARDHAT_DEFAULT_SIGNER_0,
        Account {
            nonce: 0,
            balance: 100,
        },
    );
    let fp = [1u8; 32];
    let err = st
        .apply_transaction(&native_tx(
            HARDHAT_DEFAULT_SIGNER_0,
            0,
            NativeCall::DepositConsensusStake {
                validator_fingerprint: fp,
                amount: 0,
            },
        ))
        .unwrap_err();
    assert_eq!(err, ExecError::InvalidShape);
}

#[test]
fn slash_consensus_stake_burns_totals_and_shares() {
    let mut st = State::default();
    st.accounts.insert(
        HARDHAT_DEFAULT_SIGNER_0,
        Account {
            nonce: 0,
            balance: 500,
        },
    );
    let fp = [0xceu8; 32];
    st.apply_transaction(&native_tx(
        HARDHAT_DEFAULT_SIGNER_0,
        0,
        NativeCall::DepositConsensusStake {
            validator_fingerprint: fp,
            amount: 500,
        },
    ))
    .unwrap();

    let ev = [0x11u8; 32];
    st.apply_transaction(&native_tx(
        HARDHAT_DEFAULT_SIGNER_0,
        1,
        NativeCall::CommitSlashingEvidence {
            evidence_hash: ev,
        },
    ))
    .unwrap();

    st.apply_transaction(&native_tx(
        HARDHAT_DEFAULT_SIGNER_0,
        2,
        NativeCall::SlashConsensusStake {
            validator_fingerprint: fp,
            evidence_hash: ev,
        },
    ))
    .unwrap();

    assert_eq!(st.consensus_stake_total_for_fingerprint(&fp), 0);
    assert!(st.consensus_stake_shares.is_empty());
}

#[test]
fn slash_consensus_stake_without_committed_evidence_fails() {
    let mut st = State::default();
    st.accounts.insert(
        HARDHAT_DEFAULT_SIGNER_0,
        Account {
            nonce: 0,
            balance: 500,
        },
    );
    let fp = [0xdeu8; 32];
    st.apply_transaction(&native_tx(
        HARDHAT_DEFAULT_SIGNER_0,
        0,
        NativeCall::DepositConsensusStake {
            validator_fingerprint: fp,
            amount: 100,
        },
    ))
    .unwrap();
    let err = st
        .apply_transaction(&native_tx(
            HARDHAT_DEFAULT_SIGNER_0,
            1,
            NativeCall::SlashConsensusStake {
                validator_fingerprint: fp,
                evidence_hash: [0x99; 32],
            },
        ))
        .unwrap_err();
    assert_eq!(err, ExecError::MissingSlashingEvidence);
}

#[test]
fn withdraw_more_than_share_fails() {
    let mut st = State::default();
    st.accounts.insert(
        HARDHAT_DEFAULT_SIGNER_0,
        Account {
            nonce: 0,
            balance: 100,
        },
    );
    let fp = [2u8; 32];
    st.apply_transaction(&native_tx(
        HARDHAT_DEFAULT_SIGNER_0,
        0,
        NativeCall::DepositConsensusStake {
            validator_fingerprint: fp,
            amount: 50,
        },
    ))
    .unwrap();
    let err = st
        .apply_transaction(&native_tx(
            HARDHAT_DEFAULT_SIGNER_0,
            1,
            NativeCall::WithdrawConsensusStake {
                validator_fingerprint: fp,
                amount: 51,
            },
        ))
        .unwrap_err();
    assert_eq!(err, ExecError::InsufficientBalance);
}

fn enable_permissionless_bft7(st: &mut State) {
    st.chain_economics.permissionless_validator_entry = true;
    st.chain_economics.min_validator_stake_wei = 1;
    let set = ValidatorSet::phase2_bft7_fixture();
    for entry in set.entries() {
        st.apply_transaction(&native_tx(
            HARDHAT_DEFAULT_SIGNER_0,
            st.accounts.get(&HARDHAT_DEFAULT_SIGNER_0).unwrap().nonce,
            NativeCall::DepositConsensusStake {
                validator_fingerprint: entry.fingerprint,
                amount: 100,
            },
        ))
        .unwrap();
        st.apply_transaction(&native_tx(
            HARDHAT_DEFAULT_SIGNER_0,
            st.accounts.get(&HARDHAT_DEFAULT_SIGNER_0).unwrap().nonce,
            NativeCall::RegisterValidator {
                validator_fingerprint: entry.fingerprint,
                bls_pubkey: entry.bls_pubkey.0,
            },
        ))
        .unwrap();
    }
}

#[test]
fn slash_consensus_stake_verified_double_vote() {
    let mut st = State::default();
    st.accounts.insert(
        HARDHAT_DEFAULT_SIGNER_0,
        Account {
            nonce: 0,
            balance: 50_000,
        },
    );
    enable_permissionless_bft7(&mut st);
    let set = ValidatorSet::phase2_bft7_fixture();
    let fixture_idx = 3usize;
    let fp = set.entry(fixture_idx).unwrap().fingerprint;
    let sk = set.dev_bls_secret(fixture_idx).unwrap();
    let idx = on_chain_validator_index(&st, fp);
    let vote_a = Vote::sign(
        VoteSignBody {
            view: 2,
            height: 4,
            header_hash: [0x01; 32],
        },
        idx,
        &sk,
    );
    let vote_b = Vote::sign(
        VoteSignBody {
            view: 2,
            height: 4,
            header_hash: [0x02; 32],
        },
        idx,
        &sk,
    );
    let evidence = ConsensusMisbehaviorEvidenceV1::DoubleVote {
        offender_fingerprint: fp,
        vote_a,
        vote_b,
    };
    let evidence_borsh = to_vec(&evidence).unwrap();
    st.apply_transaction(&native_tx(
        HARDHAT_DEFAULT_SIGNER_0,
        st.accounts.get(&HARDHAT_DEFAULT_SIGNER_0).unwrap().nonce,
        NativeCall::SlashConsensusStakeVerified {
            validator_fingerprint: fp,
            evidence_borsh,
        },
    ))
    .unwrap();
    assert_eq!(st.consensus_stake_total_for_fingerprint(&fp), 0);
    assert!(
        !st
            .consensus_stake_shares
            .keys()
            .any(|(_, f)| *f == fp)
    );
}

#[test]
fn slash_consensus_stake_verified_rejects_replay() {
    let mut st = State::default();
    st.accounts.insert(
        HARDHAT_DEFAULT_SIGNER_0,
        Account {
            nonce: 0,
            balance: 50_000,
        },
    );
    enable_permissionless_bft7(&mut st);
    let set = ValidatorSet::phase2_bft7_fixture();
    let fixture_idx = 1usize;
    let fp = set.entry(fixture_idx).unwrap().fingerprint;
    let sk = set.dev_bls_secret(fixture_idx).unwrap();
    let idx = on_chain_validator_index(&st, fp);
    let evidence = ConsensusMisbehaviorEvidenceV1::DoubleVote {
        offender_fingerprint: fp,
        vote_a: Vote::sign(
            VoteSignBody {
                view: 1,
                height: 2,
                header_hash: [0x0a; 32],
            },
            idx,
            &sk,
        ),
        vote_b: Vote::sign(
            VoteSignBody {
                view: 1,
                height: 2,
                header_hash: [0x0b; 32],
            },
            idx,
            &sk,
        ),
    };
    let evidence_borsh = to_vec(&evidence).unwrap();
    let nonce = st.accounts.get(&HARDHAT_DEFAULT_SIGNER_0).unwrap().nonce;
    st.apply_transaction(&native_tx(
        HARDHAT_DEFAULT_SIGNER_0,
        nonce,
        NativeCall::SlashConsensusStakeVerified {
            validator_fingerprint: fp,
            evidence_borsh: evidence_borsh.clone(),
        },
    ))
    .unwrap();
    let err = st
        .apply_transaction(&native_tx(
            HARDHAT_DEFAULT_SIGNER_0,
            nonce + 1,
            NativeCall::SlashConsensusStakeVerified {
                validator_fingerprint: fp,
                evidence_borsh,
            },
        ))
        .unwrap_err();
    assert_eq!(err, ExecError::DuplicateMisbehaviorEvidence);
}
