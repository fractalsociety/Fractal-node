use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::{BTreeMap, BTreeSet};

use fractal_crypto::hash::keccak256;
use fractal_crypto::verify_message;

use crate::address::Address;
use crate::chain_economics::ChainEconomicsParams;
use crate::error::ExecError;
use crate::merkle::{merkle_root, verify_merkle_proof};
use crate::native_types::{
    AgentRecord, DisputeRecord, OnChainTaskReceipt, PayoutEntry, SettleBatchPayload, StoredBatch,
};
use crate::tx::{
    NativeCall, OwnedObjectId, OwnedObjectPrecheck, OwnedObjectPrecheckError, OwnedObjectVersion,
    Transaction, TxBody, TxExecutionScope, VmKind,
};
use crate::tx_gas_limit;
use crate::EvmEngine;

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub nonce: u64,
    pub balance: u128,
}

fn hash_receipt(r: &OnChainTaskReceipt) -> fractal_crypto::Hash256 {
    keccak256(&borsh::to_vec(r).expect("receipt borsh"))
}

fn hash_payout_entry(e: &PayoutEntry) -> fractal_crypto::Hash256 {
    keccak256(&borsh::to_vec(e).expect("payout borsh"))
}

/// Unified execution state (accounts + native subtries, PRD §9.1 / M3).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub accounts: BTreeMap<Address, Account>,
    /// Phase-1 admin (`[0; 20]` = unrestricted admin path for local dev).
    pub governance: Address,
    pub next_agent_id: u64,
    pub agents: BTreeMap<u64, AgentRecord>,
    pub address_to_agent: BTreeMap<Address, u64>,
    pub receipts: BTreeMap<fractal_crypto::Hash256, OnChainTaskReceipt>,
    pub batches: BTreeMap<fractal_crypto::Hash256, StoredBatch>,
    pub claimed_payouts: BTreeSet<(fractal_crypto::Hash256, u32)>,
    pub next_dispute_id: u64,
    pub disputes: BTreeMap<u64, DisputeRecord>,
    pub stakes: BTreeMap<Address, u128>,
    pub delegated: BTreeMap<(Address, Address), u128>,
    /// Devnet EVM code storage (M4): address → bytecode.
    pub evm_code: BTreeMap<Address, Vec<u8>>,
    /// Devnet EVM storage (M4): (address, slot) → value.
    pub evm_storage: BTreeMap<(Address, [u8; 32]), [u8; 32]>,
    /// Devnet per-tx EVM gas used (M4): tx_hash -> gas_used.
    pub evm_tx_gas_used: BTreeMap<fractal_crypto::Hash256, u64>,
    /// Devnet per-tx EVM logs (M4): tx_hash -> logs.
    pub evm_tx_logs: BTreeMap<fractal_crypto::Hash256, Vec<EvmLog>>,
    /// Receipt `status`: `true` = `0x1` (success). Only written for successful `EvmCall` / `EvmCreate`; absent defaults to success for legacy state.
    pub evm_tx_success: BTreeMap<fractal_crypto::Hash256, bool>,
    /// W6-d: first signer to anchor a wallet `TaskReceipt` commitment (`docs/wallet.md` §9.2).
    pub wallet_task_receipt_anchors: BTreeMap<fractal_crypto::Hash256, Address>,
    /// Fractal Society research proof/package commitments anchored as native transactions.
    pub proof_commitments: BTreeMap<fractal_crypto::Hash256, Address>,
    /// Monotonic versions for owned objects whose version is not already the account nonce.
    pub owned_object_versions: BTreeMap<OwnedObjectId, u64>,
    pub chain_economics: ChainEconomicsParams,
}

