use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::{BTreeMap, BTreeSet};

use fractal_crypto::hash::keccak256;
use fractal_crypto::verify_message;
use rlp::RlpStream;

use crate::address::Address;
use crate::error::ExecError;
use crate::merkle::{merkle_root, verify_merkle_proof};
use crate::native_types::{
    AgentRecord, DisputeRecord, OnChainTaskReceipt, PayoutEntry, SettleBatchPayload, StoredBatch,
};
use crate::EvmEngine;
use crate::tx::{NativeCall, Transaction, TxBody, VmKind};

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
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct EvmLog {
    pub address: Address,
    pub topics: Vec<fractal_crypto::Hash256>,
    pub data: Vec<u8>,
}

fn create_address(from: Address, nonce: u64) -> Address {
    // Ethereum CREATE address: keccak256(rlp([from, nonce]))[12..]
    let mut s = RlpStream::new_list(2);
    s.append(&from.as_slice());
    s.append(&nonce);
    let h = keccak256(&s.out());
    let mut a = [0u8; 20];
    a.copy_from_slice(&h[12..]);
    a
}

impl State {
    pub fn apply_transaction(&mut self, tx: &Transaction) -> Result<(), ExecError> {
        let signer = tx.signer;
        let account = self.accounts.get(&signer).ok_or(ExecError::UnknownSigner)?;
        if account.nonce != tx.nonce {
            return Err(ExecError::BadNonce {
                expected: account.nonce,
                actual: tx.nonce,
            });
        }

        match (&tx.vm, &tx.body) {
            (VmKind::Native, TxBody::Native(call)) => self.apply_native(signer, call),
            (VmKind::Evm, TxBody::Transfer { to, amount }) => self.apply_transfer(signer, *to, *amount),
            _ => Err(ExecError::InvalidShape),
        }
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

        match (&tx.vm, &tx.body) {
            (VmKind::Native, TxBody::Native(call)) => self.apply_native(signer, call),
            (VmKind::Evm, TxBody::Transfer { to, amount }) => self.apply_transfer(signer, *to, *amount),
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
                }
                self.bump_nonce(signer);
                Ok(())
            }
            (VmKind::Evm, TxBody::EvmCreate { value, init_code, gas_limit: _ }) => {
                if *value != 0 {
                    return Err(ExecError::InvalidShape);
                }
                // Devnet CREATE: store "runtime code" directly.
                let addr = create_address(signer, tx.nonce);
                self.evm_code.insert(addr, init_code.clone());
                self.bump_nonce(signer);
                Ok(())
            }
            _ => Err(ExecError::InvalidShape),
        }
    }

    fn bump_nonce(&mut self, signer: Address) {
        let a = self.accounts.get_mut(&signer).expect("signer exists");
        a.nonce = a.nonce.saturating_add(1);
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

    fn apply_transfer(&mut self, from: Address, to: Address, amount: u128) -> Result<(), ExecError> {
        {
            let from_acc = self.accounts.get(&from).ok_or(ExecError::UnknownSigner)?;
            if from_acc.balance < amount {
                return Err(ExecError::InsufficientBalance);
            }
        }
        {
            let from_acc = self.accounts.get_mut(&from).expect("from exists");
            from_acc.balance -= amount;
        }
        self.accounts
            .entry(to)
            .or_insert(Account { nonce: 0, balance: 0 })
            .balance += amount;
        self.bump_nonce(from);
        Ok(())
    }

    fn apply_native(&mut self, signer: Address, call: &NativeCall) -> Result<(), ExecError> {
        self.apply_native_impl(signer, call, true)
    }

    /// Native syscall entrypoint for EVM precompiles. Does not bump tx nonce.
    pub fn apply_native_syscall(&mut self, signer: Address, call: &NativeCall) -> Result<(), ExecError> {
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
                    self.bump_nonce(signer);
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
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::SuspendAgent { agent_id, reason: _ } => {
                self.require_governance(signer)?;
                let ag = self.agents.get_mut(agent_id).ok_or(ExecError::NotFound)?;
                ag.status = 1;
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::SettleReceipt(r) => {
                if self.receipts.contains_key(&r.receipt_id) {
                    return Err(ExecError::DuplicateReceipt);
                }
                self.receipts.insert(r.receipt_id, r.clone());
                if bump_nonce {
                    self.bump_nonce(signer);
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
            } => self.apply_claim_payout(signer, batch_id, *account, *amount, *leaf_index, proof, bump_nonce),
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
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::ResolveDispute {
                dispute_id,
                resolution,
                payouts_diff: _,
            } => {
                self.require_governance(signer)?;
                let d = self.disputes.get_mut(dispute_id).ok_or(ExecError::NotFound)?;
                d.status = *resolution;
                if bump_nonce {
                    self.bump_nonce(signer);
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
                    let acc = self.accounts.get_mut(&signer).expect("signer");
                    acc.balance -= amount;
                }
                *self.stakes.entry(signer).or_insert(0) += amount;
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::Unstake { amount } => {
                let st = self.stakes.get_mut(&signer).ok_or(ExecError::NotFound)?;
                if *st < *amount {
                    return Err(ExecError::InsufficientBalance);
                }
                *st -= amount;
                self.accounts.entry(signer).or_insert(Account { nonce: 0, balance: 0 }).balance += amount;
                if bump_nonce {
                    self.bump_nonce(signer);
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
                    self.bump_nonce(signer);
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
                    let acc = self.accounts.get_mut(&signer).expect("signer");
                    acc.balance -= amount;
                }
                *self.delegated.entry((signer, *validator)).or_insert(0) += amount;
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WithdrawRewards { validator: _ } => {
                self.accounts.entry(signer).or_insert(Account { nonce: 0, balance: 0 }).balance += 0;
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::NoOp => {
                if bump_nonce {
                    self.bump_nonce(signer);
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
            let op_acc = self.accounts.get_mut(&p.operator).ok_or(ExecError::UnknownSigner)?;
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
                verify_message(&ag.pubkey, &msg, &p.operator_sig).map_err(|_| ExecError::BadSignature)?;
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
            self.bump_nonce(signer);
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
            .or_insert(Account { nonce: 0, balance: 0 })
            .balance += amount;
        if bump_nonce {
            self.bump_nonce(signer);
        }
        Ok(())
    }
}
