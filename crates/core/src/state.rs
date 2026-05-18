use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::{BTreeMap, BTreeSet};

use fractal_crypto::hash::keccak256;
use fractal_crypto::verify_message;

use crate::EvmEngine;
use crate::address::Address;
use crate::chain_economics::{ChainEconomicsParams, ValidatorRegistryEntry};
use crate::error::ExecError;
use crate::merkle::{merkle_root, verify_merkle_proof};
use crate::native_types::{
    AgentRecord, DisputeRecord, OnChainProviderRow, OnChainProviderSlashRecord,
    OnChainProviderStakeRow, OnChainProviderUnstakeRequest, OnChainTaskReceipt, PayoutEntry,
    SettleBatchPayload, StoredBatch, WalletEmergencyScopeV1, WalletScopedEmergencyStopRecordV1,
};
use crate::tx::{NativeCall, Transaction, TxBody, VmKind};

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Account {
    pub nonce: u64,
    pub balance: u128,
}

fn hash_receipt(r: &OnChainTaskReceipt) -> fractal_crypto::Hash256 {
    keccak256(&borsh::to_vec(r).expect("receipt borsh"))
}

#[must_use]
fn default_wallet_revocation_merkle_root() -> fractal_crypto::Hash256 {
    #[cfg(feature = "wallet")]
    {
        fractal_wallet::empty_tree_root()
    }
    #[cfg(not(feature = "wallet"))]
    {
        [0u8; 32]
    }
}

