//! Dev validator node: HotStuff-2 (500 ms) or HyperBFT shard (70 ms) + JSON-RPC + libp2p sync.
//! Optional async STWO/RISC-V proof condenser worker (`docs/prd.md` §7.8, `crates/proof-condenser`).

mod chain_snapshot;
mod eth_signed;
mod hyperbft_tick;
mod metrics;
pub mod p2p;
pub mod prune;

pub use chain_snapshot::{
    CHAIN_SYNC_PROOF_SNAPSHOT_V1_VERSION, CHAIN_SYNC_SNAPSHOT_V1_VERSION,
    CHAIN_SYNC_SNAPSHOT_V2_VERSION, ChainSyncProofSnapshotV1, ChainSyncSnapshotV1,
    ChainSyncSnapshotV2,
};

pub use fractal_consensus::{ValidatorEntry, ValidatorSet};

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use borsh::BorshDeserialize;
use fractal_consensus::{
    Block, BlockFinalizeContext, ConsensusMode, FormedQc, HyperBftConfig, HyperBftPipeline,
    OptimisticExecution, ParentQcResolution, QuorumCertificate, RecordTimeoutOutcome,
    RecordVoteOutcome, ThreeStagePipeline, Timeout, TimeoutPool, TimeoutSignBody, Vote, VotePool,
    VoteSignBody, execute_and_build_block, genesis_parent_qc, hash_qc, header_hash, high_qc_rank,
    is_genesis_parent_qc, ordered_tx_root, parent_qc_bundle, resolve_parent_qc, verify_formed_qc,
};
use fractal_core::{
    Address, ChainEconomicsParams, EvmEngine, MAINNET_MIN_VALIDATOR_STAKE_WEI, NativeCall, State,
    Transaction, permissionless_validator_entries,
};
use fractal_crypto::hash::keccak256;
use fractal_crypto::{BlsPublicKey, BlsSecretKey};
use fractal_masterchain::MasterchainLedger;
use fractal_mempool::{BaseFeeParams, Mempool, PooledTx, next_base_fee};
use fractal_proof_aggregator::{VerifiedStwoStatementV1, verify_stwo_artifact_submission};
use fractal_proof_condenser::{
    CheckpointJob, ProofArtifactRegistry, ProofPersistenceConfig, Tier1DigestSink,
    checkpoint_job_from_block, spawn_async_proof_condenser,
};
use fractal_rpc::{ChainInteraction, RpcCallStats, logs_bloom_256, make_rpc_log};
use fractal_shard::{
    CrossShardMessageV1, MasterchainBlockV1, ProofSubmissionV1, ShardAnchor, ShardRoutingError,
    ShardTopology, anchor_interval_from_env, should_emit_anchor_at_height,
};
use libp2p::Multiaddr;
use libp2p::multiaddr::Protocol;
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
    #[error("parent_qc_hash does not match keccak256(borsh(parent_qc)))")]
    ParentQcHash,
    #[error("parent QC does not bind to the expected parent header / genesis")]
    ParentQcBinding,
    #[error("parent QC aggregate signature verification failed: {0}")]
    InvalidQcSignature(String),
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
    #[error("chain snapshot: {0}")]
    Snapshot(String),
    #[error("shard: {0}")]
    Shard(#[from] ShardRoutingError),
    #[error(transparent)]
    Exec(#[from] fractal_core::ExecError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

const GENESIS_TAG: &[u8] = b"FRACTALCHAIN_GENESIS_V0";

/// Hardhat / Anvil default signer #0 — re-exported from `fractal_core::devnet_accounts`.
pub use fractal_core::HARDHAT_DEFAULT_SIGNER_0;
/// Hardhat default signer #1 (M5 MVP agent for `CLAIM_PAYOUT` demos).
pub use fractal_core::HARDHAT_DEFAULT_SIGNER_1;

pub fn genesis_parent_hash() -> fractal_crypto::Hash256 {
    keccak256(GENESIS_TAG)
}

fn economics_profile_from_env() -> String {
    std::env::var("FRACTAL_ECONOMICS_PROFILE")
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn devnet_validator_set_from_env() -> ValidatorSet {
    match std::env::var("FRACTAL_VALIDATOR_SET")
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Ok("7") | Ok("bft7") => ValidatorSet::phase2_bft7_fixture(),
        Ok("21") | Ok("bft21") => ValidatorSet::phase3_bft21_fixture(),
        _ => ValidatorSet::phase1_singleton(),
    }
}

fn proof_chain_fast_sync_from_env() -> bool {
    match std::env::var("FRACTAL_PROOF_CHAIN_FAST_SYNC") {
        Ok(v) => {
            let t = v.trim().to_ascii_lowercase();
            !(t.is_empty() || t == "0" || t == "false" || t == "no" || t == "off")
        }
        Err(_) => false,
    }
}

/// Human-readable operator material for the **in-repo dev validator sets** only
/// (`phase1_singleton`, `phase2_bft7_fixture`, `phase3_bft21_fixture`). Does not read process environment;
/// use [`devnet_validator_onboarding_report`] from the binary wrapper.
pub fn devnet_validator_onboarding_report_for(
    validators: &ValidatorSet,
    fractal_validator_set_raw_env: &str,
) -> String {
    let n = validators.len();
    let quorum = validators.quorum_threshold();
    let mut out = String::new();
    out.push_str("FractalChain — devnet validator onboarding (PRD §18 M7 / M8)\n");
    out.push_str("(Fixture fingerprints + BLS pubkeys + optional dev signing secrets.)\n\n");
    out.push_str("FRACTAL_VALIDATOR_SET raw env: ");
    if fractal_validator_set_raw_env.is_empty() {
        out.push_str("<unset> → phase-1 singleton (n=1)\n");
    } else {
        out.push_str(&format!("{fractal_validator_set_raw_env:?}\n"));
    }
    out.push_str(&format!(
        "Effective set: n={n}  PBFT quorum votes = {quorum}\n"
    ));
    out.push_str("\nPer-binary env (each validator process):\n");
    out.push_str("  FRACTAL_VALIDATOR_SET     — same on every host (unset = singleton; `7`/`bft7` or `21`/`bft21` = fixture)\n");
    out.push_str("  FRACTAL_VALIDATOR_INDEX   — this host's row in the table (0 .. n-1)\n");
    out.push_str("  FRACTAL_VALIDATOR_SECRET_HEX — 32-byte signing key (optional on fixtures: dev fallback matches table)\n");
    out.push_str(
        "\n*** Dev fixture secrets are NOT for mainnet. Use HSM-held keys in production. ***\n\n",
    );
    out.push_str("index | proposer (0x…20)     | bls_pubkey (0x…48)                         | dev FRACTAL_VALIDATOR_SECRET_HEX (if available)\n");
    out.push_str("------+------------------------+----------------------------------------------+--------------------------------------------------\n");
    for i in 0..n {
        let Some(entry) = validators.entry(i) else {
            continue;
        };
        let fp = hex::encode(entry.fingerprint);
        let pk = hex::encode(entry.bls_pubkey.0);
        let secret_col = match validators.dev_bls_secret(i) {
            Some(sk) => format!("0x{}", hex::encode(sk.to_bytes())),
            None => "(no fixture secret — supply a key whose pubkey matches column 3)".into(),
        };
        out.push_str(&format!("{i:5} | 0x{fp} | 0x{pk} | {secret_col}\n"));
    }
    out.push_str("\nLeader at view v is validator index (v % n) (`docs/prd.md` §7.5).\n");
    out.push_str("BFT multi-validator continuous production needs gossip votes or QUIC sync between all processes.\n");
    out
}

/// Same as [`devnet_validator_onboarding_report_for`] using [`devnet_validator_set_from_env`]
/// and the raw `FRACTAL_VALIDATOR_SET` string for the heading.
#[must_use]
pub fn devnet_validator_onboarding_report() -> String {
    let raw = std::env::var("FRACTAL_VALIDATOR_SET").unwrap_or_default();
    let validators = devnet_validator_set_from_env();
    devnet_validator_onboarding_report_for(&validators, &raw)
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

pub struct NodeInner {
    pub chain_id: u64,
    /// This process serves this execution shard (`FRACTAL_SHARD_ID`, default 0).
    pub shard_id: u32,
    /// Network shard topology (`FRACTAL_SHARD_COUNT`, default 1 = monolith).
    pub shard_topology: ShardTopology,
    pub height: u64,
    pub view: u64,
    pub head_hash: fractal_crypto::Hash256,
    pub parent_qc_hash: fractal_crypto::Hash256,
    /// Highest prepare-phase QC this node has observed (`docs/prd.md` §7.4); embedded in timeouts.
    pub high_prepare_qc: QuorumCertificate,
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
    /// When set, serialized [`Timeout`]s are sent on the timeouts gossip topic (M7-f).
    pub timeout_sink: Option<tokio::sync::mpsc::UnboundedSender<Vec<u8>>>,
    /// Wall-clock ms when the current `view` started (pacemaker / PRD §7.4).
    pub view_entered_at_ms: u64,
    /// Exponential backoff for view timeout after consecutive timeout-driven advances.
    pub consecutive_timeout_failures: u32,
    /// `Some(v)` if this process already published a timeout for view `v` in this round.
    pub timeout_sent_for_view: Option<u64>,
    pub timeout_pool: TimeoutPool,
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
    /// When set, finalized blocks enqueue [`CheckpointJob`] for the async proof worker (`docs/prd.md` §7.8).
    pub proof_job_tx: Option<tokio::sync::mpsc::Sender<CheckpointJob>>,
    /// When set with async proof worker, completed checkpoint proofs (digest + optional v1 artifact).
    pub proof_artifact_registry: Option<Arc<ProofArtifactRegistry>>,
    /// Optional PRD §10.3 RocksDB (`FRACTAL_CHAIN_ROCKSDB_PATH` / shared with `FRACTAL_PROOF_ROCKSDB_PATH`).
    pub chain_store: Option<fractal_storage::FractalRocksDb>,
    /// Path behind [`NodeInner::chain_store`], used for PRD §16.1 DB size metrics.
    pub rocksdb_path: Option<PathBuf>,
    /// PRD §16.1: JSON-RPC `fractal_rpc_requests_total` counters (method × ok/err).
    pub rpc_call_stats: RpcCallStats,
    /// PRD §16.1: libp2p QUIC connection count (increment on establish, decrement on close).
    pub p2p_connected_peers: Arc<AtomicUsize>,
    /// PRD §16.1: latency histograms, proof worker stats, p2p topic counters.
    pub metrics: Arc<crate::metrics::MetricsState>,
    /// PRD §12 / M7: minimum [`State::consensus_stake_total_for_fingerprint`] (this node's
    /// validator fingerprint) before the producer will build a block. `0` = disabled (default).
    /// Set via **`FRACTAL_MIN_CONSENSUS_STAKE_WEI`** once at process start (`run_dev` / `run_follower`).
    pub min_consensus_stake_wei: u128,
    /// Shard blocks between masterchain anchors (`FRACTAL_ANCHOR_INTERVAL`; `0` = off on monolith).
    pub anchor_interval: u64,
    /// Local masterchain ledger (anchors + coordination blocks).
    pub masterchain_ledger: MasterchainLedger,
    /// Destination-shard delivery journal keyed by `(masterchain_height, message_index)`.
    pub delivered_cross_shard_messages: Vec<DeliveredCrossShardMessageV1>,
    delivered_cross_shard_keys: BTreeSet<(u64, u32)>,
    /// HotStuff-2 vs HyperBFT pipelined (`FRACTAL_CONSENSUS_MODE`).
    pub consensus_mode: ConsensusMode,
    /// HyperBFT timing + pipeline state (Track B).
    pub hyperbft_config: HyperBftConfig,
    pub hyperbft_pipeline: HyperBftPipeline,
    /// Propose / vote / commit slots (§7.9.3).
    pub three_stage: ThreeStagePipeline,
    /// Speculative execution + rollback (§7.9.1).
    pub optimistic: OptimisticExecution,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliveredCrossShardMessageV1 {
    pub masterchain_height: u64,
    pub message_index: u32,
    pub message: CrossShardMessageV1,
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
        let economics_profile = economics_profile_from_env();
        state.chain_economics = ChainEconomicsParams::from_profile_name(&economics_profile);
        let min_stake = std::env::var("FRACTAL_MIN_CONSENSUS_STAKE_WEI")
            .ok()
            .and_then(|s| s.trim().parse::<u128>().ok())
            .unwrap_or_else(|| {
                if economics_profile.eq_ignore_ascii_case("mainnet") {
                    MAINNET_MIN_VALIDATOR_STAKE_WEI
                } else {
                    0
                }
            });
        let shard_topology = ShardTopology::from_env();
        let shard_id = shard_topology.node_shard_id_from_env();
        let anchor_interval = anchor_interval_from_env(&shard_topology);
        let default_hyperbft = !shard_topology.is_monolith();
        let consensus_mode = ConsensusMode::parse_env(
            std::env::var(ConsensusMode::ENV).ok().as_deref(),
            default_hyperbft,
        );
        let hyperbft_config = HyperBftConfig::from_env_overrides(HyperBftConfig::default());
        let hyperbft_pipeline = HyperBftPipeline::new(hyperbft_config);
        let three_stage = ThreeStagePipeline::new(hyperbft_config);
        let optimistic = OptimisticExecution::new(&state);
        Self {
            chain_id: 41,
            shard_id,
            shard_topology,
            height: 0,
            view: 0,
            head_hash: genesis_parent_hash(),
            parent_qc_hash: hash_qc(&genesis_parent_qc()).expect("genesis_parent_qc borsh"),
            high_prepare_qc: genesis_parent_qc(),
            validators,
            validator_index,
            validator_secret,
            vote_pool: VotePool::new(),
            vote_sink: None,
            timeout_sink: None,
            view_entered_at_ms: now_ms(),
            consecutive_timeout_failures: 0,
            timeout_sent_for_view: None,
            timeout_pool: TimeoutPool::new(),
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
            proof_job_tx: None,
            proof_artifact_registry: None,
            chain_store: None,
            rocksdb_path: None,
            rpc_call_stats: RpcCallStats::new(),
            p2p_connected_peers: Arc::new(AtomicUsize::new(0)),
            metrics: Arc::new(crate::metrics::MetricsState::default()),
            min_consensus_stake_wei: min_stake,
            anchor_interval,
            masterchain_ledger: MasterchainLedger::default(),
            delivered_cross_shard_messages: Vec::new(),
            delivered_cross_shard_keys: BTreeSet::new(),
            consensus_mode,
            hyperbft_config,
            hyperbft_pipeline,
            three_stage,
            optimistic,
        }
    }

    /// Accept cross-shard messages from a canonical masterchain block addressed to this shard.
    ///
    /// Delivery is idempotent per `(masterchain_height, message_index)` and preserves the
    /// block's canonical message order. Application-specific payload execution is layered
    /// above this journal.
    pub fn apply_masterchain_cross_shard_deliveries(
        &mut self,
        block: &MasterchainBlockV1,
    ) -> usize {
        let mut delivered = 0usize;
        for (message_index, message) in block.cross_shard_messages.iter().enumerate() {
            if message.to_shard != self.shard_id {
                continue;
            }
            let key = (block.height, message_index as u32);
            if !self.delivered_cross_shard_keys.insert(key) {
                continue;
            }
            let actual_hash = keccak256(&message.payload);
            if actual_hash != message.payload_hash {
                self.delivered_cross_shard_keys.remove(&key);
                eprintln!(
                    "fractal-node: cross-shard payload hash mismatch masterchain_height={} message_index={message_index}",
                    block.height
                );
                continue;
            }
            let call = match NativeCall::try_from_slice(&message.payload) {
                Ok(call) => call,
                Err(e) => {
                    self.delivered_cross_shard_keys.remove(&key);
                    eprintln!(
                        "fractal-node: cross-shard payload decode failed masterchain_height={} message_index={message_index}: {e}",
                        block.height
                    );
                    continue;
                }
            };
            if let Err(e) = self.state.apply_native_syscall([0u8; 20], &call) {
                self.delivered_cross_shard_keys.remove(&key);
                eprintln!(
                    "fractal-node: cross-shard payload execution failed masterchain_height={} message_index={message_index}: {e}",
                    block.height
                );
                continue;
            }
            self.delivered_cross_shard_messages
                .push(DeliveredCrossShardMessageV1 {
                    masterchain_height: block.height,
                    message_index: message_index as u32,
                    message: message.clone(),
                });
            delivered = delivered.saturating_add(1);
        }
        delivered
    }

    pub fn apply_synced_masterchain_block(
        &mut self,
        block: &MasterchainBlockV1,
    ) -> Result<(), String> {
        let next = self.masterchain_ledger.masterchain_height.saturating_add(1);
        if block.height <= self.masterchain_ledger.masterchain_height {
            if self
                .masterchain_ledger
                .blocks
                .iter()
                .any(|b| b.height == block.height && b == block)
            {
                return Ok(());
            }
            return Err(format!(
                "stale masterchain block height={} local={}",
                block.height, self.masterchain_ledger.masterchain_height
            ));
        }
        if block.height != next {
            return Err(format!(
                "non-contiguous masterchain block height={} expected={next}",
                block.height
            ));
        }
        for anchor in &block.shard_anchors {
            self.masterchain_ledger
                .latest_anchors
                .insert(anchor.shard_id, anchor.clone());
        }
        self.masterchain_ledger.masterchain_height = block.height;
        self.masterchain_ledger.blocks.push(block.clone());
        if self.masterchain_ledger.blocks.len() > MasterchainLedger::MAX_BLOCKS {
            let drop = self.masterchain_ledger.blocks.len() - MasterchainLedger::MAX_BLOCKS;
            self.masterchain_ledger.blocks.drain(0..drop);
        }
        self.masterchain_ledger.pending_anchor_updates = false;
        self.apply_masterchain_cross_shard_deliveries(block);
        if let Some(ref db) = self.chain_store {
            for anchor in &block.shard_anchors {
                if let Err(e) = db.persist_shard_anchor_v1(anchor) {
                    eprintln!(
                        "fractal-node: synced shard anchor RocksDB shard={} height={} err={e}",
                        anchor.shard_id, anchor.block_height
                    );
                }
            }
            if let Err(e) = db.persist_masterchain_block_v1(block) {
                eprintln!(
                    "fractal-node: synced masterchain block RocksDB height={} err={e}",
                    block.height
                );
            }
        }
        Ok(())
    }

    #[must_use]
    pub fn delivered_cross_shard_messages_json(&self) -> serde_json::Value {
        let mut delivered = self.delivered_cross_shard_messages.clone();
        delivered.sort_by_key(|d| (d.masterchain_height, d.message_index));
        let messages: Vec<serde_json::Value> = delivered
            .iter()
            .map(|d| {
                serde_json::json!({
                    "masterchainHeight": format!("0x{:x}", d.masterchain_height),
                    "messageIndex": format!("0x{:x}", d.message_index),
                    "fromShard": format!("0x{:x}", d.message.from_shard),
                    "toShard": format!("0x{:x}", d.message.to_shard),
                    "payloadHash": format!("0x{}", hex::encode(d.message.payload_hash)),
                    "payload": format!("0x{}", hex::encode(&d.message.payload)),
                })
            })
            .collect();
        serde_json::json!({
            "shardId": format!("0x{:x}", self.shard_id),
            "messages": messages,
            "count": messages.len(),
        })
    }

    /// Block producer tick interval (ms): 70 for HyperBFT shards, 500 for HotStuff-2 monolith.
    #[must_use]
    pub fn effective_block_cadence_ms(&self) -> u64 {
        match self.consensus_mode {
            ConsensusMode::HyperBft => self.hyperbft_config.target_block_time_ms.max(10),
            ConsensusMode::HotStuff2 => 500,
        }
    }

    /// Pacemaker base timeout (ms) before emitting a local timeout vote.
    #[must_use]
    pub fn pacemaker_base_ms(&self) -> u64 {
        match self.consensus_mode {
            ConsensusMode::HyperBft => self.hyperbft_config.pacemaker_base_ms.max(10),
            ConsensusMode::HotStuff2 => 500,
        }
    }

    /// Rebuild the in-memory validator set from on-chain permissionless registry rows.
    pub fn sync_permissionless_validators(&mut self) {
        if !self.state.chain_economics.permissionless_validator_entry {
            return;
        }
        let rows = permissionless_validator_entries(&self.state);
        if rows.is_empty() {
            return;
        }
        let entries: Vec<ValidatorEntry> = rows
            .into_iter()
            .map(|(fingerprint, bls_pubkey)| ValidatorEntry {
                fingerprint,
                bls_pubkey: BlsPublicKey(bls_pubkey),
            })
            .collect();
        self.validators = ValidatorSet::from_entries(entries);
        if self.validator_index >= self.validators.len() {
            self.validator_index = 0;
        }
    }

    fn evm_gas_used_for_txs(&self, txs: &[Transaction]) -> u64 {
        txs.iter()
            .filter_map(|tx| borsh::to_vec(tx).ok())
            .map(|raw| keccak256(&raw))
            .filter_map(|h| self.state.evm_tx_gas_used.get(&h).copied())
            .sum()
    }

    /// Whether this node should propose for `view` (`docs/prd.md` §7 M7-c).
    /// In single-validator (Phase 1) setups, always `true` for `validator_index = 0`.
    #[must_use]
    pub fn is_my_turn(&self, view: u64) -> bool {
        self.validators
            .is_proposer_for_view(view, self.validator_index)
    }

    /// Per-validator bonded weights for [`fractal_core::validator_stake_weights`] / stake QC.
    fn consensus_stake_weights(&self) -> Vec<u128> {
        let fps = self.validators.ids();
        fractal_core::validator_stake_weights(&self.state, &fps)
    }

    fn maybe_upgrade_high_prepare_qc(&mut self, qc: &QuorumCertificate) {
        if high_qc_rank(qc) > high_qc_rank(&self.high_prepare_qc) {
            self.high_prepare_qc = qc.clone();
        }
    }

    /// Wire async STWO/RISC-V proof condenser jobs (`docs/prd.md` §7.8). `None` disables background proving.
    pub fn set_proof_job_tx(&mut self, sink: Option<tokio::sync::mpsc::Sender<CheckpointJob>>) {
        self.proof_job_tx = sink;
    }

    fn maybe_enqueue_proof_checkpoint(&self, block: &Block) {
        let Some(ref tx) = self.proof_job_tx else {
            return;
        };
        let Ok(job) = checkpoint_job_from_block(self.chain_id, block) else {
            return;
        };
        match tx.try_send(job) {
            Ok(()) => {
                self.metrics
                    .proof_jobs_enqueued_total
                    .fetch_add(1, Ordering::Relaxed);
            }
            Err(tokio::sync::mpsc::error::TrySendError::Full(j)) => {
                self.metrics
                    .proof_jobs_dropped_total
                    .fetch_add(1, Ordering::Relaxed);
                eprintln!(
                    "fractal-node: async proof queue full; dropping checkpoint height={}",
                    j.height
                );
            }
            Err(tokio::sync::mpsc::error::TrySendError::Closed(_)) => {
                self.metrics
                    .proof_jobs_dropped_total
                    .fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    /// Buffer tier-1 STWO digest until the next masterchain anchor seal.
    pub fn record_stwo_for_masterchain(
        &mut self,
        job: &CheckpointJob,
        digest: [u8; 32],
    ) -> Result<(), String> {
        self.masterchain_ledger
            .record_stwo_digest_range(job.start_block, job.end_block, digest)
            .map_err(|e| e.to_string())
    }

    /// Buffer a verified STWO public statement for Plonky2 aggregation.
    pub fn record_verified_stwo_for_masterchain(
        &mut self,
        stmt: VerifiedStwoStatementV1,
    ) -> Result<(), String> {
        self.masterchain_ledger
            .record_verified_stwo_statement(stmt)
            .map_err(|e| e.to_string())
    }

    /// Wire gossip vote publishing (`docs/prd.md` §18 M7-d-5). When `None`, votes stay local only.
    pub fn set_vote_sink(&mut self, sink: Option<tokio::sync::mpsc::UnboundedSender<Vec<u8>>>) {
        self.vote_sink = sink;
    }

    /// Wire gossip timeout publishing (`docs/prd.md` §7.4 M7-f).
    pub fn set_timeout_sink(&mut self, sink: Option<tokio::sync::mpsc::UnboundedSender<Vec<u8>>>) {
        self.timeout_sink = sink;
    }

    /// If the timeout pool holds a quorum certificate for [`Self::view`], verify it and advance
    /// `view` (liveness path). No-op for singleton sets.
    pub fn try_advance_view_on_timeout_quorum(&mut self) {
        if self.validators.len() <= 1 {
            return;
        }
        let view = self.view;
        let Some(formed) = self
            .timeout_pool
            .try_form_best_timeout_cert_for_view(view, &self.validators)
        else {
            return;
        };
        if formed.view != view {
            return;
        }
        self.maybe_upgrade_high_prepare_qc(&formed.high_qc);
        self.timeout_pool.prune_view(view);
        self.view = view.wrapping_add(1);
        self.view_entered_at_ms = now_ms();
        self.consecutive_timeout_failures = self.consecutive_timeout_failures.saturating_add(1);
        self.timeout_sent_for_view = None;
        self.timeout_pool
            .prune_views_before(self.view.saturating_sub(10));
    }

    fn maybe_emit_local_timeout(&mut self) {
        if self.validators.len() <= 1 {
            return;
        }
        let Some(ref sk) = self.validator_secret else {
            return;
        };
        let now = now_ms();
        let base = self.pacemaker_base_ms();
        let pow = self.consecutive_timeout_failures.min(17);
        let timeout_ms = base.saturating_mul(1u64 << pow).min(60_000);
        if now < self.view_entered_at_ms.saturating_add(timeout_ms) {
            return;
        }
        if self.timeout_sent_for_view == Some(self.view) {
            return;
        }
        let body = TimeoutSignBody {
            view: self.view,
            high_qc: self.high_prepare_qc.clone(),
        };
        let t = Timeout::sign(body, self.validator_index as u32, sk);
        let out = self.timeout_pool.record(t.clone(), &self.validators);
        match out {
            RecordTimeoutOutcome::BadSignature | RecordTimeoutOutcome::OutOfRange => {
                eprintln!("fractal-node: local timeout record failed: {out:?}");
                return;
            }
            _ => {}
        }
        self.timeout_sent_for_view = Some(self.view);
        if let Some(ref tx) = self.timeout_sink {
            if let Ok(bytes) = borsh::to_vec(&t) {
                let _ = tx.send(bytes);
            }
        }
    }

    pub fn record_timeout(&mut self, t: Timeout) -> RecordTimeoutOutcome {
        self.timeout_pool.record(t, &self.validators)
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
        let w = self.consensus_stake_weights();
        let out = self
            .vote_pool
            .record(vote.clone(), &self.validators, Some(&w));
        if matches!(
            out,
            RecordVoteOutcome::Accepted | RecordVoteOutcome::ReachedQuorum
        ) {
            if let Some(ref db) = self.chain_store {
                if let Err(e) = db.persist_consensus_vote_v1(
                    self.shard_id,
                    self.shard_topology.shard_count,
                    &vote,
                ) {
                    eprintln!(
                        "fractal-node: consensus vote RocksDB height={} err={e}",
                        vote.height
                    );
                }
            }
        }
        out
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
        let w = self.consensus_stake_weights();
        let started = Instant::now();
        let formed =
            self.vote_pool
                .try_form_qc(view, block_height, header_hash, &self.validators, Some(&w));
        if formed.is_some() {
            self.metrics
                .qc_formation_latency_ms
                .observe_ms(started.elapsed().as_millis() as u64);
        }
        formed
    }

    /// Inject dev BLS votes for the pipeline vote slot or chain tip (BFT-7 / load tests).
    pub fn inject_quorum_votes_for_pipeline_or_tip(&mut self) {
        let (view, height, hh) = if let Some(slot) = self.three_stage.vote.as_ref() {
            (
                slot.block.header.view,
                slot.block.header.height,
                slot.header_hash,
            )
        } else if let Some(slot) = self.three_stage.commit.as_ref() {
            (
                slot.block.header.view,
                slot.block.header.height,
                slot.header_hash,
            )
        } else if let Some(tip) = self.blocks.last() {
            let hh = header_hash(&tip.header).expect("header_hash");
            (tip.header.view, tip.header.height, hh)
        } else {
            return;
        };
        let need = self.validators.quorum_threshold();
        for idx in 0u32..(self.validators.len() as u32) {
            if self.vote_pool.count(view, hh) >= need {
                break;
            }
            let Some(sk) = self.validators.dev_bls_secret(idx as usize) else {
                continue;
            };
            let body = VoteSignBody {
                view,
                height,
                header_hash: hh,
            };
            let v = Vote::sign(body, idx, &sk);
            let _ = self.record_vote(v);
        }
    }

    /// Replay txs and check roots against a received block (follower verification).
    pub fn apply_synced_block(&mut self, block: &Block) -> Result<(), SyncApplyError> {
        if block.header.chain_id != self.chain_id {
            return Err(SyncApplyError::ChainId);
        }
        fractal_shard::validate_block_shard(
            block.header.shard_id,
            self.shard_id,
            &self.shard_topology,
        )?;
        if block.header.height != self.height + 1 {
            return Err(SyncApplyError::Height {
                expected: self.height + 1,
                got: block.header.height,
            });
        }
        if block.header.parent_hash != self.head_hash {
            return Err(SyncApplyError::ParentHash);
        }
        let computed_qc_hash = hash_qc(&block.parent_qc)?;
        if computed_qc_hash != block.header.parent_qc_hash {
            return Err(SyncApplyError::ParentQcHash);
        }
        if self.height == 0 {
            if !is_genesis_parent_qc(&block.parent_qc) {
                return Err(SyncApplyError::ParentQcBinding);
            }
            if !block.parent_qc_signer_indices.is_empty() {
                return Err(SyncApplyError::ParentQcBinding);
            }
        } else {
            let Some(parent_block) = self.block_at_height(self.height) else {
                return Err(SyncApplyError::ParentQcBinding);
            };
            let ph = header_hash(&parent_block.header)?;
            if block.parent_qc.block_header_hash != ph {
                return Err(SyncApplyError::ParentQcBinding);
            }
            if block.parent_qc.block_height != parent_block.header.height {
                return Err(SyncApplyError::ParentQcBinding);
            }
            if block.parent_qc.view != parent_block.header.view {
                return Err(SyncApplyError::ParentQcBinding);
            }
            let formed = FormedQc {
                qc: block.parent_qc.clone(),
                signer_indices: block.parent_qc_signer_indices.clone(),
            };
            let w = self.consensus_stake_weights();
            verify_formed_qc(&formed, &self.validators, Some(&w))
                .map_err(|e| SyncApplyError::InvalidQcSignature(format!("{e:?}")))?;
        }
        let expected_proposer = self.validators.expected_proposer(block.header.view);
        if block.header.proposer != expected_proposer {
            return Err(SyncApplyError::InvalidProposer);
        }
        if block.eth_signed_raw.len() != block.transactions.len() {
            return Err(SyncApplyError::BlockEthRawLayout);
        }
        let mut scratch = self.state.clone();
        let mut evm = fractal_evm::RevmEngine::default();
        let gas = fractal_core::apply_block_with_evm(&mut scratch, &block.transactions, &mut evm)?;
        if gas != block.header.gas_used {
            return Err(SyncApplyError::GasUsedMismatch {
                header: block.header.gas_used,
                replay: gas,
            });
        }
        let state_root_started = Instant::now();
        let sr = fractal_core::state_root(&scratch)?;
        self.metrics
            .state_root_computation_ms
            .observe_ms(state_root_started.elapsed().as_millis() as u64);
        if sr != block.header.state_root {
            return Err(SyncApplyError::StateRoot);
        }
        let tr = ordered_tx_root(&block.transactions)?;
        if tr != block.header.tx_root {
            return Err(SyncApplyError::TxRoot);
        }
        self.state = scratch;
        self.sync_permissionless_validators();
        self.height = block.header.height;
        let hh = header_hash(&block.header)?;
        self.head_hash = hh;
        self.view = block.header.view.wrapping_add(1);
        self.view_entered_at_ms = now_ms();
        self.consecutive_timeout_failures = 0;
        self.timeout_sent_for_view = None;
        self.base_fee = next_base_fee(self.base_fee, block.header.gas_used, &self.fee_params);
        self.sync_rpc_index_from_block(block);
        self.forward_vote_after_commit(block);
        self.maybe_upgrade_high_prepare_qc(&block.parent_qc);
        self.blocks.push(block.clone());
        self.maybe_persist_committed_block_to_rocksdb(block);
        self.maybe_emit_shard_anchor(block);
        if let Some(formed) = self.try_form_qc(block.header.view, block.header.height, hh) {
            self.parent_qc_hash = hash_qc(&formed.qc)?;
            self.maybe_upgrade_high_prepare_qc(&formed.qc);
            if let Some(ref db) = self.chain_store {
                if let Err(e) = db.persist_consensus_formed_qc_v1(
                    self.shard_id,
                    self.shard_topology.shard_count,
                    &formed,
                ) {
                    eprintln!(
                        "fractal-node: consensus formed QC RocksDB height={} err={e}",
                        block.header.height
                    );
                }
            }
        }
        self.vote_pool
            .prune_below_height(block.header.height.saturating_sub(1));
        self.maybe_enqueue_proof_checkpoint(block);
        Ok(())
    }

    /// Serialize the current chain tip bundle for [`fractal_network::SyncRequest::GetSnapshot`].
    pub fn encode_chain_snapshot_v1(&self) -> Result<Vec<u8>, std::io::Error> {
        let snap = crate::chain_snapshot::ChainSyncSnapshotV1 {
            version: crate::chain_snapshot::CHAIN_SYNC_SNAPSHOT_V1_VERSION,
            chain_id: self.chain_id,
            shard_id: self.shard_id,
            shard_count: self.shard_topology.shard_count,
            height: self.height,
            view: self.view,
            head_hash: self.head_hash,
            parent_qc_hash: self.parent_qc_hash,
            high_prepare_qc: self.high_prepare_qc.clone(),
            validators: self.validators.entries().to_vec(),
            state: self.state.clone(),
            blocks: self.blocks.clone(),
            base_fee: self.base_fee,
            gas_limit: self.gas_limit,
            fee_params: self.fee_params.clone(),
            min_consensus_stake_wei: self.min_consensus_stake_wei,
        };
        borsh::to_vec(&snap)
    }

    /// Serialize a PRD §10.4 v2 snapshot: chunked state payload + state root + EVM account MPT root.
    pub fn encode_chain_snapshot_v2(&self) -> Result<Vec<u8>, std::io::Error> {
        let state_bytes = borsh::to_vec(&self.state)?;
        let state_root_started = Instant::now();
        let state_root = fractal_core::state_root(&self.state)?;
        self.metrics
            .state_root_computation_ms
            .observe_ms(state_root_started.elapsed().as_millis() as u64);
        let (evm_account_mpt_root, _nodes) =
            fractal_storage::evm_accounts_mpt::evm_account_mpt_root_and_nodes(&self.state);
        let snap = crate::chain_snapshot::ChainSyncSnapshotV2 {
            version: crate::chain_snapshot::CHAIN_SYNC_SNAPSHOT_V2_VERSION,
            chain_id: self.chain_id,
            shard_id: self.shard_id,
            shard_count: self.shard_topology.shard_count,
            height: self.height,
            view: self.view,
            head_hash: self.head_hash,
            parent_qc_hash: self.parent_qc_hash,
            high_prepare_qc: self.high_prepare_qc.clone(),
            validators: self.validators.entries().to_vec(),
            state_root,
            evm_account_mpt_root,
            state_borsh_hash: keccak256(&state_bytes),
            state_len: state_bytes.len() as u64,
            state_chunks: crate::chain_snapshot::snapshot_v2_chunks(
                crate::chain_snapshot::SNAPSHOT_V2_STATE_CHUNK_KIND,
                &state_bytes,
                crate::chain_snapshot::SNAPSHOT_V2_DEFAULT_CHUNK_BYTES,
            ),
            blocks: self.blocks.clone(),
            base_fee: self.base_fee,
            gas_limit: self.gas_limit,
            fee_params: self.fee_params.clone(),
            min_consensus_stake_wei: self.min_consensus_stake_wei,
        };
        borsh::to_vec(&snap)
    }

    /// Serialize a pruned proof-chain fast-sync snapshot: verified state + checkpoint tip block +
    /// masterchain proof chain, without the full raw execution block vector.
    pub fn encode_chain_proof_snapshot_v1(&self) -> Result<Vec<u8>, std::io::Error> {
        if self.height == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "proof-chain snapshot requires height > 0",
            ));
        }
        let Some(tip_block) = self.block_at_height(self.height).cloned() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "proof-chain snapshot requires retained tip block",
            ));
        };
        let state_bytes = borsh::to_vec(&self.state)?;
        let state_root_started = Instant::now();
        let state_root = fractal_core::state_root(&self.state)?;
        self.metrics
            .state_root_computation_ms
            .observe_ms(state_root_started.elapsed().as_millis() as u64);
        let (evm_account_mpt_root, _nodes) =
            fractal_storage::evm_accounts_mpt::evm_account_mpt_root_and_nodes(&self.state);
        let snap = crate::chain_snapshot::ChainSyncProofSnapshotV1 {
            version: crate::chain_snapshot::CHAIN_SYNC_PROOF_SNAPSHOT_V1_VERSION,
            chain_id: self.chain_id,
            shard_id: self.shard_id,
            shard_count: self.shard_topology.shard_count,
            height: self.height,
            view: self.view,
            head_hash: self.head_hash,
            parent_qc_hash: self.parent_qc_hash,
            high_prepare_qc: self.high_prepare_qc.clone(),
            validators: self.validators.entries().to_vec(),
            state_root,
            evm_account_mpt_root,
            state_borsh_hash: keccak256(&state_bytes),
            state_len: state_bytes.len() as u64,
            state_chunks: crate::chain_snapshot::snapshot_v2_chunks(
                crate::chain_snapshot::SNAPSHOT_V2_STATE_CHUNK_KIND,
                &state_bytes,
                crate::chain_snapshot::SNAPSHOT_V2_DEFAULT_CHUNK_BYTES,
            ),
            tip_block,
            masterchain_blocks: self.masterchain_ledger.blocks.clone(),
            plonky2: self.masterchain_ledger.plonky2_bundle().cloned(),
            base_fee: self.base_fee,
            gas_limit: self.gas_limit,
            fee_params: self.fee_params.clone(),
            min_consensus_stake_wei: self.min_consensus_stake_wei,
        };
        borsh::to_vec(&snap)
    }

    pub fn encode_preferred_chain_snapshot(&self) -> Result<Vec<u8>, std::io::Error> {
        if proof_chain_fast_sync_from_env() {
            match self.encode_chain_proof_snapshot_v1() {
                Ok(bytes) => return Ok(bytes),
                Err(e) => eprintln!(
                    "fractal-node: proof-chain snapshot unavailable ({e}); falling back to v2"
                ),
            }
        }
        self.encode_chain_snapshot_v2()
    }

    fn apply_decoded_chain_snapshot_v1(
        &mut self,
        snap: crate::chain_snapshot::ChainSyncSnapshotV1,
    ) -> Result<(), SyncApplyError> {
        if self.height != 0 {
            return Err(SyncApplyError::Snapshot(
                "refusing snapshot: local chain is not empty (height > 0)".into(),
            ));
        }
        if snap.version != crate::chain_snapshot::CHAIN_SYNC_SNAPSHOT_V1_VERSION {
            return Err(SyncApplyError::Snapshot(format!(
                "unsupported snapshot version {}",
                snap.version
            )));
        }
        if snap.chain_id != self.chain_id {
            return Err(SyncApplyError::ChainId);
        }
        if snap.shard_id != self.shard_id {
            return Err(SyncApplyError::Snapshot(format!(
                "snapshot shard_id {} != local {}",
                snap.shard_id, self.shard_id
            )));
        }
        if snap.shard_count != self.shard_topology.shard_count {
            return Err(SyncApplyError::Snapshot(format!(
                "snapshot shard_count {} != local {}",
                snap.shard_count, self.shard_topology.shard_count
            )));
        }
        if snap.height != snap.blocks.len() as u64 {
            return Err(SyncApplyError::Snapshot(format!(
                "height {} != blocks.len {}",
                snap.height,
                snap.blocks.len()
            )));
        }
        if snap.height > 0 {
            let Some(last) = snap.blocks.last() else {
                return Err(SyncApplyError::Snapshot(
                    "height > 0 but blocks empty".into(),
                ));
            };
            let hh = header_hash(&last.header)?;
            if hh != snap.head_hash {
                return Err(SyncApplyError::Snapshot(
                    "head_hash does not match last block header".into(),
                ));
            }
            let sr = fractal_core::state_root(&snap.state)?;
            if sr != last.header.state_root {
                return Err(SyncApplyError::StateRoot);
            }
        } else if !snap.blocks.is_empty() {
            return Err(SyncApplyError::Snapshot(
                "height 0 but blocks non-empty".into(),
            ));
        } else if snap.head_hash != genesis_parent_hash() {
            return Err(SyncApplyError::Snapshot(
                "height-0 snapshot head_hash must match genesis parent hash".into(),
            ));
        }

        self.shard_id = snap.shard_id;
        self.shard_topology = ShardTopology {
            shard_count: snap.shard_count,
        };
        self.state = snap.state;
        self.blocks = snap.blocks;
        self.height = snap.height;
        self.view = snap.view;
        self.head_hash = snap.head_hash;
        self.parent_qc_hash = snap.parent_qc_hash;
        self.high_prepare_qc = snap.high_prepare_qc;
        self.validators = ValidatorSet::from_entries(snap.validators);
        self.sync_permissionless_validators();

        self.vote_pool = VotePool::new();
        self.timeout_pool = TimeoutPool::new();
        self.mempool = Mempool::default();
        self.pending_txs.clear();
        self.mined_txs.clear();
        self.eth_signed_raw.clear();
        self.eth_rpc_to_internal_tx_hash.clear();
        self.eth_internal_to_rpc_tx_hash.clear();

        self.base_fee = snap.base_fee;
        self.gas_limit = snap.gas_limit;
        self.fee_params = snap.fee_params;
        self.min_consensus_stake_wei = snap.min_consensus_stake_wei;

        self.view_entered_at_ms = now_ms();
        self.consecutive_timeout_failures = 0;
        self.timeout_sent_for_view = None;

        for block in self.blocks.clone() {
            self.sync_rpc_index_from_block(&block);
        }

        self.validator_secret =
            devnet_validator_secret_from_env(&self.validators, self.validator_index);
        if let Some(tip) = self.blocks.last().cloned() {
            self.forward_vote_after_commit(&tip);
        }
        self.reindex_chain_store_after_snapshot();
        Ok(())
    }

    /// Apply a trusted v1 snapshot from another node (follower fast sync). Requires local
    /// `height == 0`. Rebuilds RPC indexes from `blocks`; refreshes `validator_secret` from the
    /// same env-based rules as `run_follower` (custom `FRACTAL_VALIDATOR_SECRET_HEX` is not
    /// re-parsed mid-process elsewhere).
    pub fn apply_chain_snapshot_v1(&mut self, bytes: &[u8]) -> Result<(), SyncApplyError> {
        let snap: crate::chain_snapshot::ChainSyncSnapshotV1 = borsh::from_slice(bytes)
            .map_err(|e| SyncApplyError::Snapshot(format!("decode: {e}")))?;
        self.apply_decoded_chain_snapshot_v1(snap)
    }

    fn apply_decoded_chain_snapshot_v2(
        &mut self,
        snap: crate::chain_snapshot::ChainSyncSnapshotV2,
    ) -> Result<(), SyncApplyError> {
        if self.height != 0 {
            return Err(SyncApplyError::Snapshot(
                "refusing snapshot: local chain is not empty (height > 0)".into(),
            ));
        }
        if snap.version != crate::chain_snapshot::CHAIN_SYNC_SNAPSHOT_V2_VERSION {
            return Err(SyncApplyError::Snapshot(format!(
                "unsupported snapshot v2 version {}",
                snap.version
            )));
        }
        if snap.chain_id != self.chain_id {
            return Err(SyncApplyError::ChainId);
        }
        if snap.shard_id != self.shard_id {
            return Err(SyncApplyError::Snapshot(format!(
                "snapshot shard_id {} != local {}",
                snap.shard_id, self.shard_id
            )));
        }
        if snap.shard_count != self.shard_topology.shard_count {
            return Err(SyncApplyError::Snapshot(format!(
                "snapshot shard_count {} != local {}",
                snap.shard_count, self.shard_topology.shard_count
            )));
        }
        if snap.height != snap.blocks.len() as u64 {
            return Err(SyncApplyError::Snapshot(format!(
                "height {} != blocks.len {}",
                snap.height,
                snap.blocks.len()
            )));
        }
        let state_bytes = crate::chain_snapshot::reassemble_snapshot_v2_chunks(
            &snap.state_chunks,
            crate::chain_snapshot::SNAPSHOT_V2_STATE_CHUNK_KIND,
            snap.state_len,
            snap.state_borsh_hash,
        )
        .map_err(|e| SyncApplyError::Snapshot(format!("snapshot v2 chunks: {e}")))?;
        let state: State = borsh::from_slice(&state_bytes)
            .map_err(|e| SyncApplyError::Snapshot(format!("snapshot v2 state decode: {e}")))?;
        if fractal_core::state_root(&state)? != snap.state_root {
            return Err(SyncApplyError::StateRoot);
        }
        let (evm_account_mpt_root, _nodes) =
            fractal_storage::evm_accounts_mpt::evm_account_mpt_root_and_nodes(&state);
        if evm_account_mpt_root != snap.evm_account_mpt_root {
            return Err(SyncApplyError::Snapshot(
                "snapshot v2 EVM account MPT root mismatch".into(),
            ));
        }
        self.persist_snapshot_v2_chunks(&snap, &state_bytes);
        let v1_equivalent = crate::chain_snapshot::ChainSyncSnapshotV1 {
            version: crate::chain_snapshot::CHAIN_SYNC_SNAPSHOT_V1_VERSION,
            chain_id: snap.chain_id,
            shard_id: snap.shard_id,
            shard_count: snap.shard_count,
            height: snap.height,
            view: snap.view,
            head_hash: snap.head_hash,
            parent_qc_hash: snap.parent_qc_hash,
            high_prepare_qc: snap.high_prepare_qc,
            validators: snap.validators,
            state,
            blocks: snap.blocks,
            base_fee: snap.base_fee,
            gas_limit: snap.gas_limit,
            fee_params: snap.fee_params,
            min_consensus_stake_wei: snap.min_consensus_stake_wei,
        };
        self.apply_decoded_chain_snapshot_v1(v1_equivalent)
    }

    fn apply_decoded_chain_proof_snapshot_v1(
        &mut self,
        snap: crate::chain_snapshot::ChainSyncProofSnapshotV1,
    ) -> Result<(), SyncApplyError> {
        if self.height != 0 {
            return Err(SyncApplyError::Snapshot(
                "refusing snapshot: local chain is not empty (height > 0)".into(),
            ));
        }
        if snap.version != crate::chain_snapshot::CHAIN_SYNC_PROOF_SNAPSHOT_V1_VERSION {
            return Err(SyncApplyError::Snapshot(format!(
                "unsupported proof snapshot version {}",
                snap.version
            )));
        }
        if snap.chain_id != self.chain_id {
            return Err(SyncApplyError::ChainId);
        }
        if snap.shard_id != self.shard_id {
            return Err(SyncApplyError::Snapshot(format!(
                "snapshot shard_id {} != local {}",
                snap.shard_id, self.shard_id
            )));
        }
        if snap.shard_count != self.shard_topology.shard_count {
            return Err(SyncApplyError::Snapshot(format!(
                "snapshot shard_count {} != local {}",
                snap.shard_count, self.shard_topology.shard_count
            )));
        }
        if snap.height == 0 || snap.tip_block.header.height != snap.height {
            return Err(SyncApplyError::Snapshot(
                "proof snapshot tip block must match non-zero height".into(),
            ));
        }
        if snap.tip_block.header.chain_id != snap.chain_id
            || snap.tip_block.header.shard_id != snap.shard_id
        {
            return Err(SyncApplyError::Snapshot(
                "proof snapshot tip block chain/shard mismatch".into(),
            ));
        }
        let tip_hash = header_hash(&snap.tip_block.header)?;
        if tip_hash != snap.head_hash {
            return Err(SyncApplyError::Snapshot(
                "proof snapshot head_hash does not match tip block header".into(),
            ));
        }
        if snap.tip_block.header.state_root != snap.state_root {
            return Err(SyncApplyError::StateRoot);
        }
        self.verify_proof_snapshot_masterchain(&snap)?;

        let state_bytes = crate::chain_snapshot::reassemble_snapshot_v2_chunks(
            &snap.state_chunks,
            crate::chain_snapshot::SNAPSHOT_V2_STATE_CHUNK_KIND,
            snap.state_len,
            snap.state_borsh_hash,
        )
        .map_err(|e| SyncApplyError::Snapshot(format!("proof snapshot chunks: {e}")))?;
        let state: State = borsh::from_slice(&state_bytes)
            .map_err(|e| SyncApplyError::Snapshot(format!("proof snapshot state decode: {e}")))?;
        if fractal_core::state_root(&state)? != snap.state_root {
            return Err(SyncApplyError::StateRoot);
        }
        let (evm_account_mpt_root, _nodes) =
            fractal_storage::evm_accounts_mpt::evm_account_mpt_root_and_nodes(&state);
        if evm_account_mpt_root != snap.evm_account_mpt_root {
            return Err(SyncApplyError::Snapshot(
                "proof snapshot EVM account MPT root mismatch".into(),
            ));
        }
        self.persist_proof_snapshot_chunks(&snap);

        self.shard_id = snap.shard_id;
        self.shard_topology = ShardTopology {
            shard_count: snap.shard_count,
        };
        self.state = state;
        self.blocks = vec![snap.tip_block];
        self.height = snap.height;
        self.view = snap.view;
        self.head_hash = snap.head_hash;
        self.parent_qc_hash = snap.parent_qc_hash;
        self.high_prepare_qc = snap.high_prepare_qc;
        self.validators = ValidatorSet::from_entries(snap.validators);
        self.sync_permissionless_validators();
        self.masterchain_ledger.blocks = snap.masterchain_blocks;
        self.masterchain_ledger.masterchain_height = self
            .masterchain_ledger
            .blocks
            .last()
            .map_or(0, |b| b.height);
        self.masterchain_ledger.latest_anchors.clear();
        if let Some(head) = self.masterchain_ledger.blocks.last() {
            for anchor in &head.shard_anchors {
                self.masterchain_ledger
                    .latest_anchors
                    .insert(anchor.shard_id, anchor.clone());
            }
        }
        self.masterchain_ledger.last_plonky2_bundle = snap.plonky2;

        self.vote_pool = VotePool::new();
        self.timeout_pool = TimeoutPool::new();
        self.mempool = Mempool::default();
        self.pending_txs.clear();
        self.mined_txs.clear();
        self.eth_signed_raw.clear();
        self.eth_rpc_to_internal_tx_hash.clear();
        self.eth_internal_to_rpc_tx_hash.clear();

        self.base_fee = snap.base_fee;
        self.gas_limit = snap.gas_limit;
        self.fee_params = snap.fee_params;
        self.min_consensus_stake_wei = snap.min_consensus_stake_wei;
        self.view_entered_at_ms = now_ms();
        self.consecutive_timeout_failures = 0;
        self.timeout_sent_for_view = None;

        if let Some(tip) = self.blocks.last().cloned() {
            self.sync_rpc_index_from_block(&tip);
            self.forward_vote_after_commit(&tip);
        }
        self.validator_secret =
            devnet_validator_secret_from_env(&self.validators, self.validator_index);
        self.persist_proof_snapshot_import(&state_bytes);
        Ok(())
    }

    fn verify_proof_snapshot_masterchain(
        &self,
        snap: &crate::chain_snapshot::ChainSyncProofSnapshotV1,
    ) -> Result<(), SyncApplyError> {
        let Some(head) = snap.masterchain_blocks.last() else {
            return Err(SyncApplyError::Snapshot(
                "proof snapshot missing masterchain proof chain".into(),
            ));
        };
        for pair in snap.masterchain_blocks.windows(2) {
            if pair[1].height != pair[0].height.saturating_add(1) {
                return Err(SyncApplyError::Snapshot(
                    "proof snapshot masterchain heights are not contiguous".into(),
                ));
            }
        }
        let Some(anchor) = head
            .shard_anchors
            .iter()
            .find(|a| a.shard_id == snap.shard_id && a.block_height == snap.height)
        else {
            return Err(SyncApplyError::Snapshot(
                "proof snapshot masterchain head lacks matching shard anchor".into(),
            ));
        };
        if anchor.state_root != snap.state_root {
            return Err(SyncApplyError::StateRoot);
        }
        let computed_gsr = fractal_shard::global_state_root_from_anchors(&head.shard_anchors);
        if computed_gsr != head.global_state_root {
            return Err(SyncApplyError::Snapshot(
                "proof snapshot globalStateRoot mismatch".into(),
            ));
        }
        let proofs = fractal_proof_aggregator::dedupe_submissions(&head.validity_proofs)
            .map_err(|e| SyncApplyError::Snapshot(format!("proof snapshot submissions: {e}")))?;
        for proof in &proofs {
            fractal_proof_aggregator::validate_proof_submission(proof, &head.shard_anchors)
                .map_err(|e| SyncApplyError::Snapshot(format!("proof snapshot proof: {e}")))?;
        }
        match snap.plonky2.as_ref() {
            Some(bundle) => {
                if bundle.masterchain_height != head.height
                    || bundle.statement.global_state_root != head.global_state_root
                    || bundle.statement.global_zk_root != head.global_zk_root
                    || bundle.statement.validity_proofs != proofs
                {
                    return Err(SyncApplyError::Snapshot(
                        "proof snapshot Plonky2 statement mismatch".into(),
                    ));
                }
                bundle.verify().map_err(|e| {
                    SyncApplyError::Snapshot(format!("proof snapshot Plonky2: {e}"))
                })?;
            }
            None => {
                if !proofs.is_empty() {
                    return Err(SyncApplyError::Snapshot(
                        "proof snapshot missing Plonky2 bundle".into(),
                    ));
                }
                fractal_proof_aggregator::verify_global_zk_root(
                    head.height,
                    &head.global_state_root,
                    &proofs,
                    &head.global_zk_root,
                    None,
                )
                .map_err(|e| SyncApplyError::Snapshot(format!("proof snapshot zk root: {e}")))?;
            }
        }
        Ok(())
    }

    pub fn apply_chain_snapshot_v2(&mut self, bytes: &[u8]) -> Result<(), SyncApplyError> {
        let snap: crate::chain_snapshot::ChainSyncSnapshotV2 = borsh::from_slice(bytes)
            .map_err(|e| SyncApplyError::Snapshot(format!("decode v2: {e}")))?;
        self.apply_decoded_chain_snapshot_v2(snap)
    }

    pub fn apply_chain_proof_snapshot_v1(&mut self, bytes: &[u8]) -> Result<(), SyncApplyError> {
        let snap: crate::chain_snapshot::ChainSyncProofSnapshotV1 = borsh::from_slice(bytes)
            .map_err(|e| SyncApplyError::Snapshot(format!("decode proof snapshot: {e}")))?;
        self.apply_decoded_chain_proof_snapshot_v1(snap)
    }

    pub fn apply_chain_snapshot_auto(&mut self, bytes: &[u8]) -> Result<(), SyncApplyError> {
        match borsh::from_slice::<crate::chain_snapshot::ChainSyncProofSnapshotV1>(bytes) {
            Ok(snap)
                if snap.version == crate::chain_snapshot::CHAIN_SYNC_PROOF_SNAPSHOT_V1_VERSION =>
            {
                return self.apply_decoded_chain_proof_snapshot_v1(snap);
            }
            _ => {}
        }
        match borsh::from_slice::<crate::chain_snapshot::ChainSyncSnapshotV2>(bytes) {
            Ok(snap) if snap.version == crate::chain_snapshot::CHAIN_SYNC_SNAPSHOT_V2_VERSION => {
                return self.apply_decoded_chain_snapshot_v2(snap);
            }
            _ => {}
        }
        self.apply_chain_snapshot_v1(bytes)
    }

    fn persist_snapshot_v2_chunks(
        &self,
        snap: &crate::chain_snapshot::ChainSyncSnapshotV2,
        full_state_bytes: &[u8],
    ) {
        let Some(ref db) = self.chain_store else {
            return;
        };
        let sc = self.shard_topology.shard_count;
        if let Ok(manifest) = borsh::to_vec(snap) {
            if let Err(e) = db.put_snapshot_v2_manifest(self.shard_id, sc, snap.height, &manifest) {
                eprintln!("fractal-node: snapshot v2 manifest RocksDB err={e}");
            }
        }
        if let Err(e) = db.put_snapshot_blob(self.shard_id, sc, snap.height, full_state_bytes) {
            eprintln!("fractal-node: snapshot v2 state payload RocksDB err={e}");
        }
        for chunk in &snap.state_chunks {
            if let Err(e) = db.put_snapshot_v2_state_chunk(
                self.shard_id,
                sc,
                snap.height,
                chunk.index,
                &chunk.bytes,
            ) {
                eprintln!(
                    "fractal-node: snapshot v2 chunk {} RocksDB err={e}",
                    chunk.index
                );
            }
        }
    }

    fn persist_proof_snapshot_chunks(
        &self,
        snap: &crate::chain_snapshot::ChainSyncProofSnapshotV1,
    ) {
        let Some(ref db) = self.chain_store else {
            return;
        };
        let sc = self.shard_topology.shard_count;
        if let Ok(manifest) = borsh::to_vec(snap) {
            if let Err(e) = db.put_snapshot_v2_manifest(self.shard_id, sc, snap.height, &manifest) {
                eprintln!("fractal-node: proof snapshot manifest RocksDB err={e}");
            }
        }
        for chunk in &snap.state_chunks {
            if let Err(e) = db.put_snapshot_v2_state_chunk(
                self.shard_id,
                sc,
                snap.height,
                chunk.index,
                &chunk.bytes,
            ) {
                eprintln!(
                    "fractal-node: proof snapshot chunk {} RocksDB err={e}",
                    chunk.index
                );
            }
        }
    }

    fn persist_proof_snapshot_import(&self, full_state_bytes: &[u8]) {
        let Some(ref db) = self.chain_store else {
            return;
        };
        let sc = self.shard_topology.shard_count;
        if let Err(e) = db.put_snapshot_blob(self.shard_id, sc, self.height, full_state_bytes) {
            eprintln!("fractal-node: proof snapshot state payload RocksDB err={e}");
        }
        if let Err(e) = db.persist_state_at_height_v1(self.shard_id, sc, self.height, &self.state) {
            eprintln!("fractal-node: proof snapshot cf_state err={e}");
        }
        if let Some(tip) = self.blocks.last() {
            if let Some(hashes) = self.rpc_tx_hashes_for_committed_block(tip) {
                if let Err(e) = db.persist_block_indexes_v1(tip, &self.state, &hashes, sc) {
                    eprintln!("fractal-node: proof snapshot tip block index err={e}");
                }
            }
        }
        if let Some(head) = self.masterchain_ledger.blocks.last() {
            for anchor in &head.shard_anchors {
                if let Err(e) = db.persist_shard_anchor_v1(anchor) {
                    eprintln!(
                        "fractal-node: proof snapshot shard anchor RocksDB shard={} height={} err={e}",
                        anchor.shard_id, anchor.block_height
                    );
                }
            }
        }
        for mc in &self.masterchain_ledger.blocks {
            if let Err(e) = db.persist_masterchain_block_v1(mc) {
                eprintln!(
                    "fractal-node: proof snapshot masterchain block RocksDB height={} err={e}",
                    mc.height
                );
            }
        }
    }

    /// Explicit Track A → Track B cutover path: import a monolith snapshot as shard 0.
    ///
    /// This is intentionally narrow. It only accepts snapshots from `shard_id=0` /
    /// `shard_count=1`, only into an empty local shard-0 node configured with
    /// `FRACTAL_SHARD_COUNT > 1`, and leaves block headers tagged as shard 0.
    pub fn apply_monolith_snapshot_to_shard0_v1(
        &mut self,
        bytes: &[u8],
    ) -> Result<(), SyncApplyError> {
        if self.shard_id != 0 {
            return Err(SyncApplyError::Snapshot(
                "monolith migration target must be shard_id 0".into(),
            ));
        }
        if self.shard_topology.shard_count <= 1 {
            return Err(SyncApplyError::Snapshot(
                "monolith migration target must set shard_count > 1".into(),
            ));
        }
        let target_shard_count = self.shard_topology.shard_count;
        let mut snap: crate::chain_snapshot::ChainSyncSnapshotV1 = borsh::from_slice(bytes)
            .map_err(|e| SyncApplyError::Snapshot(format!("decode: {e}")))?;
        if snap.shard_id != 0 || snap.shard_count != 1 {
            return Err(SyncApplyError::Snapshot(format!(
                "source snapshot must be Track A shard 0 monolith (got shard_id={} shard_count={})",
                snap.shard_id, snap.shard_count
            )));
        }
        if snap.blocks.iter().any(|b| b.header.shard_id != 0) {
            return Err(SyncApplyError::Snapshot(
                "source snapshot contains non-zero shard block".into(),
            ));
        }
        snap.shard_count = target_shard_count;
        self.apply_decoded_chain_snapshot_v1(snap)
    }

    /// RPC / eth transaction hashes for each tx in a committed block (aligned with `sync_rpc_index_from_block`).
    fn rpc_tx_hashes_for_committed_block(
        &self,
        block: &Block,
    ) -> Option<Vec<fractal_crypto::Hash256>> {
        let mut out = Vec::with_capacity(block.transactions.len());
        for (i, tx) in block.transactions.iter().enumerate() {
            let borsh_raw = borsh::to_vec(tx).ok()?;
            let ih = keccak256(&borsh_raw);
            let rpc_h = if let Some(Some(eth_raw)) = block.eth_signed_raw.get(i) {
                let eh = keccak256(eth_raw);
                if eh != ih { eh } else { ih }
            } else {
                ih
            };
            out.push(rpc_h);
        }
        Some(out)
    }

    fn block_at_height(&self, height: u64) -> Option<&Block> {
        self.blocks.iter().find(|b| b.header.height == height)
    }

    fn maybe_persist_committed_block_to_rocksdb(&self, block: &Block) {
        let Some(ref db) = self.chain_store else {
            return;
        };
        let Some(hashes) = self.rpc_tx_hashes_for_committed_block(block) else {
            eprintln!(
                "fractal-node: chain RocksDB persist skipped (tx borsh) height={}",
                block.header.height
            );
            return;
        };
        let sc = self.shard_topology.shard_count;
        if let Err(e) = db.persist_block_commit_v1(block, &self.state, &hashes, sc) {
            eprintln!(
                "fractal-node: chain RocksDB persist height={} err={e}",
                block.header.height
            );
        }
    }

    /// On anchor cadence, seal a [`ShardAnchor`] and append a local masterchain block (§7.10).
    fn maybe_emit_shard_anchor(&mut self, block: &Block) {
        if !should_emit_anchor_at_height(block.header.height, self.anchor_interval) {
            return;
        }
        let anchor = fractal_masterchain::anchor_from_block_header(&block.header);
        if let Some(url) = std::env::var("FRACTAL_MASTERCHAIN_RPC")
            .ok()
            .filter(|s| !s.trim().is_empty())
        {
            match fractal_masterchain::client::submit_shard_anchor_sync(url.trim(), &anchor) {
                Ok(()) => eprintln!(
                    "fractal-node: submitted shard anchor to masterchain shard={} height={}",
                    anchor.shard_id, anchor.block_height
                ),
                Err(e) => eprintln!(
                    "fractal-node: masterchain anchor submit failed shard={} height={}: {e}",
                    anchor.shard_id, anchor.block_height
                ),
            }
            return;
        }
        let prover = prover_address_from_env();
        let Ok(mc) = self.masterchain_ledger.seal_anchor(anchor.clone(), prover) else {
            eprintln!(
                "fractal-node: masterchain seal failed shard={} height={}",
                anchor.shard_id, anchor.block_height
            );
            return;
        };
        eprintln!(
            "fractal-node: masterchain height={} shard={} anchor_height={} global_state_root=0x{} global_zk_root=0x{}",
            mc.height,
            anchor.shard_id,
            anchor.block_height,
            hex::encode(mc.global_state_root),
            hex::encode(mc.global_zk_root)
        );
        if let Some(ref db) = self.chain_store {
            if let Err(e) = db.persist_shard_anchor_v1(&anchor) {
                eprintln!(
                    "fractal-node: shard anchor RocksDB shard={} height={} err={e}",
                    anchor.shard_id, anchor.block_height
                );
            }
            if let Err(e) = db.persist_masterchain_block_v1(&mc) {
                eprintln!(
                    "fractal-node: masterchain block RocksDB height={} err={e}",
                    mc.height
                );
            }
        }
        prune::maybe_prune_after_masterchain_seal(
            &mc,
            &mut self.blocks,
            &self.chain_store,
            self.shard_id,
            self.shard_topology.shard_count,
            self.height,
        );
    }

    /// After bulk snapshot import: rewrite block indexes from tip `State`, then one **`cf_state`** row.
    fn reindex_chain_store_after_snapshot(&self) {
        let Some(ref db) = self.chain_store else {
            return;
        };
        for block in &self.blocks {
            let Some(hashes) = self.rpc_tx_hashes_for_committed_block(block) else {
                continue;
            };
            let sc = self.shard_topology.shard_count;
            if let Err(e) = db.persist_block_indexes_v1(block, &self.state, &hashes, sc) {
                eprintln!(
                    "fractal-node: chain RocksDB reindex height={} err={e}",
                    block.header.height
                );
            }
        }
        if self.height > 0 {
            if let Err(e) = db.persist_state_at_height_v1(
                self.shard_id,
                self.shard_topology.shard_count,
                self.height,
                &self.state,
            ) {
                eprintln!("fractal-node: chain RocksDB tip state err={e}");
            }
        }
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
            if let Some(ref db) = self.chain_store {
                if let Err(e) =
                    db.mempool_delete_backup(self.shard_id, self.shard_topology.shard_count, &rpc_h)
                {
                    eprintln!("fractal-node: mempool RocksDB delete err={e}");
                }
            }
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

    fn shard_id(&self) -> u32 {
        self.shard_id
    }

    fn shard_count(&self) -> u32 {
        self.shard_topology.shard_count
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

    fn pending_transaction_count(&self, addr: &Address) -> u64 {
        let latest = self.transaction_count(addr);
        self.pending_txs
            .values()
            .filter(|tx| &tx.signer == addr)
            .map(|tx| tx.nonce.saturating_add(1))
            .max()
            .unwrap_or(latest)
            .max(latest)
    }

    fn submit_raw_tx(&mut self, raw: &[u8]) -> Result<[u8; 32], String> {
        // Accept either (a) borsh-encoded internal txs, or (b) Ethereum EIP-1559 signed bytes (0x02).
        if let Ok(tx) = Transaction::try_from_slice(raw) {
            fractal_shard::check_accepts_transaction(
                &tx.signer,
                self.shard_id,
                &self.shard_topology,
            )
            .map_err(|e| e.to_string())?;
            let h = keccak256(raw);
            self.pending_txs.insert(h, tx.clone());
            self.mempool.insert(PooledTx {
                tx,
                max_priority_fee_per_gas: 1,
                max_fee_per_gas: u128::MAX,
                eth_signed_raw: None,
            });
            if let Some(ref db) = self.chain_store {
                if let Err(e) = db.mempool_put_backup_v1(
                    self.shard_id,
                    self.shard_topology.shard_count,
                    &h,
                    raw,
                ) {
                    eprintln!("fractal-node: mempool RocksDB put err={e}");
                }
            }
            return Ok(h);
        }

        let (tx, h, max_priority_fee_per_gas, max_fee_per_gas) =
            eth_signed::to_core_tx(raw, self.chain_id)?;
        fractal_shard::check_accepts_transaction(&tx.signer, self.shard_id, &self.shard_topology)
            .map_err(|e| e.to_string())?;
        self.pending_txs.insert(h, tx.clone());
        self.eth_signed_raw.insert(h, raw.to_vec());
        self.mempool.insert(PooledTx {
            tx,
            max_priority_fee_per_gas,
            max_fee_per_gas,
            eth_signed_raw: Some(raw.to_vec()),
        });
        if let Some(ref db) = self.chain_store {
            if let Err(e) =
                db.mempool_put_backup_v1(self.shard_id, self.shard_topology.shard_count, &h, raw)
            {
                eprintln!("fractal-node: mempool RocksDB put err={e}");
            }
        }
        Ok(h)
    }

    fn base_fee_per_gas(&self) -> u128 {
        self.base_fee
    }

    fn block_hash_by_number(&self, number: u64) -> Option<[u8; 32]> {
        if number == 0 {
            return Some(genesis_parent_hash());
        }
        let b = self.block_at_height(number)?;
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
                    gas_used: 0,
                    gas_limit: self.gas_limit,
                    shard_id: self.shard_id,
                    extra: [0u8; 32],
                },
                transactions: Vec::new(),
                eth_signed_raw: Vec::new(),
                parent_qc: genesis_parent_qc(),
                parent_qc_signer_indices: Vec::new(),
            });
        }
        self.blocks
            .iter()
            .find(|b| header_hash(&b.header).ok().as_ref() == Some(hash))
            .cloned()
    }

    fn tx_by_hash(&self, hash: &[u8; 32]) -> Option<Transaction> {
        if let Some(tx) = self.pending_txs.get(hash) {
            return Some(tx.clone());
        }
        if let Some((bn, _bh, idx)) = self.mined_txs.get(hash) {
            if *bn == 0 {
                return None;
            }
            let block = self.block_at_height(*bn)?;
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

    fn fractal_getCheckpointProof(&self, height: u64) -> serde_json::Value {
        let Some(reg) = &self.proof_artifact_registry else {
            return serde_json::Value::Null;
        };
        let Some(entry) = reg.get(height) else {
            return serde_json::Value::Null;
        };
        entry.to_rpc_json()
    }

    fn fractal_getCheckpointProofDigest(&self, height: u64) -> serde_json::Value {
        let Some(reg) = &self.proof_artifact_registry else {
            return serde_json::Value::Null;
        };
        let Some(entry) = reg.get(height) else {
            return serde_json::Value::Null;
        };
        serde_json::Value::String(format!("0x{}", hex::encode(entry.proof_digest)))
    }

    fn fractal_get_wallet_revocation_merkle_root(&self) -> String {
        format!(
            "0x{}",
            hex::encode(self.state.wallet_revocation_merkle_root)
        )
    }

    fn fractal_get_wallet_revocation_entries(&self) -> serde_json::Value {
        let entries: Vec<serde_json::Value> = self
            .state
            .wallet_revocation_entries
            .iter()
            .map(|(cap_id, e)| {
                serde_json::json!({
                    "capId": format!("0x{}", hex::encode(cap_id)),
                    "revokedAtMs": e.revoked_at_ms,
                    "reasonCode": e.reason_code,
                    "cascade": e.cascade,
                })
            })
            .collect();
        serde_json::json!({
            "revocationRoot": format!("0x{}", hex::encode(self.state.wallet_revocation_merkle_root)),
            "entries": entries,
            "count": entries.len(),
        })
    }

    fn fractal_get_wallet_reputation(&self) -> serde_json::Value {
        let scores: Vec<serde_json::Value> = self
            .state
            .wallet_reputation_milli
            .iter()
            .map(|((provider_id, tool_class), score)| {
                let commitment = self
                    .state
                    .wallet_reputation_ledger_commitment
                    .get(&(*provider_id, *tool_class))
                    .copied();
                serde_json::json!({
                    "providerId": format!("0x{}", hex::encode(provider_id)),
                    "toolClass": tool_class,
                    "scoreMilli": score.to_string(),
                    "ledgerCommitment": commitment.map(|c| format!("0x{}", hex::encode(c))),
                })
            })
            .collect();
        serde_json::json!({
            "scores": scores,
            "count": scores.len(),
        })
    }

    fn fractal_get_wallet_emergency_stop(&self) -> bool {
        self.state.wallet_emergency_stop
    }

    fn fractal_home_shard_for_address(&self, addr: &[u8; 20]) -> u32 {
        fractal_shard::home_shard_for_address(addr, self.shard_topology.shard_count)
    }

    fn fractal_get_shard_anchor(
        &self,
        shard_id: u32,
        block_height: Option<u64>,
    ) -> Option<ShardAnchor> {
        let height = block_height.or_else(|| {
            self.masterchain_ledger
                .anchor_for_shard(shard_id)
                .map(|a| a.block_height)
        })?;
        if let Some(ref db) = self.chain_store {
            if let Ok(Some(a)) = db.get_shard_anchor_v1(shard_id, height) {
                return Some(a);
            }
        }
        self.masterchain_ledger
            .anchor_for_shard(shard_id)
            .filter(|a| a.block_height == height)
            .cloned()
    }

    fn fractal_get_consensus_mode(&self) -> &'static str {
        match self.consensus_mode {
            ConsensusMode::HyperBft => "hyperbft",
            ConsensusMode::HotStuff2 => "hotstuff2",
        }
    }

    fn fractal_get_target_block_time_ms(&self) -> u64 {
        self.effective_block_cadence_ms()
    }

    fn fractal_get_masterchain_head(&self) -> Option<fractal_shard::MasterchainBlockV1> {
        if let Some(h) = self.masterchain_ledger.head() {
            return Some(h.clone());
        }
        let mc_h = self.masterchain_ledger.masterchain_height;
        if mc_h == 0 {
            return None;
        }
        self.chain_store
            .as_ref()
            .and_then(|db| db.get_masterchain_block_v1(mc_h).ok().flatten())
    }

    fn fractal_get_delivered_cross_shard_messages(&self) -> serde_json::Value {
        self.delivered_cross_shard_messages_json()
    }

    fn fractal_submit_validity_proof(
        &mut self,
        submission: ProofSubmissionV1,
    ) -> Result<(), String> {
        self.masterchain_ledger
            .submit_validity_proof(submission)
            .map_err(|e| e.to_string())
    }

    fn fractal_get_global_zk_root(&self) -> Option<[u8; 32]> {
        self.masterchain_ledger.global_zk_root()
    }

    fn fractal_get_global_zk_proof(
        &self,
    ) -> Option<fractal_proof_aggregator::Plonky2ProofBundleV1> {
        self.masterchain_ledger.plonky2_bundle().cloned()
    }

    fn fractal_execution_tip_state_root(&self) -> Option<[u8; 32]> {
        fractal_core::state_root(&self.state).ok()
    }
}

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub(crate) const ENV_MIGRATE_MONOLITH_TO_SHARD0: &str = "FRACTAL_MIGRATE_MONOLITH_TO_SHARD0";
pub(crate) const ENV_MIGRATE_MONOLITH_SNAPSHOT_PATH: &str =
    "FRACTAL_MIGRATE_MONOLITH_SNAPSHOT_PATH";

