//! Singleton dev node: 500 ms block cadence + JSON-RPC + libp2p/QUIC sync (`docs/prd.md` §18 M2).

mod eth_signed;
pub mod p2p;

pub use fractal_consensus::ValidatorSet;

use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use borsh::BorshDeserialize;
use fractal_consensus::{
    da_encoded_bytes, da_root, execute_and_build_block, expected_parent_qc_for_parent_header,
    genesis_parent_qc, hash_qc, header_hash, next_parent_qc_hash_after_commit, ordered_tx_root,
    reconstruct_da_payload, Block, BlockValidityProof, DaShare, FormedQc, ProofVerifyError,
    RecordVoteOutcome, Vote, VotePool,
};
use fractal_core::{Address, EvmEngine, State, Transaction};
use fractal_crypto::hash::keccak256;
use fractal_crypto::BlsSecretKey;
use fractal_mempool::{next_base_fee, BaseFeeParams, Mempool, PooledTx};
use fractal_rpc::{
    logs_bloom_256, make_rpc_log, ChainInteraction, RpcChainConfig, RpcDaMetrics,
    RpcMempoolLaneMetrics, RpcProofMetrics, RpcProofRejectionMetric,
};
use fractal_storage::{ProofFinalityStore, StoredProofFinalityRecord};
use libp2p::multiaddr::Protocol;
use libp2p::Multiaddr;
use thiserror::Error;
use tokio::sync::Mutex;

pub type NodeHandle = Arc<Mutex<NodeInner>>;

