//! On-chain wallet capabilities and budget accounts (`docs/wallet.md` §14.1–14.2).
//!
//! Capability revocation uses `wallet_revocation_entries` + sparse Merkle trie root (§4.6 / §12.3).

use crate::address::Address;
use crate::error::ExecError;
#[cfg(feature = "wallet")]
use crate::native_types::WalletEmergencyScopeV1;
use crate::native_types::{
    OnChainBudgetAccount, OnChainRevocationEntry, WalletBudgetSeed, WalletRevokeCapabilitySignBody,
};
use crate::state::State;

#[cfg(feature = "wallet")]
fn load_capability_token(
    state: &State,
    cap_id: &[u8; 32],
) -> Result<fractal_wallet::CapabilityToken, ExecError> {
    let bytes = state
        .wallet_capabilities
        .get(cap_id)
        .ok_or(ExecError::WalletCapabilityNotFound)?;
    borsh::from_slice(bytes).map_err(|_| ExecError::InvalidShape)
}

/// Parent chain for a not-yet-registered child token (immediate parent first).
#[cfg(feature = "wallet")]
pub fn ancestor_chain_for_mint(
    state: &State,
    child: &fractal_wallet::CapabilityToken,
) -> Vec<[u8; 32]> {
    let mut out = Vec::new();
    let mut cur = child.body.parent_cap_id;
    while let Some(pid) = cur {
        out.push(pid);
        cur = load_capability_token(state, &pid)
            .ok()
            .and_then(|t| t.body.parent_cap_id);
    }
    out
}

/// Build `RevocationVerifyProof` borsh for on-chain mint (`docs/wallet.md` §14.1).
#[cfg(feature = "wallet")]
pub fn mint_revocation_proof_bytes(
    state: &State,
    child: &fractal_wallet::CapabilityToken,
) -> Result<Vec<u8>, ExecError> {
    use fractal_wallet::{RevocationEntry, RevocationSet};
    let set = RevocationSet::from_entries(state.wallet_revocation_entries.iter().map(|(id, e)| {
        (
            *id,
            RevocationEntry {
                revoked_at_ms: e.revoked_at_ms,
                reason_code: e.reason_code,
                cascade: e.cascade,
            },
        )
    }));
    let ancestors = ancestor_chain_for_mint(state, child);
    let proof = set
        .build_verify_proof(child.body.cap_id, &ancestors)
        .map_err(|_| ExecError::WalletRevocationProofInvalid)?;
    borsh::to_vec(&proof).map_err(|_| ExecError::InvalidShape)
}

/// Verify §4.6 non-revocation proof bundled with mint.
#[cfg(feature = "wallet")]
pub fn verify_mint_revocation_proof(
    state: &State,
    child: &fractal_wallet::CapabilityToken,
    revocation_proof_borsh: &[u8],
) -> Result<(), ExecError> {
    if revocation_proof_borsh.is_empty() {
        return Err(ExecError::WalletRevocationProofRequired);
    }
    let proof: fractal_wallet::RevocationVerifyProof = borsh::from_slice(revocation_proof_borsh)
        .map_err(|_| ExecError::WalletRevocationProofInvalid)?;
    if proof.revocation_root != state.wallet_revocation_merkle_root {
        return Err(ExecError::WalletRevocationProofInvalid);
    }
    let ancestors = ancestor_chain_for_mint(state, child);
    fractal_wallet::verify_capability_with_revocation(
        child,
        state.execution_timestamp_ms,
        &state.wallet_revocation_merkle_root,
        &ancestors,
        &proof,
    )
    .map_err(|_| ExecError::WalletRevocationProofInvalid)
}

/// Parent chain from registered `cap_id` toward root (immediate parent first).
#[cfg(feature = "wallet")]
pub fn capability_ancestor_chain(state: &State, cap_id: [u8; 32]) -> Vec<[u8; 32]> {
    let mut out = Vec::new();
    let mut cur = load_capability_token(state, &cap_id)
        .ok()
        .and_then(|t| t.body.parent_cap_id);
    while let Some(pid) = cur {
        out.push(pid);
        cur = load_capability_token(state, &pid)
            .ok()
            .and_then(|t| t.body.parent_cap_id);
    }
    out
}

#[cfg(feature = "wallet")]
fn root_issuer_for_registered_capability(
    state: &State,
    cap_id: [u8; 32],
) -> Result<[u8; 32], ExecError> {
    let mut token = load_capability_token(state, &cap_id)?;
    while let Some(parent_id) = token.body.parent_cap_id {
        token = load_capability_token(state, &parent_id)?;
    }
    Ok(token.body.issuer)
}

