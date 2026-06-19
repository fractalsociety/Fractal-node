//! PRD §12.2 block rewards + §12.4 unbonding hooks run once per committed block (`execute_and_build_block`).

use crate::address::Address;
use crate::consensus_stake::distribute_fingerprint_block_reward;
use crate::error::ExecError;
use crate::state::{ConsensusUnbondEntry, State};

/// Inputs for [`finalize_block_hooks`] (deterministic; uses `block_timestamp_ms`, not wall clock).
#[derive(Clone, Debug)]
pub struct BlockFinalizeContext<'a> {
    pub block_timestamp_ms: u64,
    pub unbonding_period_ms: u64,
    pub proposer: [u8; 32],
    pub parent_qc_signer_indices: &'a [u32],
    pub validator_fingerprints: &'a [[u8; 32]],
    pub treasury: Address,
    /// PRD §12.2 fixed subsidy per block (testnet: debited from `treasury`, credited into consensus stake).
    pub block_reward_wei: u128,
    /// When `State.chain_economics.evm_base_fee_burn`, destroy `base_fee_per_gas * evm_gas_used` (mainnet).
    pub base_fee_per_gas: u128,
    pub evm_gas_used: u64,
}

/// Per-validator bonded weight for stake-weighted QC (`docs/prd.md` §12.2).
#[must_use]
pub fn validator_stake_weights(state: &State, fingerprints: &[[u8; 32]]) -> Vec<u128> {
    fingerprints
        .iter()
        .map(|fp| state.consensus_stake_total_for_fingerprint(fp))
        .collect()
}

pub use fractal_bft_wire::quorum_stake_threshold;

/// Pay matured consensus unbondings, anchor new zeros to `block_timestamp + period`, then PRD §12.2 rewards.
pub fn finalize_block_hooks(
    state: &mut State,
    ctx: &BlockFinalizeContext<'_>,
) -> Result<(), ExecError> {
    let now = ctx.block_timestamp_ms;
    let period = if ctx.unbonding_period_ms > 0 {
        ctx.unbonding_period_ms
    } else {
        state.chain_economics.unbonding_period_ms
    };

    if state.chain_economics.evm_base_fee_burn && ctx.evm_gas_used > 0 && ctx.base_fee_per_gas > 0 {
        let burn = ctx
            .base_fee_per_gas
            .saturating_mul(u128::from(ctx.evm_gas_used));
        if burn > 0 {
            state.protocol_burned_wei = state.protocol_burned_wei.saturating_add(burn);
            if let Some(treasury) = state.accounts.get_mut(&ctx.treasury) {
                treasury.balance = treasury.balance.saturating_sub(burn.min(treasury.balance));
            }
        }
    }

    // 1) Pay matured entries (release_ms > 0 and due).
    let mut kept: Vec<ConsensusUnbondEntry> = Vec::new();
    for e in std::mem::take(&mut state.consensus_unbonding) {
        if e.release_ms != 0 && e.release_ms <= now {
            let acc = state.accounts.entry(e.owner).or_insert(crate::Account {
                nonce: 0,
                balance: 0,
            });
            acc.balance = acc.balance.saturating_add(e.amount);
        } else {
            kept.push(e);
        }
    }
    state.consensus_unbonding = kept;

    // 2) Anchor freshly requested unbonds (release_ms == 0).
    for e in &mut state.consensus_unbonding {
        if e.release_ms == 0 {
            e.release_ms = now.saturating_add(period);
        }
    }

    if ctx.block_reward_wei == 0 {
        return Ok(());
    }

    // 3) Block reward: debit treasury, split across proposer + parent-QC signers by effective stake.
    let treasury = ctx.treasury;
    {
        let bal = state
            .accounts
            .get(&treasury)
            .map(|a| a.balance)
            .unwrap_or(0);
        if bal < ctx.block_reward_wei {
            return Err(ExecError::InsufficientBalance);
        }
        state
            .accounts
            .get_mut(&treasury)
            .ok_or(ExecError::InsufficientBalance)?
            .balance -= ctx.block_reward_wei;
    }

    let mut idxs: std::collections::BTreeSet<u32> =
        ctx.parent_qc_signer_indices.iter().copied().collect();
    if let Some(pi) = ctx
        .validator_fingerprints
        .iter()
        .position(|fp| fp == &ctx.proposer)
    {
        idxs.insert(pi as u32);
    }
    if idxs.is_empty() {
        if let Some(fp) = ctx.validator_fingerprints.first() {
            distribute_fingerprint_block_reward(state, *fp, ctx.block_reward_wei);
        }
        return Ok(());
    }

    let mut eff: Vec<(u32, u128)> = Vec::new();
    let mut total_eff: u128 = 0;
    for i in &idxs {
        let wi = ctx
            .validator_fingerprints
            .get(*i as usize)
            .and_then(|fp| state.consensus_stakes.get(fp).copied())
            .unwrap_or(0)
            .max(1);
        eff.push((*i, wi));
        total_eff = total_eff.saturating_add(wi);
    }

    let mut allocated: u128 = 0;
    let total_reward = ctx.block_reward_wei;
    for (k, &(i, wi)) in eff.iter().enumerate() {
        let fp = *ctx
            .validator_fingerprints
            .get(i as usize)
            .ok_or(ExecError::InvalidShape)?;
        let share = if k + 1 == eff.len() {
            total_reward.saturating_sub(allocated)
        } else {
            total_reward.saturating_mul(wi) / total_eff.max(1)
        };
        allocated = allocated.saturating_add(share);
        distribute_fingerprint_block_reward(state, fp, share);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_treasury_returns_error_instead_of_panicking() {
        let mut state = State::default();
        let ctx = BlockFinalizeContext {
            block_timestamp_ms: 1,
            unbonding_period_ms: 1,
            proposer: [1u8; 32],
            parent_qc_signer_indices: &[],
            validator_fingerprints: &[[1u8; 32]],
            treasury: [9u8; 20],
            block_reward_wei: 1,
            base_fee_per_gas: 0,
            evm_gas_used: 0,
        };

        let err = finalize_block_hooks(&mut state, &ctx).unwrap_err();
        assert_eq!(err, ExecError::InsufficientBalance);
    }
}