pub(crate) fn monolith_migration_from_env() -> bool {
    match std::env::var(ENV_MIGRATE_MONOLITH_TO_SHARD0)
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "1" | "true" | "yes" | "on" => true,
        _ => false,
    }
}

/// When true (default), `run_dev` / `run_follower` spawn the async proof condenser (`docs/prd.md` §7.8).
/// Dev-only: synthesize quorum votes locally (BFT-7 lab until proposal gossip ships).
pub(crate) fn dev_inject_quorum_from_env() -> bool {
    match std::env::var("FRACTAL_DEV_INJECT_QUORUM")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "1" | "true" | "yes" | "on" => true,
        _ => false,
    }
}

fn async_proof_condenser_from_env() -> bool {
    match std::env::var("FRACTAL_ASYNC_PROOF")
        .unwrap_or_else(|_| "1".into())
        .to_ascii_lowercase()
        .as_str()
    {
        "0" | "false" | "off" | "no" => false,
        _ => true,
    }
}

/// When true (default on Track B / when anchors enabled), STWO digests auto-queue tier-1 proofs.
fn auto_validity_proof_from_env(anchor_interval: u64) -> bool {
    match std::env::var("FRACTAL_AUTO_VALIDITY_PROOF")
        .ok()
        .map(|s| s.trim().to_ascii_lowercase())
    {
        Some(s) if matches!(s.as_str(), "0" | "false" | "off" | "no") => false,
        Some(_) => true,
        None => anchor_interval > 0,
    }
}