#[cfg(feature = "wallet")]
fn root_issuer_for_mint(
    state: &State,
    child: &fractal_wallet::CapabilityToken,
) -> Result<[u8; 32], ExecError> {
    match child.body.parent_cap_id {
        Some(parent_id) => root_issuer_for_registered_capability(state, parent_id),
        None => Ok(child.body.issuer),
    }
}

/// Direct revoke or cascade from a revoked ancestor (`docs/wallet.md` §12.3).
#[cfg(feature = "wallet")]
#[must_use]
pub fn is_capability_revoked(state: &State, cap_id: &[u8; 32]) -> bool {
    if state.wallet_revocation_entries.contains_key(cap_id) {
        return true;
    }
    for ancestor in capability_ancestor_chain(state, *cap_id) {
        if let Some(entry) = state.wallet_revocation_entries.get(&ancestor) {
            if entry.cascade {
                return true;
            }
        }
    }
    false
}

#[cfg(feature = "wallet")]
fn emergency_scope_covers_capability(
    rule: &WalletEmergencyScopeV1,
    cap: &fractal_wallet::types::Scope,
) -> bool {
    if let Some(workspace_id) = rule.workspace_id {
        if cap.workspace_id != Some(workspace_id) {
            return false;
        }
    }
    if let Some(project_id) = rule.project_id {
        if cap.project_id != Some(project_id) {
            return false;
        }
    }
    if let Some(task_id) = rule.task_id {
        if cap.task_id != Some(task_id) {
            return false;
        }
    }
    if rule.tool_class_mask != 0 && (cap.tool_class_mask & rule.tool_class_mask) == 0 {
        return false;
    }
    if let Some(provider_id) = rule.provider_id {
        match &cap.providers {
            Some(providers) if providers.contains(&provider_id) => {}
            None => {}
            _ => return false,
        }
    }
    true
}

#[cfg(feature = "wallet")]
#[must_use]
pub fn is_scope_stopped_for_master(
    state: &State,
    master_public_key: &[u8; 32],
    scope: &fractal_wallet::types::Scope,
) -> bool {
    state
        .wallet_scoped_emergency_stops
        .iter()
        .any(|((pk, _), rec)| {
            pk == master_public_key && emergency_scope_covers_capability(&rec.scope, scope)
        })
}

#[cfg(not(feature = "wallet"))]
#[must_use]
pub fn is_scope_stopped_for_master(
    _state: &State,
    _master_public_key: &[u8; 32],
    _scope: &(),
) -> bool {
    false
}

#[cfg(not(feature = "wallet"))]
#[must_use]
pub fn is_capability_revoked(_state: &State, _cap_id: &[u8; 32]) -> bool {
    false
}

#[cfg(feature = "wallet")]
fn verify_revoke_issuer_sig(
    token: &fractal_wallet::CapabilityToken,
    reason_code: u8,
    cascade: bool,
    chain_id: u32,
    issuer_sig: &[u8; 64],
) -> Result<(), ExecError> {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    let body = WalletRevokeCapabilitySignBody {
        cap_id: token.body.cap_id,
        reason_code,
        cascade,
        chain_id,
    };
    let msg = borsh::to_vec(&body).map_err(|_| ExecError::InvalidShape)?;
    let vk = VerifyingKey::from_bytes(&token.body.issuer)
        .map_err(|_| ExecError::WalletRevokeSignatureInvalid)?;
    let sig = Signature::from_bytes(issuer_sig);
    vk.verify(&msg, &sig)
        .map_err(|_| ExecError::WalletRevokeSignatureInvalid)
}

fn debit_signer(state: &mut State, signer: Address, amount: u128) -> Result<(), ExecError> {
    if amount == 0 {
        return Ok(());
    }
    let bal = state
        .accounts
        .get(&signer)
        .ok_or(ExecError::UnknownSigner)?
        .balance;
    if bal < amount {
        return Err(ExecError::InsufficientBalance);
    }
    state.accounts.get_mut(&signer).expect("signer").balance -= amount;
    Ok(())
}

fn credit_signer(state: &mut State, signer: Address, amount: u128) {
    if amount == 0 {
        return;
    }
    state
        .accounts
        .entry(signer)
        .or_insert(crate::state::Account {
            nonce: 0,
            balance: 0,
        })
        .balance += amount;
}

fn budget_owner(state: &State, id: u64) -> Result<Address, ExecError> {
    state
        .wallet_budgets
        .get(&id)
        .map(|b| b.owner)
        .ok_or(ExecError::WalletBudgetNotFound)
}