#[derive(Debug, Error)]
pub enum SyncApplyError {
    #[error("chain id mismatch")]
    ChainId,
    #[error("expected block height {expected}, got {got}")]
    Height { expected: u64, got: u64 },
    #[error("parent hash does not match local head")]
    ParentHash,
    #[error("parent_qc_hash does not match expected HotStuff-2 singleton QC chain")]
    ParentQcHash,
    #[error("block proposer does not match validator set leader for this view")]
    InvalidProposer,
    #[error("state root mismatch after replay")]
    StateRoot,
    #[error("tx root mismatch after replay")]
    TxRoot,
    #[error("gas used mismatch: header {header}, replay {replay}")]
    GasUsedMismatch { header: u64, replay: u64 },
    #[error("synced block eth_signed_raw length does not match transactions")]
    BlockEthRawLayout,
    #[error("synced block data availability sidecar does not match header")]
    DataAvailability,
    #[error(transparent)]
    Exec(#[from] fractal_core::ExecError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum ProofFinalityError {
    #[error("proved block not found")]
    BlockNotFound,
    #[error(transparent)]
    Verify(#[from] ProofVerifyError),
    #[error("proof-finality store: {0}")]
    Store(#[from] fractal_storage::ProofFinalityStoreError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockFinality {
    Soft,
    Proof,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SettlementAccessError {
    #[error("block not found")]
    BlockNotFound,
    #[error("block is not proof-final")]
    NotProofFinal,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DaMetrics {
    pub committed_blocks: u64,
    pub committed_original_bytes: u64,
    pub committed_encoded_bytes: u64,
    pub committed_da_gas: u64,
    pub da_fee_revenue: u128,
    pub sampling_success: u64,
    pub sampling_failure: u64,
    pub reconstruction_success: u64,
    pub reconstruction_failure: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChainConfig {
    pub proof_required_settlement: bool,
}

impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            proof_required_settlement: false,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProofMetrics {
    pub proofs_accepted: u64,
    pub proofs_rejected: u64,
    pub latest_proof_latency_ms: u64,
    pub total_proof_latency_ms: u128,
    pub proof_final_height: u64,
    pub latest_rejection_reason: Option<String>,
    pub rejection_reasons: BTreeMap<String, u64>,
}

const GENESIS_TAG: &[u8] = b"FRACTALCHAIN_GENESIS_V0";

/// Hardhat / Anvil default signer #0 — re-exported from `fractal_core::devnet_accounts`.
pub use fractal_core::HARDHAT_DEFAULT_SIGNER_0;
/// Hardhat default signer #1 (M5 MVP agent for `CLAIM_PAYOUT` demos).
pub use fractal_core::HARDHAT_DEFAULT_SIGNER_1;

pub fn genesis_parent_hash() -> fractal_crypto::Hash256 {
    keccak256(GENESIS_TAG)
}

fn devnet_validator_set_from_env() -> ValidatorSet {
    match std::env::var("FRACTAL_VALIDATOR_SET")
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Ok("7") | Ok("bft7") => ValidatorSet::phase2_bft7_fixture(),
        _ => ValidatorSet::phase1_singleton(),
    }
}

/// Reads `FRACTAL_VALIDATOR_INDEX` (`docs/prd.md` §7 M7-c). Defaults to `0`;
/// clamped into `[0, validators.len())` so a stale env var on a singleton
/// devnet never silently disables block production.
fn devnet_validator_index_from_env(validators: &ValidatorSet) -> usize {
    let raw = std::env::var("FRACTAL_VALIDATOR_INDEX").unwrap_or_default();
    let parsed: usize = raw.trim().parse().unwrap_or(0);
    let n = validators.len().max(1);
    if parsed >= n {
        eprintln!(
            "fractal-node: FRACTAL_VALIDATOR_INDEX={raw} ≥ validator_set_size={n}; clamping to 0"
        );
        0
    } else {
        parsed
    }
}

/// Reads `FRACTAL_VALIDATOR_SECRET_HEX` (`docs/prd.md` §7.3 / M7-d).
///
/// Returns the operator-supplied BLS signing key if provided. If the env var is
/// missing or empty, falls back to the deterministic dev key for
/// `(validators, validator_index)` so single-binary devnets keep working
/// without configuration.
///
/// A malformed env var (bad hex / wrong length / not on-curve) is logged and the
/// dev fallback is used so a typo cannot silently take a validator offline.
/// Returns `None` only when no dev key is available (e.g. operator-provisioned
/// sets that don't expose `dev_bls_secret`); the caller then disables vote
/// signing on this node.
fn devnet_validator_secret_from_env(
    validators: &ValidatorSet,
    validator_index: usize,
) -> Option<BlsSecretKey> {
    if let Ok(raw) = std::env::var("FRACTAL_VALIDATOR_SECRET_HEX") {
        let trimmed = raw.trim().trim_start_matches("0x");
        if !trimmed.is_empty() {
            match hex::decode(trimmed) {
                Ok(bytes) if bytes.len() == 32 => {
                    let mut arr = [0u8; 32];
                    arr.copy_from_slice(&bytes);
                    match BlsSecretKey::from_bytes(&arr) {
                        Ok(sk) => {
                            let pk = sk.public_key();
                            if let Some(expected) = validators.bls_pubkey(validator_index) {
                                if &pk != expected {
                                    eprintln!(
                                        "fractal-node: FRACTAL_VALIDATOR_SECRET_HEX pubkey does NOT match validators[{validator_index}].bls_pubkey — votes from this node will be rejected by peers"
                                    );
                                }
                            }
                            return Some(sk);
                        }
                        Err(e) => eprintln!(
                            "fractal-node: FRACTAL_VALIDATOR_SECRET_HEX rejected by blst ({e}); using dev fallback"
                        ),
                    }
                }
                Ok(bytes) => eprintln!(
                    "fractal-node: FRACTAL_VALIDATOR_SECRET_HEX must be 32 bytes (got {}); using dev fallback",
                    bytes.len()
                ),
                Err(e) => eprintln!(
                    "fractal-node: FRACTAL_VALIDATOR_SECRET_HEX hex decode error ({e}); using dev fallback"
                ),
            }
        }
    }
    validators.dev_bls_secret(validator_index)
}

fn proof_required_settlement_from_env() -> bool {
    match std::env::var("FRACTAL_PROOF_REQUIRED_SETTLEMENT") {
        Ok(raw) => matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on" | "proof" | "required"
        ),
        Err(_) => false,
    }
}

fn proof_finality_store_from_env() -> Option<std::path::PathBuf> {
    std::env::var_os("FRACTAL_PROOF_FINALITY_STORE")
        .filter(|raw| !raw.is_empty())
        .map(std::path::PathBuf::from)
}

pub struct NodeInner {
    pub chain_id: u64,
    pub height: u64,
    pub view: u64,
    pub head_hash: fractal_crypto::Hash256,
    pub parent_qc_hash: fractal_crypto::Hash256,
    /// Static validator set (`docs/prd.md` §7.2). Block leader = `validators.expected_proposer(view)`.
    pub validators: ValidatorSet,
    /// This node's index inside `validators` (`docs/prd.md` §7 M7-c). `producer_loop`
    /// only proposes when `validators.is_proposer_for_view(view, validator_index)`.
    /// Defaults to `0`; set via `FRACTAL_VALIDATOR_INDEX` in `run_dev`/`run_follower`.
    pub validator_index: usize,
    /// This node's BLS signing key (`docs/prd.md` §7.3 / M7-d). `None` means
    /// the node cannot sign votes (e.g. read-only follower with no operator-supplied
    /// secret and no dev key available). Set from `FRACTAL_VALIDATOR_SECRET_HEX`
    /// with a deterministic dev fallback in `run_dev`/`run_follower`.
    pub validator_secret: Option<BlsSecretKey>,
    /// HotStuff-2 vote pool (`docs/prd.md` §7.3 / M7-d-4): records each peer's
    /// `Vote` after BLS verification and aggregates into a [`FormedQc`] once
    /// `validators.quorum_threshold()` is met.
    pub vote_pool: VotePool,
    /// When set, serialized [`Vote`]s are sent to the libp2p task for gossipsub publish.
    pub vote_sink: Option<tokio::sync::mpsc::UnboundedSender<Vec<u8>>>,
    pub state: State,
    pub mempool: Mempool,
    pub base_fee: u128,
    pub gas_limit: u64,
    pub fee_params: BaseFeeParams,
    pub blocks: Vec<Block>,
    pub pending_txs: BTreeMap<fractal_crypto::Hash256, Transaction>,
    pub mined_txs: BTreeMap<fractal_crypto::Hash256, (u64, fractal_crypto::Hash256, u32)>,
    /// Signed EIP-1559 bytes keyed by `keccak256(raw)` (RPC tx hash).
    pub eth_signed_raw: BTreeMap<fractal_crypto::Hash256, Vec<u8>>,
    /// When RPC hash differs from `keccak(borsh(tx))` (EVM state keys), map RPC → internal.
    pub eth_rpc_to_internal_tx_hash: BTreeMap<fractal_crypto::Hash256, fractal_crypto::Hash256>,
    /// Inverse of the above for log `transactionHash` fields (`eth_getLogs`).
    pub eth_internal_to_rpc_tx_hash: BTreeMap<fractal_crypto::Hash256, fractal_crypto::Hash256>,
    /// Blocks that have passed the validity-proof gate. Committee commits are soft-final;
    /// entries here are proof-final for settlement/bridging purposes.
    pub proof_finalized_blocks: BTreeMap<fractal_crypto::Hash256, BlockValidityProof>,
    pub proof_finality_store: Option<ProofFinalityStore>,
    pub chain_config: ChainConfig,
    pub da_metrics: DaMetrics,
    pub proof_metrics: ProofMetrics,
}

impl NodeInner {
    /// Default local/test node: Phase-1 singleton validator set (deterministic; ignores process env).
    pub fn devnet() -> Self {
        Self::devnet_with_validators(ValidatorSet::phase1_singleton())
    }

    /// Devnet with an explicit validator set + this node's `validator_index = 0`.
    /// For multi-index tests, use [`devnet_with_validator_index`].
    pub fn devnet_with_validators(validators: ValidatorSet) -> Self {
        Self::devnet_with_validator_index(validators, 0)
    }

    /// Devnet with an explicit validator set and this node's `validator_index`
    /// (`docs/prd.md` §7 M7-c). Defaults `validator_secret` to the dev fallback
    /// for `(validators, validator_index)`; for tests that need a specific secret
    /// (or want to assert "no signing key"), use [`devnet_with_validator_secret`].
    pub fn devnet_with_validator_index(validators: ValidatorSet, validator_index: usize) -> Self {
        let secret = validators.dev_bls_secret(validator_index);
        Self::devnet_with_validator_secret(validators, validator_index, secret)
    }

    /// Devnet with explicit validator set, index, and BLS secret.
    pub fn devnet_with_validator_secret(
        validators: ValidatorSet,
        validator_index: usize,
        validator_secret: Option<BlsSecretKey>,
    ) -> Self {
        let mut state = State::default();
        state.accounts.insert(
            HARDHAT_DEFAULT_SIGNER_0,
            fractal_core::Account {
                nonce: 0,
                balance: 1_000_000_000_000_000_000_000_000u128,
            },
        );
        state.accounts.insert(
            HARDHAT_DEFAULT_SIGNER_1,
            fractal_core::Account {
                nonce: 0,
                balance: 0,
            },
        );
        state.accounts.insert(
            fractal_core::DEVNET_FAUCET_TREASURY,
            fractal_core::Account {
                nonce: 0,
                balance: 1_000_000_000_000_000_000_000_000u128,
            },
        );
        Self {
            chain_id: 41,
            height: 0,
            view: 0,
            head_hash: genesis_parent_hash(),
            parent_qc_hash: hash_qc(&genesis_parent_qc()).expect("genesis_parent_qc borsh"),
            validators,
            validator_index,
            validator_secret,
            vote_pool: VotePool::new(),
            vote_sink: None,
            state,
            mempool: Mempool::default(),
            base_fee: 1,
            gas_limit: 60_000_000,
            fee_params: BaseFeeParams::default(),
            blocks: Vec::new(),
            pending_txs: BTreeMap::new(),
            mined_txs: BTreeMap::new(),
            eth_signed_raw: BTreeMap::new(),
            eth_rpc_to_internal_tx_hash: BTreeMap::new(),
            eth_internal_to_rpc_tx_hash: BTreeMap::new(),
            proof_finalized_blocks: BTreeMap::new(),
            proof_finality_store: None,
            chain_config: ChainConfig::default(),
            da_metrics: DaMetrics::default(),
            proof_metrics: ProofMetrics::default(),
        }
    }

    /// Whether this node should propose for `view` (`docs/prd.md` §7 M7-c).
    /// In single-validator (Phase 1) setups, always `true` for `validator_index = 0`.
    #[must_use]
    pub fn is_my_turn(&self, view: u64) -> bool {
        self.validators
            .is_proposer_for_view(view, self.validator_index)
    }

    /// Wire gossip vote publishing (`docs/prd.md` §18 M7-d-5). When `None`, votes stay local only.
    pub fn set_vote_sink(&mut self, sink: Option<tokio::sync::mpsc::UnboundedSender<Vec<u8>>>) {
        self.vote_sink = sink;
    }

    pub fn set_proof_required_settlement(&mut self, required: bool) {
        self.chain_config.proof_required_settlement = required;
    }

    #[must_use]
    pub fn settlement_requires_proof(&self) -> bool {
        self.chain_config.proof_required_settlement
    }

    pub fn set_proof_finality_store(
        &mut self,
        store: ProofFinalityStore,
    ) -> Result<(), ProofFinalityError> {
        self.restore_proof_finality_records(&store)?;
        self.proof_finality_store = Some(store);
        Ok(())
    }

    fn restore_proof_finality_records(
        &mut self,
        store: &ProofFinalityStore,
    ) -> Result<(), ProofFinalityError> {
        for record in store.load_records()? {
            self.proof_metrics.proof_final_height =
                self.proof_metrics.proof_final_height.max(record.height);
            self.proof_metrics.proofs_accepted =
                self.proof_metrics.proofs_accepted.saturating_add(1);
            self.proof_finalized_blocks
                .insert(record.block_hash, record.proof);
        }
        Ok(())
    }

    /// Record this validator's vote for `committed` in the local pool and enqueue it for
    /// gossipsub when [`Self::vote_sink`] is set.
    pub fn forward_vote_after_commit(&mut self, committed: &Block) {
        let Ok(hh) = header_hash(&committed.header) else {
            return;
        };
        let Some(vote) = self.build_self_vote(committed.header.view, committed.header.height, hh)
        else {
            return;
        };
        let _ = self.record_vote(vote.clone());
        if let Some(ref tx) = self.vote_sink {
            if let Ok(bytes) = borsh::to_vec(&vote) {
                let _ = tx.send(bytes);
            }
        }
    }

    /// Build a [`Vote`] for the just-committed block at `(view, height, header_hash)`
    /// using this node's `validator_secret`. Returns `None` if the node has no
    /// signing key (e.g. read-only follower).
    pub fn build_self_vote(
        &self,
        view: u64,
        height: u64,
        header_hash: fractal_crypto::Hash256,
    ) -> Option<Vote> {
        let sk = self.validator_secret.as_ref()?;
        let body = fractal_consensus::VoteSignBody {
            view,
            height,
            header_hash,
        };
        Some(Vote::sign(body, self.validator_index as u32, sk))
    }

    /// Record `vote` into the local pool after BLS verification (`docs/prd.md`
    /// §7.3 / M7-d-4). Thin wrapper over [`VotePool::record`] using this node's
    /// active `validators`.
    pub fn record_vote(&mut self, vote: Vote) -> RecordVoteOutcome {
        self.vote_pool.record(vote, &self.validators)
    }

    /// Attempt to form a QC for `(view, block_height, header_hash)` from the
    /// local vote pool. Returns `None` until `quorum_threshold` is reached.
    /// Wrapper over [`VotePool::try_form_qc`].
    pub fn try_form_qc(
        &self,
        view: u64,
        block_height: u64,
        header_hash: fractal_crypto::Hash256,
    ) -> Option<FormedQc> {
        self.vote_pool
            .try_form_qc(view, block_height, header_hash, &self.validators)
    }

    pub fn submit_validity_proof(
        &mut self,
        proof: BlockValidityProof,
    ) -> Result<(), ProofFinalityError> {
        let block = match self.blocks.iter().find(|b| {
            b.header.height == proof.height && header_hash(&b.header).ok() == Some(proof.block_hash)
        }) {
            Some(block) => block,
            None => {
                self.record_proof_rejection("block_not_found");
                return Err(ProofFinalityError::BlockNotFound);
            }
        };
        let block_timestamp_ms = block.header.timestamp_ms;
        let block_height = block.header.height;
        if let Err(e) = fractal_consensus::verify_block_validity_proof(block, &proof) {
            self.record_proof_rejection(&proof_rejection_reason(&e));
            return Err(e.into());
        }
        let block_hash = proof.block_hash;
        let accepted_at_ms = now_ms();
        self.proof_finalized_blocks
            .insert(block_hash, proof.clone());
        if let Some(store) = &self.proof_finality_store {
            store.put_record(StoredProofFinalityRecord {
                block_hash,
                height: block_height,
                accepted_at_ms,
                proof,
            })?;
        }
        self.record_proof_acceptance(block_height, block_timestamp_ms, accepted_at_ms);
        Ok(())
    }

    pub fn finality_for_block_hash(
        &self,
        block_hash: &fractal_crypto::Hash256,
    ) -> Option<BlockFinality> {
        if self.proof_finalized_blocks.contains_key(block_hash) {
            return Some(BlockFinality::Proof);
        }
        self.blocks
            .iter()
            .any(|b| header_hash(&b.header).ok().as_ref() == Some(block_hash))
            .then_some(BlockFinality::Soft)
    }

    pub fn settlement_finality_for_block_hash(
        &self,
        block_hash: &fractal_crypto::Hash256,
    ) -> Result<BlockFinality, SettlementAccessError> {
        let finality = self
            .finality_for_block_hash(block_hash)
            .ok_or(SettlementAccessError::BlockNotFound)?;
        if self.settlement_requires_proof() && finality != BlockFinality::Proof {
            return Err(SettlementAccessError::NotProofFinal);
        }
        Ok(finality)
    }

    pub fn da_shares_by_block_hash(
        &self,
        block_hash: &fractal_crypto::Hash256,
        indexes: &[u32],
    ) -> Option<Vec<DaShare>> {
        let block = self
            .blocks
            .iter()
            .find(|b| header_hash(&b.header).ok().as_ref() == Some(block_hash))?;
        let mut out = Vec::with_capacity(indexes.len());
        for index in indexes {
            let share = block.da_sidecar.shares.get(*index as usize)?;
            if share.index != *index {
                return None;
            }
            out.push(share.clone());
        }
        Some(out)
    }

    pub fn da_sample_indexes_for_block(block: &Block, sample_count: usize, seed: u64) -> Vec<u32> {
        if block.da_sidecar.shares.is_empty() || sample_count == 0 {
            return Vec::new();
        }
        let mut indexes = Vec::with_capacity(sample_count);
        let mut state = seed
            ^ u64::from_le_bytes(block.header.da_root[..8].try_into().unwrap_or([0u8; 8]))
            ^ block.header.height;
        for _ in 0..sample_count {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            indexes.push((state as usize % block.da_sidecar.shares.len()) as u32);
        }
        indexes
    }

    pub fn verify_da_sampled_shares(
        block: &Block,
        indexes: &[u32],
        shares: &[DaShare],
    ) -> Result<(), fractal_consensus::DaVerifyError> {
        if indexes.len() != shares.len() {
            return Err(fractal_consensus::DaVerifyError::SampleMissing);
        }
        if da_root(&block.da_sidecar) != block.header.da_root {
            return Err(fractal_consensus::DaVerifyError::Root);
        }
        for (expected_index, share) in indexes.iter().zip(shares.iter()) {
            if share.index != *expected_index {
                return Err(fractal_consensus::DaVerifyError::ShareIndex);
            }
            let local = block
                .da_sidecar
                .shares
                .get(*expected_index as usize)
                .ok_or(fractal_consensus::DaVerifyError::SampleMissing)?;
            if share != local {
                return Err(fractal_consensus::DaVerifyError::ShareCommitment);
            }
            let expected = fractal_consensus::da_share_commitment(
                share.namespace,
                share.index,
                share.is_parity,
                &share.data,
            );
            if share.commitment != expected {
                return Err(fractal_consensus::DaVerifyError::ShareCommitment);
            }
        }
        Ok(())
    }

    pub fn record_da_sampling_result(&mut self, ok: bool) {
        if ok {
            self.da_metrics.sampling_success = self.da_metrics.sampling_success.saturating_add(1);
        } else {
            self.da_metrics.sampling_failure = self.da_metrics.sampling_failure.saturating_add(1);
        }
    }

    pub fn record_da_reconstruction_result(&mut self, ok: bool) {
        if ok {
            self.da_metrics.reconstruction_success =
                self.da_metrics.reconstruction_success.saturating_add(1);
        } else {
            self.da_metrics.reconstruction_failure =
                self.da_metrics.reconstruction_failure.saturating_add(1);
        }
    }

    fn record_proof_acceptance(
        &mut self,
        height: u64,
        block_timestamp_ms: u64,
        accepted_at_ms: u64,
    ) {
        let latency_ms = accepted_at_ms.saturating_sub(block_timestamp_ms);
        self.proof_metrics.proofs_accepted = self.proof_metrics.proofs_accepted.saturating_add(1);
        self.proof_metrics.latest_proof_latency_ms = latency_ms;
        self.proof_metrics.total_proof_latency_ms = self
            .proof_metrics
            .total_proof_latency_ms
            .saturating_add(latency_ms as u128);
        self.proof_metrics.proof_final_height = self.proof_metrics.proof_final_height.max(height);
    }

    fn record_proof_rejection(&mut self, reason: &str) {
        self.proof_metrics.proofs_rejected = self.proof_metrics.proofs_rejected.saturating_add(1);
        self.proof_metrics.latest_rejection_reason = Some(reason.to_owned());
        *self
            .proof_metrics
            .rejection_reasons
            .entry(reason.to_owned())
            .or_insert(0) += 1;
    }

    fn record_committed_da_metrics(&mut self, block: &Block) {
        self.da_metrics.committed_blocks = self.da_metrics.committed_blocks.saturating_add(1);
        self.da_metrics.committed_original_bytes = self
            .da_metrics
            .committed_original_bytes
            .saturating_add(block.header.da_bytes);
        self.da_metrics.committed_encoded_bytes = self
            .da_metrics
            .committed_encoded_bytes
            .saturating_add(da_encoded_bytes(&block.da_sidecar));
        self.da_metrics.committed_da_gas = self
            .da_metrics
            .committed_da_gas
            .saturating_add(block.header.da_gas_used);
        self.da_metrics.da_fee_revenue = self
            .da_metrics
            .da_fee_revenue
            .saturating_add(block.header.da_fee_paid);
    }

    /// Replay txs and check roots against a received block (follower verification).
    pub fn apply_synced_block(&mut self, block: &Block) -> Result<(), SyncApplyError> {
        if block.header.chain_id != self.chain_id {
            return Err(SyncApplyError::ChainId);
        }
        if block.header.height != self.height + 1 {
            return Err(SyncApplyError::Height {
                expected: self.height + 1,
                got: block.header.height,
            });
        }
        if block.header.parent_hash != self.head_hash {
            return Err(SyncApplyError::ParentHash);
        }
        let expected_parent_qc = if self.height == 0 {
            hash_qc(&genesis_parent_qc())?
        } else {
            let parent_block = &self.blocks[(self.height - 1) as usize];
            expected_parent_qc_for_parent_header(&parent_block.header)?
        };
        if block.header.parent_qc_hash != expected_parent_qc {
            return Err(SyncApplyError::ParentQcHash);
        }
        let expected_proposer = self.validators.expected_proposer(block.header.view);
        if block.header.proposer != expected_proposer {
            return Err(SyncApplyError::InvalidProposer);
        }
        if block.eth_signed_raw.len() != block.transactions.len() {
            return Err(SyncApplyError::BlockEthRawLayout);
        }
        fractal_consensus::verify_da_sidecar(&block.header, &block.da_sidecar)
            .map_err(|_| SyncApplyError::DataAvailability)?;
        let reconstructed_da_payload = reconstruct_da_payload(&block.da_sidecar).map_err(|_| {
            self.record_da_reconstruction_result(false);
            SyncApplyError::DataAvailability
        })?;
        let expected_da_payload = borsh::to_vec(&block.transactions)?;
        if reconstructed_da_payload != expected_da_payload {
            self.record_da_reconstruction_result(false);
            return Err(SyncApplyError::DataAvailability);
        }
        self.record_da_reconstruction_result(true);
        let mut scratch = self.state.clone();
        let mut evm = fractal_evm::RevmEngine::default();
        let gas = fractal_core::apply_block_with_evm(&mut scratch, &block.transactions, &mut evm)?;
        if gas != block.header.gas_used {
            return Err(SyncApplyError::GasUsedMismatch {
                header: block.header.gas_used,
                replay: gas,
            });
        }
        let sr = fractal_core::state_root(&scratch)?;
        if sr != block.header.state_root {
            return Err(SyncApplyError::StateRoot);
        }
        let tr = ordered_tx_root(&block.transactions)?;
        if tr != block.header.tx_root {
            return Err(SyncApplyError::TxRoot);
        }
        self.state = scratch;
        self.height = block.header.height;
        let hh = header_hash(&block.header)?;
        self.head_hash = hh;
        self.parent_qc_hash = next_parent_qc_hash_after_commit(&block.header, hh)?;
        self.view = block.header.view.wrapping_add(1);
        self.base_fee = next_base_fee(self.base_fee, block.header.gas_used, &self.fee_params);
        self.blocks.push(block.clone());
        self.record_committed_da_metrics(block);
        self.sync_rpc_index_from_block(block);
        self.forward_vote_after_commit(block);
        Ok(())
    }

    /// Populate `mined_txs`, `eth_signed_raw`, and RPC hash maps from a committed block (producer
    /// after local mine; follower after `apply_synced_block` replay).
    fn sync_rpc_index_from_block(&mut self, block: &Block) {
        let hh = header_hash(&block.header).unwrap_or([0u8; 32]);
        for (i, tx) in block.transactions.iter().enumerate() {
            let Ok(borsh_raw) = borsh::to_vec(tx) else {
                continue;
            };
            let ih = keccak256(&borsh_raw);
            let rpc_h = if let Some(Some(eth_raw)) = block.eth_signed_raw.get(i) {
                let eh = keccak256(eth_raw);
                if eh != ih {
                    self.eth_rpc_to_internal_tx_hash.insert(eh, ih);
                    self.eth_internal_to_rpc_tx_hash.insert(ih, eh);
                }
                self.eth_signed_raw.insert(eh, eth_raw.clone());
                eh
            } else {
                ih
            };
            self.pending_txs.remove(&rpc_h);
            self.mined_txs
                .insert(rpc_h, (block.header.height, hh, i as u32));
        }
    }

    /// Sum of log counts for transactions before `tx_index` in `block_number`.
    fn log_index_base_in_block(&self, block_number: u64, tx_index: u32) -> u64 {
        let Some(idx) = block_number.checked_sub(1).map(|x| x as usize) else {
            return 0;
        };
        let Some(block) = self.blocks.get(idx) else {
            return 0;
        };
        let mut n = 0u64;
        for (i, tx) in block.transactions.iter().enumerate() {
            if i >= tx_index as usize {
                break;
            }
            let Ok(raw) = borsh::to_vec(tx) else {
                continue;
            };
            let th = keccak256(&raw);
            if let Some(ls) = self.state.evm_tx_logs.get(&th) {
                n += ls.len() as u64;
            }
        }
        n
    }

    fn internal_tx_hash_for_state(&self, rpc_hash: &[u8; 32]) -> fractal_crypto::Hash256 {
        self.eth_rpc_to_internal_tx_hash
            .get(rpc_hash)
            .copied()
            .unwrap_or(*rpc_hash)
    }
}

impl ChainInteraction for NodeInner {
    fn block_number(&self) -> u64 {
        self.height
    }

    fn chain_id(&self) -> u64 {
        self.chain_id
    }

    fn balance_of(&self, addr: &Address) -> u128 {
        self.state
            .accounts
            .get(addr)
            .map(|a| a.balance)
            .unwrap_or(0)
    }

    fn transaction_count(&self, addr: &Address) -> u64 {
        self.state.accounts.get(addr).map(|a| a.nonce).unwrap_or(0)
    }

    fn submit_raw_tx(&mut self, raw: &[u8]) -> Result<(), String> {
        // Dev stub: accept either (a) borsh-encoded internal txs, or (b) real Ethereum EIP-1559
        // signed tx bytes (type 0x02) for Hardhat/MetaMask compatibility.
        if let Ok(tx) = Transaction::try_from_slice(raw) {
            let h = keccak256(raw);
            self.pending_txs.insert(h, tx.clone());
            self.mempool.insert(PooledTx {
                tx,
                max_priority_fee_per_gas: 1,
                max_fee_per_gas: u128::MAX,
                eth_signed_raw: None,
            });
            return Ok(());
        }

        let (tx, h, max_priority_fee_per_gas, max_fee_per_gas) =
            eth_signed::to_core_tx(raw, self.chain_id)?;
        self.pending_txs.insert(h, tx.clone());
        self.eth_signed_raw.insert(h, raw.to_vec());
        self.mempool.insert(PooledTx {
            tx,
            max_priority_fee_per_gas,
            max_fee_per_gas,
            eth_signed_raw: Some(raw.to_vec()),
        });
        Ok(())
    }

    fn base_fee_per_gas(&self) -> u128 {
        self.base_fee
    }

    fn block_hash_by_number(&self, number: u64) -> Option<[u8; 32]> {
        if number == 0 {
            return Some(genesis_parent_hash());
        }
        let idx = number.checked_sub(1)? as usize;
        let b = self.blocks.get(idx)?;
        header_hash(&b.header).ok()
    }

    fn block_by_hash(&self, hash: &[u8; 32]) -> Option<Block> {
        if hash == &genesis_parent_hash() {
            // Synthetic genesis block: minimal header-like object.
            return Some(Block {
                header: fractal_consensus::BlockHeader {
                    version: 1,
                    chain_id: self.chain_id,
                    height: 0,
                    view: 0,
                    parent_hash: [0u8; 32],
                    parent_qc_hash: [0u8; 32],
                    proposer: [0u8; 32],
                    timestamp_ms: 0,
                    state_root: [0u8; 32],
                    tx_root: [0u8; 32],
                    zone_namespace: fractal_consensus::MASTERCHAIN_ZONE_NAMESPACE,
                    da_root: [0u8; 32],
                    da_bytes: 0,
                    da_share_count: 0,
                    da_gas_used: 0,
                    da_fee_paid: 0,
                    gas_used: 0,
                    gas_limit: self.gas_limit,
                    extra: [0u8; 32],
                },
                transactions: Vec::new(),
                eth_signed_raw: Vec::new(),
                da_sidecar: fractal_consensus::DaSidecar {
                    namespace: fractal_consensus::DEFAULT_DA_NAMESPACE,
                    original_len: 0,
                    share_size: fractal_consensus::DEFAULT_DA_SHARE_SIZE,
                    data_share_count: 0,
                    parity_share_count: 0,
                    shares: Vec::new(),
                },
            });
        }
        self.blocks
            .iter()
            .find(|b| header_hash(&b.header).ok().as_ref() == Some(hash))
            .cloned()
    }

    fn block_is_proof_final(&self, hash: &[u8; 32]) -> bool {
        matches!(
            self.finality_for_block_hash(hash),
            Some(BlockFinality::Proof)
        )
    }

    fn tx_by_hash(&self, hash: &[u8; 32]) -> Option<Transaction> {
        if let Some(tx) = self.pending_txs.get(hash) {
            return Some(tx.clone());
        }
        if let Some((bn, _bh, idx)) = self.mined_txs.get(hash) {
            if *bn == 0 {
                return None;
            }
            let bi = (*bn as usize).checked_sub(1)?;
            let block = self.blocks.get(bi)?;
            return block.transactions.get(*idx as usize).cloned();
        }
        for b in &self.blocks {
            for tx in &b.transactions {
                let raw = borsh::to_vec(tx).ok()?;
                if &keccak256(&raw) == hash {
                    return Some(tx.clone());
                }
            }
        }
        None
    }

    fn mined_tx_info(&self, hash: &[u8; 32]) -> Option<(u64, [u8; 32], u32)> {
        self.mined_txs.get(hash).cloned()
    }

    fn eth_signed_raw(&self, tx_hash: &[u8; 32]) -> Option<Vec<u8>> {
        self.eth_signed_raw.get(tx_hash).cloned()
    }

    fn simulate_eth_call(
        &self,
        from: Address,
        to: Option<Address>,
        value: u128,
        data: Vec<u8>,
    ) -> Result<Vec<u8>, fractal_core::ExecError> {
        let mut scratch = self.state.clone();
        let mut evm = fractal_evm::RevmEngine::default();
        match to {
            Some(to) => evm
                .execute_call(&mut scratch, from, to, value, data, self.gas_limit)
                .map(|o| o.return_data),
            None => evm
                .execute_create(&mut scratch, from, value, data, self.gas_limit)
                .map(|o| o.return_data),
        }
    }

    fn estimate_eth_gas(
        &self,
        from: Address,
        to: Option<Address>,
        value: u128,
        data: Vec<u8>,
    ) -> Result<u64, fractal_core::ExecError> {
        let mut scratch = self.state.clone();
        let mut evm = fractal_evm::RevmEngine::default();
        match to {
            Some(to) => evm
                .execute_call(&mut scratch, from, to, value, data, self.gas_limit)
                .map(|o| o.gas_used),
            None => evm
                .execute_create(&mut scratch, from, value, data, self.gas_limit)
                .map(|o| o.gas_used),
        }
    }

    fn code_at(&self, addr: &Address) -> Vec<u8> {
        self.state.evm_code.get(addr).cloned().unwrap_or_default()
    }

    fn storage_at(&self, addr: &Address, slot: [u8; 32]) -> [u8; 32] {
        self.state
            .evm_storage
            .get(&(*addr, slot))
            .copied()
            .unwrap_or([0u8; 32])
    }

    fn gas_used_for_tx(&self, tx_hash: &[u8; 32]) -> Option<u64> {
        let k = self.internal_tx_hash_for_state(tx_hash);
        self.state.evm_tx_gas_used.get(&k).copied()
    }

    fn evm_receipt_success(&self, tx_hash: &[u8; 32]) -> bool {
        let k = self.internal_tx_hash_for_state(tx_hash);
        self.state.evm_tx_success.get(&k).copied().unwrap_or(true)
    }

    fn logs_for_filter(&self, filter: &fractal_rpc::LogsFilter) -> Vec<fractal_rpc::RpcLog> {
        let mut out = Vec::new();
        let start = filter.from_block.max(1);
        let end = filter.to_block.max(1);

        for height in start..=end {
            let idx = match height.checked_sub(1) {
                Some(i) => i as usize,
                None => continue,
            };
            let Some(block) = self.blocks.get(idx) else {
                continue;
            };
            let bh = match header_hash(&block.header) {
                Ok(h) => h,
                Err(_) => continue,
            };
            let mut block_log_index: u64 = 0;
            for (txi, tx) in block.transactions.iter().enumerate() {
                let raw = match borsh::to_vec(tx) {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                let th = keccak256(&raw);
                let rpc_h = self
                    .eth_internal_to_rpc_tx_hash
                    .get(&th)
                    .copied()
                    .unwrap_or(th);
                let Some(logs) = self.state.evm_tx_logs.get(&th) else {
                    continue;
                };
                for l in logs {
                    if let Some(ref addrs) = filter.addresses {
                        if !addrs.contains(&l.address) {
                            continue;
                        }
                    }
                    if !fractal_rpc::evm_log_matches_topic_filters(l, &filter.topic_filters) {
                        continue;
                    }
                    out.push(make_rpc_log(
                        l,
                        &bh,
                        height,
                        &rpc_h,
                        txi as u32,
                        block_log_index,
                    ));
                    block_log_index += 1;
                }
            }
        }
        out
    }

    fn receipt_rpc_logs(
        &self,
        tx_hash: &[u8; 32],
        block_number: u64,
        block_hash: &[u8; 32],
        tx_index: u32,
    ) -> (Vec<fractal_rpc::RpcLog>, [u8; 256]) {
        let k = self.internal_tx_hash_for_state(tx_hash);
        let Some(evm_logs) = self.state.evm_tx_logs.get(&k) else {
            return (Vec::new(), [0u8; 256]);
        };
        let bloom = logs_bloom_256(evm_logs);
        let start = self.log_index_base_in_block(block_number, tx_index);
        let rpc_logs = evm_logs
            .iter()
            .enumerate()
            .map(|(i, l)| {
                make_rpc_log(
                    l,
                    block_hash,
                    block_number,
                    tx_hash,
                    tx_index,
                    start + i as u64,
                )
            })
            .collect();
        (rpc_logs, bloom)
    }

    fn logs_bloom_for_block(&self, block: &Block) -> [u8; 256] {
        let mut acc = [0u8; 256];
        for tx in &block.transactions {
            let Ok(raw) = borsh::to_vec(tx) else {
                continue;
            };
            let th = keccak256(&raw);
            let Some(logs) = self.state.evm_tx_logs.get(&th) else {
                continue;
            };
            let b = logs_bloom_256(logs);
            for i in 0..256 {
                acc[i] |= b[i];
            }
        }
        acc
    }

    fn da_metrics(&self) -> RpcDaMetrics {
        RpcDaMetrics {
            committed_blocks: format!("0x{:x}", self.da_metrics.committed_blocks),
            committed_original_bytes: format!("0x{:x}", self.da_metrics.committed_original_bytes),
            committed_encoded_bytes: format!("0x{:x}", self.da_metrics.committed_encoded_bytes),
            committed_da_gas: format!("0x{:x}", self.da_metrics.committed_da_gas),
            da_fee_revenue: format!("0x{:x}", self.da_metrics.da_fee_revenue),
            sampling_success: format!("0x{:x}", self.da_metrics.sampling_success),
            sampling_failure: format!("0x{:x}", self.da_metrics.sampling_failure),
            reconstruction_success: format!("0x{:x}", self.da_metrics.reconstruction_success),
            reconstruction_failure: format!("0x{:x}", self.da_metrics.reconstruction_failure),
        }
    }

    fn da_fee_revenue(&self) -> u128 {
        self.da_metrics.da_fee_revenue
    }

    fn proof_metrics(&self) -> RpcProofMetrics {
        let average = if self.proof_metrics.proofs_accepted == 0 {
            0
        } else {
            (self.proof_metrics.total_proof_latency_ms / self.proof_metrics.proofs_accepted as u128)
                as u64
        };
        RpcProofMetrics {
            proofs_accepted: format!("0x{:x}", self.proof_metrics.proofs_accepted),
            proofs_rejected: format!("0x{:x}", self.proof_metrics.proofs_rejected),
            latest_proof_latency_ms: format!("0x{:x}", self.proof_metrics.latest_proof_latency_ms),
            average_proof_latency_ms: format!("0x{:x}", average),
            proof_final_height: format!("0x{:x}", self.proof_metrics.proof_final_height),
            latest_rejection_reason: self.proof_metrics.latest_rejection_reason.clone(),
            rejection_reasons: self
                .proof_metrics
                .rejection_reasons
                .iter()
                .map(|(reason, count)| RpcProofRejectionMetric {
                    reason: reason.clone(),
                    count: format!("0x{count:x}"),
                })
                .collect(),
        }
    }

    fn mempool_lane_metrics(&self) -> RpcMempoolLaneMetrics {
        let metrics = self.mempool.lane_metrics();
        RpcMempoolLaneMetrics {
            pending_total: format!("0x{:x}", metrics.pending_total),
            pending_owned: format!("0x{:x}", metrics.pending_owned),
            pending_mixed: format!("0x{:x}", metrics.pending_mixed),
            pending_consensus: format!("0x{:x}", metrics.pending_consensus),
            pending_consensus_lane: format!("0x{:x}", metrics.pending_consensus_lane),
        }
    }

    fn chain_config(&self) -> RpcChainConfig {
        RpcChainConfig {
            proof_required_settlement: self.chain_config.proof_required_settlement,
            settlement_finality: if self.chain_config.proof_required_settlement {
                "proof"
            } else {
                "soft"
            }
            .into(),
        }
    }

    fn submit_validity_proof(&mut self, proof: BlockValidityProof) -> Result<[u8; 32], String> {
        let block_hash = proof.block_hash;
        NodeInner::submit_validity_proof(self, proof).map_err(|e| e.to_string())?;
        Ok(block_hash)
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn proof_rejection_reason(err: &ProofVerifyError) -> String {
    match err {
        ProofVerifyError::ChainId => "chain_id",
        ProofVerifyError::Height => "height",
        ProofVerifyError::BlockHash => "block_hash",
        ProofVerifyError::StateRoot => "state_root",
        ProofVerifyError::TxRoot => "tx_root",
        ProofVerifyError::DaRoot => "da_root",
        ProofVerifyError::ZoneNamespace => "zone_namespace",
        ProofVerifyError::EmptyProof => "empty_proof",
        ProofVerifyError::Production(_) => "production_proof",
        ProofVerifyError::BadDevDigest => "bad_dev_digest",
        ProofVerifyError::DataAvailability => "data_availability",
        ProofVerifyError::Io(_) => "io",
    }
    .to_owned()
}

/// Outcome of one produce-tick (`docs/prd.md` §7 M7-c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProduceTickOutcome {
    /// Block produced; height advanced.
    Produced(u64),
    /// Skipped because `validators.is_proposer_for_view(view, validator_index)` is false.
    NotMyTurn,
    /// Tick reached the producer but `apply_block_with_evm` failed (already logged).
    BuildFailed,
}

/// Build one block from the mempool if this node is the current view's leader.
/// Extracted from `producer_loop` so tests can drive single ticks deterministically.
pub async fn try_produce_one_tick(node: &NodeHandle) -> ProduceTickOutcome {
    let mut n = node.lock().await;
    let view = n.view;
    if !n.is_my_turn(view) {
        return ProduceTickOutcome::NotMyTurn;
    }
    let base = n.base_fee;
    let gas_limit_cfg = n.gas_limit;
    let pooled = n.mempool.drain_ready_gas_budget(gas_limit_cfg, base);
    let eth_raws: Vec<Option<Vec<u8>>> = pooled.iter().map(|p| p.eth_signed_raw.clone()).collect();
    let txs: Vec<Transaction> = pooled.into_iter().map(|p| p.tx).collect();
    let parent = n.head_hash;
    let qc = n.parent_qc_hash;
    let height = n.height + 1;
    let ts = now_ms();
    let chain_id = n.chain_id;
    let proposer = n.validators.expected_proposer(view);
    let gas_limit = n.gas_limit;
    match execute_and_build_block(
        chain_id,
        height,
        view,
        parent,
        qc,
        proposer,
        ts,
        gas_limit,
        &mut n.state,
        txs,
        eth_raws,
    ) {
        Ok(block) => {
            let hh = header_hash(&block.header).unwrap_or([0u8; 32]);
            n.head_hash = hh;
            n.height = block.header.height;
            match next_parent_qc_hash_after_commit(&block.header, hh) {
                Ok(next_qc) => n.parent_qc_hash = next_qc,
                Err(e) => eprintln!("fractal-node: parent_qc_hash advance failed: {e}"),
            }
            n.view = n.view.wrapping_add(1);
            n.base_fee = next_base_fee(n.base_fee, block.header.gas_used, &n.fee_params);
            n.sync_rpc_index_from_block(&block);
            n.forward_vote_after_commit(&block);
            n.record_committed_da_metrics(&block);
            n.blocks.push(block);
            ProduceTickOutcome::Produced(n.height)
        }
        Err(e) => {
            eprintln!("fractal-node: block execution failed: {e}");
            ProduceTickOutcome::BuildFailed
        }
    }
}

pub async fn producer_loop(node: NodeHandle) {
    let mut ticker = tokio::time::interval(tokio::time::Duration::from_millis(500));
    loop {
        ticker.tick().await;
        let _ = try_produce_one_tick(&node).await;
    }
}

pub async fn run_dev() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let validators = devnet_validator_set_from_env();
    let validator_index = devnet_validator_index_from_env(&validators);
    let validator_secret = devnet_validator_secret_from_env(&validators, validator_index);
    eprintln!(
        "fractal-node: validator_set_size={} validator_index={validator_index} bls_signing={}",
        validators.len(),
        if validator_secret.is_some() {
            "enabled"
        } else {
            "disabled"
        }
    );
    let (vote_tx, vote_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut inner =
        NodeInner::devnet_with_validator_secret(validators, validator_index, validator_secret);
    inner.set_proof_required_settlement(proof_required_settlement_from_env());
    if let Some(path) = proof_finality_store_from_env() {
        inner.set_proof_finality_store(ProofFinalityStore::open(&path)?)?;
        eprintln!("fractal-node: proof_finality_store={}", path.display());
    }
    inner.set_vote_sink(Some(vote_tx));
    eprintln!(
        "fractal-node: settlement_finality={}",
        if inner.settlement_requires_proof() {
            "proof"
        } else {
            "soft"
        }
    );
    let node: NodeHandle = Arc::new(Mutex::new(inner));
    let addr: std::net::SocketAddr = std::env::var("FRACTAL_RPC_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8545".into())
        .parse()?;
    let (handle, bound) = fractal_rpc::serve_http(addr, node.clone()).await?;
    eprintln!("fractal-node json-rpc at http://{bound}");

    let listen: Multiaddr = std::env::var("FRACTAL_P2P_LISTEN")
        .unwrap_or_else(|_| "/ip4/0.0.0.0/udp/4001/quic-v1".into())
        .parse()?;
    let (tx_ready, rx_ready) = tokio::sync::oneshot::channel();
    let p2p_node = node.clone();
    tokio::spawn(async move {
        if let Err(e) =
            p2p::producer_network_task(p2p_node, listen, Some(tx_ready), Some(vote_rx)).await
        {
            eprintln!("fractal-node p2p: {e}");
        }
    });
    match tokio::time::timeout(Duration::from_secs(8), rx_ready).await {
        Ok(Ok((bound_p2p, peer))) => {
            let mut bootstrap = bound_p2p.clone();
            bootstrap.push(Protocol::P2p(peer));
            eprintln!("fractal-node p2p (QUIC) listening {bound_p2p}; follower env FRACTAL_BOOTSTRAP={bootstrap}");
        }
        Ok(Err(_)) => eprintln!("fractal-node p2p: ready channel dropped"),
        Err(_) => eprintln!("fractal-node p2p: timed out waiting for listen address"),
    }

    tokio::spawn(producer_loop(node));
    tokio::signal::ctrl_c().await?;
    handle.stop()?;
    Ok(())
}

/// Follower: JSON-RPC + sync from `FRACTAL_BOOTSTRAP`; optionally sample DA from
/// `FRACTAL_DA_BOOTSTRAP` peers.
pub async fn run_follower() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let raw = std::env::var("FRACTAL_BOOTSTRAP")?;
    let bootstraps = crate::p2p::parse_fractal_bootstraps(&raw)?;
    let da_bootstraps = match std::env::var("FRACTAL_DA_BOOTSTRAP") {
        Ok(raw) if !raw.trim().is_empty() => crate::p2p::parse_fractal_da_bootstraps(&raw)?,
        _ => Vec::new(),
    };
    eprintln!(
        "fractal-node follower: {} sync bootstrap multiaddr(s), {} DA bootstrap multiaddr(s)",
        bootstraps.len(),
        da_bootstraps.len()
    );
    let validators = devnet_validator_set_from_env();
    let validator_index = devnet_validator_index_from_env(&validators);
    let validator_secret = devnet_validator_secret_from_env(&validators, validator_index);
    eprintln!(
        "fractal-node follower: validator_set_size={} validator_index={validator_index} bls_signing={}",
        validators.len(),
        if validator_secret.is_some() { "enabled" } else { "disabled" }
    );
    let (vote_tx, vote_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut inner =
        NodeInner::devnet_with_validator_secret(validators, validator_index, validator_secret);
    inner.set_proof_required_settlement(proof_required_settlement_from_env());
    if let Some(path) = proof_finality_store_from_env() {
        inner.set_proof_finality_store(ProofFinalityStore::open(&path)?)?;
        eprintln!(
            "fractal-node follower: proof_finality_store={}",
            path.display()
        );
    }
    inner.set_vote_sink(Some(vote_tx));
    eprintln!(
        "fractal-node follower: settlement_finality={}",
        if inner.settlement_requires_proof() {
            "proof"
        } else {
            "soft"
        }
    );
    let node: NodeHandle = Arc::new(Mutex::new(inner));
    let addr: std::net::SocketAddr = std::env::var("FRACTAL_RPC_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8546".into())
        .parse()?;
    let (handle, bound) = fractal_rpc::serve_http(addr, node.clone()).await?;
    eprintln!("fractal-node follower json-rpc at http://{bound}");
    tokio::spawn(p2p::follower_network_task_with_da_peers(
        node,
        bootstraps,
        da_bootstraps,
        Some(vote_rx),
    ));
    tokio::signal::ctrl_c().await?;
    handle.stop()?;
    Ok(())
}