fn prover_address_from_env() -> fractal_core::Address {
    let raw = std::env::var("FRACTAL_PROVER_ADDRESS").ok();
    let Some(s) = raw.filter(|s| !s.trim().is_empty()) else {
        return [0u8; 20];
    };
    let s = s.trim().strip_prefix("0x").unwrap_or(s.trim());
    let mut addr = [0u8; 20];
    if s.len() == 40 {
        if let Ok(bytes) = hex::decode(s) {
            if bytes.len() == 20 {
                addr.copy_from_slice(&bytes);
            }
        }
    }
    addr
}

async fn tier1_proof_submission_task(
    node: NodeHandle,
    mut rx: tokio::sync::mpsc::UnboundedReceiver<(CheckpointJob, [u8; 32])>,
) {
    while let Some((job, digest)) = rx.recv().await {
        let mut n = node.lock().await;
        let verified = n
            .proof_artifact_registry
            .as_ref()
            .and_then(|r| r.get(job.height))
            .and_then(|entry| entry.artifact_v1_borsh)
            .and_then(|artifact| {
                let sub = fractal_proof_aggregator::proof_submission_from_checkpoint_digest(
                    n.shard_id,
                    job.start_block,
                    job.end_block,
                    prover_address_from_env(),
                    digest,
                    0,
                );
                match verify_stwo_artifact_submission(&sub, &artifact) {
                    Ok(stmt) => Some(stmt),
                    Err(e) => {
                        eprintln!(
                            "fractal-node: STWO artifact verification skipped range=[{}..{}]: {e}",
                            job.start_block, job.end_block
                        );
                        None
                    }
                }
            });
        let recorded = if let Some(stmt) = verified {
            n.record_verified_stwo_for_masterchain(stmt)
        } else {
            n.record_stwo_for_masterchain(&job, digest)
        };
        match recorded {
            Ok(()) => eprintln!(
                "fractal-node: STWO digest buffered for anchor shard={} range=[{}..{}] digest=0x{}",
                n.shard_id,
                job.start_block,
                job.end_block,
                hex::encode(digest)
            ),
            Err(e) => eprintln!(
                "fractal-node: STWO digest buffer failed height={}: {e}",
                job.height
            ),
        }
    }
}

