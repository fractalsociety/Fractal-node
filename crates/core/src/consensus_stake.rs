//! PRD §12.2–12.3 consensus stake, delegation, commission, and reward distribution.

use crate::address::Address;
use crate::error::ExecError;
use crate::state::State;

/// Maximum validator commission (100%).
pub const MAX_COMMISSION_BPS: u16 = 10_000;

/// Bond `amount` from `signer` liquid balance into `validator_fingerprint` stake (shared by deposit + delegate).
pub fn deposit_consensus_stake(
    state: &mut State,
    signer: Address,
    validator_fingerprint: [u8; 32],
    amount: u128,
) -> Result<(), ExecError> {
    if amount == 0 {
        return Err(ExecError::InvalidShape);
    }
    {
        let acc = state
            .accounts
            .get(&signer)
            .ok_or(ExecError::UnknownSigner)?;
        if acc.balance < amount {
            return Err(ExecError::InsufficientBalance);
        }
    }
    state.accounts.get_mut(&signer).expect("signer").balance -= amount;
    *state
        .consensus_stakes
        .entry(validator_fingerprint)
        .or_insert(0) += amount;
    *state
        .consensus_stake_shares
        .entry((signer, validator_fingerprint))
        .or_insert(0) += amount;
    Ok(())
}

/// Address with the largest bonded share for `fp` (validator operator for commission settings).
#[must_use]
pub fn validator_operator_address(state: &State, fp: &[u8; 32]) -> Option<Address> {
    state
        .consensus_stake_shares
        .iter()
        .filter(|((_, fingerprint), _)| fingerprint == fp)
        .max_by(|a, b| a.1.cmp(b.1).then_with(|| a.0.0.cmp(&b.0.0)))
        .map(|((owner, _), _)| *owner)
}

#[must_use]
pub fn commission_bps_for(state: &State, fp: &[u8; 32]) -> u16 {
    state
        .consensus_commission_bps
        .get(fp)
        .copied()
        .unwrap_or(0)
        .min(MAX_COMMISSION_BPS)
}

/// Only the validator operator (largest shareholder) may set commission (`docs/prd.md` §12.3).
pub fn set_validator_commission(
    state: &mut State,
    signer: Address,
    validator_fingerprint: [u8; 32],
    commission_bps: u16,
) -> Result<(), ExecError> {
    if commission_bps > MAX_COMMISSION_BPS {
        return Err(ExecError::InvalidShape);
    }
    let operator =
        validator_operator_address(state, &validator_fingerprint).ok_or(ExecError::NotFound)?;
    if signer != operator {
        return Err(ExecError::NotAuthorized);
    }
    state
        .consensus_commission_bps
        .insert(validator_fingerprint, commission_bps);
    Ok(())
}

/// Pay out accrued delegation rewards for `(signer, fingerprint)` to liquid balance (`WITHDRAW_REWARDS`).
pub fn withdraw_consensus_rewards(
    state: &mut State,
    signer: Address,
    validator_fingerprint: [u8; 32],
) -> Result<(), ExecError> {
    let amount = state
        .consensus_reward_credits
        .remove(&(signer, validator_fingerprint))
        .unwrap_or(0);
    if amount > 0 {
        state
            .accounts
            .entry(signer)
            .or_insert(crate::Account {
                nonce: 0,
                balance: 0,
            })
            .balance = state
            .accounts
            .get(&signer)
            .expect("signer")
            .balance
            .saturating_add(amount);
    }
    Ok(())
}

fn shareholders_for_fingerprint(state: &State, fp: [u8; 32]) -> Vec<(Address, u128)> {
    state
        .consensus_stake_shares
        .iter()
        .filter(|((_, fingerprint), _)| *fingerprint == fp)
        .map(|((owner, _), share)| (*owner, *share))
        .collect()
}

/// Split a block reward for `fp`: commission → operator reward credits; remainder compounds into shares.
pub fn distribute_fingerprint_block_reward(state: &mut State, fp: [u8; 32], reward: u128) {
    if reward == 0 {
        return;
    }
    let total = state.consensus_stake_total_for_fingerprint(&fp);
    if total == 0 {
        *state.consensus_stakes.entry(fp).or_insert(0) += reward;
        return;
    }

    let bps = commission_bps_for(state, &fp);
    let commission = reward.saturating_mul(bps as u128) / u128::from(MAX_COMMISSION_BPS);
    let mut net = reward.saturating_sub(commission);

    if commission > 0 {
        if let Some(operator) = validator_operator_address(state, &fp) {
            *state
                .consensus_reward_credits
                .entry((operator, fp))
                .or_insert(0) += commission;
        } else {
            net = net.saturating_add(commission);
        }
    }

    if net == 0 {
        return;
    }

    let holders = shareholders_for_fingerprint(state, fp);
    if holders.is_empty() {
        *state.consensus_stakes.entry(fp).or_insert(0) += net;
        return;
    }

    let mut allocated: u128 = 0;
    for (k, (owner, share)) in holders.iter().enumerate() {
        let add = if k + 1 == holders.len() {
            net.saturating_sub(allocated)
        } else {
            net.saturating_mul(*share) / total
        };
        allocated = allocated.saturating_add(add);
        if add > 0 {
            *state
                .consensus_stake_shares
                .get_mut(&(*owner, fp))
                .expect("shareholder") += add;
        }
    }
    *state.consensus_stakes.entry(fp).or_insert(0) += net;
}