impl Default for State {
    fn default() -> Self {
        Self {
            accounts: BTreeMap::new(),
            governance: [0u8; 20],
            next_agent_id: 1,
            agents: BTreeMap::new(),
            address_to_agent: BTreeMap::new(),
            receipts: BTreeMap::new(),
            batches: BTreeMap::new(),
            claimed_payouts: BTreeSet::new(),
            next_dispute_id: 1,
            disputes: BTreeMap::new(),
            stakes: BTreeMap::new(),
            delegated: BTreeMap::new(),
            evm_code: BTreeMap::new(),
            evm_storage: BTreeMap::new(),
            evm_tx_gas_used: BTreeMap::new(),
            evm_tx_logs: BTreeMap::new(),
            evm_tx_success: BTreeMap::new(),
            wallet_task_receipt_anchors: BTreeMap::new(),
            proof_commitments: BTreeMap::new(),
            owned_object_versions: BTreeMap::new(),
            chain_economics: ChainEconomicsParams::default(),
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct EvmLog {
    pub address: Address,
    pub topics: Vec<fractal_crypto::Hash256>,
    pub data: Vec<u8>,
}

impl State {
    pub fn owned_object_version(&self, object_id: &OwnedObjectId) -> u64 {
        match object_id {
            OwnedObjectId::AccountNonce(address) => {
                self.accounts.get(address).map(|a| a.nonce).unwrap_or(0)
            }
            _ => self
                .owned_object_versions
                .get(object_id)
                .copied()
                .unwrap_or(0),
        }
    }

    pub fn owned_object_versions_for_transaction(
        &self,
        tx: &Transaction,
    ) -> Option<Vec<OwnedObjectVersion>> {
        let TxExecutionScope::Owned { objects, .. } = tx.execution_scope() else {
            return None;
        };
        Some(
            objects
                .into_iter()
                .map(|object_id| OwnedObjectVersion {
                    version: self.owned_object_version(&object_id),
                    object_id,
                })
                .collect(),
        )
    }

    pub fn precheck_owned_transaction(
        &self,
        tx: &Transaction,
        object_versions: &[OwnedObjectVersion],
        gas_limit: u64,
        max_fee_per_gas: u128,
        base_fee_per_gas: u128,
    ) -> Result<OwnedObjectPrecheck, OwnedObjectPrecheckError> {
        let TxExecutionScope::Owned { owner, objects } = tx.execution_scope() else {
            return Err(OwnedObjectPrecheckError::NotOwnedObject);
        };
        if owner != tx.signer {
            return Err(OwnedObjectPrecheckError::Owner);
        }

        let account = self
            .accounts
            .get(&tx.signer)
            .ok_or(OwnedObjectPrecheckError::UnknownSigner)?;
        if account.nonce != tx.nonce {
            return Err(OwnedObjectPrecheckError::BadNonce {
                expected: account.nonce,
                actual: tx.nonce,
            });
        }

        let mut supplied_versions = object_versions.to_vec();
        supplied_versions.sort();
        supplied_versions.dedup();
        let supplied_objects = supplied_versions
            .iter()
            .map(|v| v.object_id.clone())
            .collect::<Vec<_>>();
        if supplied_objects != objects {
            return Err(OwnedObjectPrecheckError::ObjectVersionSet);
        }
        for supplied in &supplied_versions {
            let expected = self.owned_object_version(&supplied.object_id);
            if supplied.version != expected {
                return Err(OwnedObjectPrecheckError::ObjectVersion {
                    object_id: supplied.object_id.clone(),
                    expected,
                    actual: supplied.version,
                });
            }
        }

        let tx_gas = tx_gas_limit(tx).map_err(|_| OwnedObjectPrecheckError::InvalidShape)?;
        if tx_gas > gas_limit {
            return Err(OwnedObjectPrecheckError::GasLimit { tx_gas, gas_limit });
        }
        if max_fee_per_gas < base_fee_per_gas {
            return Err(OwnedObjectPrecheckError::FeeBelowBase {
                max_fee_per_gas,
                base_fee_per_gas,
            });
        }
        let required = u128::from(tx_gas)
            .checked_mul(max_fee_per_gas)
            .ok_or(OwnedObjectPrecheckError::FeeOverflow)?;
        if account.balance < required {
            return Err(OwnedObjectPrecheckError::InsufficientFeeBalance {
                balance: account.balance,
                required,
            });
        }
        let tx_hash = keccak256(&borsh::to_vec(tx).map_err(|_| OwnedObjectPrecheckError::Encode)?);

        Ok(OwnedObjectPrecheck {
            tx_hash,
            owner,
            signer_nonce: tx.nonce,
            object_versions: supplied_versions,
            tx_gas,
            max_fee_per_gas,
            base_fee_per_gas,
        })
    }

    pub fn apply_transaction(&mut self, tx: &Transaction) -> Result<(), ExecError> {
        let signer = tx.signer;
        let account = self.accounts.get(&signer).ok_or(ExecError::UnknownSigner)?;
        if account.nonce != tx.nonce {
            return Err(ExecError::BadNonce {
                expected: account.nonce,
                actual: tx.nonce,
            });
        }

        let scope = tx.execution_scope();
        let result = match (&tx.vm, &tx.body) {
            (VmKind::Native, TxBody::Native(call)) => self.apply_native(signer, call),
            (VmKind::Evm, TxBody::Transfer { to, amount }) => {
                self.apply_transfer(signer, *to, *amount)
            }
            _ => Err(ExecError::InvalidShape),
        };
        if result.is_ok() {
            self.bump_owned_object_versions_for_scope(&scope);
        }
        result
    }

    /// Apply a transaction, delegating EVM execution to `evm` when needed.
    pub fn apply_transaction_with_evm(
        &mut self,
        tx: &Transaction,
        evm: &mut dyn EvmEngine,
    ) -> Result<(), ExecError> {
        let signer = tx.signer;
        let account = self.accounts.get(&signer).ok_or(ExecError::UnknownSigner)?;
        if account.nonce != tx.nonce {
            return Err(ExecError::BadNonce {
                expected: account.nonce,
                actual: tx.nonce,
            });
        }

        let scope = tx.execution_scope();
        let result = match (&tx.vm, &tx.body) {
            (VmKind::Native, TxBody::Native(call)) => self.apply_native(signer, call),
            (VmKind::Evm, TxBody::Transfer { to, amount }) => {
                self.apply_transfer(signer, *to, *amount)
            }
            (
                VmKind::Evm,
                TxBody::EvmCall {
                    to,
                    value,
                    calldata,
                    gas_limit,
                },
            ) => {
                if *value != 0 {
                    return Err(ExecError::InvalidShape);
                }
                let outcome =
                    evm.execute_call(self, signer, *to, *value, calldata.clone(), *gas_limit)?;
                // Deterministic tx hash: keccak(borsh(tx))
                if let Ok(raw) = borsh::to_vec(tx) {
                    let h = keccak256(&raw);
                    self.evm_tx_gas_used.insert(h, outcome.gas_used);
                    self.evm_tx_logs.insert(h, outcome.logs);
                    self.evm_tx_success.insert(h, true);
                }
                self.bump_nonce(signer)?;
                Ok(())
            }
            (
                VmKind::Evm,
                TxBody::EvmCreate {
                    value,
                    init_code,
                    gas_limit,
                },
            ) => {
                if *value != 0 {
                    return Err(ExecError::InvalidShape);
                }
                let outcome =
                    evm.execute_create(self, signer, *value, init_code.clone(), *gas_limit)?;
                if let Ok(raw) = borsh::to_vec(tx) {
                    let h = keccak256(&raw);
                    self.evm_tx_gas_used.insert(h, outcome.gas_used);
                    self.evm_tx_logs.insert(h, outcome.logs);
                    self.evm_tx_success.insert(h, true);
                }
                // Caller nonce was incremented inside revm for top-level CREATE.
                Ok(())
            }
            _ => Err(ExecError::InvalidShape),
        };
        if result.is_ok() {
            self.bump_owned_object_versions_for_scope(&scope);
        }
        result
    }

    fn bump_nonce(&mut self, signer: Address) -> Result<(), ExecError> {
        let a = self
            .accounts
            .get_mut(&signer)
            .ok_or(ExecError::UnknownSigner)?;
        a.nonce = a.nonce.saturating_add(1);
        Ok(())
    }

    fn bump_owned_object_versions_for_scope(&mut self, scope: &TxExecutionScope) {
        let objects = match scope {
            TxExecutionScope::Owned { objects, .. } => objects,
            TxExecutionScope::Mixed { owned_objects, .. } => owned_objects,
            TxExecutionScope::Consensus => return,
        };
        for object_id in objects {
            if matches!(object_id, OwnedObjectId::AccountNonce(_)) {
                continue;
            }
            let version = self
                .owned_object_versions
                .entry(object_id.clone())
                .or_insert(0);
            *version = version.saturating_add(1);
        }
    }

    fn require_governance(&self, signer: Address) -> Result<(), ExecError> {
        if self.governance == [0u8; 20] {
            return Ok(());
        }
        if signer != self.governance {
            return Err(ExecError::NotAuthorized);
        }
        Ok(())
    }

    fn apply_transfer(
        &mut self,
        from: Address,
        to: Address,
        amount: u128,
    ) -> Result<(), ExecError> {
        {
            let from_acc = self.accounts.get(&from).ok_or(ExecError::UnknownSigner)?;
            if from_acc.balance < amount {
                return Err(ExecError::InsufficientBalance);
            }
        }
        {
            let from_acc = self
                .accounts
                .get_mut(&from)
                .ok_or(ExecError::UnknownSigner)?;
            from_acc.balance -= amount;
        }
        self.accounts
            .entry(to)
            .or_insert(Account {
                nonce: 0,
                balance: 0,
            })
            .balance += amount;
        self.bump_nonce(from)?;
        Ok(())
    }

    fn apply_native(&mut self, signer: Address, call: &NativeCall) -> Result<(), ExecError> {
        self.apply_native_impl(signer, call, true)
    }

    /// Native syscall entrypoint for EVM precompiles. Does not bump tx nonce.
    pub fn apply_native_syscall(
        &mut self,
        signer: Address,
        call: &NativeCall,
    ) -> Result<(), ExecError> {
        self.apply_native_impl(signer, call, false)
    }

    fn apply_native_impl(
        &mut self,
        signer: Address,
        call: &NativeCall,
        bump_nonce: bool,
    ) -> Result<(), ExecError> {
        match call {
            NativeCall::RegisterAgent {
                operator,
                pubkey,
                kind,
                metadata_uri,
            } => {
                let id = self.next_agent_id;
                self.next_agent_id = self.next_agent_id.saturating_add(1);
                if self.address_to_agent.contains_key(&signer) {
                    return Err(ExecError::AgentIdCollision);
                }
                let now = 0u64;
                let rec = AgentRecord {
                    agent_id: id,
                    address: signer,
                    operator: *operator,
                    pubkey: *pubkey,
                    kind: *kind,
                    metadata_uri: metadata_uri.clone(),
                    reputation_score: 0,
                    completed_jobs: 0,
                    status: 0,
                    registered_at: now,
                    schema_version: 1,
                };
                self.agents.insert(id, rec);
                self.address_to_agent.insert(signer, id);
                if bump_nonce {
                    self.bump_nonce(signer)?;
                }
                Ok(())
            }
            NativeCall::UpdateAgent {
                agent_id,
                new_metadata_uri,
                new_pubkey,
            } => {
                let ag = self.agents.get_mut(agent_id).ok_or(ExecError::NotFound)?;
                if ag.address != signer {
                    return Err(ExecError::NotAuthorized);
                }
                ag.metadata_uri = new_metadata_uri.clone();
                if let Some(pk) = new_pubkey {
                    ag.pubkey = *pk;
                }
                if bump_nonce {
                    self.bump_nonce(signer)?;
                }
                Ok(())
            }
            NativeCall::SuspendAgent {
                agent_id,
                reason: _,
            } => {
                self.require_governance(signer)?;
                let ag = self.agents.get_mut(agent_id).ok_or(ExecError::NotFound)?;
                ag.status = 1;
                if bump_nonce {
                    self.bump_nonce(signer)?;
                }
                Ok(())
            }
            NativeCall::SettleReceipt(r) => {
                if self.receipts.contains_key(&r.receipt_id) {
                    return Err(ExecError::DuplicateReceipt);
                }
                self.receipts.insert(r.receipt_id, r.clone());
                if bump_nonce {
                    self.bump_nonce(signer)?;
                }
                Ok(())
            }
            NativeCall::SettleBatch(p) => self.apply_settle_batch(signer, p, bump_nonce),
            NativeCall::ClaimPayout {
                batch_id,
                account,
                amount,
                leaf_index,
                proof,
            } => self.apply_claim_payout(
                signer,
                batch_id,
                *account,
                *amount,
                *leaf_index,
                proof,
                bump_nonce,
            ),
            NativeCall::FileDispute {
                receipt_id,
                reason_code,
                evidence_hash,
            } => {
                let id = self.next_dispute_id;
                self.next_dispute_id = self.next_dispute_id.saturating_add(1);
                self.disputes.insert(
                    id,
                    DisputeRecord {
                        receipt_id: *receipt_id,
                        filer: signer,
                        reason_code: *reason_code,
                        evidence_hash: *evidence_hash,
                        status: 0,
                    },
                );
                if bump_nonce {
                    self.bump_nonce(signer)?;
                }
                Ok(())
            }
            NativeCall::ResolveDispute {
                dispute_id,
                resolution,
                payouts_diff: _,
            } => {
                self.require_governance(signer)?;
                let d = self
                    .disputes
                    .get_mut(dispute_id)
                    .ok_or(ExecError::NotFound)?;
                d.status = *resolution;
                if bump_nonce {
                    self.bump_nonce(signer)?;
                }
                Ok(())
            }
            NativeCall::Stake { amount } => {
                {
                    let acc = self.accounts.get(&signer).ok_or(ExecError::UnknownSigner)?;
                    if acc.balance < *amount {
                        return Err(ExecError::InsufficientBalance);
                    }
                }
                {
                    let acc = self
                        .accounts
                        .get_mut(&signer)
                        .ok_or(ExecError::UnknownSigner)?;
                    acc.balance -= amount;
                }
                *self.stakes.entry(signer).or_insert(0) += amount;
                if bump_nonce {
                    self.bump_nonce(signer)?;
                }
                Ok(())
            }
            NativeCall::Unstake { amount } => {
                let st = self.stakes.get_mut(&signer).ok_or(ExecError::NotFound)?;
                if *st < *amount {
                    return Err(ExecError::InsufficientBalance);
                }
                *st -= amount;
                self.accounts
                    .entry(signer)
                    .or_insert(Account {
                        nonce: 0,
                        balance: 0,
                    })
                    .balance += amount;
                if bump_nonce {
                    self.bump_nonce(signer)?;
                }
                Ok(())
            }
            NativeCall::Slash {
                validator_id,
                evidence_hash: _,
            } => {
                self.require_governance(signer)?;
                self.stakes.remove(validator_id);
                if bump_nonce {
                    self.bump_nonce(signer)?;
                }
                Ok(())
            }
            NativeCall::Delegate { validator, amount } => {
                {
                    let acc = self.accounts.get(&signer).ok_or(ExecError::UnknownSigner)?;
                    if acc.balance < *amount {
                        return Err(ExecError::InsufficientBalance);
                    }
                }
                {
                    let acc = self
                        .accounts
                        .get_mut(&signer)
                        .ok_or(ExecError::UnknownSigner)?;
                    acc.balance -= amount;
                }
                *self.delegated.entry((signer, *validator)).or_insert(0) += amount;
                if bump_nonce {
                    self.bump_nonce(signer)?;
                }
                Ok(())
            }
            NativeCall::WithdrawRewards { validator: _ } => {
                self.accounts
                    .entry(signer)
                    .or_insert(Account {
                        nonce: 0,
                        balance: 0,
                    })
                    .balance += 0;
                if bump_nonce {
                    self.bump_nonce(signer)?;
                }
                Ok(())
            }
            NativeCall::WalletTaskReceiptAnchorV1 {
                commitment,
                receipt_witness,
            } => {
                if !receipt_witness.is_empty() {
                    #[cfg(feature = "wallet")]
                    {
                        let tr = fractal_wallet::TaskReceipt::try_from_slice(receipt_witness)
                            .map_err(|_| ExecError::InvalidShape)?;
                        let c = crate::wallet_anchor::task_receipt_commitment(&tr)
                            .map_err(|_| ExecError::InvalidShape)?;
                        if c != *commitment {
                            return Err(ExecError::WalletCommitmentMismatch);
                        }
                    }
                    #[cfg(not(feature = "wallet"))]
                    {
                        return Err(ExecError::WalletFeatureDisabled);
                    }
                }
                if self.wallet_task_receipt_anchors.contains_key(commitment) {
                    return Err(ExecError::DuplicateWalletAnchor);
                }
                self.wallet_task_receipt_anchors.insert(*commitment, signer);
                if bump_nonce {
                    self.bump_nonce(signer)?;
                }
                Ok(())
            }
            NativeCall::ProofCommitmentV1 { proof_hash } => {
                if self.proof_commitments.contains_key(proof_hash) {
                    return Err(ExecError::DuplicateProofCommitment);
                }
                self.proof_commitments.insert(*proof_hash, signer);
                if bump_nonce {
                    self.bump_nonce(signer)?;
                }
                Ok(())
            }
            NativeCall::NoOp => {
                if bump_nonce {
                    self.bump_nonce(signer)?;
                }
                Ok(())
            }
            NativeCall::SetChainEconomics { params } => {
                self.require_governance(signer)?;
                self.chain_economics = params.clone();
                if bump_nonce {
                    self.bump_nonce(signer)?;
                }
                Ok(())
            }
        }
    }

    fn apply_settle_batch(
        &mut self,
        signer: Address,
        p: &SettleBatchPayload,
        bump_nonce: bool,
    ) -> Result<(), ExecError> {
        if signer != p.operator {
            return Err(ExecError::NotAuthorized);
        }
        for (i, e) in p.payout_entries.iter().enumerate() {
            if e.index != i as u32 {
                return Err(ExecError::BadPayoutOrdering);
            }
        }
        for r in &p.receipts {
            if self.receipts.contains_key(&r.receipt_id) {
                return Err(ExecError::DuplicateReceipt);
            }
        }
        let r_leaves: Vec<_> = p.receipts.iter().map(hash_receipt).collect();
        let receipt_root = merkle_root(&r_leaves);
        let p_leaves: Vec<_> = p.payout_entries.iter().map(hash_payout_entry).collect();
        let payout_root = merkle_root(&p_leaves);
        let total: u128 = p.payout_entries.iter().map(|e| e.amount).sum();
        {
            let op_acc = self
                .accounts
                .get_mut(&p.operator)
                .ok_or(ExecError::UnknownSigner)?;
            if op_acc.balance < total {
                return Err(ExecError::InsufficientBalance);
            }
            op_acc.balance -= total;
        }
        if p.operator_sig != [0u8; 64] {
            if let Some(&aid) = self.address_to_agent.get(&p.operator) {
                let ag = self.agents.get(&aid).ok_or(ExecError::NotFound)?;
                let mut msg = Vec::new();
                msg.extend_from_slice(&p.batch_id);
                msg.extend_from_slice(&receipt_root);
                msg.extend_from_slice(&payout_root);
                verify_message(&ag.pubkey, &msg, &p.operator_sig)
                    .map_err(|_| ExecError::BadSignature)?;
            }
        }
        let stored = StoredBatch {
            operator: p.operator,
            receipt_root,
            payout_root,
            receipt_count: p.receipts.len() as u32,
            payout_count: p.payout_entries.len() as u32,
            total_payout: total,
            submitted_at: p.submitted_at,
        };
        self.batches.insert(p.batch_id, stored);
        for r in &p.receipts {
            self.receipts.insert(r.receipt_id, r.clone());
        }
        if bump_nonce {
            self.bump_nonce(signer)?;
        }
        Ok(())
    }

    fn apply_claim_payout(
        &mut self,
        signer: Address,
        batch_id: &fractal_crypto::Hash256,
        account: Address,
        amount: u128,
        leaf_index: u32,
        proof: &[fractal_crypto::Hash256],
        bump_nonce: bool,
    ) -> Result<(), ExecError> {
        let key = (*batch_id, leaf_index);
        if self.claimed_payouts.contains(&key) {
            return Err(ExecError::AlreadyClaimed);
        }
        let entry = PayoutEntry {
            index: leaf_index,
            account,
            amount,
        };
        let leaf = hash_payout_entry(&entry);
        let batch = self.batches.get(batch_id).ok_or(ExecError::BatchNotFound)?;
        if !verify_merkle_proof(batch.payout_root, leaf, leaf_index as usize, proof) {
            return Err(ExecError::InvalidProof);
        }
        self.claimed_payouts.insert(key);
        self.accounts
            .entry(account)
            .or_insert(Account {
                nonce: 0,
                balance: 0,
            })
            .balance += amount;
        if bump_nonce {
            self.bump_nonce(signer)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signer() -> Address {
        [7u8; 20]
    }

    fn funded_state() -> State {
        let mut state = State::default();
        state.accounts.insert(
            signer(),
            Account {
                nonce: 0,
                balance: 1_000_000,
            },
        );
        state
    }

    fn receipt(receipt_id: fractal_crypto::Hash256) -> OnChainTaskReceipt {
        OnChainTaskReceipt {
            receipt_id,
            job_id: [1u8; 32],
            requester: signer(),
            worker: 1,
            verifier: 2,
            artifact_root: [3u8; 32],
            output_hash: [4u8; 32],
            score: 100,
            payout_amount: 10,
            verifier_fee: 1,
            protocol_fee: 1,
            final_status: 1,
            finalized_at: 123,
            schema_version: 1,
        }
    }

    #[test]
    fn owned_object_versions_include_account_nonce_and_agent_version() {
        let mut state = funded_state();
        state.agents.insert(
            42,
            AgentRecord {
                agent_id: 42,
                address: signer(),
                operator: signer(),
                pubkey: [1u8; 32],
                kind: 1,
                metadata_uri: "ipfs://old".into(),
                reputation_score: 0,
                completed_jobs: 0,
                status: 0,
                registered_at: 0,
                schema_version: 1,
            },
        );
        let tx = Transaction {
            signer: signer(),
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::UpdateAgent {
                agent_id: 42,
                new_metadata_uri: "ipfs://new".into(),
                new_pubkey: None,
            }),
        };

        assert_eq!(
            state.owned_object_versions_for_transaction(&tx).unwrap(),
            vec![
                OwnedObjectVersion {
                    object_id: OwnedObjectId::AccountNonce(signer()),
                    version: 0,
                },
                OwnedObjectVersion {
                    object_id: OwnedObjectId::Agent(42),
                    version: 0,
                },
            ]
        );

        state.apply_transaction(&tx).unwrap();

        assert_eq!(
            state.owned_object_version(&OwnedObjectId::AccountNonce(signer())),
            1
        );
        assert_eq!(state.owned_object_version(&OwnedObjectId::Agent(42)), 1);
    }

    fn state_with_agent() -> State {
        let mut state = funded_state();
        state.accounts.get_mut(&signer()).unwrap().balance = 10_000_000;
        state.agents.insert(
            42,
            AgentRecord {
                agent_id: 42,
                address: signer(),
                operator: signer(),
                pubkey: [1u8; 32],
                kind: 1,
                metadata_uri: "ipfs://old".into(),
                reputation_score: 0,
                completed_jobs: 0,
                status: 0,
                registered_at: 0,
                schema_version: 1,
            },
        );
        state
    }

    fn update_agent_tx(nonce: u64) -> Transaction {
        Transaction {
            signer: signer(),
            nonce,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::UpdateAgent {
                agent_id: 42,
                new_metadata_uri: "ipfs://new".into(),
                new_pubkey: None,
            }),
        }
    }

    #[test]
    fn owned_transaction_precheck_accepts_current_versions_gas_and_fee() {
        let state = state_with_agent();
        let tx = update_agent_tx(0);
        let versions = state.owned_object_versions_for_transaction(&tx).unwrap();

        let precheck = state
            .precheck_owned_transaction(&tx, &versions, 10_000, 2, 1)
            .expect("precheck");

        assert_eq!(precheck.owner, signer());
        assert_eq!(precheck.signer_nonce, 0);
        assert_eq!(precheck.object_versions, versions);
        assert!(precheck.tx_gas <= 10_000);
        assert_eq!(precheck.max_fee_per_gas, 2);
        assert_eq!(precheck.base_fee_per_gas, 1);
    }

    #[test]
    fn owned_transaction_precheck_rejects_bad_nonce() {
        let mut state = state_with_agent();
        state.accounts.get_mut(&signer()).unwrap().nonce = 3;
        let tx = update_agent_tx(0);
        let versions = vec![
            OwnedObjectVersion {
                object_id: OwnedObjectId::AccountNonce(signer()),
                version: 3,
            },
            OwnedObjectVersion {
                object_id: OwnedObjectId::Agent(42),
                version: 0,
            },
        ];

        assert_eq!(
            state.precheck_owned_transaction(&tx, &versions, 10_000, 2, 1),
            Err(OwnedObjectPrecheckError::BadNonce {
                expected: 3,
                actual: 0
            })
        );
    }

    #[test]
    fn owned_transaction_precheck_rejects_stale_object_version() {
        let state = state_with_agent();
        let tx = update_agent_tx(0);
        let versions = vec![
            OwnedObjectVersion {
                object_id: OwnedObjectId::AccountNonce(signer()),
                version: 0,
            },
            OwnedObjectVersion {
                object_id: OwnedObjectId::Agent(42),
                version: 9,
            },
        ];

        assert_eq!(
            state.precheck_owned_transaction(&tx, &versions, 10_000, 2, 1),
            Err(OwnedObjectPrecheckError::ObjectVersion {
                object_id: OwnedObjectId::Agent(42),
                expected: 0,
                actual: 9,
            })
        );
    }

    #[test]
    fn owned_transaction_precheck_rejects_mixed_transaction() {
        let state = state_with_agent();
        let tx = Transaction {
            signer: signer(),
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::FileDispute {
                receipt_id: [5u8; 32],
                reason_code: 1,
                evidence_hash: [6u8; 32],
            }),
        };

        assert_eq!(
            state.precheck_owned_transaction(&tx, &[], 10_000, 2, 1),
            Err(OwnedObjectPrecheckError::NotOwnedObject)
        );
    }

    #[test]
    fn owned_transaction_precheck_rejects_gas_and_fee_failures() {
        let mut state = state_with_agent();
        let tx = update_agent_tx(0);
        let versions = state.owned_object_versions_for_transaction(&tx).unwrap();

        assert_eq!(
            state.precheck_owned_transaction(&tx, &versions, 1, 2, 1),
            Err(OwnedObjectPrecheckError::GasLimit {
                tx_gas: tx_gas_limit(&tx).unwrap(),
                gas_limit: 1,
            })
        );
        assert_eq!(
            state.precheck_owned_transaction(&tx, &versions, 10_000, 1, 2),
            Err(OwnedObjectPrecheckError::FeeBelowBase {
                max_fee_per_gas: 1,
                base_fee_per_gas: 2,
            })
        );

        state.accounts.get_mut(&signer()).unwrap().balance = 1;
        let required = u128::from(tx_gas_limit(&tx).unwrap()).saturating_mul(2);
        assert_eq!(
            state.precheck_owned_transaction(&tx, &versions, 10_000, 2, 1),
            Err(OwnedObjectPrecheckError::InsufficientFeeBalance {
                balance: 1,
                required,
            })
        );
    }

    #[test]
    fn owned_receipt_version_bumps_after_settle_receipt() {
        let mut state = funded_state();
        let receipt_id = [9u8; 32];
        let tx = Transaction {
            signer: signer(),
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::SettleReceipt(receipt(receipt_id))),
        };

        assert_eq!(
            state.owned_object_version(&OwnedObjectId::Receipt(receipt_id)),
            0
        );
        state.apply_transaction(&tx).unwrap();
        assert_eq!(
            state.owned_object_version(&OwnedObjectId::Receipt(receipt_id)),
            1
        );
    }

    #[test]
    fn mixed_settle_batch_versions_owned_receipts_but_is_not_certificate_eligible() {
        let mut state = funded_state();
        let receipt_id = [8u8; 32];
        let tx = Transaction {
            signer: signer(),
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::SettleBatch(SettleBatchPayload {
                batch_id: [3u8; 32],
                operator: signer(),
                receipts: vec![receipt(receipt_id)],
                payout_entries: Vec::new(),
                submitted_at: 123,
                operator_sig: [0u8; 64],
            })),
        };

        assert!(state.owned_object_versions_for_transaction(&tx).is_none());
        assert!(tx.is_mixed_object_tx());

        state.apply_transaction(&tx).unwrap();

        assert_eq!(
            state.owned_object_version(&OwnedObjectId::Receipt(receipt_id)),
            1
        );
    }

    #[test]
    fn governance_updates_chain_economics_phase_and_reward_params() {
        let governance = [9u8; 20];
        let mut state = funded_state();
        state.governance = governance;
        state.accounts.insert(
            governance,
            Account {
                nonce: 0,
                balance: 1_000,
            },
        );
        let params = crate::ChainEconomicsParams::mainnet();
        let tx = Transaction {
            signer: governance,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::SetChainEconomics {
                params: params.clone(),
            }),
        };

        state.apply_transaction(&tx).unwrap();

        assert_eq!(state.chain_economics, params);
        assert_eq!(state.accounts[&governance].nonce, 1);
        assert!(state.chain_economics.phase_config.proof_final_settlement);
        assert!(state.chain_economics.prover_rewards.enabled);
        assert!(state.chain_economics.sequencer_rewards.enabled);
    }

    #[test]
    fn non_governance_cannot_update_chain_economics() {
        let mut state = funded_state();
        state.governance = [9u8; 20];
        let tx = Transaction {
            signer: signer(),
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::SetChainEconomics {
                params: crate::ChainEconomicsParams::mainnet(),
            }),
        };

        assert_eq!(state.apply_transaction(&tx), Err(ExecError::NotAuthorized));
    }
}