fn hash_payout_entry(e: &PayoutEntry) -> fractal_crypto::Hash256 {
    keccak256(&borsh::to_vec(e).expect("payout borsh"))
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ConsensusUnbondEntry {
    pub owner: Address,
    pub validator_fingerprint: [u8; 32],
    pub amount: u128,
    /// `0` = not yet anchored to a block time; set in [`crate::finalize_block_hooks`].
    pub release_ms: u64,
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
    /// PRD §12 / M7: total bonded stake per validator fingerprint (`validators.entry(i).fingerprint`).
    pub consensus_stakes: BTreeMap<[u8; 32], u128>,
    /// Per-depositor shares so [`NativeCall::WithdrawConsensusStake`] cannot drain others' bonds.
    pub consensus_stake_shares: BTreeMap<(Address, [u8; 32]), u128>,
    /// PRD §12.4: unbonding entries (slashable while pending); `release_ms == 0` until block finalize anchors.
    pub consensus_unbonding: Vec<ConsensusUnbondEntry>,
    /// Governance-committed hashes required before [`NativeCall::SlashConsensusStake`].
    pub slashing_evidence_hashes: BTreeSet<fractal_crypto::Hash256>,
    /// Replay protection for [`NativeCall::SlashConsensusStakeVerified`] (`keccak256(borsh(evidence))`).
    pub applied_misbehavior_evidence_hashes: BTreeSet<fractal_crypto::Hash256>,
    /// PRD §12.3: validator commission on block rewards (basis points, keyed by fingerprint).
    pub consensus_commission_bps: BTreeMap<[u8; 32], u16>,
    /// PRD §12.3: liquid reward balance per `(delegator, validator_fingerprint)`; withdraw via [`NativeCall::WithdrawRewards`].
    pub consensus_reward_credits: BTreeMap<(Address, [u8; 32]), u128>,
    /// PRD §12 / mainnet economics (governance may update via [`NativeCall::SetChainEconomics`]).
    pub chain_economics: ChainEconomicsParams,
    /// Permissionless validators: fingerprint → operator + BLS pubkey (`RegisterValidator`).
    pub validator_registry: BTreeMap<[u8; 32], ValidatorRegistryEntry>,
    /// Cumulative EVM base-fee destruction when [`ChainEconomicsParams::evm_base_fee_burn`] is enabled.
    pub protocol_burned_wei: u128,
    /// Legacy Phase-1 map (unused by §12.3); retained for stable [`State`] borsh layout.
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
    /// W6 / §10.4 + §17 `core::reputation`: last committed score (milli) per `(provider_id, tool_class_u8)`.
    pub wallet_reputation_milli: BTreeMap<([u8; 32], u8), u128>,
    /// `keccak256(borsh(ReputationLedgerSummary))` for the row above (audit / indexer replay).
    pub wallet_reputation_ledger_commitment: BTreeMap<([u8; 32], u8), fractal_crypto::Hash256>,
    /// Wallet capabilities must match this `CapabilitySignBody.chain_id` (§14.1).
    pub wallet_chain_id: u32,
    /// Block time for on-chain `CapabilityToken::verify_time` (0 = skip time check in tests).
    pub execution_timestamp_ms: u64,
    pub next_wallet_budget_id: u64,
    /// §14.2 on-chain budget accounts.
    pub wallet_budgets: BTreeMap<u64, crate::native_types::OnChainBudgetAccount>,
    /// §14.1 registered capabilities (`borsh(CapabilityToken)`).
    pub wallet_capabilities: BTreeMap<[u8; 32], Vec<u8>>,
    /// Address that registered / holds delegation rights for `cap_id`.
    pub wallet_cap_holders: BTreeMap<[u8; 32], Address>,
    /// Revoked capabilities (sparse map; §12.3 cascade via `cascade` flag).
    pub wallet_revocation_entries: BTreeMap<[u8; 32], crate::native_types::OnChainRevocationEntry>,
    /// Sparse Merkle trie root over revoked capabilities (`fractal_wallet::RevocationSet::root`).
    pub wallet_revocation_merkle_root: fractal_crypto::Hash256,
    /// §14.5 `docs/wallet.md`: monotonic task id (first task is `1`).
    pub next_wallet_task_id: u64,
    /// §14.5 on-chain task rows (`PostTask` … `FinalizeTask`).
    pub wallet_tasks: BTreeMap<u64, crate::native_types::OnChainTaskRow>,
    /// §29: when true, [`State::require_wallet_activity_allowed`] rejects new wallet activity.
    pub wallet_emergency_stop: bool,
    /// §14.1 scoped master-wallet stops, keyed by `(master_public_key, scope)`.
    pub wallet_scoped_emergency_stops:
        BTreeMap<([u8; 32], WalletEmergencyScopeV1), WalletScopedEmergencyStopRecordV1>,
    /// §16.3 wallet tool batches (`WalletBatchSettleV1`); distinct from M3 [`StoredBatch`].
    pub wallet_tool_batches:
        BTreeMap<fractal_crypto::Hash256, crate::native_types::StoredWalletToolBatch>,
    /// Settled wallet `ToolReceipt::receipt_id` → committing `batch_id`.
    pub wallet_settled_tool_receipt_ids: BTreeMap<[u8; 32], fractal_crypto::Hash256>,
    /// §14.4 provider identity registry.
    pub wallet_providers: BTreeMap<[u8; 32], OnChainProviderRow>,
    /// §14.4 per-class provider stake.
    pub wallet_provider_stakes: BTreeMap<([u8; 32], u8), OnChainProviderStakeRow>,
    pub next_wallet_provider_unstake_request_id: u64,
    /// §14.4 delayed provider stake withdrawals.
    pub wallet_provider_unstake_requests: BTreeMap<u64, OnChainProviderUnstakeRequest>,
    /// §10.3 / §14.4 provider slash history.
    pub wallet_provider_slashes: Vec<OnChainProviderSlashRecord>,
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
            consensus_stakes: BTreeMap::new(),
            consensus_stake_shares: BTreeMap::new(),
            consensus_unbonding: Vec::new(),
            slashing_evidence_hashes: BTreeSet::new(),
            applied_misbehavior_evidence_hashes: BTreeSet::new(),
            consensus_commission_bps: BTreeMap::new(),
            consensus_reward_credits: BTreeMap::new(),
            chain_economics: ChainEconomicsParams::testnet(),
            validator_registry: BTreeMap::new(),
            protocol_burned_wei: 0,
            delegated: BTreeMap::new(),
            evm_code: BTreeMap::new(),
            evm_storage: BTreeMap::new(),
            evm_tx_gas_used: BTreeMap::new(),
            evm_tx_logs: BTreeMap::new(),
            evm_tx_success: BTreeMap::new(),
            wallet_task_receipt_anchors: BTreeMap::new(),
            wallet_reputation_milli: BTreeMap::new(),
            wallet_reputation_ledger_commitment: BTreeMap::new(),
            wallet_chain_id: 41,
            execution_timestamp_ms: 0,
            next_wallet_budget_id: 1,
            wallet_budgets: BTreeMap::new(),
            wallet_capabilities: BTreeMap::new(),
            wallet_cap_holders: BTreeMap::new(),
            wallet_revocation_entries: BTreeMap::new(),
            wallet_revocation_merkle_root: default_wallet_revocation_merkle_root(),
            next_wallet_task_id: 1,
            wallet_tasks: BTreeMap::new(),
            wallet_emergency_stop: false,
            wallet_scoped_emergency_stops: BTreeMap::new(),
            wallet_tool_batches: BTreeMap::new(),
            wallet_settled_tool_receipt_ids: BTreeMap::new(),
            wallet_providers: BTreeMap::new(),
            wallet_provider_stakes: BTreeMap::new(),
            next_wallet_provider_unstake_request_id: 1,
            wallet_provider_unstake_requests: BTreeMap::new(),
            wallet_provider_slashes: Vec::new(),
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
    /// Total [`NativeCall::DepositConsensusStake`] for `validator_fingerprint` (PRD §12 / M7).
    #[must_use]
    pub fn consensus_stake_total_for_fingerprint(&self, validator_fingerprint: &[u8; 32]) -> u128 {
        self.consensus_stakes
            .get(validator_fingerprint)
            .copied()
            .unwrap_or(0)
    }

    /// Last committed wallet reputation score (milli) for `(provider_id, tool_class_u8)`, if any.
    #[must_use]
    pub fn wallet_reputation_score_milli(
        &self,
        provider_id: &[u8; 32],
        tool_class: u8,
    ) -> Option<u128> {
        self.wallet_reputation_milli
            .get(&(*provider_id, tool_class))
            .copied()
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

        match (&tx.vm, &tx.body) {
            (VmKind::Native, TxBody::Native(call)) => self.apply_native(signer, call),
            (VmKind::Evm, TxBody::Transfer { to, amount }) => {
                self.apply_transfer(signer, *to, *amount)
            }
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
                self.bump_nonce(signer);
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
        }
    }

    pub(crate) fn bump_nonce(&mut self, signer: Address) {
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

    /// Rejects mints, new budgets/funding, task lifecycle advances (except finalize), and anchors
    /// while [`State::wallet_emergency_stop`] is set. Revocation, budget close, finalize, and
    /// governance snapshot txs stay allowed.
    fn require_wallet_activity_allowed(&self) -> Result<(), ExecError> {
        if self.wallet_emergency_stop {
            return Err(ExecError::WalletEmergencyStopActive);
        }
        Ok(())
    }

    #[cfg(feature = "wallet")]
    fn apply_wallet_scoped_emergency_stop(
        &mut self,
        signer: Address,
        engage: bool,
        scope: &WalletEmergencyScopeV1,
        master_public_key: &[u8; 32],
        master_sig: &[u8; 64],
        bump_nonce: bool,
    ) -> Result<(), ExecError> {
        use ed25519_dalek::{Signature, Verifier, VerifyingKey};

        let body = crate::native_types::WalletScopedEmergencyStopSignBodyV1 {
            chain_id: self.wallet_chain_id,
            engage,
            scope: scope.clone(),
        };
        let msg = borsh::to_vec(&body).map_err(|_| ExecError::InvalidShape)?;
        let vk =
            VerifyingKey::from_bytes(master_public_key).map_err(|_| ExecError::BadSignature)?;
        let sig = Signature::from_bytes(master_sig);
        vk.verify(&msg, &sig).map_err(|_| ExecError::BadSignature)?;

        let key = (*master_public_key, scope.clone());
        if engage {
            self.wallet_scoped_emergency_stops.insert(
                key,
                WalletScopedEmergencyStopRecordV1 {
                    master_public_key: *master_public_key,
                    scope: scope.clone(),
                    engaged_at_ms: self.execution_timestamp_ms,
                },
            );
        } else {
            self.wallet_scoped_emergency_stops.remove(&key);
        }
        if bump_nonce {
            self.bump_nonce(signer);
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
            let from_acc = self.accounts.get_mut(&from).expect("from exists");
            from_acc.balance -= amount;
        }
        self.accounts
            .entry(to)
            .or_insert(Account {
                nonce: 0,
                balance: 0,
            })
            .balance += amount;
        self.bump_nonce(from);
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
            NativeCall::SuspendAgent {
                agent_id,
                reason: _,
            } => {
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
                let d = self
                    .disputes
                    .get_mut(dispute_id)
                    .ok_or(ExecError::NotFound)?;
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
                self.accounts
                    .entry(signer)
                    .or_insert(Account {
                        nonce: 0,
                        balance: 0,
                    })
                    .balance += amount;
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
            NativeCall::Delegate {
                validator_fingerprint,
                amount,
            } => {
                crate::consensus_stake::deposit_consensus_stake(
                    self,
                    signer,
                    *validator_fingerprint,
                    *amount,
                )?;
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WithdrawRewards {
                validator_fingerprint,
            } => {
                crate::consensus_stake::withdraw_consensus_rewards(
                    self,
                    signer,
                    *validator_fingerprint,
                )?;
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WalletTaskReceiptAnchorV1 {
                commitment,
                receipt_witness,
            } => {
                self.require_wallet_activity_allowed()?;
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
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::DepositConsensusStake {
                validator_fingerprint,
                amount,
            } => {
                crate::consensus_stake::deposit_consensus_stake(
                    self,
                    signer,
                    *validator_fingerprint,
                    *amount,
                )?;
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WithdrawConsensusStake {
                validator_fingerprint,
                amount,
            } => {
                if *amount == 0 {
                    return Err(ExecError::InvalidShape);
                }
                let key = (signer, *validator_fingerprint);
                let share = self
                    .consensus_stake_shares
                    .get_mut(&key)
                    .ok_or(ExecError::NotFound)?;
                if *share < *amount {
                    return Err(ExecError::InsufficientBalance);
                }
                *share -= amount;
                if *share == 0 {
                    self.consensus_stake_shares.remove(&key);
                }
                let tot = self
                    .consensus_stakes
                    .get_mut(validator_fingerprint)
                    .ok_or(ExecError::NotFound)?;
                *tot = tot
                    .checked_sub(*amount)
                    .ok_or(ExecError::InsufficientBalance)?;
                if *tot == 0 {
                    self.consensus_stakes.remove(validator_fingerprint);
                }
                self.consensus_unbonding.push(ConsensusUnbondEntry {
                    owner: signer,
                    validator_fingerprint: *validator_fingerprint,
                    amount: *amount,
                    release_ms: 0,
                });
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::CommitSlashingEvidence { evidence_hash } => {
                self.require_governance(signer)?;
                self.slashing_evidence_hashes.insert(*evidence_hash);
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::SlashConsensusStake {
                validator_fingerprint,
                evidence_hash,
            } => {
                self.require_governance(signer)?;
                if !self.slashing_evidence_hashes.remove(evidence_hash) {
                    return Err(ExecError::MissingSlashingEvidence);
                }
                crate::consensus_stake::slash_consensus_stake(self, *validator_fingerprint);
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::SlashConsensusStakeVerified {
                validator_fingerprint,
                evidence_borsh,
            } => {
                let evidence_hash =
                    crate::consensus_misbehavior::misbehavior_evidence_hash(evidence_borsh)?;
                if !self
                    .applied_misbehavior_evidence_hashes
                    .insert(evidence_hash)
                {
                    return Err(ExecError::DuplicateMisbehaviorEvidence);
                }
                crate::consensus_misbehavior::verify_slashing_evidence_borsh(
                    self,
                    validator_fingerprint,
                    evidence_borsh,
                )?;
                crate::consensus_stake::slash_consensus_stake(self, *validator_fingerprint);
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::SetValidatorCommission {
                validator_fingerprint,
                commission_bps,
            } => {
                crate::consensus_stake::set_validator_commission(
                    self,
                    signer,
                    *validator_fingerprint,
                    *commission_bps,
                )?;
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::RegisterValidator {
                validator_fingerprint,
                bls_pubkey,
            } => {
                crate::consensus_stake::register_validator(
                    self,
                    signer,
                    *validator_fingerprint,
                    *bls_pubkey,
                )?;
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::Redelegate {
                from_validator_fingerprint,
                to_validator_fingerprint,
                amount,
            } => {
                crate::consensus_stake::redelegate_consensus_stake(
                    self,
                    signer,
                    *from_validator_fingerprint,
                    *to_validator_fingerprint,
                    *amount,
                )?;
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::SetChainEconomics {
                min_validator_stake_wei,
                unbonding_period_ms,
                permissionless_validator_entry,
                evm_base_fee_burn,
            } => {
                self.require_governance(signer)?;
                self.chain_economics = ChainEconomicsParams {
                    version: ChainEconomicsParams::VERSION,
                    min_validator_stake_wei: *min_validator_stake_wei,
                    unbonding_period_ms: *unbonding_period_ms,
                    permissionless_validator_entry: *permissionless_validator_entry,
                    evm_base_fee_burn: *evm_base_fee_burn,
                };
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WalletReputationSnapshotV1 {
                provider_id,
                tool_class,
                summary_borsh,
            } => {
                self.require_governance(signer)?;
                #[cfg(feature = "wallet")]
                {
                    let summary: fractal_wallet::ReputationLedgerSummary =
                        borsh::from_slice(summary_borsh).map_err(|_| ExecError::InvalidShape)?;
                    let tc = fractal_wallet::ToolClass::from_discriminant(*tool_class)
                        .ok_or(ExecError::InvalidShape)?;
                    if summary.tool_class != tc {
                        return Err(ExecError::InvalidShape);
                    }
                    let score = fractal_wallet::compute_reputation_score_milli(
                        &summary,
                        &fractal_wallet::ReputationParams::default(),
                    );
                    let key = (*provider_id, *tool_class);
                    let commitment = keccak256(summary_borsh);
                    self.wallet_reputation_milli.insert(key, score);
                    self.wallet_reputation_ledger_commitment
                        .insert(key, commitment);
                }
                #[cfg(not(feature = "wallet"))]
                {
                    return Err(ExecError::WalletFeatureDisabled);
                }
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WalletEmergencyStopV1 { engage } => {
                self.require_governance(signer)?;
                self.wallet_emergency_stop = *engage;
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WalletMintCapabilityV1 {
                parent_cap_id,
                child_token_borsh,
                budget_seed,
                revocation_proof_borsh,
            } => {
                self.require_wallet_activity_allowed()?;
                #[cfg(feature = "wallet")]
                {
                    crate::wallet_native::mint_capability(
                        self,
                        signer,
                        *parent_cap_id,
                        child_token_borsh.clone(),
                        budget_seed.clone(),
                        revocation_proof_borsh.clone(),
                    )?;
                }
                #[cfg(not(feature = "wallet"))]
                {
                    let _ = (
                        parent_cap_id,
                        child_token_borsh,
                        budget_seed,
                        revocation_proof_borsh,
                    );
                    return Err(ExecError::WalletFeatureDisabled);
                }
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WalletCreateBudgetAccountV1 {
                parent,
                initial_deposit,
            } => {
                self.require_wallet_activity_allowed()?;
                #[cfg(feature = "wallet")]
                {
                    crate::wallet_native::create_budget_account(
                        self,
                        signer,
                        *parent,
                        *initial_deposit,
                    )?;
                }
                #[cfg(not(feature = "wallet"))]
                {
                    let _ = (parent, initial_deposit);
                    return Err(ExecError::WalletFeatureDisabled);
                }
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WalletFundBudgetAccountV1 {
                budget,
                amount,
                source_budget,
            } => {
                self.require_wallet_activity_allowed()?;
                #[cfg(feature = "wallet")]
                {
                    crate::wallet_native::fund_budget_account(
                        self,
                        signer,
                        *budget,
                        *amount,
                        *source_budget,
                    )?;
                }
                #[cfg(not(feature = "wallet"))]
                {
                    let _ = (budget, amount, source_budget);
                    return Err(ExecError::WalletFeatureDisabled);
                }
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WalletCloseBudgetAccountV1 { budget } => {
                crate::wallet_native::close_budget_account(self, signer, *budget)?;
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WalletRevokeCapabilityV1 {
                cap_id,
                reason_code,
                cascade,
                issuer_sig,
            } => {
                #[cfg(feature = "wallet")]
                {
                    crate::wallet_native::revoke_capability(
                        self,
                        *cap_id,
                        *reason_code,
                        *cascade,
                        *issuer_sig,
                    )?;
                }
                #[cfg(not(feature = "wallet"))]
                {
                    let _ = (cap_id, reason_code, cascade, issuer_sig);
                    return Err(ExecError::WalletFeatureDisabled);
                }
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WalletPostTaskV1 {
                metadata_uri,
                bounty_budget,
                tool_budget,
                verifier_budget,
            } => {
                self.require_wallet_activity_allowed()?;
                let total = bounty_budget
                    .checked_add(*tool_budget)
                    .and_then(|x| x.checked_add(*verifier_budget))
                    .ok_or(ExecError::GasOverflow)?;
                {
                    let acc = self
                        .accounts
                        .get_mut(&signer)
                        .ok_or(ExecError::UnknownSigner)?;
                    if acc.balance < total {
                        return Err(ExecError::InsufficientBalance);
                    }
                    acc.balance -= total;
                }
                let id = self.next_wallet_task_id;
                self.next_wallet_task_id = self.next_wallet_task_id.saturating_add(1);
                let ts = self.execution_timestamp_ms;
                self.wallet_tasks.insert(
                    id,
                    crate::native_types::OnChainTaskRow {
                        owner: signer,
                        metadata_uri: metadata_uri.clone(),
                        escrow_wei: total,
                        status: crate::native_types::WALLET_TASK_POSTED,
                        posted_at_ms: ts,
                        agent_session: None,
                        checkout_expiry_ms: 0,
                        checkout_signer: None,
                        artifact_pointer: String::new(),
                        tool_receipt_root: [0u8; 32],
                        verifier_sig: None,
                        verifier_score: 0,
                        renew_evidence_uri: String::new(),
                    },
                );
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WalletCheckoutTaskV1 {
                task_id,
                agent_session,
                expiry_ms,
            } => {
                self.require_wallet_activity_allowed()?;
                let row = self
                    .wallet_tasks
                    .get_mut(task_id)
                    .ok_or(ExecError::WalletTaskNotFound)?;
                if row.status != crate::native_types::WALLET_TASK_POSTED {
                    return Err(ExecError::WalletTaskState);
                }
                row.status = crate::native_types::WALLET_TASK_CHECKED_OUT;
                row.agent_session = Some(*agent_session);
                row.checkout_signer = Some(signer);
                row.checkout_expiry_ms = *expiry_ms;
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WalletRenewCheckoutV1 {
                task_id,
                evidence_uri,
                new_expiry_ms,
            } => {
                self.require_wallet_activity_allowed()?;
                let row = self
                    .wallet_tasks
                    .get_mut(task_id)
                    .ok_or(ExecError::WalletTaskNotFound)?;
                if row.status != crate::native_types::WALLET_TASK_CHECKED_OUT {
                    return Err(ExecError::WalletTaskState);
                }
                if row.checkout_signer != Some(signer) {
                    return Err(ExecError::NotAuthorized);
                }
                if *new_expiry_ms < row.checkout_expiry_ms {
                    return Err(ExecError::WalletTaskState);
                }
                row.renew_evidence_uri = evidence_uri.clone();
                row.checkout_expiry_ms = *new_expiry_ms;
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WalletSubmitTaskV1 {
                task_id,
                artifact_pointer,
                tool_receipt_root,
            } => {
                self.require_wallet_activity_allowed()?;
                let ts = self.execution_timestamp_ms;
                let row = self
                    .wallet_tasks
                    .get_mut(task_id)
                    .ok_or(ExecError::WalletTaskNotFound)?;
                if row.status != crate::native_types::WALLET_TASK_CHECKED_OUT {
                    return Err(ExecError::WalletTaskState);
                }
                if row.checkout_signer != Some(signer) {
                    return Err(ExecError::NotAuthorized);
                }
                if row.checkout_expiry_ms > 0 && ts > row.checkout_expiry_ms {
                    return Err(ExecError::WalletTaskState);
                }
                row.status = crate::native_types::WALLET_TASK_SUBMITTED;
                row.artifact_pointer = artifact_pointer.clone();
                row.tool_receipt_root = *tool_receipt_root;
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WalletVerifyTaskV1 {
                task_id,
                verifier_sig,
                score,
            } => {
                self.require_wallet_activity_allowed()?;
                let row = self
                    .wallet_tasks
                    .get_mut(task_id)
                    .ok_or(ExecError::WalletTaskNotFound)?;
                if row.status != crate::native_types::WALLET_TASK_SUBMITTED {
                    return Err(ExecError::WalletTaskState);
                }
                if row.checkout_signer == Some(signer) {
                    return Err(ExecError::NotAuthorized);
                }
                row.status = crate::native_types::WALLET_TASK_VERIFIED;
                row.verifier_sig = Some(*verifier_sig);
                row.verifier_score = *score;
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WalletFinalizeTaskV1 { task_id } => {
                let row = self
                    .wallet_tasks
                    .get_mut(task_id)
                    .ok_or(ExecError::WalletTaskNotFound)?;
                if row.status != crate::native_types::WALLET_TASK_VERIFIED {
                    return Err(ExecError::WalletTaskState);
                }
                let payee = row.checkout_signer.ok_or(ExecError::WalletTaskState)?;
                let escrow = row.escrow_wei;
                row.escrow_wei = 0;
                row.status = crate::native_types::WALLET_TASK_FINALIZED;
                let acc = self.accounts.entry(payee).or_insert(Account {
                    nonce: 0,
                    balance: 0,
                });
                acc.balance = acc.balance.saturating_add(escrow);
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            #[cfg(feature = "wallet")]
            NativeCall::WalletBatchSettleV1(p) => {
                crate::wallet_batch_settle::apply_wallet_batch_settle_v1(
                    self, signer, p, bump_nonce,
                )
            }
            #[cfg(not(feature = "wallet"))]
            NativeCall::WalletBatchSettleV1(_) => Err(ExecError::WalletFeatureDisabled),
            NativeCall::WalletRegisterProviderV1 { registration } => {
                self.require_wallet_activity_allowed()?;
                #[cfg(feature = "wallet")]
                {
                    crate::wallet_provider::register_provider(self, signer, registration.clone())?;
                }
                #[cfg(not(feature = "wallet"))]
                {
                    let _ = registration;
                    return Err(ExecError::WalletFeatureDisabled);
                }
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WalletStakeForClassV1 {
                provider_id,
                tool_class,
                amount,
            } => {
                self.require_wallet_activity_allowed()?;
                #[cfg(feature = "wallet")]
                {
                    crate::wallet_provider::stake_for_class(
                        self,
                        signer,
                        *provider_id,
                        *tool_class,
                        *amount,
                    )?;
                }
                #[cfg(not(feature = "wallet"))]
                {
                    let _ = (provider_id, tool_class, amount);
                    return Err(ExecError::WalletFeatureDisabled);
                }
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WalletProviderUnstakeRequestV1 {
                provider_id,
                tool_class,
                amount,
            } => {
                self.require_wallet_activity_allowed()?;
                #[cfg(feature = "wallet")]
                {
                    crate::wallet_provider::request_unstake(
                        self,
                        signer,
                        *provider_id,
                        *tool_class,
                        *amount,
                    )?;
                }
                #[cfg(not(feature = "wallet"))]
                {
                    let _ = (provider_id, tool_class, amount);
                    return Err(ExecError::WalletFeatureDisabled);
                }
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WalletProviderUnstakeFinalizeV1 { request_id } => {
                #[cfg(feature = "wallet")]
                {
                    crate::wallet_provider::finalize_unstake(self, signer, *request_id)?;
                }
                #[cfg(not(feature = "wallet"))]
                {
                    let _ = request_id;
                    return Err(ExecError::WalletFeatureDisabled);
                }
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WalletSlashProviderV1 { provider_id, slash } => {
                self.require_governance(signer)?;
                #[cfg(feature = "wallet")]
                {
                    crate::wallet_provider::slash_provider(self, *provider_id, slash.clone())?;
                }
                #[cfg(not(feature = "wallet"))]
                {
                    let _ = (provider_id, slash);
                    return Err(ExecError::WalletFeatureDisabled);
                }
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WalletUpdateProviderV1 {
                provider_id,
                metadata_uri,
                endpoint_uri,
                active,
            } => {
                self.require_wallet_activity_allowed()?;
                #[cfg(feature = "wallet")]
                {
                    crate::wallet_provider::update_provider(
                        self,
                        signer,
                        *provider_id,
                        metadata_uri.clone(),
                        endpoint_uri.clone(),
                        *active,
                    )?;
                }
                #[cfg(not(feature = "wallet"))]
                {
                    let _ = (provider_id, metadata_uri, endpoint_uri, active);
                    return Err(ExecError::WalletFeatureDisabled);
                }
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WalletDeregisterProviderV1 { provider_id } => {
                #[cfg(feature = "wallet")]
                {
                    crate::wallet_provider::deregister_provider(self, signer, *provider_id)?;
                }
                #[cfg(not(feature = "wallet"))]
                {
                    let _ = provider_id;
                    return Err(ExecError::WalletFeatureDisabled);
                }
                if bump_nonce {
                    self.bump_nonce(signer);
                }
                Ok(())
            }
            NativeCall::WalletScopedEmergencyStopV1 {
                engage,
                scope,
                master_public_key,
                master_sig,
            } => {
                #[cfg(feature = "wallet")]
                {
                    self.apply_wallet_scoped_emergency_stop(
                        signer,
                        *engage,
                        scope,
                        master_public_key,
                        master_sig,
                        bump_nonce,
                    )
                }
                #[cfg(not(feature = "wallet"))]
                {
                    let _ = (engage, scope, master_public_key, master_sig);
                    Err(ExecError::WalletFeatureDisabled)
                }
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
            .or_insert(Account {
                nonce: 0,
                balance: 0,
            })
            .balance += amount;
        if bump_nonce {
            self.bump_nonce(signer);
        }
        Ok(())
    }
}