/// Move bonded principal from one validator fingerprint to another for the same owner (redelegation).
pub fn redelegate_consensus_stake(
    state: &mut State,
    owner: Address,
    from_fp: [u8; 32],
    to_fp: [u8; 32],
    amount: u128,
) -> Result<(), ExecError> {
    if amount == 0 || from_fp == to_fp {
        return Err(ExecError::InvalidShape);
    }
    let from_key = (owner, from_fp);
    let share = state
        .consensus_stake_shares
        .get_mut(&from_key)
        .ok_or(ExecError::NotFound)?;
    if *share < amount {
        return Err(ExecError::InsufficientBalance);
    }
    *share -= amount;
    if *share == 0 {
        state.consensus_stake_shares.remove(&from_key);
    }
    let from_tot = state
        .consensus_stakes
        .get_mut(&from_fp)
        .ok_or(ExecError::NotFound)?;
    *from_tot = from_tot
        .checked_sub(amount)
        .ok_or(ExecError::InsufficientBalance)?;
    if *from_tot == 0 {
        state.consensus_stakes.remove(&from_fp);
    }
    *state.consensus_stakes.entry(to_fp).or_insert(0) += amount;
    *state
        .consensus_stake_shares
        .entry((owner, to_fp))
        .or_insert(0) += amount;
    Ok(())
}

/// PRD §12.3 / mainnet: enroll a validator fingerprint after meeting the economics minimum bond.
pub fn register_validator(
    state: &mut State,
    operator: Address,
    validator_fingerprint: [u8; 32],
    bls_pubkey: [u8; 48],
) -> Result<(), ExecError> {
    if !state.chain_economics.permissionless_validator_entry {
        return Err(ExecError::PermissionlessEntryDisabled);
    }
    if state
        .validator_registry
        .contains_key(&validator_fingerprint)
    {
        return Err(ExecError::ValidatorAlreadyRegistered);
    }
    let bonded = state.consensus_stake_total_for_fingerprint(&validator_fingerprint);
    if bonded < state.chain_economics.min_validator_stake_wei {
        return Err(ExecError::BelowMinValidatorStake);
    }
    state.validator_registry.insert(
        validator_fingerprint,
        crate::chain_economics::ValidatorRegistryEntry {
            operator,
            bls_pubkey,
        },
    );
    Ok(())
}

/// Burn all bonded stake, shares, unbonding, and registry metadata for a slashed validator.
pub fn slash_consensus_stake(state: &mut State, validator_fingerprint: [u8; 32]) {
    state.consensus_stakes.remove(&validator_fingerprint);
    state
        .consensus_stake_shares
        .retain(|(_, fp), _| fp != &validator_fingerprint);
    state
        .consensus_unbonding
        .retain(|e| e.validator_fingerprint != validator_fingerprint);
    clear_validator_delegation_metadata(state, validator_fingerprint);
}

/// Clear delegation metadata for a slashed validator fingerprint.
pub fn clear_validator_delegation_metadata(state: &mut State, validator_fingerprint: [u8; 32]) {
    state
        .consensus_commission_bps
        .remove(&validator_fingerprint);
    state
        .consensus_reward_credits
        .retain(|(_, fp), _| *fp != validator_fingerprint);
    state.validator_registry.remove(&validator_fingerprint);
}

/// Registry rows that still meet the bonded minimum (for dynamic validator sets).
pub fn permissionless_validator_entries(state: &State) -> Vec<([u8; 32], [u8; 48])> {
    if !state.chain_economics.permissionless_validator_entry {
        return Vec::new();
    }
    let min = state.chain_economics.min_validator_stake_wei;
    let mut out: Vec<([u8; 32], [u8; 48])> = state
        .validator_registry
        .iter()
        .filter(|(fp, _)| state.consensus_stake_total_for_fingerprint(fp) >= min)
        .map(|(fp, row)| (*fp, row.bls_pubkey))
        .collect();
    out.sort_by_key(|(fp, _)| *fp);
    out
}

/// Fingerprints with an on-chain registry entry and bonded stake ≥ economics minimum.
pub fn active_permissionless_validator_fingerprints(state: &State) -> Vec<[u8; 32]> {
    if !state.chain_economics.permissionless_validator_entry {
        return Vec::new();
    }
    let min = state.chain_economics.min_validator_stake_wei;
    let mut out: Vec<[u8; 32]> = state
        .validator_registry
        .keys()
        .filter(|fp| state.consensus_stake_total_for_fingerprint(fp) >= min)
        .copied()
        .collect();
    out.sort();
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HARDHAT_DEFAULT_SIGNER_0;
    use crate::state::Account;

    const FP: [u8; 32] = [0xabu8; 32];

    #[test]
    fn reward_split_commission_and_compound() {
        let mut st = State::default();
        let delegator = HARDHAT_DEFAULT_SIGNER_0;
        let operator = [0x22u8; 20];
        st.accounts.insert(
            delegator,
            Account {
                nonce: 0,
                balance: 0,
            },
        );
        st.accounts.insert(
            operator,
            Account {
                nonce: 0,
                balance: 0,
            },
        );
        st.consensus_stake_shares.insert((operator, FP), 600);
        st.consensus_stake_shares.insert((delegator, FP), 400);
        st.consensus_stakes.insert(FP, 1000);
        st.consensus_commission_bps.insert(FP, 1000); // 10%

        distribute_fingerprint_block_reward(&mut st, FP, 1000);

        assert_eq!(
            st.consensus_reward_credits.get(&(operator, FP)).copied(),
            Some(100)
        );
        assert_eq!(st.consensus_stake_total_for_fingerprint(&FP), 1900);
        assert_eq!(
            st.consensus_stake_shares.get(&(delegator, FP)).copied(),
            Some(760)
        );
        assert_eq!(
            st.consensus_stake_shares.get(&(operator, FP)).copied(),
            Some(1140)
        );
    }
}
