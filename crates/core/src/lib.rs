//! Pure execution state machine: native M3 subtries + canonical `state_root`.
//!
//! Full Merkle Patricia Trie lives in `fractal-storage` later; here `state_root` is
//! `keccak256(borsh(State))` with sorted `BTreeMap` fields for deterministic iteration.

mod address;
mod block_finalize;
mod chain_economics;
mod consensus_misbehavior;
mod consensus_stake;
mod devnet_accounts;
mod error;
mod evm_engine;
pub mod merkle;
mod native_gas;
mod native_types;
mod state;
mod tx;

#[cfg(feature = "wallet")]
pub mod wallet_anchor;
#[cfg(feature = "wallet")]
pub mod wallet_batch_settle;
pub mod wallet_native;
#[cfg(feature = "wallet")]
pub mod wallet_provider;

pub use address::{Address, create_contract_address};
pub use block_finalize::{
    BlockFinalizeContext, finalize_block_hooks, validator_stake_weights,
};
pub use consensus_misbehavior::{
    misbehavior_evidence_hash, validator_set_for_slashing, verify_slashing_evidence_borsh,
};
pub use fractal_bft_wire::quorum_stake_threshold;
pub use chain_economics::{
    ChainEconomicsParams, MAINNET_MIN_VALIDATOR_STAKE_WEI, MAINNET_UNBONDING_PERIOD_MS,
    TESTNET_MIN_VALIDATOR_STAKE_WEI, TESTNET_UNBONDING_PERIOD_MS, ValidatorRegistryEntry,
    WEI_PER_FRAC,
};
pub use consensus_stake::{
    MAX_COMMISSION_BPS, active_permissionless_validator_fingerprints, commission_bps_for,
    deposit_consensus_stake, distribute_fingerprint_block_reward, permissionless_validator_entries,
    redelegate_consensus_stake, register_validator, set_validator_commission,
    validator_operator_address, withdraw_consensus_rewards,
};
pub use devnet_accounts::{
    DEVNET_FAUCET_TREASURY, HARDHAT_DEFAULT_SIGNER_0, HARDHAT_DEFAULT_SIGNER_1,
};
pub use error::ExecError;
pub use evm_engine::{EvmCallOutcome, EvmEngine};
pub use merkle::{merkle_proof, merkle_root, verify_merkle_proof};
pub use native_gas::{
    EVM_CALL_BASE_GAS, PER_BYTE, TRANSFER_GAS, intrinsic_gas, is_native_precompile_address,
    native_opcode_from_precompile_address, tx_gas_limit,
};
pub use native_types::*;
pub use state::EvmLog;
pub use state::{Account, State};
pub use tx::{NativeCall, Transaction, TxBody, VmKind};
#[cfg(feature = "wallet")]
pub use wallet_native::{
    ancestor_chain_for_mint, is_capability_revoked, mint_capability, mint_revocation_proof_bytes,
    revoke_capability, sync_wallet_revocation_merkle_root, verify_mint_revocation_proof,
};
pub use wallet_native::{close_budget_account, create_budget_account, fund_budget_account};
#[cfg(feature = "wallet")]
pub use wallet_provider::{
    deregister_provider, finalize_unstake, register_provider, request_unstake, slash_provider,
    stake_for_class, update_provider,
};

use fractal_crypto::hash::{commit_borsh, keccak256};

/// Deterministic state commitment (EVM-style root uses keccak over canonical bytes).
pub fn state_root(state: &State) -> Result<fractal_crypto::Hash256, std::io::Error> {
    commit_borsh(state)
}

/// Apply an ordered list of transactions. Returns total intrinsic gas used.
pub fn apply_block(state: &mut State, txs: &[Transaction]) -> Result<u64, ExecError> {
    let mut sum = 0u64;
    for tx in txs {
        let g = intrinsic_gas(tx)?;
        sum = sum.checked_add(g).ok_or(ExecError::GasOverflow)?;
        state.apply_transaction(tx)?;
    }
    Ok(sum)
}

/// Gas charged for `tx` after a successful `apply_transaction_with_evm` (revm `tx_gas_used` for `EvmCall`, else intrinsic).
pub fn gas_used_after_apply(state: &State, tx: &Transaction) -> Result<u64, ExecError> {
    let raw = borsh::to_vec(tx).map_err(|_| ExecError::InvalidShape)?;
    let h = keccak256(&raw);
    if let Some(g) = state.evm_tx_gas_used.get(&h) {
        return Ok(*g);
    }
    intrinsic_gas(tx)
}

/// Apply a block that may contain EVM calls, using the provided `EvmEngine`.
///
/// Returns the sum of per-transaction gas used (measured EVM gas for `EvmCall`, intrinsic for other kinds).
pub fn apply_block_with_evm(
    state: &mut State,
    txs: &[Transaction],
    evm: &mut dyn EvmEngine,
) -> Result<u64, ExecError> {
    let mut sum = 0u64;
    for tx in txs {
        state.apply_transaction_with_evm(tx, evm)?;
        let g = gas_used_after_apply(state, tx)?;
        sum = sum.checked_add(g).ok_or(ExecError::GasOverflow)?;
    }
    Ok(sum)
}

#[cfg(all(test, feature = "wallet"))]
mod wallet_anchor_tests {
    use fractal_wallet::TaskReceipt;

    use super::wallet_anchor;

    #[test]
    fn task_receipt_commitment_is_deterministic() {
        let tr = TaskReceipt {
            task_id: 1,
            agent_session: [2u8; 32],
            artifact_commitment: [3u8; 32],
            artifact_pointer: "da://x".into(),
            tool_receipt_root: [4u8; 32],
            total_tool_cost: 100,
        };
        let a = wallet_anchor::task_receipt_commitment(&tr).unwrap();
        let b = wallet_anchor::task_receipt_commitment(&tr).unwrap();
        assert_eq!(a, b);
    }
}
