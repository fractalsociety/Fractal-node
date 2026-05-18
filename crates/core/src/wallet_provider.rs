//! On-chain wallet provider staking and slashing (`docs/wallet.md` §14.4).

use crate::address::Address;
use crate::error::ExecError;
use crate::native_types::{
    OnChainProviderRow, OnChainProviderSlashRecord, OnChainProviderUnstakeRequest,
    ProviderRegistration, ProviderSlashRecord, WALLET_PROVIDER_UNSTAKE_DELAY_MS,
};
use crate::state::{Account, State};

fn debit(state: &mut State, owner: Address, amount: u128) -> Result<(), ExecError> {
    if amount == 0 {
        return Ok(());
    }
    let acc = state
        .accounts
        .get_mut(&owner)
        .ok_or(ExecError::UnknownSigner)?;
    if acc.balance < amount {
        return Err(ExecError::InsufficientBalance);
    }
    acc.balance -= amount;
    Ok(())
}

fn credit(state: &mut State, owner: Address, amount: u128) {
    if amount == 0 {
        return;
    }
    state
        .accounts
        .entry(owner)
        .or_insert(Account {
            nonce: 0,
            balance: 0,
        })
        .balance += amount;
}

fn require_provider_owner(
    state: &State,
    signer: Address,
    provider_id: &[u8; 32],
) -> Result<Address, ExecError> {
    let row = state
        .wallet_providers
        .get(provider_id)
        .ok_or(ExecError::WalletProviderNotFound)?;
    if row.registration.owner != signer {
        return Err(ExecError::WalletProviderNotOwned);
    }
    Ok(row.registration.owner)
}

/// Register provider identity and escrow its registration bond.
pub fn register_provider(
    state: &mut State,
    signer: Address,
    registration: ProviderRegistration,
) -> Result<(), ExecError> {
    if registration.owner != signer {
        return Err(ExecError::WalletProviderNotOwned);
    }
    if state
        .wallet_providers
        .contains_key(&registration.provider_id)
    {
        return Err(ExecError::WalletProviderAlreadyRegistered);
    }
    debit(state, signer, registration.registration_bond)?;
    let now = state.execution_timestamp_ms;
    state.wallet_providers.insert(
        registration.provider_id,
        OnChainProviderRow {
            registration,
            registered_at_ms: now,
            updated_at_ms: now,
            active: true,
        },
    );
    Ok(())
}

/// Bond provider stake for one tool class.
pub fn stake_for_class(
    state: &mut State,
    signer: Address,
    provider_id: [u8; 32],
    tool_class: u8,
    amount: u128,
) -> Result<(), ExecError> {
    require_provider_owner(state, signer, &provider_id)?;
    debit(state, signer, amount)?;
    let row = state
        .wallet_provider_stakes
        .entry((provider_id, tool_class))
        .or_default();
    row.total = row.total.saturating_add(amount);
    row.available = row.available.saturating_add(amount);
    Ok(())
}

/// Start delayed withdrawal. Pending stake remains slashable until finalized.
pub fn request_unstake(
    state: &mut State,
    signer: Address,
    provider_id: [u8; 32],
    tool_class: u8,
    amount: u128,
) -> Result<u64, ExecError> {
    let owner = require_provider_owner(state, signer, &provider_id)?;
    let row = state
        .wallet_provider_stakes
        .get_mut(&(provider_id, tool_class))
        .ok_or(ExecError::WalletProviderStakeInsufficient)?;
    if row.available < amount {
        return Err(ExecError::WalletProviderStakeInsufficient);
    }
    row.available -= amount;
    row.pending_unstake = row.pending_unstake.saturating_add(amount);
    let request_id = state.next_wallet_provider_unstake_request_id;
    state.next_wallet_provider_unstake_request_id = state
        .next_wallet_provider_unstake_request_id
        .saturating_add(1);
    let requested_at_ms = state.execution_timestamp_ms;
    state.wallet_provider_unstake_requests.insert(
        request_id,
        OnChainProviderUnstakeRequest {
            request_id,
            provider_id,
            tool_class,
            owner,
            amount,
            requested_at_ms,
            release_ms: requested_at_ms.saturating_add(WALLET_PROVIDER_UNSTAKE_DELAY_MS),
        },
    );
    Ok(request_id)
}

/// Finalize a matured withdrawal and return stake to the provider owner.
pub fn finalize_unstake(
    state: &mut State,
    signer: Address,
    request_id: u64,
) -> Result<(), ExecError> {
    let req = state
        .wallet_provider_unstake_requests
        .get(&request_id)
        .cloned()
        .ok_or(ExecError::NotFound)?;
    if req.owner != signer {
        return Err(ExecError::WalletProviderNotOwned);
    }
    if state.execution_timestamp_ms < req.release_ms {
        return Err(ExecError::WalletProviderUnstakeNotMature);
    }
    let row = state
        .wallet_provider_stakes
        .get_mut(&(req.provider_id, req.tool_class))
        .ok_or(ExecError::WalletProviderStakeInsufficient)?;
    let amount = req.amount.min(row.pending_unstake).min(row.total);
    row.pending_unstake -= amount;
    row.total -= amount;
    if row.total == 0 && row.available == 0 && row.pending_unstake == 0 {
        state
            .wallet_provider_stakes
            .remove(&(req.provider_id, req.tool_class));
    }
    state.wallet_provider_unstake_requests.remove(&request_id);
    credit(state, req.owner, amount);
    Ok(())
}