fn require_budget_owner(state: &State, signer: Address, id: u64) -> Result<(), ExecError> {
    if budget_owner(state, id)? != signer {
        return Err(ExecError::WalletBudgetNotOwned);
    }
    Ok(())
}

fn allocate_budget(
    state: &mut State,
    from_id: u64,
    to_id: u64,
    amount: u128,
) -> Result<(), ExecError> {
    if amount == 0 {
        return Ok(());
    }
    let parent_id = state
        .wallet_budgets
        .get(&to_id)
        .ok_or(ExecError::WalletBudgetNotFound)?
        .parent
        .ok_or(ExecError::WalletBudgetLinkInvalid)?;
    if parent_id != from_id {
        return Err(ExecError::WalletBudgetLinkInvalid);
    }
    let parent_avail = {
        let p = state
            .wallet_budgets
            .get(&from_id)
            .ok_or(ExecError::WalletBudgetNotFound)?;
        p.available()
    };
    if amount > parent_avail {
        return Err(ExecError::InsufficientBalance);
    }
    let parent = state.wallet_budgets.get_mut(&from_id).expect("from");
    parent.total_deposited = parent.total_deposited.saturating_sub(amount);
    parent.nonce = parent.nonce.saturating_add(1);
    let child = state.wallet_budgets.get_mut(&to_id).expect("to");
    child.total_deposited = child.total_deposited.saturating_add(amount);
    child.nonce = child.nonce.saturating_add(1);
    Ok(())
}

/// §14.2 `CreateBudgetAccount`.
pub fn create_budget_account(
    state: &mut State,
    signer: Address,
    parent: Option<u64>,
    initial_deposit: u128,
) -> Result<u64, ExecError> {
    if let Some(pid) = parent {
        require_budget_owner(state, signer, pid)?;
    }
    debit_signer(state, signer, initial_deposit)?;
    let id = state.next_wallet_budget_id;
    state.next_wallet_budget_id = state.next_wallet_budget_id.saturating_add(1);
    state.wallet_budgets.insert(
        id,
        OnChainBudgetAccount {
            id,
            parent,
            owner: signer,
            total_deposited: initial_deposit,
            reserved: 0,
            spent: 0,
            nonce: 0,
        },
    );
    Ok(id)
}

/// §14.2 `FundBudgetAccount` — `source_budget == None` pulls from signer native balance.
pub fn fund_budget_account(
    state: &mut State,
    signer: Address,
    budget: u64,
    amount: u128,
    source_budget: Option<u64>,
) -> Result<(), ExecError> {
    require_budget_owner(state, signer, budget)?;
    match source_budget {
        None => {
            debit_signer(state, signer, amount)?;
            let b = state
                .wallet_budgets
                .get_mut(&budget)
                .ok_or(ExecError::WalletBudgetNotFound)?;
            b.total_deposited = b.total_deposited.saturating_add(amount);
            b.nonce = b.nonce.saturating_add(1);
        }
        Some(src) => {
            require_budget_owner(state, signer, src)?;
            allocate_budget(state, src, budget, amount)?;
        }
    }
    Ok(())
}

/// §14.2 `CloseBudgetAccount` — returns available to parent budget or signer balance.
pub fn close_budget_account(
    state: &mut State,
    signer: Address,
    budget: u64,
) -> Result<(), ExecError> {
    require_budget_owner(state, signer, budget)?;
    let row = state
        .wallet_budgets
        .remove(&budget)
        .ok_or(ExecError::WalletBudgetNotFound)?;
    if row.reserved != 0 {
        return Err(ExecError::WalletBudgetNotEmpty);
    }
    let available = row.available();
    if let Some(pid) = row.parent {
        let parent = state
            .wallet_budgets
            .get_mut(&pid)
            .ok_or(ExecError::WalletBudgetNotFound)?;
        parent.total_deposited = parent.total_deposited.saturating_add(available);
        parent.nonce = parent.nonce.saturating_add(1);
    } else {
        credit_signer(state, signer, available);
    }
    Ok(())
}