async fn wire_async_proof_stack(
    node: &NodeHandle,
    shared_rocks: &Option<fractal_storage::FractalRocksDb>,
    anchor_interval: u64,
) {
    if !async_proof_condenser_from_env() {
        return;
    }
    let (proof_tx, proof_rx) = tokio::sync::mpsc::channel::<CheckpointJob>(64);
    let (shard_id, shard_count) = {
        let n = node.lock().await;
        (n.shard_id, n.shard_topology.shard_count)
    };
    let reg = Arc::new(ProofArtifactRegistry::new(ProofPersistenceConfig {
        filesystem_dir: proof_artifact_dir_from_env(),
        rocksdb_path: if shared_rocks.is_some() {
            None
        } else {
            proof_rocksdb_path_from_env()
        },
        shared_rocksdb: shared_rocks.clone(),
        shard_id,
        shard_count,
    }));
    let tier1_tx: Option<Tier1DigestSink> = if auto_validity_proof_from_env(anchor_interval) {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        tokio::spawn(tier1_proof_submission_task(node.clone(), rx));
        eprintln!("fractal-node: STWO → tier1 auto-submit on (anchor_interval={anchor_interval})");
        Some(tx)
    } else {
        None
    };
    {
        let mut n = node.lock().await;
        n.set_proof_job_tx(Some(proof_tx));
        n.proof_artifact_registry = Some(reg.clone());
    }
    let _proof_worker = spawn_async_proof_condenser(proof_rx, Some(reg), tier1_tx);
}