/// Burn provider stake after governance has committed the evidence hash.
pub fn slash_provider(
    state: &mut State,
    provider_id: [u8; 32],
    slash: ProviderSlashRecord,
) -> Result<(), ExecError> {
    if !state.wallet_providers.contains_key(&provider_id) {
        return Err(ExecError::WalletProviderNotFound);
    }
    if !state
        .slashing_evidence_hashes
        .contains(&slash.evidence_hash)
    {
        return Err(ExecError::MissingSlashingEvidence);
    }
    let key = (provider_id, slash.tool_class);
    let (burned, remaining) = {
        let row = state
            .wallet_provider_stakes
            .get_mut(&key)
            .ok_or(ExecError::WalletProviderStakeInsufficient)?;
        if row.total == 0 || slash.amount == 0 {
            return Err(ExecError::WalletProviderStakeInsufficient);
        }
        let burned = slash.amount.min(row.total);
        let from_available = burned.min(row.available);
        row.available -= from_available;
        let remaining = burned - from_available;
        if remaining > 0 {
            row.pending_unstake = row.pending_unstake.saturating_sub(remaining);
        }
        row.total -= burned;
        row.slashed_total = row.slashed_total.saturating_add(burned);
        (burned, remaining)
    };
    if remaining > 0 {
        reduce_pending_unstake_requests(state, provider_id, slash.tool_class, remaining);
    }
    state.protocol_burned_wei = state.protocol_burned_wei.saturating_add(burned);
    state
        .wallet_provider_slashes
        .push(OnChainProviderSlashRecord {
            provider_id,
            tool_class: slash.tool_class,
            requested_amount: slash.amount,
            burned_amount: burned,
            reason_code: slash.reason_code,
            evidence_hash: slash.evidence_hash,
            challenger: slash.challenger,
            slashed_at_ms: state.execution_timestamp_ms,
        });
    if state
        .wallet_provider_stakes
        .get(&key)
        .is_some_and(|row| row.total == 0 && row.available == 0 && row.pending_unstake == 0)
    {
        state.wallet_provider_stakes.remove(&key);
    }
    Ok(())
}

fn reduce_pending_unstake_requests(
    state: &mut State,
    provider_id: [u8; 32],
    tool_class: u8,
    mut amount: u128,
) {
    let ids: Vec<u64> = state
        .wallet_provider_unstake_requests
        .iter()
        .filter_map(|(id, req)| {
            (req.provider_id == provider_id && req.tool_class == tool_class).then_some(*id)
        })
        .collect();
    for id in ids {
        if amount == 0 {
            break;
        }
        let Some(req) = state.wallet_provider_unstake_requests.get_mut(&id) else {
            continue;
        };
        let take = amount.min(req.amount);
        req.amount -= take;
        amount -= take;
        if req.amount == 0 {
            state.wallet_provider_unstake_requests.remove(&id);
        }
    }
}

/// Update mutable provider metadata.
pub fn update_provider(
    state: &mut State,
    signer: Address,
    provider_id: [u8; 32],
    metadata_uri: String,
    endpoint_uri: String,
    active: bool,
) -> Result<(), ExecError> {
    require_provider_owner(state, signer, &provider_id)?;
    let row = state
        .wallet_providers
        .get_mut(&provider_id)
        .ok_or(ExecError::WalletProviderNotFound)?;
    row.registration.metadata_uri = metadata_uri;
    row.registration.endpoint_uri = endpoint_uri;
    row.active = active;
    row.updated_at_ms = state.execution_timestamp_ms;
    Ok(())
}

/// Deregister a provider after all class stake and pending withdrawals are gone.
pub fn deregister_provider(
    state: &mut State,
    signer: Address,
    provider_id: [u8; 32],
) -> Result<(), ExecError> {
    let owner = require_provider_owner(state, signer, &provider_id)?;
    if state
        .wallet_provider_stakes
        .iter()
        .any(|((pid, _), row)| pid == &provider_id && row.total > 0)
        || state
            .wallet_provider_unstake_requests
            .values()
            .any(|req| req.provider_id == provider_id && req.amount > 0)
    {
        return Err(ExecError::WalletProviderStakeNotEmpty);
    }
    let row = state
        .wallet_providers
        .remove(&provider_id)
        .ok_or(ExecError::WalletProviderNotFound)?;
    credit(state, owner, row.registration.registration_bond);
    Ok(())
}
