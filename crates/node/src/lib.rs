//! Singleton dev node: 500 ms block cadence + JSON-RPC + libp2p/QUIC sync (`docs/prd.md` §18 M2).

mod eth_signed;
pub mod p2p;

pub use fractal_consensus::ValidatorSet;

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use borsh::BorshDeserialize;
use fractal_consensus::payload::{RlvrProofCommitmentV1, RlvrProofTypeTag};
use fractal_consensus::{
    build_zone_blob_da_sidecar, coverage_manifest_for_circuit_version, da_encoded_bytes,
    da_fee_for_gas, da_gas_for_sidecar, da_root, execute_and_build_block,
    expected_parent_qc_for_parent_header, genesis_parent_qc, hash_qc, header_hash,
    next_parent_qc_hash_after_commit, ordered_tx_root, proof_ingestion_header_extra,
    reconstruct_da_payload, validity_proof_public_input_digest, Block, BlockPayload,
    BlockPayloadItem, BlockPayloadKind, BlockValidityProof, CircuitVersion, DaSamplingParamsV1,
    DaShare, ExecutionFeatureSetV1, FormedQc, MixedExecutionWitnessMetadataV1,
    OwnedObjectCertificateBatchV1, ProofVerifyError, RecordVoteOutcome, StwoPlonky2ProofEnvelope,
    Vote, VotePool, ZoneBlobDaV1, ZoneProofUpdateV1,
};
use fractal_core::{
    Address, EvmEngine, NativeCall, OwnedObjectCertificate, OwnedObjectCertificateSignBody,
    OwnedObjectValidatorSignature, OwnedObjectVersion, ProtocolPhaseConfig, State, Transaction,
    TxBody, VmKind,
};
use fractal_crypto::hash::keccak256;
use fractal_crypto::BlsSecretKey;
use fractal_mempool::{
    next_base_fee, BaseFeeParams, CertificateFinalityRecord, CertificatePool, CertificatePoolError,
    Mempool, PooledProofUpdate, PooledTx, ProofPool, ProofPoolError,
};
use fractal_rlvr::{
    RlvrNodeFlags, RlvrPooledProof, RlvrProofObject, RlvrProofPool, RlvrProofType, RouteTraceInput,
    RouteTraceLogger, RouteTraceRow,
};
use fractal_rpc::{
    logs_bloom_256, make_rpc_log, ChainInteraction, ProofCommitmentResponse, RlmfAttestationRecord,
    RlmfAttestationResponse, RlmfAttestationStored, RpcChainConfig, RpcConsensusDiagnostics,
    RpcDaMetrics, RpcMempoolLaneMetrics, RpcOwnedObjectCertificate, RpcOwnedObjectCountersignature,
    RpcOwnedObjectPrecheck, RpcProofMetrics, RpcProofRejectionMetric, RpcProofUpdateSubmission,
    RpcRoutingDiagnostics,
};
use fractal_storage::{
    ProofFinalityStore, StoredProofFinalityRecord, StoredZoneProofFinalityRecord,
};
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
    #[error("proof-ingestion apply path is not wired yet")]
    ProofIngestionApplyUnavailable,
    #[error("header-only apply requires an already proof-final block")]
    HeaderOnlyRequiresProofFinal,
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
    #[error("native transition proofs are disabled")]
    NativeTransitionProofsDisabled,
    #[error("proof circuit coverage does not cover block feature set")]
    CircuitCoverage,
    #[error("proof witness digest does not match stored witness metadata")]
    WitnessDigestMismatch,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ZoneProofFinalityRecord {
    pub zone_id: u64,
    pub height: u64,
    pub block_hash: fractal_crypto::Hash256,
    pub accepted_at_ms: u64,
    pub circuit_version: CircuitVersion,
    pub public_input_digest: fractal_crypto::Hash256,
    pub finality: BlockFinality,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockPayloadMode {
    Legacy,
    ProofIngestion,
    Mixed,
}

impl BlockPayloadMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Legacy => "legacy",
            Self::ProofIngestion => "proof_ingestion",
            Self::Mixed => "mixed",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "" | "legacy" => Some(Self::Legacy),
            "proof_ingestion" | "proof-ingestion" | "proof" => Some(Self::ProofIngestion),
            "mixed" => Some(Self::Mixed),
            _ => None,
        }
    }

    fn from_env() -> Self {
        match std::env::var("FRACTAL_BLOCK_PAYLOAD_MODE") {
            Ok(raw) => match Self::parse(&raw) {
                Some(mode) => mode,
                None => {
                    eprintln!(
                        "fractal-node: invalid FRACTAL_BLOCK_PAYLOAD_MODE={raw:?}; defaulting to legacy"
                    );
                    Self::Legacy
                }
            },
            Err(_) => Self::Legacy,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockApplyMode {
    ReplayFullTransactions,
    VerifyProofAndDa,
    HeaderOnlyAfterProofFinal,
}

pub trait BlockApplyVerifier {
    fn verify_proof_and_da(&mut self, block: &Block) -> Result<(), SyncApplyError>;

    fn apply_header_only_after_proof_final(&mut self, block: &Block) -> Result<(), SyncApplyError>;
}

#[must_use]
pub fn block_apply_mode_for_payload_kind(payload_kind: BlockPayloadKind) -> BlockApplyMode {
    match payload_kind {
        BlockPayloadKind::FullTransactions => BlockApplyMode::ReplayFullTransactions,
        BlockPayloadKind::ProofUpdates
        | BlockPayloadKind::CertificateBatches
        | BlockPayloadKind::Mixed => BlockApplyMode::VerifyProofAndDa,
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SettlementAccessError {
    #[error("block not found")]
    BlockNotFound,
    #[error("block is not proof-final")]
    NotProofFinal,
    #[error("block proof circuit does not cover requested settlement features")]
    UncoveredCircuit,
    #[error("block data availability is unavailable")]
    UnavailableDa,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainConfig {
    pub proof_required_settlement: bool,
    pub native_transition_proofs_enabled: bool,
    pub proofs_required_for_settlement: ExecutionFeatureSetV1,
    pub phase_config: ProtocolPhaseConfig,
    pub block_payload_mode: BlockPayloadMode,
    pub rlvr: RlvrNodeFlags,
}

impl Default for ChainConfig {
    fn default() -> Self {
        Self {
            proof_required_settlement: false,
            native_transition_proofs_enabled: false,
            proofs_required_for_settlement: ExecutionFeatureSetV1::empty(),
            phase_config: ProtocolPhaseConfig::testnet(),
            block_payload_mode: BlockPayloadMode::Legacy,
            rlvr: RlvrNodeFlags {
                enabled: false,
                chain_commit_enabled: false,
                raw_data_on_chain: false,
                raw_data_on_chain_requested: false,
            },
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProofMetrics {
    pub proofs_accepted: u64,
    pub proofs_rejected: u64,
    pub witness_gen_latency_ms: u64,
    pub latest_proof_final_lag_ms: u64,
    pub latest_proof_latency_ms: u64,
    pub total_proof_latency_ms: u128,
    pub proof_final_height: u64,
    pub unsupported_feature_rejections: u64,
    pub latest_rejection_reason: Option<String>,
    pub rejection_reasons: BTreeMap<String, u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QcDiagnostics {
    pub status: String,
    pub reason: String,
    pub height: u64,
    pub view: u64,
    pub vote_count: u64,
    pub threshold: u64,
}

impl Default for QcDiagnostics {
    fn default() -> Self {
        Self {
            status: "none".into(),
            reason: "not_attempted".into(),
            height: 0,
            view: 0,
            vote_count: 0,
            threshold: 0,
        }
    }
}

const GENESIS_TAG: &[u8] = b"FRACTALCHAIN_GENESIS_V0";

/// Hardhat / Anvil default signer #0 — re-exported from `fractal_core::devnet_accounts`.
pub use fractal_core::HARDHAT_DEFAULT_SIGNER_0;
/// Hardhat default signer #1 (M5 MVP agent for `CLAIM_PAYOUT` demos).
pub use fractal_core::HARDHAT_DEFAULT_SIGNER_1;

pub fn genesis_parent_hash() -> fractal_crypto::Hash256 {
    keccak256(GENESIS_TAG)
}

fn validator_set_hash(validators: &ValidatorSet) -> fractal_crypto::Hash256 {
    let mut bytes = Vec::with_capacity(validators.len() * (32 + 48));
    for entry in validators.entries() {
        bytes.extend_from_slice(&entry.fingerprint);
        bytes.extend_from_slice(&entry.bls_pubkey.0);
    }
    keccak256(&bytes)
}

fn hash_hex(hash: &fractal_crypto::Hash256) -> String {
    format!("0x{}", hex::encode(hash))
}

fn addr_hex(address: &Address) -> String {
    format!("0x{}", hex::encode(address))
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

fn shard_id_from_env() -> u32 {
    std::env::var("FRACTAL_SHARD_ID")
        .ok()
        .and_then(|raw| raw.trim().parse().ok())
        .unwrap_or(0)
}

fn shard_count_from_env() -> u32 {
    std::env::var("FRACTAL_SHARD_COUNT")
        .ok()
        .and_then(|raw| raw.trim().parse().ok())
        .filter(|count| *count > 0)
        .unwrap_or(1)
}

fn consensus_mode_from_env() -> String {
    std::env::var("FRACTAL_CONSENSUS_MODE")
        .ok()
        .filter(|raw| !raw.trim().is_empty())
        .unwrap_or_else(|| "singleton".into())
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

fn native_transition_proofs_enabled_from_env() -> bool {
    env_flag("FRACTAL_NATIVE_TRANSITION_PROOFS_ENABLED", false)
}

fn env_flag(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(raw) => matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on" | "enabled"
        ),
        Err(_) => default,
    }
}

fn protocol_phase_config_from_env() -> ProtocolPhaseConfig {
    let mut cfg = ProtocolPhaseConfig::testnet();
    cfg.owned_object_certificates = env_flag(
        "FRACTAL_PHASE_OWNED_OBJECT_CERTIFICATES",
        cfg.owned_object_certificates,
    );
    cfg.da_sampling = env_flag("FRACTAL_PHASE_DA_SAMPLING", cfg.da_sampling);
    cfg.proof_final_settlement = env_flag(
        "FRACTAL_PHASE_PROOF_FINAL_SETTLEMENT",
        proof_required_settlement_from_env(),
    );
    cfg.execution_zones = env_flag("FRACTAL_PHASE_EXECUTION_ZONES", cfg.execution_zones);
    cfg.forced_inclusion = env_flag("FRACTAL_PHASE_FORCED_INCLUSION", cfg.forced_inclusion);
    cfg.prover_rewards = env_flag("FRACTAL_PHASE_PROVER_REWARDS", cfg.prover_rewards);
    cfg.sequencer_rewards = env_flag("FRACTAL_PHASE_SEQUENCER_REWARDS", cfg.sequencer_rewards);
    cfg
}

fn proof_required_feature_mask_from_env(global_required: bool) -> ExecutionFeatureSetV1 {
    let mut mask = if global_required {
        ExecutionFeatureSetV1::all_known()
    } else {
        ExecutionFeatureSetV1::empty()
    };
    if env_flag("FRACTAL_PROOF_REQUIRED_NATIVE", false) {
        mask.insert(fractal_consensus::FEATURE_NATIVE_TX);
        mask.insert(fractal_consensus::FEATURE_NATIVE_SHARED_STATE);
    }
    if env_flag("FRACTAL_PROOF_REQUIRED_EVM", false) {
        mask.insert(fractal_consensus::FEATURE_EVM_TRANSFER);
        mask.insert(fractal_consensus::FEATURE_EVM_CALL);
        mask.insert(fractal_consensus::FEATURE_EVM_CREATE);
        mask.insert(fractal_consensus::FEATURE_EVM_TO_NATIVE_PRECOMPILE);
    }
    mask
}

fn proof_finality_store_from_env() -> Option<std::path::PathBuf> {
    std::env::var_os("FRACTAL_PROOF_FINALITY_STORE")
        .filter(|raw| !raw.is_empty())
        .map(std::path::PathBuf::from)
}

/// RLVR-009: explicit override for the local route-trace JSONL path. When RLVR
/// is enabled but this is unset, the logger defaults to a local-only file under
/// `fractal_rlvr/data/`.
fn rlvr_trace_log_path_from_env() -> Option<std::path::PathBuf> {
    std::env::var_os("FRACTAL_RLVR_TRACE_LOG_PATH")
        .filter(|raw| !raw.is_empty())
        .map(std::path::PathBuf::from)
}

/// Open (or report) the RLVR-009 route trace logger when RLVR is enabled. Logs
/// the resolved path or the reason tracing stayed disabled.
fn attach_rlvr_route_trace_logger(inner: &mut NodeInner, prefix: &str) {
    if !inner.chain_config.rlvr.enabled {
        return;
    }
    let path = rlvr_trace_log_path_from_env()
        .unwrap_or_else(|| std::path::PathBuf::from("fractal_rlvr/data/route_traces.jsonl"));
    match RouteTraceLogger::open(&path, true) {
        Ok(logger) => {
            eprintln!(
                "{prefix}: rlvr_route_trace_log={} local_only=true",
                path.display()
            );
            inner.set_route_trace_logger(logger);
        }
        Err(err) => {
            eprintln!(
                "{prefix}: rlvr_route_trace_log disabled (open failed at {}: {err})",
                path.display()
            );
        }
    }
}

pub struct NodeInner {
    pub chain_id: u64,
    pub shard_id: u32,
    pub shard_count: u32,
    pub consensus_mode: String,
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
    pub proof_pool: ProofPool,
    pub certificate_pool: CertificatePool,
    pub rlvr_proof_pool: RlvrProofPool,
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
    /// Proof-finality indexed by `(zone_id, zone/update height)`.
    pub zone_proof_finality: BTreeMap<(u64, u64), ZoneProofFinalityRecord>,
    pub latest_proof_final_height_by_zone: BTreeMap<u64, u64>,
    /// Research proof-hash commitments keyed by proof hash.
    pub proof_commitments: BTreeMap<fractal_crypto::Hash256, u64>,
    /// Indexed RLMF attestation records keyed by commitment hash.
    /// In-memory devnet index (same durability as `proof_commitments`).
    pub rlmf_attestations: BTreeMap<fractal_crypto::Hash256, RlmfAttestationStored>,
    pub proof_finality_store: Option<ProofFinalityStore>,
    pub witness_metadata: BTreeMap<fractal_crypto::Hash256, MixedExecutionWitnessMetadataV1>,
    pub chain_config: ChainConfig,
    pub da_metrics: DaMetrics,
    pub proof_metrics: ProofMetrics,
    pub p2p_connected_peers: AtomicU64,
    pub qc_diagnostics: QcDiagnostics,
    /// RLVR-009 node trace logger. `None` when RLVR tracing is disabled. When
    /// `Some`, each [`NodeInner::record_route_trace`] call appends one hash-only
    /// row to a local JSONL file; raw prompt/answer text never reaches disk.
    pub route_trace_logger: Option<RouteTraceLogger>,
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
            shard_id: 0,
            shard_count: 1,
            consensus_mode: "singleton".into(),
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
            proof_pool: ProofPool::default(),
            certificate_pool: CertificatePool::default(),
            rlvr_proof_pool: RlvrProofPool::default(),
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
            zone_proof_finality: BTreeMap::new(),
            latest_proof_final_height_by_zone: BTreeMap::new(),
            proof_commitments: BTreeMap::new(),
            rlmf_attestations: BTreeMap::new(),
            proof_finality_store: None,
            witness_metadata: BTreeMap::new(),
            chain_config: ChainConfig::default(),
            da_metrics: DaMetrics::default(),
            proof_metrics: ProofMetrics::default(),
            p2p_connected_peers: AtomicU64::new(0),
            qc_diagnostics: QcDiagnostics::default(),
            route_trace_logger: None,
        }
    }

    /// Whether this node should propose for `view` (`docs/prd.md` §7 M7-c).
    /// In single-validator (Phase 1) setups, always `true` for `validator_index = 0`.
    #[must_use]
    pub fn is_my_turn(&self, view: u64) -> bool {
        self.validators
            .is_proposer_for_view(view, self.validator_index)
    }

    #[must_use]
    pub fn connected_validator_count(&self) -> u64 {
        let peer_count = self.p2p_connected_peers.load(Ordering::Relaxed);
        let with_self = peer_count.saturating_add(1);
        with_self.min(self.validators.len() as u64)
    }

    #[must_use]
    pub fn validator_set_hash(&self) -> fractal_crypto::Hash256 {
        validator_set_hash(&self.validators)
    }

    #[must_use]
    pub fn consensus_diagnostics_rpc(&self) -> RpcConsensusDiagnostics {
        let leader_index = self.validators.leader_index(self.view);
        let leader_fingerprint = self.validators.expected_proposer(self.view);
        let height2 = self.vote_pool.best_slot_for_height(2);
        let (height2_vote_view, height2_vote_header_hash, height2_votes_received, signers) =
            match height2 {
                Some((view, header_hash, count, signers)) => (
                    Some(format!("0x{view:x}")),
                    Some(hash_hex(&header_hash)),
                    count as u64,
                    signers,
                ),
                None => (None, None, 0, Vec::new()),
            };
        RpcConsensusDiagnostics {
            height: format!("0x{:x}", self.height),
            current_view: format!("0x{:x}", self.view),
            validator_index: format!("0x{:x}", self.validator_index),
            validator_set_size: format!("0x{:x}", self.validators.len()),
            quorum_threshold: format!("0x{:x}", self.validators.quorum_threshold()),
            connected_peer_count: format!(
                "0x{:x}",
                self.p2p_connected_peers.load(Ordering::Relaxed)
            ),
            connected_validator_count: format!("0x{:x}", self.connected_validator_count()),
            current_leader_index: format!("0x{leader_index:x}"),
            current_leader_fingerprint: hash_hex(&leader_fingerprint),
            height2_votes_received: format!("0x{height2_votes_received:x}"),
            height2_vote_view,
            height2_vote_header_hash,
            height2_vote_signers: signers
                .into_iter()
                .map(|idx| format!("0x{idx:x}"))
                .collect(),
            qc_status: self.qc_diagnostics.status.clone(),
            qc_reason: self.qc_diagnostics.reason.clone(),
            qc_height: format!("0x{:x}", self.qc_diagnostics.height),
            qc_view: format!("0x{:x}", self.qc_diagnostics.view),
            qc_vote_count: format!("0x{:x}", self.qc_diagnostics.vote_count),
            qc_threshold: format!("0x{:x}", self.qc_diagnostics.threshold),
            genesis_hash: hash_hex(&genesis_parent_hash()),
            validator_set_hash: hash_hex(&self.validator_set_hash()),
        }
    }

    pub fn log_startup_consensus_diagnostics(&self, prefix: &str) {
        let leader_index = self.validators.leader_index(self.view);
        let leader_fingerprint = self.validators.expected_proposer(self.view);
        eprintln!(
            "{prefix}: consensus_diagnostics genesis_hash={} validator_set_hash={} current_view={} current_leader_index={} current_leader_fingerprint={} quorum_threshold={} shard_id={} shard_count={} consensus_mode={} block_payload_mode={}",
            hash_hex(&genesis_parent_hash()),
            hash_hex(&self.validator_set_hash()),
            self.view,
            leader_index,
            hash_hex(&leader_fingerprint),
            self.validators.quorum_threshold(),
            self.shard_id,
            self.shard_count,
            self.consensus_mode,
            self.chain_config.block_payload_mode.as_str()
        );
    }

    /// Wire gossip vote publishing (`docs/prd.md` §18 M7-d-5). When `None`, votes stay local only.
    pub fn set_vote_sink(&mut self, sink: Option<tokio::sync::mpsc::UnboundedSender<Vec<u8>>>) {
        self.vote_sink = sink;
    }

    pub fn set_rlvr_node_flags(&mut self, flags: RlvrNodeFlags) {
        self.chain_config.rlvr = flags;
    }

    /// Attach an RLVR-009 route trace logger. Only takes effect while
    /// [`RlvrNodeFlags::enabled`] is set on the chain config.
    pub fn set_route_trace_logger(&mut self, logger: RouteTraceLogger) {
        self.route_trace_logger = Some(logger);
    }

    /// Path of the local RLVR trace log, when tracing is enabled.
    pub fn route_trace_log_path(&self) -> Option<&std::path::Path> {
        self.route_trace_logger.as_ref().map(|logger| logger.path())
    }

    /// RLVR-009: record one local hash-only trace row for a chat/route request.
    ///
    /// Returns `Ok(None)` (a no-op) when RLVR is disabled or no logger is
    /// attached. When enabled, exactly one JSONL row is appended per call.
    /// Raw prompt/answer/correction text is hashed in place and never written.
    pub fn record_route_trace(
        &self,
        input: RouteTraceInput<'_>,
    ) -> Result<Option<RouteTraceRow>, fractal_rlvr::RlvrError> {
        if !self.chain_config.rlvr.enabled {
            return Ok(None);
        }
        let Some(logger) = self.route_trace_logger.as_ref() else {
            return Ok(None);
        };
        Ok(Some(logger.record(&input)?))
    }

    pub fn set_proof_required_settlement(&mut self, required: bool) {
        self.chain_config.proof_required_settlement = required;
        self.chain_config.proofs_required_for_settlement = if required {
            ExecutionFeatureSetV1::all_known()
        } else {
            ExecutionFeatureSetV1::empty()
        };
        self.chain_config.phase_config.proof_final_settlement = required;
    }

    pub fn set_native_transition_proofs_enabled(&mut self, enabled: bool) {
        self.chain_config.native_transition_proofs_enabled = enabled;
    }

    pub fn set_proofs_required_for_settlement(&mut self, required: ExecutionFeatureSetV1) {
        self.chain_config.proofs_required_for_settlement = required;
        self.chain_config.proof_required_settlement = required.bits != 0;
        self.chain_config.phase_config.proof_final_settlement = required.bits != 0;
    }

    #[must_use]
    pub fn settlement_requires_proof(&self) -> bool {
        self.chain_config.proof_required_settlement
    }

    #[must_use]
    pub fn settlement_requires_proof_for_features(&self, features: ExecutionFeatureSetV1) -> bool {
        self.chain_config.proof_required_settlement
            || (features.bits & self.chain_config.proofs_required_for_settlement.bits) != 0
    }

    pub fn set_protocol_phase_config(&mut self, config: ProtocolPhaseConfig) {
        self.chain_config.proof_required_settlement = config.proof_final_settlement;
        self.chain_config.proofs_required_for_settlement = if config.proof_final_settlement {
            ExecutionFeatureSetV1::all_known()
        } else {
            ExecutionFeatureSetV1::empty()
        };
        self.chain_config.phase_config = config;
    }

    pub fn set_block_payload_mode(&mut self, mode: BlockPayloadMode) {
        self.chain_config.block_payload_mode = mode;
    }

    #[must_use]
    pub fn block_payload_mode(&self) -> BlockPayloadMode {
        self.chain_config.block_payload_mode
    }

    fn shard_topology(&self) -> fractal_shard::ShardTopology {
        fractal_shard::ShardTopology {
            shard_count: self.shard_count,
        }
    }

    fn routing_diagnostics_for_tx(&self, tx: &Transaction) -> RpcRoutingDiagnostics {
        let topology = self.shard_topology();
        let diagnostics =
            fractal_shard::routing_diagnostics_for_signer(&tx.signer, self.shard_id, &topology);
        RpcRoutingDiagnostics {
            source_shard: format!("0x{:x}", diagnostics.source_shard),
            expected_shard: format!("0x{:x}", diagnostics.expected_shard),
            shard_count: format!("0x{:x}", diagnostics.shard_count),
            route_key: diagnostics.route_key,
            accepted: diagnostics.accepted,
        }
    }

    fn check_tx_shard_route(&self, tx: &Transaction) -> Result<RpcRoutingDiagnostics, String> {
        let topology = self.shard_topology();
        match fractal_shard::check_accepts_transaction_with_diagnostics(
            &tx.signer,
            self.shard_id,
            &topology,
        ) {
            Ok(_) => Ok(self.routing_diagnostics_for_tx(tx)),
            Err((_err, diagnostics)) => Err(format!(
                "wrong shard: source_shard=0x{:x} expected_shard=0x{:x} shard_count=0x{:x} route_key={}",
                diagnostics.source_shard,
                diagnostics.expected_shard,
                diagnostics.shard_count,
                diagnostics.route_key
            )),
        }
    }

    pub fn submit_proof_update(
        &mut self,
        update: ZoneProofUpdateV1,
        max_priority_fee: u128,
    ) -> Result<fractal_crypto::Hash256, ProofPoolError> {
        let update_hash = fractal_consensus::proof_update_leaf_hash(&update)
            .expect("ZoneProofUpdateV1 proof leaf hash is infallible for fixed-size fields");
        self.proof_pool.insert(PooledProofUpdate {
            update,
            max_priority_fee,
        })?;
        Ok(update_hash)
    }

    pub fn submit_rlvr_proof(&mut self, proof: RlvrProofObject) -> Result<String, String> {
        self.rlvr_proof_pool
            .insert(proof)
            .map_err(|err| err.to_string())
    }

    pub fn submit_owned_object_certificate(
        &mut self,
        certificate: OwnedObjectCertificate,
    ) -> Result<fractal_crypto::Hash256, CertificatePoolError> {
        let pubkeys = (0..self.validators.len())
            .filter_map(|idx| self.validators.bls_pubkey(idx).copied())
            .collect::<Vec<_>>();
        self.certificate_pool
            .insert(certificate, &pubkeys, self.validators.quorum_threshold())
    }

    pub fn owned_object_certificate_finality(
        &self,
        object_version: &OwnedObjectVersion,
    ) -> Option<&CertificateFinalityRecord> {
        self.certificate_pool
            .finality_for_object_version(object_version)
    }

    pub fn certificate_batch_payload_root_hook(
        &self,
    ) -> Result<fractal_crypto::Hash256, std::io::Error> {
        BlockPayload::CertificateBatches(vec![OwnedObjectCertificateBatchV1 {
            certificates: self.certificate_pool.accepted_certificates(),
        }])
        .payload_root()
    }

    pub fn record_witness_metadata(&mut self, metadata: MixedExecutionWitnessMetadataV1) {
        let started = now_ms();
        self.witness_metadata.insert(metadata.block_hash, metadata);
        self.proof_metrics.witness_gen_latency_ms = now_ms().saturating_sub(started);
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
            for zone in record.zone_records {
                self.insert_zone_proof_finality_record(ZoneProofFinalityRecord {
                    zone_id: zone.zone_id,
                    height: zone.height,
                    block_hash: zone.block_hash,
                    accepted_at_ms: zone.accepted_at_ms,
                    circuit_version: zone.circuit_version,
                    public_input_digest: zone.public_input_digest,
                    finality: BlockFinality::Proof,
                });
            }
        }
        Ok(())
    }

    fn zone_id_from_namespace(namespace: fractal_consensus::ExecutionZoneNamespace) -> u64 {
        u64::from_be_bytes(namespace)
    }

    fn zone_record_from_block_proof(
        proof: &BlockValidityProof,
        accepted_at_ms: u64,
        public_input_digest: fractal_crypto::Hash256,
    ) -> ZoneProofFinalityRecord {
        ZoneProofFinalityRecord {
            zone_id: Self::zone_id_from_namespace(proof.zone_namespace),
            height: proof.height,
            block_hash: proof.block_hash,
            accepted_at_ms,
            circuit_version: proof.circuit_version,
            public_input_digest,
            finality: BlockFinality::Proof,
        }
    }

    fn insert_zone_proof_finality_record(&mut self, record: ZoneProofFinalityRecord) {
        self.latest_proof_final_height_by_zone
            .entry(record.zone_id)
            .and_modify(|height| *height = (*height).max(record.height))
            .or_insert(record.height);
        self.zone_proof_finality
            .insert((record.zone_id, record.height), record);
    }

    pub fn latest_proof_final_height_for_zone(&self, zone_id: u64) -> Option<u64> {
        self.latest_proof_final_height_by_zone
            .get(&zone_id)
            .copied()
    }

    pub fn zone_update_finality(&self, zone_id: u64, height: u64) -> Option<BlockFinality> {
        self.zone_proof_finality
            .get(&(zone_id, height))
            .map(|record| record.finality)
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
        let view = vote.view;
        let height = vote.height;
        let header_hash = vote.header_hash;
        let outcome = self.vote_pool.record(vote, &self.validators);
        let count = self.vote_pool.count(view, header_hash);
        if height == 2 || matches!(outcome, RecordVoteOutcome::ReachedQuorum) {
            let signers = self.vote_pool.signer_indices(view, header_hash);
            eprintln!(
                "fractal-node: vote_pool height={height} view={view} header_hash={} votes={count}/{} signers={signers:?} outcome={outcome:?}",
                hash_hex(&header_hash),
                self.validators.quorum_threshold()
            );
        }
        outcome
    }

    /// Attempt to form a QC for `(view, block_height, header_hash)` from the
    /// local vote pool. Returns `None` until `quorum_threshold` is reached.
    /// Wrapper over [`VotePool::try_form_qc`].
    pub fn try_form_qc(
        &mut self,
        view: u64,
        block_height: u64,
        header_hash: fractal_crypto::Hash256,
    ) -> Option<FormedQc> {
        let vote_count = self.vote_pool.count(view, header_hash) as u64;
        let threshold = self.validators.quorum_threshold() as u64;
        let formed = self
            .vote_pool
            .try_form_qc(view, block_height, header_hash, &self.validators);
        let (status, reason) = if formed.is_some() {
            ("formed".to_owned(), "quorum_reached".to_owned())
        } else if vote_count < threshold {
            (
                "not_formed".to_owned(),
                format!("insufficient_votes:{vote_count}/{threshold}"),
            )
        } else {
            ("not_formed".to_owned(), "aggregation_failed".to_owned())
        };
        self.qc_diagnostics = QcDiagnostics {
            status,
            reason,
            height: block_height,
            view,
            vote_count,
            threshold,
        };
        eprintln!(
            "fractal-node: qc_diagnostics height={block_height} view={view} header_hash={} status={} reason={} votes={vote_count}/{threshold}",
            hash_hex(&header_hash),
            self.qc_diagnostics.status,
            self.qc_diagnostics.reason
        );
        formed
    }

    pub fn submit_validity_proof(
        &mut self,
        proof: BlockValidityProof,
    ) -> Result<(), ProofFinalityError> {
        let block = match self.blocks.iter().find(|b| {
            b.header.height == proof.height && header_hash(&b.header).ok() == Some(proof.block_hash)
        }) {
            Some(block) => block.clone(),
            None => {
                self.record_proof_rejection("block_not_found");
                return Err(ProofFinalityError::BlockNotFound);
            }
        };
        let block_timestamp_ms = block.header.timestamp_ms;
        let block_height = block.header.height;
        let block_feature_set = block.header.feature_set;
        if proof.circuit_version == CircuitVersion::NativeStateTransitionV1
            && !self.chain_config.native_transition_proofs_enabled
        {
            self.record_proof_rejection("native_transition_proofs_disabled");
            return Err(ProofFinalityError::NativeTransitionProofsDisabled);
        }
        let manifest = coverage_manifest_for_circuit_version(proof.circuit_version);
        if !block_feature_set.contains_only(manifest.covered_features) {
            self.record_proof_rejection("circuit_coverage");
            return Err(ProofFinalityError::CircuitCoverage);
        }
        if let Some(metadata) = self.witness_metadata.get(&proof.block_hash) {
            if proof_witness_digest(&proof)? != Some(metadata.witness_digest) {
                self.record_proof_rejection("witness_digest");
                return Err(ProofFinalityError::WitnessDigestMismatch);
            }
        }
        if let Err(e) = fractal_consensus::verify_block_validity_proof(&block, &proof) {
            self.record_proof_rejection(&proof_rejection_reason(&e));
            return Err(e.into());
        }
        let block_hash = proof.block_hash;
        let accepted_at_ms = now_ms();
        let public_input_digest = validity_proof_public_input_digest(&proof)?;
        let zone_record =
            Self::zone_record_from_block_proof(&proof, accepted_at_ms, public_input_digest);
        self.proof_finalized_blocks
            .insert(block_hash, proof.clone());
        self.insert_zone_proof_finality_record(zone_record.clone());
        if let Some(store) = &self.proof_finality_store {
            store.put_record(StoredProofFinalityRecord {
                block_hash,
                height: block_height,
                accepted_at_ms,
                circuit_version: proof.circuit_version,
                coverage_manifest_digest: proof.coverage_manifest_digest,
                public_input_digest,
                proof,
                zone_records: vec![StoredZoneProofFinalityRecord {
                    zone_id: zone_record.zone_id,
                    height: zone_record.height,
                    block_hash: zone_record.block_hash,
                    accepted_at_ms: zone_record.accepted_at_ms,
                    circuit_version: zone_record.circuit_version,
                    public_input_digest: zone_record.public_input_digest,
                }],
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
        let block = self
            .blocks
            .iter()
            .find(|b| header_hash(&b.header).ok().as_ref() == Some(block_hash));
        let requires = block
            .map(|b| self.settlement_requires_proof_for_features(b.header.feature_set))
            .unwrap_or_else(|| self.settlement_requires_proof());
        if requires && finality != BlockFinality::Proof {
            return Err(SettlementAccessError::NotProofFinal);
        }
        if let Some(block) = block {
            if fractal_consensus::verify_da_sidecar(&block.header, &block.da_sidecar).is_err() {
                return Err(SettlementAccessError::UnavailableDa);
            }
            if let Some(proof) = self.proof_finalized_blocks.get(block_hash) {
                let manifest = coverage_manifest_for_circuit_version(proof.circuit_version);
                if !block
                    .header
                    .feature_set
                    .contains_only(manifest.covered_features)
                {
                    return Err(SettlementAccessError::UncoveredCircuit);
                }
            } else if requires {
                return Err(SettlementAccessError::NotProofFinal);
            }
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
        self.proof_metrics.latest_proof_final_lag_ms = latency_ms;
        self.proof_metrics.total_proof_latency_ms = self
            .proof_metrics
            .total_proof_latency_ms
            .saturating_add(latency_ms as u128);
        self.proof_metrics.proof_final_height = self.proof_metrics.proof_final_height.max(height);
    }

    fn record_proof_rejection(&mut self, reason: &str) {
        self.proof_metrics.proofs_rejected = self.proof_metrics.proofs_rejected.saturating_add(1);
        if matches!(
            reason,
            "circuit_coverage" | "coverage_manifest" | "unsupported_feature"
        ) {
            self.proof_metrics.unsupported_feature_rejections = self
                .proof_metrics
                .unsupported_feature_rejections
                .saturating_add(1);
        }
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

    fn validate_synced_block_envelope(&self, block: &Block) -> Result<(), SyncApplyError> {
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
        Ok(())
    }

    fn apply_synced_block_replay_full_transactions(
        &mut self,
        block: &Block,
    ) -> Result<(), SyncApplyError> {
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
        self.commit_synced_block_header_and_indexes(block)?;
        Ok(())
    }

    fn commit_synced_block_header_and_indexes(
        &mut self,
        block: &Block,
    ) -> Result<(), SyncApplyError> {
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

    /// Apply a received block using an explicit mode. This keeps replay available
    /// for archive nodes while letting validators select proof-driven paths once
    /// proof-ingestion payloads are wired.
    pub fn apply_synced_block_with_mode(
        &mut self,
        block: &Block,
        mode: BlockApplyMode,
    ) -> Result<(), SyncApplyError> {
        self.validate_synced_block_envelope(block)?;
        match mode {
            BlockApplyMode::ReplayFullTransactions => {
                self.apply_synced_block_replay_full_transactions(block)
            }
            BlockApplyMode::VerifyProofAndDa => {
                <Self as BlockApplyVerifier>::verify_proof_and_da(self, block)?;
                self.commit_synced_block_header_and_indexes(block)
            }
            BlockApplyMode::HeaderOnlyAfterProofFinal => {
                <Self as BlockApplyVerifier>::apply_header_only_after_proof_final(self, block)?;
                self.commit_synced_block_header_and_indexes(block)
            }
        }
    }

    /// Replay txs and check roots against a received block (follower verification).
    pub fn apply_synced_block(&mut self, block: &Block) -> Result<(), SyncApplyError> {
        let mode = block_apply_mode_for_payload_kind(block.payload_kind());
        self.apply_synced_block_with_mode(block, mode)
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

impl BlockApplyVerifier for NodeInner {
    fn verify_proof_and_da(&mut self, block: &Block) -> Result<(), SyncApplyError> {
        fractal_consensus::verify_da_sidecar(&block.header, &block.da_sidecar)
            .map_err(|_| SyncApplyError::DataAvailability)?;
        Err(SyncApplyError::ProofIngestionApplyUnavailable)
    }

    fn apply_header_only_after_proof_final(&mut self, block: &Block) -> Result<(), SyncApplyError> {
        let hh = header_hash(&block.header)?;
        if self.proof_finalized_blocks.contains_key(&hh) {
            Ok(())
        } else {
            Err(SyncApplyError::HeaderOnlyRequiresProofFinal)
        }
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
        self.shard_count
    }

    fn consensus_mode(&self) -> String {
        self.consensus_mode.clone()
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
            self.check_tx_shard_route(&tx)?;
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
        self.check_tx_shard_route(&tx)?;
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

    fn routing_diagnostics_for_raw_tx(&self, raw: &[u8]) -> Result<RpcRoutingDiagnostics, String> {
        if let Ok(tx) = Transaction::try_from_slice(raw) {
            return Ok(self.routing_diagnostics_for_tx(&tx));
        }
        let (tx, _h, _max_priority_fee_per_gas, _max_fee_per_gas) =
            eth_signed::to_core_tx(raw, self.chain_id)?;
        Ok(self.routing_diagnostics_for_tx(&tx))
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
                    parent_state_root: [0u8; 32],
                    state_root: [0u8; 32],
                    tx_root: [0u8; 32],
                    receipt_root: [0u8; 32],
                    native_event_root: [0u8; 32],
                    evm_log_root: [0u8; 32],
                    zone_namespace: fractal_consensus::MASTERCHAIN_ZONE_NAMESPACE,
                    da_root: [0u8; 32],
                    da_bytes: 0,
                    da_share_count: 0,
                    da_gas_used: 0,
                    da_fee_paid: 0,
                    gas_used: 0,
                    gas_limit: self.gas_limit,
                    feature_set: fractal_consensus::ExecutionFeatureSetV1::empty(),
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

    fn proof_for_block(&self, hash: &[u8; 32]) -> Option<BlockValidityProof> {
        self.proof_finalized_blocks.get(hash).cloned()
    }

    fn latest_proof_final_height_for_zone(&self, zone_id: u64) -> Option<u64> {
        NodeInner::latest_proof_final_height_for_zone(self, zone_id)
    }

    fn zone_update_finality(&self, zone_id: u64, height: u64) -> Option<String> {
        NodeInner::zone_update_finality(self, zone_id, height).map(|finality| match finality {
            BlockFinality::Soft => "soft".to_owned(),
            BlockFinality::Proof => "proof".to_owned(),
        })
    }

    fn owned_object_finality(
        &self,
        object_version: &OwnedObjectVersion,
    ) -> Option<(String, String)> {
        let record = self.owned_object_certificate_finality(object_version)?;
        let certificate_borsh = borsh::to_vec(&record.certificate).ok()?;
        Some((
            format!("0x{}", hex::encode(record.certificate_hash)),
            format!("0x{}", hex::encode(certificate_borsh)),
        ))
    }

    fn settlement_requires_proof_for_features(&self, features: ExecutionFeatureSetV1) -> bool {
        NodeInner::settlement_requires_proof_for_features(self, features)
    }

    fn settlement_finality_for_block_hash(&self, hash: &[u8; 32]) -> Result<(), String> {
        NodeInner::settlement_finality_for_block_hash(self, hash)
            .map(|_| ())
            .map_err(|e| e.to_string())
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
            witness_gen_latency_ms: format!("0x{:x}", self.proof_metrics.witness_gen_latency_ms),
            latest_proof_latency_ms: format!("0x{:x}", self.proof_metrics.latest_proof_latency_ms),
            latest_proof_final_lag_ms: format!(
                "0x{:x}",
                self.proof_metrics.latest_proof_final_lag_ms
            ),
            average_proof_latency_ms: format!("0x{:x}", average),
            proof_final_height: format!("0x{:x}", self.proof_metrics.proof_final_height),
            unsupported_feature_rejections: format!(
                "0x{:x}",
                self.proof_metrics.unsupported_feature_rejections
            ),
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

    fn consensus_diagnostics(&self) -> RpcConsensusDiagnostics {
        self.consensus_diagnostics_rpc()
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
            native_transition_proofs_enabled: self.chain_config.native_transition_proofs_enabled,
            proofs_required_for_settlement: format!(
                "0x{:x}",
                self.chain_config.proofs_required_for_settlement.bits
            ),
            owned_object_certificates: self.chain_config.phase_config.owned_object_certificates,
            da_sampling: self.chain_config.phase_config.da_sampling,
            proof_final_settlement: self.chain_config.phase_config.proof_final_settlement,
            execution_zones: self.chain_config.phase_config.execution_zones,
            forced_inclusion: self.chain_config.phase_config.forced_inclusion,
            prover_rewards: self.chain_config.phase_config.prover_rewards,
            sequencer_rewards: self.chain_config.phase_config.sequencer_rewards,
            block_payload_mode: self.chain_config.block_payload_mode.as_str().into(),
            rlvr_enabled: self.chain_config.rlvr.enabled,
            rlvr_chain_commit_enabled: self.chain_config.rlvr.chain_commit_enabled,
            rlvr_raw_data_on_chain: self.chain_config.rlvr.raw_data_on_chain,
            rlvr_raw_data_on_chain_requested: self.chain_config.rlvr.raw_data_on_chain_requested,
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

    fn owned_object_precheck(
        &self,
        raw_tx: &[u8],
        max_fee_per_gas: u128,
    ) -> Result<RpcOwnedObjectPrecheck, String> {
        self.owned_object_precheck_response(raw_tx, max_fee_per_gas)
            .map(|(_, response)| response)
    }

    fn countersign_owned_object_tx(
        &self,
        raw_tx: &[u8],
        max_fee_per_gas: u128,
    ) -> Result<RpcOwnedObjectCountersignature, String> {
        let (_tx, precheck) = self.owned_object_precheck_response(raw_tx, max_fee_per_gas)?;
        let sign_body_bytes = hex::decode(
            precheck
                .sign_body_borsh
                .strip_prefix("0x")
                .unwrap_or(&precheck.sign_body_borsh),
        )
        .map_err(|e| format!("decode sign body: {e}"))?;
        let sign_body = OwnedObjectCertificateSignBody::try_from_slice(&sign_body_bytes)
            .map_err(|e| format!("decode sign body borsh: {e}"))?;
        let secret = self
            .validator_secret
            .as_ref()
            .ok_or_else(|| "validator has no BLS signing key".to_owned())?;
        let signature =
            OwnedObjectCertificate::countersign(&sign_body, self.validator_index as u32, secret)
                .map_err(|e| e.to_string())?;
        let signature_borsh = borsh::to_vec(&signature).map_err(|e| e.to_string())?;
        Ok(RpcOwnedObjectCountersignature {
            validator_index: format!("0x{:x}", signature.validator_index),
            signature_borsh: hex_bytes(&signature_borsh),
            sign_body_borsh: precheck.sign_body_borsh,
        })
    }

    fn aggregate_owned_object_certificate(
        &self,
        raw_tx: &[u8],
        object_versions_borsh: &[u8],
        signatures_borsh: Vec<Vec<u8>>,
    ) -> Result<RpcOwnedObjectCertificate, String> {
        let tx = decode_borsh_tx(raw_tx)?;
        let object_versions = Vec::<OwnedObjectVersion>::try_from_slice(object_versions_borsh)
            .map_err(|e| format!("decode object versions borsh: {e}"))?;
        let signatures = signatures_borsh
            .iter()
            .map(|bytes| {
                OwnedObjectValidatorSignature::try_from_slice(bytes)
                    .map_err(|e| format!("decode validator signature borsh: {e}"))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let cert = OwnedObjectCertificate::aggregate(
            &tx,
            object_versions,
            signatures,
            self.validators.quorum_threshold(),
        )
        .map_err(|e| e.to_string())?;
        let pubkeys = self.validator_pubkeys_for_certificates()?;
        cert.verify(&pubkeys, self.validators.quorum_threshold())
            .map_err(|e| e.to_string())?;
        let certificate_hash = cert.certificate_hash().map_err(|e| e.to_string())?;
        let certificate_borsh = borsh::to_vec(&cert).map_err(|e| e.to_string())?;
        Ok(RpcOwnedObjectCertificate {
            certificate_hash: hash_hex(&certificate_hash),
            certificate_borsh: hex_bytes(&certificate_borsh),
            signer_indices: cert
                .signer_indices
                .iter()
                .map(|idx| format!("0x{idx:x}"))
                .collect(),
        })
    }

    fn submit_proof_hash(
        &mut self,
        proof_hash: [u8; 32],
    ) -> Result<ProofCommitmentResponse, String> {
        let block_number = self.block_number();
        self.proof_commitments.insert(proof_hash, block_number);

        let signer = HARDHAT_DEFAULT_SIGNER_0;
        let tx = Transaction {
            signer,
            nonce: self.transaction_count(&signer),
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::ProofCommitmentV1 { proof_hash }),
        };
        let raw = borsh::to_vec(&tx)
            .map_err(|err| format!("failed to encode proof commitment tx: {err}"))?;
        let tx_hash = keccak256(&raw);
        self.pending_txs.insert(tx_hash, tx.clone());
        self.mempool.insert(PooledTx {
            tx,
            max_priority_fee_per_gas: 1,
            max_fee_per_gas: u128::MAX,
            eth_signed_raw: None,
        });

        Ok(ProofCommitmentResponse {
            network: format!("fractalchain-{}", self.chain_id()),
            transaction_hash: format!("0x{}", hex::encode(tx_hash)),
            block_number,
            finalized: true,
        })
    }

    fn submit_rlmf_attestation(
        &mut self,
        record: RlmfAttestationRecord,
    ) -> Result<RlmfAttestationResponse, String> {
        let commitment = record
            .validate()
            .map_err(|e| format!("invalid RLMF attestation: {e}"))?;
        if let Some(existing) = self.rlmf_attestations.get(&commitment) {
            if existing.record == record {
                // Idempotent resubmission of an identical record returns the
                // original inclusion data instead of double-committing.
                return Ok(RlmfAttestationResponse {
                    network: format!("fractalchain-{}", self.chain_id()),
                    transaction_hash: existing.transaction_hash.clone(),
                    block_number: existing.block_number,
                    finalized: existing.finalized,
                    attestation: existing.record.clone(),
                });
            }
            return Err(
                "attestation already recorded with different contents for this commitment"
                    .to_string(),
            );
        }
        // Reuse the generic proof-commitment path: the canonical commitment is
        // the 32-byte on-chain footprint; the full record stays in the index.
        let response = ChainInteraction::submit_proof_hash(self, commitment)?;
        let stored = RlmfAttestationStored {
            record: record.clone(),
            transaction_hash: response.transaction_hash.clone(),
            block_number: response.block_number,
            finalized: response.finalized,
        };
        self.rlmf_attestations.insert(commitment, stored);
        Ok(RlmfAttestationResponse {
            network: response.network,
            transaction_hash: response.transaction_hash,
            block_number: response.block_number,
            finalized: response.finalized,
            attestation: record,
        })
    }

    fn rlmf_attestation_by_commitment(&self, hash: [u8; 32]) -> Option<RlmfAttestationStored> {
        self.rlmf_attestations.get(&hash).cloned()
    }

    fn list_rlmf_attestations(
        &self,
        subject_id: Option<&str>,
        source_system: Option<&str>,
        block_number: Option<u64>,
        transaction_hash: Option<&str>,
        limit: usize,
    ) -> Vec<RlmfAttestationStored> {
        let wanted_tx = transaction_hash.map(|raw| {
            let raw = raw.strip_prefix("0x").unwrap_or(raw).to_ascii_lowercase();
            format!("0x{raw}")
        });
        let mut records: Vec<RlmfAttestationStored> = self
            .rlmf_attestations
            .values()
            .filter(|stored| {
                subject_id.is_none_or(|wanted| stored.record.subject_id == wanted)
                    && source_system.is_none_or(|wanted| stored.record.source_system == wanted)
                    && block_number.is_none_or(|wanted| stored.block_number == wanted)
                    && wanted_tx
                        .as_deref()
                        .is_none_or(|wanted| stored.transaction_hash.eq_ignore_ascii_case(wanted))
            })
            .cloned()
            .collect();
        records.sort_by(|a, b| {
            b.block_number
                .cmp(&a.block_number)
                .then_with(|| a.record.commitment_hash.cmp(&b.record.commitment_hash))
        });
        records.truncate(limit.min(256));
        records
    }

    fn submit_proof_update(
        &mut self,
        update: ZoneProofUpdateV1,
        max_priority_fee: u128,
    ) -> Result<RpcProofUpdateSubmission, String> {
        let update_hash = NodeInner::submit_proof_update(self, update.clone(), max_priority_fee)
            .map_err(|e| e.to_string())?;
        Ok(RpcProofUpdateSubmission {
            network: format!("fractalchain-{}", self.chain_id()),
            proof_update_hash: hash_hex(&update_hash),
            zone_id: format!("0x{:x}", update.zone_id),
            height: format!("0x{:x}", update.height),
            pending_proof_updates: format!("0x{:x}", self.proof_pool.len()),
        })
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
        ProofVerifyError::Timestamp => "timestamp",
        ProofVerifyError::ParentStateRoot => "parent_state_root",
        ProofVerifyError::StateRoot => "state_root",
        ProofVerifyError::TxRoot => "tx_root",
        ProofVerifyError::ReceiptRoot => "receipt_root",
        ProofVerifyError::NativeEventRoot => "native_event_root",
        ProofVerifyError::EvmLogRoot => "evm_log_root",
        ProofVerifyError::DaRoot => "da_root",
        ProofVerifyError::ZoneNamespace => "zone_namespace",
        ProofVerifyError::FeatureSet => "feature_set",
        ProofVerifyError::CoverageManifest => "coverage_manifest",
        ProofVerifyError::CircuitCoverage => "circuit_coverage",
        ProofVerifyError::EmptyProof => "empty_proof",
        ProofVerifyError::Production(_) => "production_proof",
        ProofVerifyError::BadDevDigest => "bad_dev_digest",
        ProofVerifyError::DevDigestDisabled => "dev_digest_disabled",
        ProofVerifyError::DataAvailability => "data_availability",
        ProofVerifyError::Io(_) => "io",
    }
    .to_owned()
}

fn proof_witness_digest(
    proof: &BlockValidityProof,
) -> Result<Option<fractal_crypto::Hash256>, ProofFinalityError> {
    if proof.proof_system != fractal_consensus::ValidityProofSystem::StwoPlonky2 {
        return Ok(None);
    }
    let Ok(envelope) = StwoPlonky2ProofEnvelope::try_from_slice(&proof.proof_bytes) else {
        return Ok(None);
    };
    Ok(match envelope {
        StwoPlonky2ProofEnvelope::NativeRecursiveFixtureV1 { statement, .. } => {
            Some(statement.witness_digest)
        }
        StwoPlonky2ProofEnvelope::EvmZkVmFixtureV1 { fixture } => {
            Some(fixture.statement.witness_digest)
        }
        StwoPlonky2ProofEnvelope::MixedIntraBlockAggregateFixtureV1 { fixture } => {
            Some(fixture.witness_digest)
        }
        _ => None,
    })
}

fn hex_bytes(bytes: &[u8]) -> String {
    format!("0x{}", hex::encode(bytes))
}

fn rpc_owned_object_id(object_id: &fractal_core::OwnedObjectId) -> String {
    match object_id {
        fractal_core::OwnedObjectId::AccountNonce(address) => {
            format!("accountNonce:{}", addr_hex(address))
        }
        fractal_core::OwnedObjectId::Agent(agent_id) => format!("agent:{agent_id}"),
        fractal_core::OwnedObjectId::Receipt(receipt_id) => {
            format!("receipt:{}", hash_hex(receipt_id))
        }
        fractal_core::OwnedObjectId::WalletTaskReceipt(commitment) => {
            format!("walletTaskReceipt:{}", hash_hex(commitment))
        }
        fractal_core::OwnedObjectId::ProofCommitment(proof_hash) => {
            format!("proofCommitment:{}", hash_hex(proof_hash))
        }
    }
}

fn rpc_owned_object_version(version: &OwnedObjectVersion) -> String {
    format!(
        "{}@{}",
        rpc_owned_object_id(&version.object_id),
        version.version
    )
}

fn decode_borsh_tx(raw_tx: &[u8]) -> Result<Transaction, String> {
    Transaction::try_from_slice(raw_tx).map_err(|e| format!("invalid borsh transaction: {e}"))
}

impl NodeInner {
    fn owned_object_precheck_response(
        &self,
        raw_tx: &[u8],
        max_fee_per_gas: u128,
    ) -> Result<(Transaction, RpcOwnedObjectPrecheck), String> {
        let tx = decode_borsh_tx(raw_tx)?;
        let object_versions = self
            .state
            .owned_object_versions_for_transaction(&tx)
            .ok_or_else(|| {
                "transaction is not eligible for owned-object certificate path".to_owned()
            })?;
        let precheck = self
            .state
            .precheck_owned_transaction(
                &tx,
                &object_versions,
                self.gas_limit,
                max_fee_per_gas,
                self.base_fee,
            )
            .map_err(|e| e.to_string())?;
        let sign_body = OwnedObjectCertificateSignBody {
            tx_hash: precheck.tx_hash,
            owner: precheck.owner,
            signer_nonce: precheck.signer_nonce,
            object_versions: precheck.object_versions.clone(),
        };
        let object_versions_borsh =
            borsh::to_vec(&precheck.object_versions).map_err(|e| e.to_string())?;
        let sign_body_borsh = borsh::to_vec(&sign_body).map_err(|e| e.to_string())?;
        let response = RpcOwnedObjectPrecheck {
            tx_hash: hash_hex(&precheck.tx_hash),
            owner: addr_hex(&precheck.owner),
            signer_nonce: format!("0x{:x}", precheck.signer_nonce),
            object_versions: precheck
                .object_versions
                .iter()
                .map(rpc_owned_object_version)
                .collect(),
            object_versions_borsh: hex_bytes(&object_versions_borsh),
            sign_body_borsh: hex_bytes(&sign_body_borsh),
            tx_gas: format!("0x{:x}", precheck.tx_gas),
            max_fee_per_gas: format!("0x{:x}", precheck.max_fee_per_gas),
            base_fee_per_gas: format!("0x{:x}", precheck.base_fee_per_gas),
        };
        Ok((tx, response))
    }

    fn validator_pubkeys_for_certificates(
        &self,
    ) -> Result<Vec<fractal_crypto::BlsPublicKey>, String> {
        (0..self.validators.len())
            .map(|idx| {
                self.validators
                    .bls_pubkey(idx)
                    .copied()
                    .ok_or_else(|| format!("missing validator BLS pubkey at index {idx}"))
            })
            .collect()
    }
}

fn is_proof_ingestion_compat_tx(tx: &Transaction) -> bool {
    matches!(
        (&tx.vm, &tx.body),
        (
            VmKind::Native,
            TxBody::Native(NativeCall::ProofCommitmentV1 { .. })
        )
    )
}

fn include_tx_for_payload_mode(mode: BlockPayloadMode, tx: &Transaction) -> bool {
    match mode {
        BlockPayloadMode::Legacy => true,
        BlockPayloadMode::ProofIngestion => is_proof_ingestion_compat_tx(tx),
        BlockPayloadMode::Mixed => is_proof_ingestion_compat_tx(tx) || !tx.is_owned_object_tx(),
    }
}

fn rlvr_chain_commit_active(config: &ChainConfig) -> bool {
    config.rlvr.enabled && config.rlvr.chain_commit_enabled && !config.rlvr.raw_data_on_chain
}

fn rlvr_proof_type_tag(proof_type: RlvrProofType) -> RlvrProofTypeTag {
    match proof_type {
        RlvrProofType::ProofOfRoute => RlvrProofTypeTag::ProofOfRoute,
        RlvrProofType::ProofOfEval => RlvrProofTypeTag::ProofOfEval,
        RlvrProofType::ProofOfTraining => RlvrProofTypeTag::ProofOfTraining,
    }
}

fn decode_hash32(name: &str, raw: &str) -> Result<fractal_crypto::Hash256, String> {
    let trimmed = raw.trim().trim_start_matches("0x");
    let bytes = hex::decode(trimmed).map_err(|err| format!("invalid {name}: {err}"))?;
    bytes
        .try_into()
        .map_err(|bytes: Vec<u8>| format!("{name} must be 32 bytes, got {}", bytes.len()))
}

fn decode_optional_hash32(
    name: &str,
    raw: Option<&String>,
) -> Result<fractal_crypto::Hash256, String> {
    match raw {
        Some(value) => decode_hash32(name, value),
        None => Ok([0u8; 32]),
    }
}

fn rlvr_commitment_from_pooled(pooled: &RlvrPooledProof) -> Result<RlvrProofCommitmentV1, String> {
    let proof_hash = decode_hash32("proof_hash", &pooled.proof_hash)?;
    let computed_hash = decode_hash32(
        "computed proof_hash",
        &pooled.proof.proof_hash().map_err(|e| e.to_string())?,
    )?;
    if computed_hash != proof_hash {
        return Err("pooled RLVR proof hash does not match proof object".into());
    }
    Ok(RlvrProofCommitmentV1 {
        proof_type: rlvr_proof_type_tag(pooled.proof.proof_type),
        proof_hash,
        trace_hash: decode_hash32("trace_hash", &pooled.proof.trace_hash)?,
        route_policy_hash: decode_hash32("route_policy_hash", &pooled.proof.route_policy_hash)?,
        reward_policy_hash: decode_hash32("reward_policy_hash", &pooled.proof.reward_policy_hash)?,
        model_id_hash: decode_hash32("model_id_hash", &pooled.proof.model_id_hash)?,
        adapter_hash: decode_optional_hash32("adapter_hash", pooled.proof.adapter_hash.as_ref())?,
        eval_result_hash: decode_optional_hash32(
            "eval_result_hash",
            pooled.proof.eval_result_hash.as_ref(),
        )?,
        timestamp_unix: pooled.proof.timestamp,
    })
}

fn proposal_payload_for_mode(
    mode: BlockPayloadMode,
    txs: &[Transaction],
    eth_raws: &[Option<Vec<u8>>],
    proof_updates: &[ZoneProofUpdateV1],
    certificates: &[OwnedObjectCertificate],
    rlvr_proofs: &[RlvrProofCommitmentV1],
) -> BlockPayload {
    match mode {
        BlockPayloadMode::Legacy => BlockPayload::FullTransactions {
            transactions: txs.to_vec(),
            eth_signed_raw: eth_raws.to_vec(),
        },
        BlockPayloadMode::ProofIngestion
            if txs.is_empty() && certificates.is_empty() && rlvr_proofs.is_empty() =>
        {
            BlockPayload::ProofUpdates(proof_updates.to_vec())
        }
        BlockPayloadMode::ProofIngestion
            if txs.is_empty() && proof_updates.is_empty() && rlvr_proofs.is_empty() =>
        {
            BlockPayload::CertificateBatches(vec![OwnedObjectCertificateBatchV1 {
                certificates: certificates.to_vec(),
            }])
        }
        BlockPayloadMode::ProofIngestion | BlockPayloadMode::Mixed => {
            let mut items = Vec::with_capacity(
                txs.len()
                    + proof_updates.len()
                    + usize::from(!certificates.is_empty())
                    + rlvr_proofs.len(),
            );
            for (idx, tx) in txs.iter().enumerate() {
                items.push(BlockPayloadItem::Transaction {
                    transaction: tx.clone(),
                    eth_signed_raw: eth_raws.get(idx).cloned().unwrap_or(None),
                });
            }
            items.extend(
                proof_updates
                    .iter()
                    .cloned()
                    .map(BlockPayloadItem::ProofUpdate),
            );
            if !certificates.is_empty() {
                items.push(BlockPayloadItem::CertificateBatch(
                    OwnedObjectCertificateBatchV1 {
                        certificates: certificates.to_vec(),
                    },
                ));
            }
            items.extend(rlvr_proofs.iter().cloned().map(BlockPayloadItem::RlvrProof));
            BlockPayload::Mixed(items)
        }
    }
}

fn apply_zone_blob_da_to_block(
    block: &mut Block,
    payload: &BlockPayload,
    sampling_seed: u64,
) -> Result<(), String> {
    let payload_root = payload
        .payload_root()
        .map_err(|e| format!("proof payload root failed: {e}"))?;
    let payload_bytes =
        borsh::to_vec(payload).map_err(|e| format!("proof payload DA encode failed: {e}"))?;
    let blob = ZoneBlobDaV1 {
        namespace: block.header.zone_namespace,
        payload: payload_bytes,
        share_size: fractal_consensus::DEFAULT_DA_SHARE_SIZE,
        sampling: DaSamplingParamsV1 {
            seed: sampling_seed,
            sample_count: 16,
            min_samples: 4,
        },
    };
    let (sidecar, commitment) =
        build_zone_blob_da_sidecar(&blob).map_err(|e| format!("zone blob DA failed: {e}"))?;
    block.header.da_root = commitment.da_root;
    block.header.da_bytes = commitment.byte_count;
    block.header.da_share_count = commitment.share_count;
    block.header.da_gas_used = da_gas_for_sidecar(&sidecar);
    block.header.da_fee_paid = da_fee_for_gas(block.header.da_gas_used);
    block.header.extra = proof_ingestion_header_extra(payload_root, &commitment)
        .map_err(|e| format!("proof-ingestion header extra failed: {e}"))?;
    block.da_sidecar = sidecar;
    Ok(())
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
    let payload_mode = n.block_payload_mode();
    let include_rlvr_proofs =
        payload_mode != BlockPayloadMode::Legacy && rlvr_chain_commit_active(&n.chain_config);
    let pooled = n
        .mempool
        .drain_ready_gas_budget_filtered(gas_limit_cfg, base, |tx| {
            include_tx_for_payload_mode(payload_mode, tx)
        });
    let eth_raws: Vec<Option<Vec<u8>>> = pooled.iter().map(|p| p.eth_signed_raw.clone()).collect();
    let txs: Vec<Transaction> = pooled.into_iter().map(|p| p.tx).collect();
    let proof_updates = match payload_mode {
        BlockPayloadMode::Legacy => Vec::new(),
        BlockPayloadMode::ProofIngestion | BlockPayloadMode::Mixed => {
            n.proof_pool.drain_ready(1024)
        }
    };
    let certificates = match payload_mode {
        BlockPayloadMode::Legacy => Vec::new(),
        BlockPayloadMode::ProofIngestion | BlockPayloadMode::Mixed => {
            n.certificate_pool.accepted_certificates()
        }
    };
    let rlvr_proofs = if include_rlvr_proofs {
        let pooled_rlvr_proofs = n.rlvr_proof_pool.drain_ready(1024);
        let mut commitments = Vec::with_capacity(pooled_rlvr_proofs.len());
        for pooled_rlvr_proof in &pooled_rlvr_proofs {
            match rlvr_commitment_from_pooled(pooled_rlvr_proof) {
                Ok(commitment) => commitments.push(commitment),
                Err(err) => {
                    eprintln!("fractal-node: RLVR proof commitment conversion failed: {err}");
                    return ProduceTickOutcome::BuildFailed;
                }
            }
        }
        commitments
    } else {
        Vec::new()
    };
    let proposal_payload = proposal_payload_for_mode(
        payload_mode,
        &txs,
        &eth_raws,
        &proof_updates,
        &certificates,
        &rlvr_proofs,
    );
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
        Ok(mut block) => {
            if payload_mode != BlockPayloadMode::Legacy {
                if let Err(e) =
                    apply_zone_blob_da_to_block(&mut block, &proposal_payload, ts ^ height)
                {
                    eprintln!("fractal-node: {e}");
                    return ProduceTickOutcome::BuildFailed;
                }
            }
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
    inner.shard_id = shard_id_from_env();
    inner.shard_count = shard_count_from_env();
    inner.consensus_mode = consensus_mode_from_env();
    let proof_required = proof_required_settlement_from_env();
    inner.set_protocol_phase_config(protocol_phase_config_from_env());
    inner.set_native_transition_proofs_enabled(native_transition_proofs_enabled_from_env());
    inner.set_proofs_required_for_settlement(proof_required_feature_mask_from_env(proof_required));
    inner.set_block_payload_mode(BlockPayloadMode::from_env());
    inner.set_rlvr_node_flags(RlvrNodeFlags::from_env());
    if let Some(path) = proof_finality_store_from_env() {
        inner.set_proof_finality_store(ProofFinalityStore::open(&path)?)?;
        eprintln!("fractal-node: proof_finality_store={}", path.display());
    }
    attach_rlvr_route_trace_logger(&mut inner, "fractal-node");
    inner.set_vote_sink(Some(vote_tx));
    eprintln!(
        "fractal-node: settlement_finality={} block_payload_mode={} rlvr_enabled={} rlvr_chain_commit_enabled={} rlvr_raw_data_on_chain={}",
        if inner.settlement_requires_proof() {
            "proof"
        } else {
            "soft"
        },
        inner.block_payload_mode().as_str(),
        inner.chain_config.rlvr.enabled,
        inner.chain_config.rlvr.chain_commit_enabled,
        inner.chain_config.rlvr.raw_data_on_chain
    );
    inner.log_startup_consensus_diagnostics("fractal-node");
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
    inner.shard_id = shard_id_from_env();
    inner.shard_count = shard_count_from_env();
    inner.consensus_mode = consensus_mode_from_env();
    let proof_required = proof_required_settlement_from_env();
    inner.set_protocol_phase_config(protocol_phase_config_from_env());
    inner.set_native_transition_proofs_enabled(native_transition_proofs_enabled_from_env());
    inner.set_proofs_required_for_settlement(proof_required_feature_mask_from_env(proof_required));
    inner.set_block_payload_mode(BlockPayloadMode::from_env());
    inner.set_rlvr_node_flags(RlvrNodeFlags::from_env());
    if let Some(path) = proof_finality_store_from_env() {
        inner.set_proof_finality_store(ProofFinalityStore::open(&path)?)?;
        eprintln!(
            "fractal-node follower: proof_finality_store={}",
            path.display()
        );
    }
    attach_rlvr_route_trace_logger(&mut inner, "fractal-node follower");
    inner.set_vote_sink(Some(vote_tx));
    eprintln!(
        "fractal-node follower: settlement_finality={} block_payload_mode={} rlvr_enabled={} rlvr_chain_commit_enabled={} rlvr_raw_data_on_chain={}",
        if inner.settlement_requires_proof() {
            "proof"
        } else {
            "soft"
        },
        inner.block_payload_mode().as_str(),
        inner.chain_config.rlvr.enabled,
        inner.chain_config.rlvr.chain_commit_enabled,
        inner.chain_config.rlvr.raw_data_on_chain
    );
    inner.log_startup_consensus_diagnostics("fractal-node follower");
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