/// Optional directory for `borsh` checkpoint proof **filesystem** sidecars (`{height:016}.proof.borsh`).
fn proof_artifact_dir_from_env() -> Option<std::path::PathBuf> {
    std::env::var("FRACTAL_PROOF_ARTIFACT_DIR")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
}

/// Optional RocksDB directory for checkpoint proofs and other PRD §10.3 column families (`FractalRocksDb`).
fn proof_rocksdb_path_from_env() -> Option<std::path::PathBuf> {
    std::env::var("FRACTAL_PROOF_ROCKSDB_PATH")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
}

/// When set, the node persists committed blocks / receipts / state snapshots (`cf_*`). Must match
/// [`proof_rocksdb_path_from_env`] when both are set so one process holds a single DB lock.
fn chain_rocksdb_path_from_env() -> Option<std::path::PathBuf> {
    std::env::var("FRACTAL_CHAIN_ROCKSDB_PATH")
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
}

fn unified_rocksdb_path_from_env() -> Result<Option<std::path::PathBuf>, String> {
    let chain = chain_rocksdb_path_from_env();
    let proof = proof_rocksdb_path_from_env();
    match (&chain, &proof) {
        (Some(c), Some(p)) if c != p => Err(format!(
            "FRACTAL_CHAIN_ROCKSDB_PATH and FRACTAL_PROOF_ROCKSDB_PATH must be the same directory (got {c:?} vs {p:?})"
        )),
        (Some(c), _) => Ok(Some(c.clone())),
        (None, Some(p)) => Ok(Some(p.clone())),
        _ => Ok(None),
    }
}