/// §14.1 `MintCapability` (requires `fractal-core` **`--features wallet`**).
#[cfg(feature = "wallet")]
pub fn mint_capability(
    state: &mut State,
    signer: Address,
    parent_cap_id: Option<[u8; 32]>,
    child_token_borsh: Vec<u8>,
    budget_seed: Option<WalletBudgetSeed>,
    revocation_proof_borsh: Vec<u8>,
) -> Result<(), ExecError> {
    let child: fractal_wallet::CapabilityToken =
        borsh::from_slice(&child_token_borsh).map_err(|_| ExecError::InvalidShape)?;
    verify_mint_revocation_proof(state, &child, &revocation_proof_borsh)?;
    child
        .verify()
        .map_err(|_| ExecError::WalletCapabilityInvalid)?;
    child
        .verify_autonomous_tool_mask()
        .map_err(|_| ExecError::WalletCapabilityInvalid)?;
    if state.execution_timestamp_ms > 0 {
        child
            .verify_time(state.execution_timestamp_ms)
            .map_err(|_| ExecError::WalletCapabilityInvalid)?;
    }
    if child.body.chain_id != state.wallet_chain_id {
        return Err(ExecError::WalletCapabilityInvalid);
    }
    if is_capability_revoked(state, &child.body.cap_id) {
        return Err(ExecError::WalletCapabilityRevoked);
    }
    if state.wallet_capabilities.contains_key(&child.body.cap_id) {
        return Err(ExecError::DuplicateWalletCapability);
    }
    if child.body.parent_cap_id != parent_cap_id {
        return Err(ExecError::WalletAttenuationFailed);
    }
    let root_issuer = root_issuer_for_mint(state, &child)?;
    if is_scope_stopped_for_master(state, &root_issuer, &child.body.scope) {
        return Err(ExecError::WalletEmergencyStopActive);
    }

    match parent_cap_id {
        None => {
            if child.body.parent_cap_id.is_some() {
                return Err(ExecError::WalletAttenuationFailed);
            }
        }
        Some(pid) => {
            if is_capability_revoked(state, &pid) {
                return Err(ExecError::WalletCapabilityRevoked);
            }
            let parent = load_capability_token(state, &pid)?;
            if state.wallet_cap_holders.get(&pid) != Some(&signer) {
                return Err(ExecError::NotAuthorized);
            }
            if !fractal_wallet::CapabilityToken::verify_attenuation_from_parent(
                &child.body,
                &parent.body,
            ) {
                return Err(ExecError::WalletAttenuationFailed);
            }
        }
    }

    if let Some(seed) = budget_seed {
        let child_budget = child.body.budget_account;
        require_budget_owner(state, signer, seed.from_budget)?;
        if !state.wallet_budgets.contains_key(&child_budget) {
            return Err(ExecError::WalletBudgetNotFound);
        }
        require_budget_owner(state, signer, child_budget)?;
        allocate_budget(state, seed.from_budget, child_budget, seed.amount)?;
    }

    state
        .wallet_capabilities
        .insert(child.body.cap_id, child_token_borsh);
    state.wallet_cap_holders.insert(child.body.cap_id, signer);
    Ok(())
}

/// §14.1 `RevokeCapability` (requires `fractal-core` **`--features wallet`**).
#[cfg(feature = "wallet")]
pub fn revoke_capability(
    state: &mut State,
    cap_id: [u8; 32],
    reason_code: u8,
    cascade: bool,
    issuer_sig: [u8; 64],
) -> Result<(), ExecError> {
    let token = load_capability_token(state, &cap_id)?;
    if state.wallet_revocation_entries.contains_key(&cap_id) {
        return Err(ExecError::WalletCapabilityAlreadyRevoked);
    }
    verify_revoke_issuer_sig(
        &token,
        reason_code,
        cascade,
        state.wallet_chain_id,
        &issuer_sig,
    )?;
    let revoked_at_ms = state.execution_timestamp_ms;
    state.wallet_revocation_entries.insert(
        cap_id,
        OnChainRevocationEntry {
            revoked_at_ms,
            reason_code,
            cascade,
        },
    );
    sync_wallet_revocation_merkle_root(state);
    Ok(())
}

/// Recompute [`State::wallet_revocation_merkle_root`] from `wallet_revocation_entries`.
#[cfg(feature = "wallet")]
pub fn sync_wallet_revocation_merkle_root(state: &mut State) {
    use fractal_wallet::{RevocationEntry, RevocationSet};
    let set = RevocationSet::from_entries(state.wallet_revocation_entries.iter().map(|(id, e)| {
        (
            *id,
            RevocationEntry {
                revoked_at_ms: e.revoked_at_ms,
                reason_code: e.reason_code,
                cascade: e.cascade,
            },
        )
    }));
    state.wallet_revocation_merkle_root = set.root();
}

#[cfg(not(feature = "wallet"))]
pub fn sync_wallet_revocation_merkle_root(_state: &mut State) {}