/// Outcome of one produce-tick (`docs/prd.md` §7 M7-c).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProduceTickOutcome {
    /// Block produced; height advanced.
    Produced(u64),
    /// Block built and pipelined (vote stage); commit on a later tick (HyperBFT).
    Pipelined(u64),
    /// Skipped because `validators.is_proposer_for_view(view, validator_index)` is false.
    NotMyTurn,
    /// Leader is ready but a real QC over the current tip is not in the vote pool yet (M7-d-6).
    AwaitingParentQc,
    /// PRD §12 / M7: `min_consensus_stake_wei` is set and bonded stake for this validator fingerprint is below minimum.
    AwaitingConsensusStake,
    /// Tick reached the producer but `apply_block_with_evm` failed (already logged).
    BuildFailed,
}

/// Build one block from the mempool if this node is the current view's leader.
/// Extracted from `producer_loop` so tests can drive single ticks deterministically.
pub async fn try_produce_one_tick(node: &NodeHandle) -> ProduceTickOutcome {
    let mut n = node.lock().await;
    let proposal_started = Instant::now();
    if n.consensus_mode == ConsensusMode::HyperBft {
        return n.hyperbft_three_stage_tick();
    }
    n.try_advance_view_on_timeout_quorum();
    n.maybe_emit_local_timeout();
    let view = n.view;
    if !n.is_my_turn(view) {
        return ProduceTickOutcome::NotMyTurn;
    }
    let min_stake = n.min_consensus_stake_wei;
    if min_stake > 0 {
        if let Some(entry) = n.validators.entry(n.validator_index) {
            let bonded = n
                .state
                .consensus_stake_total_for_fingerprint(&entry.fingerprint);
            if bonded < min_stake {
                eprintln!(
                    "fractal-node: awaiting consensus stake (bonded={bonded} need>={min_stake} wei for validator_index={})",
                    n.validator_index
                );
                return ProduceTickOutcome::AwaitingConsensusStake;
            }
        }
    }
    let base = n.base_fee;
    let gas_limit_cfg = n.gas_limit;
    let pooled = n.mempool.drain_ready_gas_budget(gas_limit_cfg, base);
    let eth_raws: Vec<Option<Vec<u8>>> = pooled.iter().map(|p| p.eth_signed_raw.clone()).collect();
    let txs: Vec<Transaction> = pooled.into_iter().map(|p| p.tx).collect();
    let parent = n.head_hash;
    let (parent_qc, parent_qc_signer_indices) = if n.height == 0 {
        (genesis_parent_qc(), Vec::new())
    } else {
        let Some(tip_block) = n.block_at_height(n.height) else {
            eprintln!(
                "fractal-node: missing retained tip block for height={}; cannot form parent QC",
                n.height
            );
            return ProduceTickOutcome::AwaitingParentQc;
        };
        let tip_height = tip_block.header.height;
        let tip_view = tip_block.header.view;
        let tip_hh = match header_hash(&tip_block.header) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("fractal-node: header_hash(tip) failed: {e}");
                return ProduceTickOutcome::BuildFailed;
            }
        };
        let mode = n.consensus_mode;
        let high = n.high_prepare_qc.clone();
        let pipeline = n.hyperbft_pipeline.clone();
        let stake_w = n.consensus_stake_weights();
        let chain_height = n.height;
        let mut try_form =
            |view: u64, height: u64, hh: fractal_crypto::Hash256| n.try_form_qc(view, height, hh);
        let resolution = resolve_parent_qc(
            mode,
            chain_height,
            tip_height,
            tip_view,
            tip_hh,
            &n.vote_pool,
            &n.validators,
            Some(&stake_w),
            &pipeline,
            &high,
            &mut try_form,
        );
        let Some(resolution) = resolution else {
            return ProduceTickOutcome::AwaitingParentQc;
        };
        if let ParentQcResolution::Formed(ref f) = resolution {
            n.hyperbft_pipeline.note_formed_qc(f);
            if let Some(ref db) = n.chain_store {
                if let Err(e) =
                    db.persist_consensus_formed_qc_v1(n.shard_id, n.shard_topology.shard_count, f)
                {
                    eprintln!(
                        "fractal-node: consensus formed QC RocksDB (parent) height={tip_height} err={e}"
                    );
                }
            }
        }
        parent_qc_bundle(resolution)
    };
    let parent_qc_signer_indices_for_finalize = parent_qc_signer_indices.clone();
    let height = n.height + 1;
    let ts = now_ms();
    let chain_id = n.chain_id;
    let proposer = n.validators.expected_proposer(view);
    let gas_limit = n.gas_limit;
    let validator_fingerprints = n.validators.ids();
    let unbonding_ms: u64 = std::env::var("FRACTAL_UNBONDING_PERIOD_MS")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(n.state.chain_economics.unbonding_period_ms);
    let block_reward_wei: u128 = std::env::var("FRACTAL_BLOCK_REWARD_WEI")
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0);
    let evm_gas = n.evm_gas_used_for_txs(&txs);
    let finalize = BlockFinalizeContext {
        block_timestamp_ms: ts,
        unbonding_period_ms: unbonding_ms,
        proposer,
        parent_qc_signer_indices: &parent_qc_signer_indices_for_finalize,
        validator_fingerprints: &validator_fingerprints,
        treasury: fractal_core::DEVNET_FAUCET_TREASURY,
        block_reward_wei,
        base_fee_per_gas: base,
        evm_gas_used: evm_gas,
    };
    match execute_and_build_block(
        chain_id,
        n.shard_id,
        height,
        view,
        parent,
        parent_qc,
        parent_qc_signer_indices,
        proposer,
        ts,
        gas_limit,
        &mut n.state,
        txs,
        eth_raws,
        Some(finalize),
    ) {
        Ok(block) => {
            n.metrics
                .proposal_latency_ms
                .observe_ms(proposal_started.elapsed().as_millis() as u64);
            let hh = header_hash(&block.header).unwrap_or([0u8; 32]);
            let tip_view = block.header.view;
            let tip_height = block.header.height;
            n.head_hash = hh;
            n.height = block.header.height;
            n.view = n.view.wrapping_add(1);
            n.view_entered_at_ms = now_ms();
            n.consecutive_timeout_failures = 0;
            n.timeout_sent_for_view = None;
            n.base_fee = next_base_fee(n.base_fee, block.header.gas_used, &n.fee_params);
            n.sync_permissionless_validators();
            n.sync_rpc_index_from_block(&block);
            n.forward_vote_after_commit(&block);
            n.blocks.push(block);
            if let Some(b) = n.blocks.last().cloned() {
                n.maybe_persist_committed_block_to_rocksdb(&b);
                n.maybe_emit_shard_anchor(&b);
            }
            if let Some(formed) = n.try_form_qc(tip_view, tip_height, hh) {
                match hash_qc(&formed.qc) {
                    Ok(h) => n.parent_qc_hash = h,
                    Err(e) => eprintln!("fractal-node: parent_qc_hash advance failed: {e}"),
                }
                n.hyperbft_pipeline.note_formed_qc(&formed);
                n.hyperbft_pipeline.note_block_committed(tip_height);
                n.maybe_upgrade_high_prepare_qc(&formed.qc);
                if let Some(ref db) = n.chain_store {
                    if let Err(e) = db.persist_consensus_formed_qc_v1(
                        n.shard_id,
                        n.shard_topology.shard_count,
                        &formed,
                    ) {
                        eprintln!(
                            "fractal-node: consensus formed QC RocksDB height={tip_height} err={e}"
                        );
                    }
                }
            }
            n.hyperbft_pipeline.note_block_produced(tip_height);
            let prune_below = n.height.saturating_sub(1);
            n.vote_pool.prune_below_height(prune_below);
            if let Some(b) = n.blocks.last() {
                n.maybe_enqueue_proof_checkpoint(b);
            }
            ProduceTickOutcome::Produced(n.height)
        }
        Err(e) => {
            eprintln!("fractal-node: block execution failed: {e}");
            ProduceTickOutcome::BuildFailed
        }
    }
}

pub async fn producer_loop(node: NodeHandle) {
    loop {
        let cadence_ms = node.lock().await.effective_block_cadence_ms();
        tokio::time::sleep(tokio::time::Duration::from_millis(cadence_ms)).await;
        let _ = try_produce_one_tick(&node).await;
    }
}

/// PRD §16.1: optional `GET /metrics` (Prometheus text) when `FRACTAL_METRICS_ADDR` is set.
fn maybe_spawn_metrics_server(node: &NodeHandle) {
    let Ok(mvar) = std::env::var("FRACTAL_METRICS_ADDR") else {
        return;
    };
    if mvar.is_empty() {
        return;
    }
    let Ok(maddr) = mvar.parse::<std::net::SocketAddr>() else {
        eprintln!("fractal-node: FRACTAL_METRICS_ADDR={mvar:?} invalid; metrics disabled");
        return;
    };
    let n = node.clone();
    tokio::spawn(async move {
        eprintln!("fractal-node: prometheus metrics on http://{maddr}/metrics");
        if let Err(e) = crate::metrics::serve_metrics(maddr, n).await {
            eprintln!("fractal-node: metrics server exited: {e}");
        }
    });
}

fn maybe_apply_monolith_snapshot_file(
    inner: &mut NodeInner,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let Ok(path) = std::env::var(ENV_MIGRATE_MONOLITH_SNAPSHOT_PATH) else {
        return Ok(());
    };
    if path.trim().is_empty() {
        return Ok(());
    }
    let bytes = std::fs::read(path.trim())?;
    inner.apply_monolith_snapshot_to_shard0_v1(&bytes)?;
    eprintln!(
        "fractal-node: migrated Track A monolith snapshot {:?} into shard 0/{} height={}",
        path.trim(),
        inner.shard_topology.shard_count,
        inner.height
    );
    Ok(())
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
    let mut shared_rocks_path: Option<PathBuf> = None;
    let shared_rocks: Option<fractal_storage::FractalRocksDb> =
        if let Some(path) = unified_rocksdb_path_from_env().map_err(|m| {
            format!("{m} (set one or both of FRACTAL_CHAIN_ROCKSDB_PATH / FRACTAL_PROOF_ROCKSDB_PATH identically when both are used)")
        })? {
            let db = fractal_storage::FractalRocksDb::open(&path).map_err(|e| {
                format!("fractal-node: RocksDB open {path:?} failed: {e}")
            })?;
            eprintln!(
                "fractal-node: RocksDB PRD §10 column bundle open {:?}",
                path
            );
            shared_rocks_path = Some(path);
            Some(db)
        } else {
            None
        };
    let (vote_tx, vote_rx) = tokio::sync::mpsc::unbounded_channel();
    let (timeout_tx, timeout_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut inner =
        NodeInner::devnet_with_validator_secret(validators, validator_index, validator_secret);
    inner.chain_store = shared_rocks.clone();
    inner.rocksdb_path = shared_rocks_path.clone();
    maybe_apply_monolith_snapshot_file(&mut inner)?;
    if inner.min_consensus_stake_wei > 0 {
        eprintln!(
            "fractal-node: FRACTAL_MIN_CONSENSUS_STAKE_WEI={} (producer will not build until bonded stake meets minimum for this validator_index)",
            inner.min_consensus_stake_wei
        );
    }
    inner.set_vote_sink(Some(vote_tx));
    inner.set_timeout_sink(Some(timeout_tx));
    let node: NodeHandle = Arc::new(Mutex::new(inner));
    let anchor_interval = {
        let n = node.lock().await;
        eprintln!(
            "fractal-node: shard_id={} shard_count={} consensus={:?} anchor_interval={} block_cadence_ms={}",
            n.shard_id,
            n.shard_topology.shard_count,
            n.consensus_mode,
            n.anchor_interval,
            n.effective_block_cadence_ms()
        );
        n.anchor_interval
    };
    wire_async_proof_stack(&node, &shared_rocks, anchor_interval).await;
    maybe_spawn_metrics_server(&node);
    let addr: std::net::SocketAddr = std::env::var("FRACTAL_RPC_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8545".into())
        .parse()?;
    let rpc_stats = {
        let n = node.lock().await;
        n.rpc_call_stats.clone()
    };
    let (handle, bound) = fractal_rpc::serve_http(addr, node.clone(), rpc_stats).await?;
    eprintln!("fractal-node json-rpc at http://{bound}");

    let listen: Multiaddr = std::env::var("FRACTAL_P2P_LISTEN")
        .unwrap_or_else(|_| "/ip4/0.0.0.0/udp/4001/quic-v1".into())
        .parse()?;
    let (tx_ready, rx_ready) = tokio::sync::oneshot::channel();
    let p2p_node = node.clone();
    tokio::spawn(async move {
        if let Err(e) = p2p::producer_network_task(
            p2p_node,
            listen,
            Some(tx_ready),
            Some(vote_rx),
            Some(timeout_rx),
        )
        .await
        {
            eprintln!("fractal-node p2p: {e}");
        }
    });
    match tokio::time::timeout(Duration::from_secs(8), rx_ready).await {
        Ok(Ok((bound_p2p, peer))) => {
            let mut bootstrap = bound_p2p.clone();
            bootstrap.push(Protocol::P2p(peer));
            eprintln!(
                "fractal-node p2p (QUIC) listening {bound_p2p}; follower env FRACTAL_BOOTSTRAP={bootstrap}"
            );
        }
        Ok(Err(_)) => eprintln!("fractal-node p2p: ready channel dropped"),
        Err(_) => eprintln!("fractal-node p2p: timed out waiting for listen address"),
    }

    tokio::spawn(producer_loop(node));
    tokio::signal::ctrl_c().await?;
    handle.stop()?;
    Ok(())
}

/// Follower: JSON-RPC + sync from `FRACTAL_BOOTSTRAP` (comma-separated multiaddrs, same `/p2p/<PeerId>`).
pub async fn run_follower() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let raw = std::env::var("FRACTAL_BOOTSTRAP")?;
    let bootstraps = crate::p2p::parse_fractal_bootstraps(&raw)?;
    eprintln!(
        "fractal-node follower: {} bootstrap multiaddr(s)",
        bootstraps.len()
    );
    let validators = devnet_validator_set_from_env();
    let validator_index = devnet_validator_index_from_env(&validators);
    let validator_secret = devnet_validator_secret_from_env(&validators, validator_index);
    eprintln!(
        "fractal-node follower: validator_set_size={} validator_index={validator_index} bls_signing={}",
        validators.len(),
        if validator_secret.is_some() {
            "enabled"
        } else {
            "disabled"
        }
    );
    let mut shared_rocks_path: Option<PathBuf> = None;
    let shared_rocks: Option<fractal_storage::FractalRocksDb> =
        if let Some(path) = unified_rocksdb_path_from_env().map_err(|m| {
            format!("{m} (set one or both of FRACTAL_CHAIN_ROCKSDB_PATH / FRACTAL_PROOF_ROCKSDB_PATH identically when both are used)")
        })? {
            let db = fractal_storage::FractalRocksDb::open(&path).map_err(|e| {
                format!("fractal-node follower: RocksDB open {path:?} failed: {e}")
            })?;
            eprintln!(
                "fractal-node follower: RocksDB PRD §10 column bundle open {:?}",
                path
            );
            shared_rocks_path = Some(path);
            Some(db)
        } else {
            None
        };
    let (vote_tx, vote_rx) = tokio::sync::mpsc::unbounded_channel();
    let (timeout_tx, timeout_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut inner =
        NodeInner::devnet_with_validator_secret(validators, validator_index, validator_secret);
    inner.chain_store = shared_rocks.clone();
    inner.rocksdb_path = shared_rocks_path.clone();
    maybe_apply_monolith_snapshot_file(&mut inner)?;
    inner.set_vote_sink(Some(vote_tx));
    inner.set_timeout_sink(Some(timeout_tx));
    let node: NodeHandle = Arc::new(Mutex::new(inner));
    let anchor_interval = {
        let n = node.lock().await;
        n.anchor_interval
    };
    wire_async_proof_stack(&node, &shared_rocks, anchor_interval).await;
    maybe_spawn_metrics_server(&node);
    let addr: std::net::SocketAddr = std::env::var("FRACTAL_RPC_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:8546".into())
        .parse()?;
    let rpc_stats = {
        let n = node.lock().await;
        n.rpc_call_stats.clone()
    };
    let (handle, bound) = fractal_rpc::serve_http(addr, node.clone(), rpc_stats).await?;
    eprintln!("fractal-node follower json-rpc at http://{bound}");
    let node_for_p2p = node.clone();
    tokio::spawn(p2p::follower_network_task(
        node_for_p2p,
        bootstraps,
        Some(vote_rx),
        Some(timeout_rx),
    ));
    let run_hyperbft_ticks = {
        let n = node.lock().await;
        n.consensus_mode == ConsensusMode::HyperBft
    };
    if run_hyperbft_ticks {
        eprintln!("fractal-node follower: HyperBFT producer tick loop enabled");
        tokio::spawn(producer_loop(node.clone()));
    }
    tokio::signal::ctrl_c().await?;
    handle.stop()?;
    Ok(())
}

#[cfg(test)]
mod wallet_revocation_rpc_tests {
    use super::*;
    use fractal_rpc::ChainInteraction;

    #[test]
    fn fractal_get_wallet_revocation_merkle_root_returns_state_field() {
        let mut node = NodeInner::devnet();
        let initial = format!(
            "0x{}",
            hex::encode(node.state.wallet_revocation_merkle_root)
        );
        assert_eq!(node.fractal_get_wallet_revocation_merkle_root(), initial);
        node.state.wallet_revocation_merkle_root = [0xab; 32];
        assert_eq!(
            node.fractal_get_wallet_revocation_merkle_root(),
            format!("0x{}", hex::encode([0xab; 32]))
        );
    }

    #[test]
    fn fractal_get_wallet_reputation_returns_committed_scores() {
        let mut node = NodeInner::devnet();
        let provider = [0x42u8; 32];
        node.state
            .wallet_reputation_milli
            .insert((provider, 0), 1_500);
        node.state
            .wallet_reputation_ledger_commitment
            .insert((provider, 0), [0x99; 32]);
        let v = node.fractal_get_wallet_reputation();
        assert_eq!(v.get("count").and_then(|c| c.as_u64()), Some(1));
        let row = v.get("scores").and_then(|s| s.as_array()).unwrap()[0].clone();
        assert_eq!(row.get("scoreMilli").and_then(|s| s.as_str()), Some("1500"));
    }

    #[test]
    fn fractal_get_wallet_revocation_entries_returns_rows_and_root() {
        use fractal_core::OnChainRevocationEntry;

        let mut node = NodeInner::devnet();
        let cap_id = [0xcd; 32];
        node.state.wallet_revocation_entries.insert(
            cap_id,
            OnChainRevocationEntry {
                revoked_at_ms: 42,
                reason_code: 3,
                cascade: true,
            },
        );
        node.state.wallet_revocation_merkle_root = [0x11; 32];
        let v = node.fractal_get_wallet_revocation_entries();
        assert_eq!(v["count"], 1);
        assert_eq!(
            v["entries"][0]["capId"],
            format!("0x{}", hex::encode(cap_id))
        );
        assert_eq!(v["entries"][0]["revokedAtMs"], 42);
        assert_eq!(v["entries"][0]["cascade"], true);
        assert_eq!(
            v["revocationRoot"].as_str().unwrap(),
            format!("0x{}", hex::encode([0x11; 32]))
        );
    }

    #[test]
    fn fractal_get_wallet_emergency_stop_reflects_state() {
        let mut node = NodeInner::devnet();
        assert!(!node.fractal_get_wallet_emergency_stop());
        node.state.wallet_emergency_stop = true;
        assert!(node.fractal_get_wallet_emergency_stop());
    }
}

#[cfg(test)]
mod onboarding_report_tests {
    use super::*;
    use fractal_consensus::ValidatorSet;

    #[test]
    fn singleton_report_one_row_quorum_one() {
        let v = ValidatorSet::phase1_singleton();
        let r = devnet_validator_onboarding_report_for(&v, "");
        assert!(r.contains("n=1"));
        assert!(r.contains("PBFT quorum votes = 1"));
        assert!(r.contains("index | proposer"));
        assert!(r.contains("0 | 0x"));
        assert!(r.contains("FRACTAL_VALIDATOR_SECRET_HEX"));
    }

    #[test]
    fn bft7_report_seven_rows_quorum_five() {
        let v = ValidatorSet::phase2_bft7_fixture();
        let r = devnet_validator_onboarding_report_for(&v, "bft7");
        assert!(r.contains("n=7"));
        assert!(r.contains("PBFT quorum votes = 5"));
        let rows = r.matches('\n').count();
        assert!(rows > 12, "expected table + prose, got:\n{r}");
        for i in 0..7 {
            assert!(
                r.contains(&format!("{i:5} | 0x")),
                "missing row for index {i} in:\n{r}"
            );
        }
    }

    #[test]
    fn bft21_report_twenty_one_rows_quorum_thirteen() {
        let v = ValidatorSet::phase3_bft21_fixture();
        let r = devnet_validator_onboarding_report_for(&v, "bft21");
        assert!(r.contains("n=21"));
        assert!(r.contains("PBFT quorum votes = 13"));
        for i in 0..21 {
            assert!(
                r.contains(&format!("{i:5} | 0x")),
                "missing row for index {i} in:\n{r}"
            );
        }
    }
}

#[cfg(test)]
mod chain_snapshot_v1_tests {
    use super::*;
    use fractal_consensus::{eth_signed_raws_for_txs, execute_and_build_block, genesis_parent_qc};
    use fractal_core::{NativeCall, Transaction, TxBody, VmKind};
    use fractal_shard::ProofSubmissionV1;

    #[test]
    fn chain_snapshot_v1_roundtrip_empty_chain() {
        let n = NodeInner::devnet();
        let bytes = n.encode_chain_snapshot_v1().expect("encode");
        let mut f = NodeInner::devnet();
        f.apply_chain_snapshot_v1(&bytes).expect("apply");
        assert_eq!(f.height, 0);
        assert_eq!(f.head_hash, n.head_hash);
    }

    #[test]
    fn chain_snapshot_v1_roundtrip_after_one_block() {
        let mut prod = NodeInner::devnet();
        let tx = Transaction {
            signer: HARDHAT_DEFAULT_SIGNER_0,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let mut scratch = prod.state.clone();
        let gq = genesis_parent_qc();
        let block = execute_and_build_block(
            prod.chain_id,
            prod.shard_id,
            1,
            prod.view,
            prod.head_hash,
            gq,
            vec![],
            prod.validators.expected_proposer(prod.view),
            1,
            prod.gas_limit,
            &mut scratch,
            vec![tx],
            eth_signed_raws_for_txs(1),
            None,
        )
        .expect("block");
        prod.apply_synced_block(&block).expect("apply");

        let bytes = prod.encode_chain_snapshot_v1().expect("encode");
        let mut fol = NodeInner::devnet();
        fol.apply_chain_snapshot_v1(&bytes).expect("snapshot apply");

        assert_eq!(fol.height, prod.height);
        assert_eq!(fol.head_hash, prod.head_hash);
        assert_eq!(fol.blocks, prod.blocks);
    }

    #[test]
    fn chain_snapshot_v2_roundtrip_persists_verified_state_and_mpt_rows() {
        let mut prod = NodeInner::devnet();
        let tx = Transaction {
            signer: HARDHAT_DEFAULT_SIGNER_0,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let mut scratch = prod.state.clone();
        let block = execute_and_build_block(
            prod.chain_id,
            prod.shard_id,
            1,
            prod.view,
            prod.head_hash,
            genesis_parent_qc(),
            vec![],
            prod.validators.expected_proposer(prod.view),
            1,
            prod.gas_limit,
            &mut scratch,
            vec![tx],
            eth_signed_raws_for_txs(1),
            None,
        )
        .expect("block");
        prod.apply_synced_block(&block).expect("apply");

        let bytes = prod.encode_chain_snapshot_v2().expect("encode");
        let dir = std::env::temp_dir().join(format!("fractal_snapshot_v2_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = fractal_storage::FractalRocksDb::open(&dir).expect("open db");
        let mut fol = NodeInner::devnet();
        fol.chain_store = Some(db.clone());
        fol.apply_chain_snapshot_v2(&bytes).expect("snapshot apply");

        assert_eq!(fol.height, prod.height);
        assert_eq!(fol.head_hash, prod.head_hash);
        assert_eq!(fol.blocks, prod.blocks);
        assert!(
            db.get_raw(
                fractal_storage::CF_STATE,
                &fractal_storage::state_at_height_key(prod.height)
            )
            .expect("state row")
            .is_some()
        );
        assert!(
            db.get_raw(
                fractal_storage::CF_STATE,
                &fractal_storage::evm_mpt_root_at_height_key(prod.height)
            )
            .expect("mpt root")
            .is_some()
        );
        assert!(
            db.get_snapshot_v2_manifest(
                prod.shard_id,
                prod.shard_topology.shard_count,
                prod.height
            )
            .expect("manifest")
            .is_some()
        );
        assert!(
            db.get_snapshot_v2_state_chunk(
                prod.shard_id,
                prod.shard_topology.shard_count,
                prod.height,
                0
            )
            .expect("chunk")
            .is_some()
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn chain_snapshot_v2_rejects_tampered_state_chunk() {
        let prod = NodeInner::devnet();
        let bytes = prod.encode_chain_snapshot_v2().expect("encode");
        let mut snap: crate::chain_snapshot::ChainSyncSnapshotV2 =
            borsh::from_slice(&bytes).expect("decode");
        snap.state_chunks[0].bytes[0] ^= 0x01;
        let tampered = borsh::to_vec(&snap).expect("encode tampered");
        let mut fol = NodeInner::devnet();
        let err = fol.apply_chain_snapshot_v2(&tampered).unwrap_err();
        assert!(
            matches!(err, SyncApplyError::Snapshot(msg) if msg.contains("chunk 0 hash mismatch"))
        );
    }

    #[tokio::test]
    async fn chain_proof_snapshot_imports_checkpoint_without_full_block_vector() {
        let prod_handle = Arc::new(Mutex::new(NodeInner::devnet()));
        for expected in 1..=3 {
            let out = try_produce_one_tick(&prod_handle).await;
            assert_eq!(out, ProduceTickOutcome::Produced(expected));
        }
        let bytes = {
            let mut prod = prod_handle.lock().await;
            let tip = prod.blocks.last().cloned().expect("tip");
            let anchor = fractal_masterchain::anchor_from_block_header(&tip.header);
            prod.masterchain_ledger
                .ingest_shard_anchor(anchor)
                .expect("anchor");
            let sub = ProofSubmissionV1 {
                shard_id: prod.shard_id,
                start_block: 1,
                end_block: prod.height,
                prover: [0x44; 20],
                lag_seconds: 0,
                proof_digest: [0x55; 32],
            };
            prod.masterchain_ledger
                .submit_validity_proof(sub)
                .expect("proof");
            let mc = prod
                .masterchain_ledger
                .seal_round([0x44; 20])
                .expect("seal")
                .expect("block");
            assert_ne!(mc.global_zk_root, [0u8; 32]);
            let bytes = prod.encode_chain_proof_snapshot_v1().expect("encode");
            let snap: crate::chain_snapshot::ChainSyncProofSnapshotV1 =
                borsh::from_slice(&bytes).expect("decode");
            assert_eq!(snap.height, 3);
            assert_eq!(snap.tip_block.header.height, 3);
            assert_eq!(snap.masterchain_blocks.len(), 1);
            assert!(snap.plonky2.is_some());
            bytes
        };

        let dir =
            std::env::temp_dir().join(format!("fractal_proof_snapshot_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = fractal_storage::FractalRocksDb::open(&dir).expect("open db");
        let mut follower = NodeInner::devnet();
        follower.chain_store = Some(db.clone());
        follower
            .apply_chain_proof_snapshot_v1(&bytes)
            .expect("apply proof snapshot");

        assert_eq!(follower.height, 3);
        assert_eq!(follower.blocks.len(), 1);
        assert_eq!(follower.blocks[0].header.height, 3);
        assert_eq!(follower.masterchain_ledger.masterchain_height, 1);
        assert!(
            db.get_raw(
                fractal_storage::CF_STATE,
                &fractal_storage::state_at_height_key(follower.height),
            )
            .expect("state row")
            .is_some()
        );
        assert!(
            db.get_snapshot_v2_manifest(0, 1, follower.height)
                .expect("manifest")
                .is_some()
        );
        assert!(
            db.get_snapshot_v2_state_chunk(0, 1, follower.height, 0)
                .expect("chunk")
                .is_some()
        );
        assert!(
            db.get_masterchain_block_v1(1)
                .expect("masterchain block")
                .is_some()
        );

        let follower_handle = Arc::new(Mutex::new(follower));
        let out = try_produce_one_tick(&follower_handle).await;
        assert_eq!(out, ProduceTickOutcome::Produced(4));
        assert_eq!(follower_handle.lock().await.blocks.len(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn chain_snapshot_v1_rejects_nonempty_local() {
        let mut prod = NodeInner::devnet();
        let tx = Transaction {
            signer: HARDHAT_DEFAULT_SIGNER_0,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let mut scratch = prod.state.clone();
        let gq = genesis_parent_qc();
        let block = execute_and_build_block(
            prod.chain_id,
            prod.shard_id,
            1,
            prod.view,
            prod.head_hash,
            gq,
            vec![],
            prod.validators.expected_proposer(prod.view),
            1,
            prod.gas_limit,
            &mut scratch,
            vec![tx],
            eth_signed_raws_for_txs(1),
            None,
        )
        .expect("block");
        prod.apply_synced_block(&block).expect("apply");
        let bytes = prod.encode_chain_snapshot_v1().expect("encode");
        assert!(prod.apply_chain_snapshot_v1(&bytes).is_err());
    }

    #[test]
    fn monolith_snapshot_migrates_to_track_b_shard_zero() {
        let mut prod = NodeInner::devnet();
        let tx = Transaction {
            signer: HARDHAT_DEFAULT_SIGNER_0,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let mut scratch = prod.state.clone();
        let block = execute_and_build_block(
            prod.chain_id,
            prod.shard_id,
            1,
            prod.view,
            prod.head_hash,
            genesis_parent_qc(),
            vec![],
            prod.validators.expected_proposer(prod.view),
            1,
            prod.gas_limit,
            &mut scratch,
            vec![tx],
            eth_signed_raws_for_txs(1),
            None,
        )
        .expect("block");
        prod.apply_synced_block(&block).expect("apply");
        assert_eq!(prod.shard_id, 0);
        assert_eq!(prod.shard_topology.shard_count, 1);

        let bytes = prod.encode_chain_snapshot_v1().expect("encode");
        let mut shard0 = NodeInner::devnet();
        shard0.shard_topology = ShardTopology { shard_count: 2 };
        shard0.shard_id = 0;
        shard0
            .apply_monolith_snapshot_to_shard0_v1(&bytes)
            .expect("migration apply");

        assert_eq!(shard0.shard_id, 0);
        assert_eq!(shard0.shard_topology.shard_count, 2);
        assert_eq!(shard0.height, prod.height);
        assert_eq!(shard0.head_hash, prod.head_hash);
        assert_eq!(shard0.blocks, prod.blocks);
        assert_eq!(
            shard0
                .state
                .accounts
                .get(&HARDHAT_DEFAULT_SIGNER_0)
                .unwrap()
                .nonce,
            1
        );
    }

    #[test]
    fn monolith_snapshot_migration_rejects_nonzero_target_shard() {
        let prod = NodeInner::devnet();
        let bytes = prod.encode_chain_snapshot_v1().expect("encode");
        let mut shard1 = NodeInner::devnet();
        shard1.shard_topology = ShardTopology { shard_count: 2 };
        shard1.shard_id = 1;
        assert!(shard1.apply_monolith_snapshot_to_shard0_v1(&bytes).is_err());
    }
}
