//! HotStuff-2–oriented block types for **singleton** (`n = 1`, `f = 0`) production (`docs/prd.md` §7.3, §18 M2).
//!
//! Full vote aggregation / libp2p gossip lands in later milestones; this crate freezes the
//! on-disk / wire shape and deterministic header hashing for the execution pipeline.
//!
//! [`qc`] defines quorum certificate hashing and the Phase-1 singleton `parent_qc_hash` chain
//! (`docs/prd.md` §18 M7-a).
//!
//! [`validators`] holds static validator sets and view-based leader ids (`docs/prd.md` §18 M7-b).
//!
//! [`vote`] holds per-validator HotStuff-2 vote wire types (`docs/prd.md` §18 M7-d-3).

use borsh::{BorshDeserialize, BorshSerialize};
use fractal_core::{state_root, ExecError, State, Transaction, TxBody, VmKind};
use fractal_crypto::hash::{keccak256, Hash256};
use reed_solomon_erasure::galois_8::ReedSolomon;
use std::collections::BTreeMap;
use thiserror::Error;

pub mod fees;
pub mod payload;
pub mod proof;
pub mod qc;
pub mod validators;
pub mod vote;

pub use fees::{default_fee_policy, FeeCostCategory, FeePolicyV1};
pub use fractal_core::Transaction as Tx;
pub use payload::{
    certificate_batch_conflicts, certificate_batch_leaf_hash, certificate_batch_root,
    certificate_batches_root, payload_leaf_hash, proof_update_leaf_hash, proof_updates_root,
    versioned_payload_root, BlockPayload, BlockPayloadItem, BlockPayloadKind,
    OwnedObjectCertificateBatchV1, ZoneProofUpdateV1,
};
pub use proof::{
    canonical_recursive_proof_fixture_v1, evm_zkvm_proof_envelope_v1, evm_zkvm_proof_fixture_v1,
    evm_zkvm_transition_statement_v1, mixed_intrablock_aggregate_fixture_v1,
    mixed_intrablock_aggregate_proof_envelope_v1, native_mixed_component_statement_v1,
    native_recursive_proof_envelope_v1, native_recursive_wrap_fixture_v1,
    native_state_transition_air_id, native_state_transition_air_v1,
    native_state_transition_statement_digest_v1, native_state_transition_statement_v1,
    native_state_transition_trace_columns_v1, prove_native_state_transition_fixture_v1,
    stwo_execution_air_adapter_digest, stwo_execution_air_adapter_v1, stwo_execution_air_id,
    stwo_plonky2_public_input_limbs, stwo_plonky2_verifier_id, verify_native_inter_block_chain_v1,
    verify_native_state_transition_fixture_v1, verify_stwo_plonky2_proof,
    CanonicalRecursiveProofFixtureV1, CompressedPlonky2NativeProofFixtureV1, EvmZkVmProofFixtureV1,
    EvmZkVmTransitionStatementV1, MixedIntraBlockAggregateFixtureV1,
    NativeMixedComponentStatementV1, NativeRecursiveWrapFixtureV1, NativeStateTransitionAirError,
    NativeStateTransitionAirV1, NativeStateTransitionProofFixtureV1,
    NativeStateTransitionStatementV1, NativeStateTransitionTraceColumnRowV1,
    ProductionProofVerifyError, StwoExecutionAirAdapterV1, StwoPlonky2ProofEnvelope,
};
pub use qc::{
    expected_parent_qc_for_parent_header, genesis_parent_qc, hash_qc,
    next_parent_qc_hash_after_commit, singleton_qc_certifying, QuorumCertificate,
};
pub use validators::{ValidatorEntry, ValidatorId, ValidatorSet};
pub use vote::{
    verify_formed_qc, FormedQc, RecordVoteOutcome, Vote, VoteError, VotePool, VoteSignBody,
};

#[derive(Debug, Error)]
pub enum BuildBlockError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Exec(#[from] ExecError),
    #[error("eth_signed_raw length {got} != transactions length {txs}")]
    EthRawLenMismatch { txs: usize, got: usize },
    #[error("data availability sidecar invalid")]
    DataAvailability,
    #[error(transparent)]
    Da(#[from] DaVerifyError),
}

#[derive(Debug, Error)]
pub enum MixedWitnessError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Exec(#[from] ExecError),
    #[error("witness replay gas used mismatch: header={header}, replay={replay}")]
    GasUsedMismatch { header: u64, replay: u64 },
    #[error("witness replay state root mismatch")]
    StateRoot,
    #[error("witness replay tx root mismatch")]
    TxRoot,
    #[error("witness replay receipt root mismatch")]
    ReceiptRoot,
    #[error("witness replay EVM log root mismatch")]
    EvmLogRoot,
    #[error("witness replay feature set mismatch")]
    FeatureSet,
    #[error("witness gas sum mismatch: header={header}, receipts={receipts}")]
    GasSumMismatch { header: u64, receipts: u64 },
    #[error("execution feature is not supported by the requested proving surface")]
    UnsupportedFeature,
}

#[derive(Debug, Error)]
pub enum ProofVerifyError {
    #[error("validity proof chain id does not match block")]
    ChainId,
    #[error("validity proof height does not match block")]
    Height,
    #[error("validity proof block hash does not match block")]
    BlockHash,
    #[error("validity proof timestamp does not match block")]
    Timestamp,
    #[error("validity proof state root does not match block")]
    StateRoot,
    #[error("validity proof parent state root does not match block")]
    ParentStateRoot,
    #[error("validity proof tx root does not match block")]
    TxRoot,
    #[error("validity proof receipt root does not match block")]
    ReceiptRoot,
    #[error("validity proof native event root does not match block")]
    NativeEventRoot,
    #[error("validity proof EVM log root does not match block")]
    EvmLogRoot,
    #[error("validity proof DA root does not match block")]
    DaRoot,
    #[error("validity proof zone namespace does not match block")]
    ZoneNamespace,
    #[error("validity proof feature set does not match block")]
    FeatureSet,
    #[error("validity proof circuit coverage manifest does not match circuit version")]
    CoverageManifest,
    #[error("validity proof circuit does not cover block feature set")]
    CircuitCoverage,
    #[error("validity proof bytes are empty")]
    EmptyProof,
    #[error("production proof verification failed: {0}")]
    Production(#[from] ProductionProofVerifyError),
    #[error("dev digest proof does not match public inputs")]
    BadDevDigest,
    #[error("dev digest proofs are disabled for this runtime")]
    DevDigestDisabled,
    #[error("data availability sidecar invalid")]
    DataAvailability,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProofEligibilityError {
    #[error("block feature set is not covered by circuit version")]
    CircuitCoverage,
    #[error("native-only circuit cannot prove EVM transaction at index {0}")]
    EvmTxInNativeCircuit(usize),
    #[error("native-only circuit cannot prove EVM-to-native precompile dispatch at index {0}")]
    PrecompileDispatchInNativeCircuit(usize),
    #[error("EVM transaction value is unsupported by the zkVM proving surface at index {0}")]
    UnsupportedEvmValue(usize),
    #[error("EVM call target is outside the current zkVM proving surface at index {0}")]
    UnsupportedEvmCallTarget(usize),
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DaVerifyError {
    #[error("data availability namespace mismatch")]
    Namespace,
    #[error("data availability original length mismatch")]
    OriginalLen,
    #[error("data availability share count mismatch")]
    ShareCount,
    #[error("data availability root mismatch")]
    Root,
    #[error("data availability share index mismatch")]
    ShareIndex,
    #[error("data availability share commitment mismatch")]
    ShareCommitment,
    #[error("data availability sampled share missing")]
    SampleMissing,
    #[error("data availability sampling receipt has insufficient samples")]
    InsufficientSamples,
    #[error("data availability erasure coding failed")]
    ErasureCoding,
    #[error("data availability insufficient shares for reconstruction")]
    InsufficientShares,
}

pub type DaNamespace = [u8; 8];
pub type ExecutionZoneNamespace = DaNamespace;

pub const DEFAULT_DA_NAMESPACE: DaNamespace = *b"fracbase";
pub const MASTERCHAIN_ZONE_NAMESPACE: ExecutionZoneNamespace = DEFAULT_DA_NAMESPACE;
pub const DEFAULT_DA_SHARE_SIZE: u32 = 512;
pub const DEFAULT_DA_PARITY_RATIO_NUMERATOR: u32 = 1;
pub const DEFAULT_DA_PARITY_RATIO_DENOMINATOR: u32 = 1;
pub const DEFAULT_DA_GAS_PER_BYTE: u64 = 1;
pub const DEFAULT_DA_FEE_PER_GAS: u128 = 1;

/// Legacy floor gas per tx (EVM transfer); native txs use [`fractal_core::intrinsic_gas`].
pub const MIN_TX_GAS: u64 = 21_000;

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct BlockHeader {
    pub version: u16,
    pub chain_id: u64,
    pub height: u64,
    pub view: u64,
    pub parent_hash: Hash256,
    /// Parent QC hash (HotStuff-2): `keccak256(borsh(QC))` certifying the parent block header.
    /// First real block uses [`crate::genesis_parent_qc`]; see [`crate::qc`].
    pub parent_qc_hash: Hash256,
    pub proposer: [u8; 32],
    pub timestamp_ms: u64,
    pub parent_state_root: Hash256,
    pub state_root: Hash256,
    pub tx_root: Hash256,
    pub receipt_root: Hash256,
    pub native_event_root: Hash256,
    pub evm_log_root: Hash256,
    pub zone_namespace: ExecutionZoneNamespace,
    pub da_root: Hash256,
    pub da_bytes: u64,
    pub da_share_count: u32,
    pub da_gas_used: u64,
    pub da_fee_paid: u128,
    pub gas_used: u64,
    pub gas_limit: u64,
    pub feature_set: ExecutionFeatureSetV1,
    pub extra: [u8; 32],
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct DaShare {
    pub namespace: DaNamespace,
    pub index: u32,
    pub is_parity: bool,
    pub data: Vec<u8>,
    pub commitment: Hash256,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct DaSidecar {
    pub namespace: DaNamespace,
    pub original_len: u64,
    pub share_size: u32,
    pub data_share_count: u32,
    pub parity_share_count: u32,
    pub shares: Vec<DaShare>,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct DaSamplingReceiptSample {
    pub index: u32,
    pub is_parity: bool,
    pub commitment: Hash256,
    pub merkle_path: Vec<Hash256>,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct DaSamplingReceipt {
    pub namespace: DaNamespace,
    pub da_root: Hash256,
    pub share_count: u32,
    pub seed: u64,
    pub sample_count: u32,
    pub samples: Vec<DaSamplingReceiptSample>,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct DaSamplingParamsV1 {
    pub seed: u64,
    pub sample_count: u32,
    pub min_samples: u32,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ZoneBlobDaV1 {
    pub namespace: DaNamespace,
    pub payload: Vec<u8>,
    pub share_size: u32,
    pub sampling: DaSamplingParamsV1,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ZoneBlobDaCommitmentV1 {
    pub namespace: DaNamespace,
    pub da_root: Hash256,
    pub byte_count: u64,
    pub share_count: u32,
    pub share_size: u32,
    pub sampling: DaSamplingParamsV1,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
    /// Parallel to `transactions`: optional original EIP-1559 bytes (`keccak256` = RPC tx hash).
    /// Followers replay this to populate `NodeInner::eth_signed_raw` / hash maps like the producer.
    pub eth_signed_raw: Vec<Option<Vec<u8>>>,
    /// Initial DA sidecar: chunked transaction payload committed by `header.da_root`.
    pub da_sidecar: DaSidecar,
}

impl Block {
    /// View the legacy block body through the versioned payload contract.
    ///
    /// This does not alter the legacy block wire shape; it is the compatibility
    /// bridge that lets proof-ingestion payloads land without breaking existing
    /// full-transaction blocks.
    #[must_use]
    pub fn payload(&self) -> BlockPayload {
        BlockPayload::FullTransactions {
            transactions: self.transactions.clone(),
            eth_signed_raw: self.eth_signed_raw.clone(),
        }
    }

    #[must_use]
    pub fn payload_kind(&self) -> BlockPayloadKind {
        self.payload().kind()
    }
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
#[borsh(use_discriminant = true)]
pub enum ValidityProofSystem {
    /// Test/dev proof receipt: `proof_bytes == validity_proof_public_input_digest(proof)`.
    #[cfg(feature = "dev-digest")]
    DevDigest = 0,
    /// Production target. Verification must be wired before this mode can finalize blocks.
    StwoPlonky2 = 1,
}

#[cfg(feature = "dev-digest")]
#[must_use]
pub fn dev_digest_allowed_for_runtime(network: Option<&str>, environment: Option<&str>) -> bool {
    fn production_like(value: &str) -> bool {
        matches!(
            value.to_ascii_lowercase().as_str(),
            "mainnet" | "production" | "prod" | "release"
        )
    }
    !network.is_some_and(production_like) && !environment.is_some_and(production_like)
}

#[cfg(feature = "dev-digest")]
fn dev_digest_allowed_for_current_runtime() -> bool {
    let network = std::env::var("FRACTAL_NETWORK").ok();
    let environment = std::env::var("FRACTAL_ENV").ok();
    dev_digest_allowed_for_runtime(network.as_deref(), environment.as_deref())
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct BlockValidityProof {
    pub chain_id: u64,
    pub height: u64,
    pub block_hash: Hash256,
    pub timestamp_ms: u64,
    pub parent_state_root: Hash256,
    pub state_root: Hash256,
    pub tx_root: Hash256,
    pub receipt_root: Hash256,
    pub native_event_root: Hash256,
    pub evm_log_root: Hash256,
    pub gas_used: u64,
    pub zone_namespace: ExecutionZoneNamespace,
    pub da_root: Hash256,
    pub circuit_version: CircuitVersion,
    pub coverage_manifest_digest: Hash256,
    pub feature_set: ExecutionFeatureSetV1,
    pub proof_system: ValidityProofSystem,
    pub proof_bytes: Vec<u8>,
}

pub const MIXED_EXECUTION_WITNESS_V1: u16 = 1;
pub const STATE_COMMITMENT_SCHEME_V1: StateCommitmentScheme =
    StateCommitmentScheme::FractalSnarkSmtV1;

pub const FEATURE_NATIVE_TX: u64 = 1 << 0;
pub const FEATURE_NATIVE_SHARED_STATE: u64 = 1 << 1;
pub const FEATURE_EVM_TRANSFER: u64 = 1 << 2;
pub const FEATURE_EVM_CALL: u64 = 1 << 3;
pub const FEATURE_EVM_CREATE: u64 = 1 << 4;
pub const FEATURE_EVM_TO_NATIVE_PRECOMPILE: u64 = 1 << 5;

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum CircuitVersion {
    /// Dev/test coverage. This is not a production settlement circuit.
    DevMixedV1,
    /// Native-only production path. Must prove no EVM tx/precompile row is present.
    NativeStateTransitionV1,
    /// Eventual heterogeneous native + EVM aggregate circuit.
    MixedStateTransitionV1,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ExecutionFeatureSetV1 {
    pub bits: u64,
}

impl ExecutionFeatureSetV1 {
    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    pub const fn all_known() -> Self {
        Self {
            bits: FEATURE_NATIVE_TX
                | FEATURE_NATIVE_SHARED_STATE
                | FEATURE_EVM_TRANSFER
                | FEATURE_EVM_CALL
                | FEATURE_EVM_CREATE
                | FEATURE_EVM_TO_NATIVE_PRECOMPILE,
        }
    }

    pub fn insert(&mut self, bit: u64) {
        self.bits |= bit;
    }

    pub fn contains_only(self, coverage: Self) -> bool {
        self.bits & !coverage.bits == 0
    }
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct CoverageManifestV1 {
    pub version: u16,
    pub circuit_version: CircuitVersion,
    pub covered_features: ExecutionFeatureSetV1,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateCommitmentScheme {
    /// Protocol decision for proof witnesses: SNARK-friendly sparse Merkle trees for native
    /// subtries and internal EVM proving commitments. The host-side placeholder root is still
    /// domain-separated keccak until the concrete Poseidon/Rescue parameters land in the AIR.
    FractalSnarkSmtV1,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum StateCommitmentNamespace {
    Accounts,
    Native,
    Evm,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateAccessKindV1 {
    Read,
    Write,
    ReadWrite,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct StateCommitmentV1 {
    pub scheme: StateCommitmentScheme,
    pub native_root: Hash256,
    pub evm_root: Hash256,
    pub unified_root: Hash256,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct StateMerkleAccessWitnessV1 {
    pub kind: StateAccessKindV1,
    pub namespace: StateCommitmentNamespace,
    pub key: Vec<u8>,
    pub pre_value_hash: Option<Hash256>,
    pub post_value_hash: Option<Hash256>,
    pub pre_state_path: Vec<Hash256>,
    pub post_state_path: Vec<Hash256>,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeTraceKindV1 {
    TopLevelNative,
    EvmPrecompileDispatch,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct NativeExecutionTraceRowV1 {
    pub tx_index: u32,
    pub tx_hash: Hash256,
    pub kind: NativeTraceKindV1,
    pub signer: [u8; 20],
    pub nonce: u64,
    pub call: fractal_core::NativeCall,
    pub gas_used: u64,
    pub native_event_root: Hash256,
    pub native_event_start: u32,
    pub native_event_count: u32,
    pub signer_pre_nonce: u64,
    pub signer_post_nonce: u64,
    pub signer_pre_balance: u128,
    pub signer_post_balance: u128,
    pub state_access_indices: Vec<u32>,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct NativePrecompileDispatchTraceRowV1 {
    pub tx_index: u32,
    pub tx_hash: Hash256,
    pub caller: [u8; 20],
    pub precompile_address: [u8; 20],
    pub native_opcode: u8,
    pub decoded_call: fractal_core::NativeCall,
    pub calldata_hash: Hash256,
    pub gas_used: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum EvmTraceKindV1 {
    Transfer,
    Call,
    Create,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct EvmExecutionTraceRowV1 {
    pub tx_index: u32,
    pub tx_hash: Hash256,
    pub kind: EvmTraceKindV1,
    pub caller: [u8; 20],
    pub to: Option<[u8; 20]>,
    pub value: u128,
    pub input_hash: Hash256,
    pub gas_limit: u64,
    pub gas_used: u64,
    pub success: bool,
    pub log_root: Hash256,
    pub pre_evm_root: Hash256,
    pub post_evm_root: Hash256,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZkVmChoiceV1 {
    /// Phase F decision: prove the node's `fractal_evm::RevmEngine` transition in a RISC Zero guest.
    RiscZeroRevmTransitionV1,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct EvmZkVmSurfaceV1 {
    pub version: u16,
    pub zkvm_choice: ZkVmChoiceV1,
    pub zkvm_target: String,
    pub revm_crate_version: String,
    pub covered_features: ExecutionFeatureSetV1,
    pub uncovered_features: ExecutionFeatureSetV1,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MixedProofBatchingModeV1 {
    PerBlockPreferred,
    BatchedFallback,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct EvmMixedProvingBudgetV1 {
    pub version: u16,
    pub zkvm_choice: ZkVmChoiceV1,
    pub native_block_latency_target_ms: u64,
    pub mixed_block_latency_target_ms: u64,
    pub sustained_throughput_blocks_per_minute: u32,
    pub max_proof_final_lag_ms: u64,
    pub batching_mode: MixedProofBatchingModeV1,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct MixedExecutionPublicInputsV1 {
    pub version: u16,
    pub chain_id: u64,
    pub height: u64,
    pub block_hash: Hash256,
    pub timestamp_ms: u64,
    pub parent_state_root: Hash256,
    pub post_state_root: Hash256,
    pub tx_root: Hash256,
    pub receipt_root: Hash256,
    pub native_event_root: Hash256,
    pub evm_log_root: Hash256,
    pub gas_used: u64,
    pub zone_namespace: ExecutionZoneNamespace,
    pub da_root: Hash256,
    pub circuit_version: CircuitVersion,
    pub coverage_manifest_digest: Hash256,
    pub feature_set: ExecutionFeatureSetV1,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct MixedExecutionTxReceiptV1 {
    pub tx_hash: Hash256,
    pub success: bool,
    pub gas_used: u64,
    pub evm_log_root: Hash256,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct MixedExecutionWitnessV1 {
    pub version: u16,
    pub public_inputs: MixedExecutionPublicInputsV1,
    pub pre_state_commitment: StateCommitmentV1,
    pub post_state_commitment: StateCommitmentV1,
    pub state_accesses: Vec<StateMerkleAccessWitnessV1>,
    pub native_trace_rows: Vec<NativeExecutionTraceRowV1>,
    pub precompile_dispatch_rows: Vec<NativePrecompileDispatchTraceRowV1>,
    pub evm_trace_rows: Vec<EvmExecutionTraceRowV1>,
    pub evm_zkvm_surface: EvmZkVmSurfaceV1,
    pub gas_sum: u64,
    pub transactions: Vec<Transaction>,
    pub eth_signed_raw: Vec<Option<Vec<u8>>>,
    pub tx_receipts: Vec<MixedExecutionTxReceiptV1>,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum WitnessRetentionPolicyV1 {
    /// Store metadata only; full witnesses can be regenerated from archival pre-state and block data.
    MetadataOnly,
    /// Keep full witness bytes in the proof-worker backend until the block becomes proof-final.
    UntilProofFinal,
    /// Keep full witness bytes for archival/reproducibility workflows.
    Archive,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct MixedExecutionWitnessMetadataV1 {
    pub version: u16,
    pub block_hash: Hash256,
    pub height: u64,
    pub witness_digest: Hash256,
    pub public_input_digest: Hash256,
    pub circuit_version: CircuitVersion,
    pub coverage_manifest_digest: Hash256,
    pub feature_set: ExecutionFeatureSetV1,
    pub retention_policy: WitnessRetentionPolicyV1,
}

fn tx_hash(tx: &Transaction) -> Result<Hash256, std::io::Error> {
    Ok(keccak256(&borsh::to_vec(tx)?))
}

pub fn coverage_manifest_for_circuit_version(version: CircuitVersion) -> CoverageManifestV1 {
    let covered_features = match version {
        CircuitVersion::DevMixedV1 | CircuitVersion::MixedStateTransitionV1 => {
            ExecutionFeatureSetV1::all_known()
        }
        CircuitVersion::NativeStateTransitionV1 => ExecutionFeatureSetV1 {
            bits: FEATURE_NATIVE_TX | FEATURE_NATIVE_SHARED_STATE,
        },
    };
    CoverageManifestV1 {
        version: 1,
        circuit_version: version,
        covered_features,
    }
}

pub fn coverage_manifest_digest(manifest: &CoverageManifestV1) -> Result<Hash256, std::io::Error> {
    Ok(keccak256(&borsh::to_vec(manifest)?))
}

pub fn tx_feature_set(tx: &Transaction) -> ExecutionFeatureSetV1 {
    let mut features = ExecutionFeatureSetV1::empty();
    match (&tx.vm, &tx.body) {
        (VmKind::Native, TxBody::Native(_)) => {
            features.insert(FEATURE_NATIVE_TX);
            if !tx.is_owned_object_tx() {
                features.insert(FEATURE_NATIVE_SHARED_STATE);
            }
        }
        (VmKind::Evm, TxBody::Transfer { .. }) => features.insert(FEATURE_EVM_TRANSFER),
        (VmKind::Evm, TxBody::EvmCreate { .. }) => features.insert(FEATURE_EVM_CREATE),
        (VmKind::Evm, TxBody::EvmCall { to, .. }) => {
            features.insert(FEATURE_EVM_CALL);
            if fractal_core::is_native_precompile_address(to) {
                features.insert(FEATURE_EVM_TO_NATIVE_PRECOMPILE);
                features.insert(FEATURE_NATIVE_TX);
            }
        }
        _ => {}
    }
    features
}

pub fn block_feature_set(txs: &[Transaction]) -> ExecutionFeatureSetV1 {
    let mut features = ExecutionFeatureSetV1::empty();
    for tx in txs {
        features.bits |= tx_feature_set(tx).bits;
    }
    features
}

pub fn evm_zkvm_surface_v1() -> EvmZkVmSurfaceV1 {
    EvmZkVmSurfaceV1 {
        version: 1,
        zkvm_choice: ZkVmChoiceV1::RiscZeroRevmTransitionV1,
        zkvm_target: "risc0-fractal-revm-transition-v1".to_owned(),
        revm_crate_version: "38.0.0".to_owned(),
        covered_features: ExecutionFeatureSetV1 {
            bits: FEATURE_EVM_TRANSFER
                | FEATURE_EVM_CALL
                | FEATURE_EVM_CREATE
                | FEATURE_EVM_TO_NATIVE_PRECOMPILE,
        },
        uncovered_features: ExecutionFeatureSetV1::empty(),
    }
}

pub fn evm_mixed_proving_budget_v1() -> EvmMixedProvingBudgetV1 {
    EvmMixedProvingBudgetV1 {
        version: 1,
        zkvm_choice: ZkVmChoiceV1::RiscZeroRevmTransitionV1,
        native_block_latency_target_ms: 120_000,
        mixed_block_latency_target_ms: 900_000,
        sustained_throughput_blocks_per_minute: 4,
        max_proof_final_lag_ms: 1_800_000,
        batching_mode: MixedProofBatchingModeV1::BatchedFallback,
    }
}

pub fn verify_transactions_eligible_for_circuit(
    txs: &[Transaction],
    circuit_version: CircuitVersion,
) -> Result<(), ProofEligibilityError> {
    for (idx, tx) in txs.iter().enumerate() {
        match (&tx.vm, &tx.body, circuit_version) {
            (VmKind::Evm, TxBody::EvmCall { to, .. }, CircuitVersion::NativeStateTransitionV1)
                if fractal_core::is_native_precompile_address(to) =>
            {
                return Err(ProofEligibilityError::PrecompileDispatchInNativeCircuit(
                    idx,
                ));
            }
            (VmKind::Evm, _, CircuitVersion::NativeStateTransitionV1) => {
                return Err(ProofEligibilityError::EvmTxInNativeCircuit(idx));
            }
            (VmKind::Evm, TxBody::EvmCall { value, .. }, _) if *value != 0 => {
                return Err(ProofEligibilityError::UnsupportedEvmValue(idx));
            }
            (VmKind::Evm, TxBody::EvmCreate { value, .. }, _) if *value != 0 => {
                return Err(ProofEligibilityError::UnsupportedEvmValue(idx));
            }
            (VmKind::Evm, TxBody::EvmCall { to, .. }, _)
                if to[0] == 0xfc && !fractal_core::is_native_precompile_address(to) =>
            {
                return Err(ProofEligibilityError::UnsupportedEvmCallTarget(idx));
            }
            _ => {}
        }
    }
    let features = block_feature_set(txs);
    let manifest = coverage_manifest_for_circuit_version(circuit_version);
    if !features.contains_only(manifest.covered_features) {
        return Err(ProofEligibilityError::CircuitCoverage);
    }
    Ok(())
}

pub fn verify_block_eligible_for_circuit(
    block: &Block,
    circuit_version: CircuitVersion,
) -> Result<(), ProofEligibilityError> {
    verify_transactions_eligible_for_circuit(&block.transactions, circuit_version)
}

fn evm_log_root(logs: &[fractal_core::EvmLog]) -> Result<Hash256, std::io::Error> {
    let hashes: Vec<Hash256> = logs
        .iter()
        .map(|log| Ok(keccak256(&borsh::to_vec(log)?)))
        .collect::<Result<_, std::io::Error>>()?;
    Ok(merkle_root_from_hashes(&hashes))
}

fn hash_pair(left: &Hash256, right: &Hash256) -> Hash256 {
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(left);
    buf[32..].copy_from_slice(right);
    keccak256(&buf)
}

fn merkle_root_from_hashes(hashes: &[Hash256]) -> Hash256 {
    if hashes.is_empty() {
        return [0u8; 32];
    }
    let mut level = hashes.to_vec();
    while level.len() > 1 {
        let mut next = Vec::with_capacity((level.len() + 1) / 2);
        let mut i = 0;
        while i < level.len() {
            if i + 1 < level.len() {
                next.push(hash_pair(&level[i], &level[i + 1]));
                i += 2;
            } else {
                next.push(hash_pair(&level[i], &level[i]));
                i += 1;
            }
        }
        level = next;
    }
    level[0]
}

fn merkle_path_from_hashes(hashes: &[Hash256], index: usize) -> Vec<Hash256> {
    if hashes.is_empty() || index >= hashes.len() {
        return Vec::new();
    }
    let mut path = Vec::new();
    let mut idx = index;
    let mut level = hashes.to_vec();
    while level.len() > 1 {
        let sibling = if idx % 2 == 0 {
            if idx + 1 < level.len() {
                level[idx + 1]
            } else {
                level[idx]
            }
        } else {
            level[idx - 1]
        };
        path.push(sibling);

        let mut next = Vec::with_capacity((level.len() + 1) / 2);
        let mut i = 0;
        while i < level.len() {
            if i + 1 < level.len() {
                next.push(hash_pair(&level[i], &level[i + 1]));
                i += 2;
            } else {
                next.push(hash_pair(&level[i], &level[i]));
                i += 1;
            }
        }
        idx /= 2;
        level = next;
    }
    path
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
struct StateCommitmentLeafV1 {
    namespace: StateCommitmentNamespace,
    key: Vec<u8>,
    value_hash: Hash256,
}

fn state_leaf_hash(leaf: &StateCommitmentLeafV1) -> Result<Hash256, std::io::Error> {
    Ok(keccak256(&borsh::to_vec(leaf)?))
}

fn state_commitment_root(leaves: &[StateCommitmentLeafV1]) -> Result<Hash256, std::io::Error> {
    let hashes: Vec<Hash256> = leaves
        .iter()
        .map(state_leaf_hash)
        .collect::<Result<_, _>>()?;
    Ok(merkle_root_from_hashes(&hashes))
}

fn state_value_hash<T: BorshSerialize>(value: &T) -> Result<Hash256, std::io::Error> {
    Ok(keccak256(&borsh::to_vec(value)?))
}

fn state_key<T: BorshSerialize>(tag: &str, key: &T) -> Result<Vec<u8>, std::io::Error> {
    let mut out = tag.as_bytes().to_vec();
    out.push(0);
    out.extend_from_slice(&borsh::to_vec(key)?);
    Ok(out)
}

fn push_leaf<T: BorshSerialize>(
    leaves: &mut Vec<StateCommitmentLeafV1>,
    namespace: StateCommitmentNamespace,
    tag: &str,
    key: &impl BorshSerialize,
    value: &T,
) -> Result<(), std::io::Error> {
    leaves.push(StateCommitmentLeafV1 {
        namespace,
        key: state_key(tag, key)?,
        value_hash: state_value_hash(value)?,
    });
    Ok(())
}

fn state_commitment_leaves(
    state: &State,
    namespace_filter: Option<StateCommitmentNamespace>,
) -> Result<Vec<StateCommitmentLeafV1>, std::io::Error> {
    let mut leaves = Vec::new();
    let want = |ns| match namespace_filter {
        Some(filter) => filter == ns,
        None => true,
    };

    if want(StateCommitmentNamespace::Accounts) {
        for (address, account) in &state.accounts {
            push_leaf(
                &mut leaves,
                StateCommitmentNamespace::Accounts,
                "account",
                address,
                account,
            )?;
        }
    }

    if want(StateCommitmentNamespace::Native) {
        push_leaf(
            &mut leaves,
            StateCommitmentNamespace::Native,
            "governance",
            &(),
            &state.governance,
        )?;
        push_leaf(
            &mut leaves,
            StateCommitmentNamespace::Native,
            "next_agent_id",
            &(),
            &state.next_agent_id,
        )?;
        for (id, record) in &state.agents {
            push_leaf(
                &mut leaves,
                StateCommitmentNamespace::Native,
                "agent",
                id,
                record,
            )?;
        }
        for (address, agent_id) in &state.address_to_agent {
            push_leaf(
                &mut leaves,
                StateCommitmentNamespace::Native,
                "address_to_agent",
                address,
                agent_id,
            )?;
        }
        for (receipt_id, receipt) in &state.receipts {
            push_leaf(
                &mut leaves,
                StateCommitmentNamespace::Native,
                "receipt",
                receipt_id,
                receipt,
            )?;
        }
        for (batch_id, batch) in &state.batches {
            push_leaf(
                &mut leaves,
                StateCommitmentNamespace::Native,
                "batch",
                batch_id,
                batch,
            )?;
        }
        for claim in &state.claimed_payouts {
            push_leaf(
                &mut leaves,
                StateCommitmentNamespace::Native,
                "claimed_payout",
                claim,
                &true,
            )?;
        }
        push_leaf(
            &mut leaves,
            StateCommitmentNamespace::Native,
            "next_dispute_id",
            &(),
            &state.next_dispute_id,
        )?;
        for (id, dispute) in &state.disputes {
            push_leaf(
                &mut leaves,
                StateCommitmentNamespace::Native,
                "dispute",
                id,
                dispute,
            )?;
        }
        for (address, stake) in &state.stakes {
            push_leaf(
                &mut leaves,
                StateCommitmentNamespace::Native,
                "stake",
                address,
                stake,
            )?;
        }
        for (delegation, amount) in &state.delegated {
            push_leaf(
                &mut leaves,
                StateCommitmentNamespace::Native,
                "delegated",
                delegation,
                amount,
            )?;
        }
        for (commitment, address) in &state.wallet_task_receipt_anchors {
            push_leaf(
                &mut leaves,
                StateCommitmentNamespace::Native,
                "wallet_task_receipt_anchor",
                commitment,
                address,
            )?;
        }
        for (object_id, version) in &state.owned_object_versions {
            push_leaf(
                &mut leaves,
                StateCommitmentNamespace::Native,
                "owned_object_version",
                object_id,
                version,
            )?;
        }
        push_leaf(
            &mut leaves,
            StateCommitmentNamespace::Native,
            "chain_economics",
            &(),
            &state.chain_economics,
        )?;
    }

    if want(StateCommitmentNamespace::Evm) {
        for (address, code) in &state.evm_code {
            push_leaf(
                &mut leaves,
                StateCommitmentNamespace::Evm,
                "evm_code",
                address,
                code,
            )?;
        }
        for (slot, value) in &state.evm_storage {
            push_leaf(
                &mut leaves,
                StateCommitmentNamespace::Evm,
                "evm_storage",
                slot,
                value,
            )?;
        }
    }

    leaves.sort_by(|a, b| {
        a.namespace
            .cmp(&b.namespace)
            .then_with(|| a.key.cmp(&b.key))
            .then_with(|| a.value_hash.cmp(&b.value_hash))
    });
    Ok(leaves)
}

pub fn state_commitment_v1(state: &State) -> Result<StateCommitmentV1, std::io::Error> {
    let native_leaves = state_commitment_leaves(state, Some(StateCommitmentNamespace::Native))?;
    let evm_leaves = state_commitment_leaves(state, Some(StateCommitmentNamespace::Evm))?;
    let all_leaves = state_commitment_leaves(state, None)?;
    Ok(StateCommitmentV1 {
        scheme: STATE_COMMITMENT_SCHEME_V1,
        native_root: state_commitment_root(&native_leaves)?,
        evm_root: state_commitment_root(&evm_leaves)?,
        unified_root: state_commitment_root(&all_leaves)?,
    })
}

pub fn state_access_witnesses_v1(
    pre_state: &State,
    post_state: &State,
) -> Result<Vec<StateMerkleAccessWitnessV1>, std::io::Error> {
    let pre_leaves = state_commitment_leaves(pre_state, None)?;
    let post_leaves = state_commitment_leaves(post_state, None)?;
    let pre_hashes = pre_leaves
        .iter()
        .map(state_leaf_hash)
        .collect::<Result<Vec<_>, _>>()?;
    let post_hashes = post_leaves
        .iter()
        .map(state_leaf_hash)
        .collect::<Result<Vec<_>, _>>()?;
    let mut rows =
        BTreeMap::<(StateCommitmentNamespace, Vec<u8>), StateMerkleAccessWitnessV1>::new();

    for (index, leaf) in pre_leaves.iter().enumerate() {
        rows.insert(
            (leaf.namespace, leaf.key.clone()),
            StateMerkleAccessWitnessV1 {
                kind: StateAccessKindV1::Read,
                namespace: leaf.namespace,
                key: leaf.key.clone(),
                pre_value_hash: Some(leaf.value_hash),
                post_value_hash: None,
                pre_state_path: merkle_path_from_hashes(&pre_hashes, index),
                post_state_path: Vec::new(),
            },
        );
    }

    for (index, leaf) in post_leaves.iter().enumerate() {
        let key = (leaf.namespace, leaf.key.clone());
        rows.entry(key)
            .and_modify(|row| {
                row.kind = if row.pre_value_hash == Some(leaf.value_hash) {
                    StateAccessKindV1::Read
                } else {
                    StateAccessKindV1::ReadWrite
                };
                row.post_value_hash = Some(leaf.value_hash);
                row.post_state_path = merkle_path_from_hashes(&post_hashes, index);
            })
            .or_insert_with(|| StateMerkleAccessWitnessV1 {
                kind: StateAccessKindV1::Write,
                namespace: leaf.namespace,
                key: leaf.key.clone(),
                pre_value_hash: None,
                post_value_hash: Some(leaf.value_hash),
                pre_state_path: Vec::new(),
                post_state_path: merkle_path_from_hashes(&post_hashes, index),
            });
    }

    Ok(rows.into_values().collect())
}

/// Ordered Merkle root over transaction hashes (matches canonical tx order in the block).
pub fn ordered_tx_root(txs: &[Transaction]) -> Result<Hash256, std::io::Error> {
    if txs.is_empty() {
        return Ok([0u8; 32]);
    }
    let hashes: Vec<Hash256> = txs.iter().map(tx_hash).collect::<Result<_, _>>()?;
    Ok(merkle_root_from_hashes(&hashes))
}

pub fn tx_receipt_root(receipts: &[MixedExecutionTxReceiptV1]) -> Result<Hash256, std::io::Error> {
    let hashes: Vec<Hash256> = receipts
        .iter()
        .map(|receipt| Ok(keccak256(&borsh::to_vec(receipt)?)))
        .collect::<Result<_, std::io::Error>>()?;
    Ok(merkle_root_from_hashes(&hashes))
}

pub fn block_evm_log_root(state: &State, txs: &[Transaction]) -> Result<Hash256, std::io::Error> {
    let mut hashes = Vec::new();
    for tx in txs {
        let h = tx_hash(tx)?;
        if let Some(logs) = state.evm_tx_logs.get(&h) {
            for log in logs {
                hashes.push(keccak256(&borsh::to_vec(log)?));
            }
        }
    }
    Ok(merkle_root_from_hashes(&hashes))
}

pub fn mixed_execution_tx_receipts(
    state: &State,
    txs: &[Transaction],
) -> Result<Vec<MixedExecutionTxReceiptV1>, std::io::Error> {
    txs.iter()
        .map(|tx| {
            let tx_hash = tx_hash(tx)?;
            let logs = state.evm_tx_logs.get(&tx_hash).cloned().unwrap_or_default();
            Ok(MixedExecutionTxReceiptV1 {
                tx_hash,
                success: state.evm_tx_success.get(&tx_hash).copied().unwrap_or(true),
                gas_used: fractal_core::gas_used_after_apply(state, tx)
                    .map_err(std::io::Error::other)?,
                evm_log_root: evm_log_root(&logs)?,
            })
        })
        .collect()
}

fn account_nonce_balance(state: &State, address: &[u8; 20]) -> (u64, u128) {
    state
        .accounts
        .get(address)
        .map(|account| (account.nonce, account.balance))
        .unwrap_or((0, 0))
}

fn state_access_indices_for_namespace(
    accesses: &[StateMerkleAccessWitnessV1],
    namespaces: &[StateCommitmentNamespace],
) -> Vec<u32> {
    accesses
        .iter()
        .enumerate()
        .filter(|(_, row)| namespaces.contains(&row.namespace))
        .filter_map(|(idx, _)| u32::try_from(idx).ok())
        .collect()
}

pub fn native_execution_trace_rows_v1(
    pre_state: &State,
    post_state: &State,
    txs: &[Transaction],
    state_accesses: &[StateMerkleAccessWitnessV1],
    native_event_root: Hash256,
) -> Result<Vec<NativeExecutionTraceRowV1>, std::io::Error> {
    let native_access_indices = state_access_indices_for_namespace(
        state_accesses,
        &[
            StateCommitmentNamespace::Accounts,
            StateCommitmentNamespace::Native,
        ],
    );
    let mut rows = Vec::new();
    for (idx, tx) in txs.iter().enumerate() {
        if let (VmKind::Native, TxBody::Native(call)) = (&tx.vm, &tx.body) {
            let (signer_pre_nonce, signer_pre_balance) =
                account_nonce_balance(pre_state, &tx.signer);
            let (signer_post_nonce, signer_post_balance) =
                account_nonce_balance(post_state, &tx.signer);
            rows.push(NativeExecutionTraceRowV1 {
                tx_index: idx as u32,
                tx_hash: tx_hash(tx)?,
                kind: NativeTraceKindV1::TopLevelNative,
                signer: tx.signer,
                nonce: tx.nonce,
                call: call.clone(),
                gas_used: fractal_core::gas_used_after_apply(post_state, tx)
                    .map_err(std::io::Error::other)?,
                native_event_root,
                native_event_start: 0,
                native_event_count: 0,
                signer_pre_nonce,
                signer_post_nonce,
                signer_pre_balance,
                signer_post_balance,
                state_access_indices: native_access_indices.clone(),
            });
        }
    }
    Ok(rows)
}

pub fn native_precompile_dispatch_trace_rows_v1(
    post_state: &State,
    txs: &[Transaction],
) -> Result<Vec<NativePrecompileDispatchTraceRowV1>, std::io::Error> {
    let mut rows = Vec::new();
    for (idx, tx) in txs.iter().enumerate() {
        let TxBody::EvmCall { to, calldata, .. } = &tx.body else {
            continue;
        };
        if !fractal_core::is_native_precompile_address(to) {
            continue;
        }
        let decoded_call = fractal_core::NativeCall::try_from_slice(calldata)
            .map_err(|_| std::io::Error::other("invalid native precompile calldata"))?;
        rows.push(NativePrecompileDispatchTraceRowV1 {
            tx_index: idx as u32,
            tx_hash: tx_hash(tx)?,
            caller: tx.signer,
            precompile_address: *to,
            native_opcode: fractal_core::native_opcode_from_precompile_address(to).unwrap_or(0),
            decoded_call,
            calldata_hash: keccak256(calldata),
            gas_used: fractal_core::gas_used_after_apply(post_state, tx)
                .map_err(std::io::Error::other)?,
        });
    }
    Ok(rows)
}

pub fn evm_execution_trace_rows_v1(
    pre_state: &State,
    post_state: &State,
    txs: &[Transaction],
) -> Result<Vec<EvmExecutionTraceRowV1>, std::io::Error> {
    let pre_evm_root = state_commitment_v1(pre_state)?.evm_root;
    let post_evm_root = state_commitment_v1(post_state)?.evm_root;
    let mut rows = Vec::new();
    for (idx, tx) in txs.iter().enumerate() {
        let (kind, to, value, input_hash, gas_limit) = match (&tx.vm, &tx.body) {
            (VmKind::Evm, TxBody::Transfer { to, amount }) => (
                EvmTraceKindV1::Transfer,
                Some(*to),
                *amount,
                keccak256(&[]),
                fractal_core::TRANSFER_GAS,
            ),
            (
                VmKind::Evm,
                TxBody::EvmCall {
                    to,
                    value,
                    calldata,
                    gas_limit,
                },
            ) => (
                EvmTraceKindV1::Call,
                Some(*to),
                *value,
                keccak256(calldata),
                *gas_limit,
            ),
            (
                VmKind::Evm,
                TxBody::EvmCreate {
                    value,
                    init_code,
                    gas_limit,
                },
            ) => (
                EvmTraceKindV1::Create,
                None,
                *value,
                keccak256(init_code),
                *gas_limit,
            ),
            _ => continue,
        };
        let tx_hash = tx_hash(tx)?;
        let logs = post_state
            .evm_tx_logs
            .get(&tx_hash)
            .cloned()
            .unwrap_or_default();
        rows.push(EvmExecutionTraceRowV1 {
            tx_index: idx as u32,
            tx_hash,
            kind,
            caller: tx.signer,
            to,
            value,
            input_hash,
            gas_limit,
            gas_used: fractal_core::gas_used_after_apply(post_state, tx)
                .map_err(std::io::Error::other)?,
            success: post_state
                .evm_tx_success
                .get(&tx_hash)
                .copied()
                .unwrap_or(true),
            log_root: evm_log_root(&logs)?,
            pre_evm_root,
            post_evm_root,
        });
    }
    Ok(rows)
}

pub fn witness_gas_sum(receipts: &[MixedExecutionTxReceiptV1]) -> Result<u64, MixedWitnessError> {
    receipts.iter().try_fold(0u64, |sum, receipt| {
        sum.checked_add(receipt.gas_used)
            .ok_or(MixedWitnessError::GasSumMismatch {
                header: u64::MAX,
                receipts: sum,
            })
    })
}

pub fn header_hash(header: &BlockHeader) -> Result<Hash256, std::io::Error> {
    Ok(keccak256(&borsh::to_vec(header)?))
}

#[derive(BorshSerialize)]
struct DaShareCommitment<'a> {
    namespace: DaNamespace,
    index: u32,
    is_parity: bool,
    data: &'a [u8],
}

pub fn da_share_commitment(
    namespace: DaNamespace,
    index: u32,
    is_parity: bool,
    data: &[u8],
) -> Hash256 {
    let body = DaShareCommitment {
        namespace,
        index,
        is_parity,
        data,
    };
    let bytes = borsh::to_vec(&body).unwrap_or_default();
    keccak256(&bytes)
}

pub fn da_root(sidecar: &DaSidecar) -> Hash256 {
    let commits: Vec<Hash256> = sidecar.shares.iter().map(|s| s.commitment).collect();
    merkle_root_from_hashes(&commits)
}

fn merkle_path_len_for_leaf_count(mut leaf_count: usize) -> usize {
    let mut len = 0;
    while leaf_count > 1 {
        leaf_count = leaf_count.div_ceil(2);
        len += 1;
    }
    len
}

fn verify_merkle_path(
    leaf: Hash256,
    index: u32,
    leaf_count: u32,
    path: &[Hash256],
    expected_root: Hash256,
) -> bool {
    if leaf_count == 0 || index >= leaf_count {
        return false;
    }
    if path.len() != merkle_path_len_for_leaf_count(leaf_count as usize) {
        return false;
    }
    let mut idx = index as usize;
    let mut acc = leaf;
    for sibling in path {
        acc = if idx % 2 == 0 {
            hash_pair(&acc, sibling)
        } else {
            hash_pair(sibling, &acc)
        };
        idx /= 2;
    }
    acc == expected_root
}

fn deterministic_da_sample_indexes(seed: u64, share_count: u32, sample_count: u32) -> Vec<u32> {
    if share_count == 0 || sample_count == 0 {
        return Vec::new();
    }
    let mut rng = SampleRng::new(seed);
    (0..sample_count)
        .map(|_| (rng.next() as u32) % share_count)
        .collect()
}

pub fn build_da_sampling_receipt(
    sidecar: &DaSidecar,
    expected_root: Hash256,
    seed: u64,
    sample_count: u32,
) -> Result<DaSamplingReceipt, DaVerifyError> {
    let actual_root = da_root(sidecar);
    if actual_root != expected_root {
        return Err(DaVerifyError::Root);
    }
    let share_count = u32::try_from(sidecar.shares.len()).map_err(|_| DaVerifyError::ShareCount)?;
    let commitments: Vec<Hash256> = sidecar
        .shares
        .iter()
        .map(|share| share.commitment)
        .collect();
    let indexes = deterministic_da_sample_indexes(seed, share_count, sample_count);
    let mut samples = Vec::with_capacity(indexes.len());
    for index in indexes {
        let share = sidecar
            .shares
            .get(index as usize)
            .ok_or(DaVerifyError::SampleMissing)?;
        if share.index != index {
            return Err(DaVerifyError::ShareIndex);
        }
        samples.push(DaSamplingReceiptSample {
            index,
            is_parity: share.is_parity,
            commitment: share.commitment,
            merkle_path: merkle_path_from_hashes(&commitments, index as usize),
        });
    }
    Ok(DaSamplingReceipt {
        namespace: sidecar.namespace,
        da_root: expected_root,
        share_count,
        seed,
        sample_count,
        samples,
    })
}

pub fn verify_da_sampling_receipt(
    receipt: &DaSamplingReceipt,
    expected_root: Hash256,
    expected_namespace: DaNamespace,
    min_samples: u32,
) -> Result<(), DaVerifyError> {
    if receipt.namespace != expected_namespace {
        return Err(DaVerifyError::Namespace);
    }
    if receipt.da_root != expected_root {
        return Err(DaVerifyError::Root);
    }
    if receipt.sample_count < min_samples || receipt.samples.len() < min_samples as usize {
        return Err(DaVerifyError::InsufficientSamples);
    }
    if receipt.samples.len() != receipt.sample_count as usize {
        return Err(DaVerifyError::SampleMissing);
    }
    if receipt.share_count == 0 {
        if receipt.sample_count == 0 {
            return Ok(());
        }
        return Err(DaVerifyError::ShareCount);
    }
    let expected_indexes =
        deterministic_da_sample_indexes(receipt.seed, receipt.share_count, receipt.sample_count);
    for (sample, expected_index) in receipt.samples.iter().zip(expected_indexes) {
        if sample.index != expected_index || sample.index >= receipt.share_count {
            return Err(DaVerifyError::ShareIndex);
        }
        if !verify_merkle_path(
            sample.commitment,
            sample.index,
            receipt.share_count,
            &sample.merkle_path,
            receipt.da_root,
        ) {
            return Err(DaVerifyError::ShareCommitment);
        }
    }
    Ok(())
}

pub fn da_encoded_bytes(sidecar: &DaSidecar) -> u64 {
    sidecar.shares.iter().map(|s| s.data.len() as u64).sum()
}

pub fn da_gas_for_sidecar(sidecar: &DaSidecar) -> u64 {
    da_encoded_bytes(sidecar).saturating_mul(DEFAULT_DA_GAS_PER_BYTE)
}

pub fn da_fee_for_gas(da_gas_used: u64) -> u128 {
    default_fee_policy().da_gas_fee(da_gas_used)
}

fn default_parity_share_count(data_share_count: u32) -> u32 {
    data_share_count
        .saturating_mul(DEFAULT_DA_PARITY_RATIO_NUMERATOR)
        .div_ceil(DEFAULT_DA_PARITY_RATIO_DENOMINATOR)
        .max(1)
}

fn required_data_share_count(original_len: u64, share_size: u32) -> Result<u32, DaVerifyError> {
    let share_size = share_size.max(1);
    if original_len == 0 {
        return Ok(0);
    }
    let count = original_len.div_ceil(u64::from(share_size));
    u32::try_from(count).map_err(|_| DaVerifyError::ShareCount)
}

pub fn build_da_sidecar(
    payload: &[u8],
    namespace: DaNamespace,
    share_size: u32,
) -> Result<DaSidecar, DaVerifyError> {
    let share_size = share_size.max(1) as usize;
    if payload.is_empty() {
        return Ok(DaSidecar {
            namespace,
            original_len: 0,
            share_size: share_size as u32,
            data_share_count: 0,
            parity_share_count: 0,
            shares: Vec::new(),
        });
    }
    let data_share_count = payload.len().div_ceil(share_size);
    let parity_share_count = default_parity_share_count(data_share_count as u32) as usize;
    let codec = ReedSolomon::new(data_share_count, parity_share_count)
        .map_err(|_| DaVerifyError::ErasureCoding)?;
    let mut shards = Vec::with_capacity(data_share_count + parity_share_count);
    for i in 0..data_share_count {
        let start = i * share_size;
        let end = (start + share_size).min(payload.len());
        let mut shard = vec![0u8; share_size];
        shard[..end - start].copy_from_slice(&payload[start..end]);
        shards.push(shard);
    }
    for _ in 0..parity_share_count {
        shards.push(vec![0u8; share_size]);
    }
    codec
        .encode(&mut shards)
        .map_err(|_| DaVerifyError::ErasureCoding)?;
    let mut shares = Vec::new();
    for (i, data) in shards.into_iter().enumerate() {
        let index = i as u32;
        let is_parity = i >= data_share_count;
        let commitment = da_share_commitment(namespace, index, is_parity, &data);
        shares.push(DaShare {
            namespace,
            index,
            is_parity,
            data,
            commitment,
        });
    }
    Ok(DaSidecar {
        namespace,
        original_len: payload.len() as u64,
        share_size: share_size as u32,
        data_share_count: data_share_count as u32,
        parity_share_count: parity_share_count as u32,
        shares,
    })
}

pub fn build_zone_blob_da_sidecar(
    blob: &ZoneBlobDaV1,
) -> Result<(DaSidecar, ZoneBlobDaCommitmentV1), DaVerifyError> {
    let sidecar = build_da_sidecar(&blob.payload, blob.namespace, blob.share_size)?;
    let commitment = ZoneBlobDaCommitmentV1 {
        namespace: blob.namespace,
        da_root: da_root(&sidecar),
        byte_count: sidecar.original_len,
        share_count: sidecar.shares.len() as u32,
        share_size: sidecar.share_size,
        sampling: blob.sampling.clone(),
    };
    Ok((sidecar, commitment))
}

pub fn zone_blob_da_commitment_hash(
    commitment: &ZoneBlobDaCommitmentV1,
) -> Result<Hash256, std::io::Error> {
    let mut bytes = b"fractal:zone-blob-da:v1".to_vec();
    bytes.extend_from_slice(&borsh::to_vec(commitment)?);
    Ok(keccak256(&bytes))
}

#[derive(BorshSerialize)]
struct HeaderExtraCommitmentV1 {
    payload_root: Hash256,
    zone_blob_da_commitment: Hash256,
}

pub fn proof_ingestion_header_extra(
    payload_root: Hash256,
    zone_blob_da_commitment: &ZoneBlobDaCommitmentV1,
) -> Result<Hash256, std::io::Error> {
    let body = HeaderExtraCommitmentV1 {
        payload_root,
        zone_blob_da_commitment: zone_blob_da_commitment_hash(zone_blob_da_commitment)?,
    };
    let mut bytes = b"fractal:block-extra:proof-ingestion:v1".to_vec();
    bytes.extend_from_slice(&borsh::to_vec(&body)?);
    Ok(keccak256(&bytes))
}

pub fn verify_zone_blob_da_header(
    header: &BlockHeader,
    sidecar: &DaSidecar,
    commitment: &ZoneBlobDaCommitmentV1,
    payload_root: Hash256,
) -> Result<(), DaVerifyError> {
    if commitment.namespace != header.zone_namespace {
        return Err(DaVerifyError::Namespace);
    }
    if commitment.da_root != header.da_root {
        return Err(DaVerifyError::Root);
    }
    if commitment.byte_count != header.da_bytes {
        return Err(DaVerifyError::OriginalLen);
    }
    if commitment.share_count != header.da_share_count {
        return Err(DaVerifyError::ShareCount);
    }
    if commitment.share_size != sidecar.share_size {
        return Err(DaVerifyError::OriginalLen);
    }
    verify_da_sidecar(header, sidecar)?;
    let expected_extra = proof_ingestion_header_extra(payload_root, commitment)
        .map_err(|_| DaVerifyError::ShareCommitment)?;
    if header.extra != expected_extra {
        return Err(DaVerifyError::Root);
    }
    if commitment.sampling.sample_count < commitment.sampling.min_samples {
        return Err(DaVerifyError::InsufficientSamples);
    }
    Ok(())
}

pub fn reconstruct_da_payload(sidecar: &DaSidecar) -> Result<Vec<u8>, DaVerifyError> {
    reconstruct_da_payload_from_shares(sidecar, sidecar.shares.iter().cloned())
}

pub fn reconstruct_da_payload_from_shares<I>(
    sidecar: &DaSidecar,
    shares: I,
) -> Result<Vec<u8>, DaVerifyError>
where
    I: IntoIterator<Item = DaShare>,
{
    verify_da_layout(sidecar)?;
    if sidecar.original_len == 0 {
        return Ok(Vec::new());
    }
    let data_count = sidecar.data_share_count as usize;
    let parity_count = sidecar.parity_share_count as usize;
    let total_count = data_count + parity_count;
    let share_size = sidecar.share_size as usize;
    let mut shards = vec![None; total_count];
    for share in shares {
        if share.namespace != sidecar.namespace {
            return Err(DaVerifyError::Namespace);
        }
        if share.index as usize >= total_count {
            return Err(DaVerifyError::ShareIndex);
        }
        if share.is_parity != (share.index as usize >= data_count) {
            return Err(DaVerifyError::ShareIndex);
        }
        if share.data.len() != share_size {
            return Err(DaVerifyError::OriginalLen);
        }
        let expected =
            da_share_commitment(share.namespace, share.index, share.is_parity, &share.data);
        if share.commitment != expected {
            return Err(DaVerifyError::ShareCommitment);
        }
        shards[share.index as usize] = Some(share.data);
    }
    if shards.iter().filter(|s| s.is_some()).count() < data_count {
        return Err(DaVerifyError::InsufficientShares);
    }
    let codec =
        ReedSolomon::new(data_count, parity_count).map_err(|_| DaVerifyError::ErasureCoding)?;
    codec
        .reconstruct(&mut shards)
        .map_err(|_| DaVerifyError::ErasureCoding)?;
    let mut out = Vec::new();
    for shard in shards.into_iter().take(data_count) {
        let shard = shard.ok_or(DaVerifyError::ErasureCoding)?;
        out.extend_from_slice(&shard);
    }
    out.truncate(sidecar.original_len as usize);
    Ok(out)
}

fn verify_da_layout(sidecar: &DaSidecar) -> Result<(), DaVerifyError> {
    let share_size = sidecar.share_size.max(1);
    if sidecar.share_size == 0 {
        return Err(DaVerifyError::OriginalLen);
    }
    let required_data_shares = required_data_share_count(sidecar.original_len, share_size)?;
    if sidecar.data_share_count != required_data_shares {
        return Err(DaVerifyError::ShareCount);
    }
    if sidecar.original_len == 0 {
        if sidecar.data_share_count != 0
            || sidecar.parity_share_count != 0
            || !sidecar.shares.is_empty()
        {
            return Err(DaVerifyError::ShareCount);
        }
        return Ok(());
    }
    if sidecar.parity_share_count == 0 {
        return Err(DaVerifyError::ShareCount);
    }
    let total = sidecar
        .data_share_count
        .checked_add(sidecar.parity_share_count)
        .ok_or(DaVerifyError::ShareCount)?;
    if sidecar.shares.len() != total as usize {
        return Err(DaVerifyError::ShareCount);
    }
    Ok(())
}

pub fn verify_da_sidecar(header: &BlockHeader, sidecar: &DaSidecar) -> Result<(), DaVerifyError> {
    if sidecar.namespace != header.zone_namespace {
        return Err(DaVerifyError::Namespace);
    }
    if sidecar.original_len != header.da_bytes {
        return Err(DaVerifyError::OriginalLen);
    }
    if sidecar.shares.len() != header.da_share_count as usize {
        return Err(DaVerifyError::ShareCount);
    }
    if header.da_gas_used != da_gas_for_sidecar(sidecar) {
        return Err(DaVerifyError::OriginalLen);
    }
    if header.da_fee_paid != da_fee_for_gas(header.da_gas_used) {
        return Err(DaVerifyError::OriginalLen);
    }
    verify_da_layout(sidecar)?;
    for (i, share) in sidecar.shares.iter().enumerate() {
        if share.index != i as u32 {
            return Err(DaVerifyError::ShareIndex);
        }
        if share.is_parity != (i >= sidecar.data_share_count as usize) {
            return Err(DaVerifyError::ShareIndex);
        }
        if share.namespace != header.zone_namespace || share.namespace != sidecar.namespace {
            return Err(DaVerifyError::Namespace);
        }
        if share.data.len() != sidecar.share_size as usize {
            return Err(DaVerifyError::OriginalLen);
        }
        let expected =
            da_share_commitment(share.namespace, share.index, share.is_parity, &share.data);
        if share.commitment != expected {
            return Err(DaVerifyError::ShareCommitment);
        }
    }
    if da_root(sidecar) != header.da_root {
        return Err(DaVerifyError::Root);
    }
    Ok(())
}

pub fn verify_da_samples(
    sidecar: &DaSidecar,
    expected_root: Hash256,
    expected_namespace: DaNamespace,
    seed: u64,
    sample_count: usize,
) -> Result<(), DaVerifyError> {
    if sidecar.namespace != expected_namespace {
        return Err(DaVerifyError::Namespace);
    }
    if da_root(sidecar) != expected_root {
        return Err(DaVerifyError::Root);
    }
    if sidecar.shares.is_empty() {
        return Ok(());
    }
    let mut rng = SampleRng::new(seed);
    for _ in 0..sample_count {
        let idx = (rng.next() as usize) % sidecar.shares.len();
        let share = sidecar
            .shares
            .get(idx)
            .ok_or(DaVerifyError::SampleMissing)?;
        if share.namespace != expected_namespace {
            return Err(DaVerifyError::Namespace);
        }
        let expected =
            da_share_commitment(share.namespace, share.index, share.is_parity, &share.data);
        if share.commitment != expected {
            return Err(DaVerifyError::ShareCommitment);
        }
    }
    Ok(())
}

pub fn validity_proof_public_input_digest(
    proof: &BlockValidityProof,
) -> Result<Hash256, std::io::Error> {
    let inputs = MixedExecutionPublicInputsV1 {
        version: MIXED_EXECUTION_WITNESS_V1,
        chain_id: proof.chain_id,
        height: proof.height,
        block_hash: proof.block_hash,
        timestamp_ms: proof.timestamp_ms,
        parent_state_root: proof.parent_state_root,
        post_state_root: proof.state_root,
        tx_root: proof.tx_root,
        receipt_root: proof.receipt_root,
        native_event_root: proof.native_event_root,
        evm_log_root: proof.evm_log_root,
        gas_used: proof.gas_used,
        zone_namespace: proof.zone_namespace,
        da_root: proof.da_root,
        circuit_version: proof.circuit_version,
        coverage_manifest_digest: proof.coverage_manifest_digest,
        feature_set: proof.feature_set,
    };
    Ok(keccak256(&borsh::to_vec(&inputs)?))
}

pub fn mixed_execution_public_inputs_from_block(
    block: &Block,
) -> Result<MixedExecutionPublicInputsV1, std::io::Error> {
    Ok(MixedExecutionPublicInputsV1 {
        version: MIXED_EXECUTION_WITNESS_V1,
        chain_id: block.header.chain_id,
        height: block.header.height,
        block_hash: header_hash(&block.header)?,
        timestamp_ms: block.header.timestamp_ms,
        parent_state_root: block.header.parent_state_root,
        post_state_root: block.header.state_root,
        tx_root: block.header.tx_root,
        receipt_root: block.header.receipt_root,
        native_event_root: block.header.native_event_root,
        evm_log_root: block.header.evm_log_root,
        gas_used: block.header.gas_used,
        zone_namespace: block.header.zone_namespace,
        da_root: block.header.da_root,
        circuit_version: CircuitVersion::DevMixedV1,
        coverage_manifest_digest: coverage_manifest_digest(
            &coverage_manifest_for_circuit_version(CircuitVersion::DevMixedV1),
        )?,
        feature_set: block.header.feature_set,
    })
}

pub fn mixed_execution_witness_from_state_transition(
    block: &Block,
    pre_state: &State,
    post_state: &State,
) -> Result<MixedExecutionWitnessV1, std::io::Error> {
    let state_accesses = state_access_witnesses_v1(pre_state, post_state)?;
    let tx_receipts = mixed_execution_tx_receipts(post_state, &block.transactions)?;
    let gas_sum = tx_receipts.iter().try_fold(0u64, |sum, receipt| {
        sum.checked_add(receipt.gas_used)
            .ok_or_else(|| std::io::Error::other("witness gas sum overflow"))
    })?;
    if gas_sum != block.header.gas_used {
        return Err(std::io::Error::other("witness gas sum mismatch"));
    }
    Ok(MixedExecutionWitnessV1 {
        version: MIXED_EXECUTION_WITNESS_V1,
        public_inputs: mixed_execution_public_inputs_from_block(block)?,
        pre_state_commitment: state_commitment_v1(pre_state)?,
        post_state_commitment: state_commitment_v1(post_state)?,
        state_accesses: state_accesses.clone(),
        native_trace_rows: native_execution_trace_rows_v1(
            pre_state,
            post_state,
            &block.transactions,
            &state_accesses,
            block.header.native_event_root,
        )?,
        precompile_dispatch_rows: native_precompile_dispatch_trace_rows_v1(
            post_state,
            &block.transactions,
        )?,
        evm_trace_rows: evm_execution_trace_rows_v1(pre_state, post_state, &block.transactions)?,
        evm_zkvm_surface: evm_zkvm_surface_v1(),
        gas_sum,
        transactions: block.transactions.clone(),
        eth_signed_raw: block.eth_signed_raw.clone(),
        tx_receipts,
    })
}

pub fn mixed_execution_witness_digest(
    witness: &MixedExecutionWitnessV1,
) -> Result<Hash256, std::io::Error> {
    Ok(keccak256(&borsh::to_vec(witness)?))
}

pub fn mixed_execution_witness_metadata(
    witness: &MixedExecutionWitnessV1,
    retention_policy: WitnessRetentionPolicyV1,
) -> Result<MixedExecutionWitnessMetadataV1, std::io::Error> {
    Ok(MixedExecutionWitnessMetadataV1 {
        version: MIXED_EXECUTION_WITNESS_V1,
        block_hash: witness.public_inputs.block_hash,
        height: witness.public_inputs.height,
        witness_digest: mixed_execution_witness_digest(witness)?,
        public_input_digest: keccak256(&borsh::to_vec(&witness.public_inputs)?),
        circuit_version: witness.public_inputs.circuit_version,
        coverage_manifest_digest: witness.public_inputs.coverage_manifest_digest,
        feature_set: witness.public_inputs.feature_set,
        retention_policy,
    })
}

pub fn mixed_execution_witness_from_replay(
    block: &Block,
    pre_state: &State,
) -> Result<MixedExecutionWitnessV1, MixedWitnessError> {
    let mut replayed = pre_state.clone();
    let mut evm = fractal_evm::RevmEngine::default();
    let gas_used =
        fractal_core::apply_block_with_evm(&mut replayed, &block.transactions, &mut evm)?;
    if gas_used != block.header.gas_used {
        return Err(MixedWitnessError::GasUsedMismatch {
            header: block.header.gas_used,
            replay: gas_used,
        });
    }
    if state_root(&replayed)? != block.header.state_root {
        return Err(MixedWitnessError::StateRoot);
    }
    if ordered_tx_root(&block.transactions)? != block.header.tx_root {
        return Err(MixedWitnessError::TxRoot);
    }
    let receipts = mixed_execution_tx_receipts(&replayed, &block.transactions)?;
    let receipt_gas_sum = witness_gas_sum(&receipts)?;
    if receipt_gas_sum != block.header.gas_used {
        return Err(MixedWitnessError::GasSumMismatch {
            header: block.header.gas_used,
            receipts: receipt_gas_sum,
        });
    }
    if tx_receipt_root(&receipts)? != block.header.receipt_root {
        return Err(MixedWitnessError::ReceiptRoot);
    }
    if block_evm_log_root(&replayed, &block.transactions)? != block.header.evm_log_root {
        return Err(MixedWitnessError::EvmLogRoot);
    }
    if block_feature_set(&block.transactions) != block.header.feature_set {
        return Err(MixedWitnessError::FeatureSet);
    }
    mixed_execution_witness_from_state_transition(block, pre_state, &replayed)
        .map_err(MixedWitnessError::Io)
}

pub fn mixed_execution_witness_from_executed_block(
    block: &Block,
    executed_state: &State,
) -> Result<MixedExecutionWitnessV1, std::io::Error> {
    mixed_execution_witness_from_state_transition(block, executed_state, executed_state)
}

pub fn verify_block_validity_proof(
    block: &Block,
    proof: &BlockValidityProof,
) -> Result<(), ProofVerifyError> {
    if proof.chain_id != block.header.chain_id {
        return Err(ProofVerifyError::ChainId);
    }
    if proof.height != block.header.height {
        return Err(ProofVerifyError::Height);
    }
    if proof.block_hash != header_hash(&block.header)? {
        return Err(ProofVerifyError::BlockHash);
    }
    if proof.timestamp_ms != block.header.timestamp_ms {
        return Err(ProofVerifyError::Timestamp);
    }
    if proof.parent_state_root != block.header.parent_state_root {
        return Err(ProofVerifyError::ParentStateRoot);
    }
    if proof.state_root != block.header.state_root {
        return Err(ProofVerifyError::StateRoot);
    }
    if proof.tx_root != block.header.tx_root {
        return Err(ProofVerifyError::TxRoot);
    }
    if proof.receipt_root != block.header.receipt_root {
        return Err(ProofVerifyError::ReceiptRoot);
    }
    if proof.native_event_root != block.header.native_event_root {
        return Err(ProofVerifyError::NativeEventRoot);
    }
    if proof.evm_log_root != block.header.evm_log_root {
        return Err(ProofVerifyError::EvmLogRoot);
    }
    if proof.zone_namespace != block.header.zone_namespace {
        return Err(ProofVerifyError::ZoneNamespace);
    }
    if proof.da_root != block.header.da_root {
        return Err(ProofVerifyError::DaRoot);
    }
    if proof.feature_set != block.header.feature_set {
        return Err(ProofVerifyError::FeatureSet);
    }
    let manifest = coverage_manifest_for_circuit_version(proof.circuit_version);
    if proof.coverage_manifest_digest != coverage_manifest_digest(&manifest)? {
        return Err(ProofVerifyError::CoverageManifest);
    }
    if !proof.feature_set.contains_only(manifest.covered_features) {
        return Err(ProofVerifyError::CircuitCoverage);
    }
    verify_da_sidecar(&block.header, &block.da_sidecar)
        .map_err(|_| ProofVerifyError::DataAvailability)?;
    if proof.proof_bytes.is_empty() {
        return Err(ProofVerifyError::EmptyProof);
    }
    match proof.proof_system {
        #[cfg(feature = "dev-digest")]
        ValidityProofSystem::DevDigest => {
            if !dev_digest_allowed_for_current_runtime() {
                return Err(ProofVerifyError::DevDigestDisabled);
            }
            let expected = validity_proof_public_input_digest(proof)?;
            if proof.proof_bytes.as_slice() != expected {
                return Err(ProofVerifyError::BadDevDigest);
            }
            Ok(())
        }
        ValidityProofSystem::StwoPlonky2 => {
            if let Ok(envelope) = StwoPlonky2ProofEnvelope::try_from_slice(&proof.proof_bytes) {
                if matches!(
                    envelope,
                    StwoPlonky2ProofEnvelope::EvmZkVmFixtureV1 { .. }
                        | StwoPlonky2ProofEnvelope::MixedIntraBlockAggregateFixtureV1 { .. }
                ) && proof.circuit_version != CircuitVersion::MixedStateTransitionV1
                {
                    return Err(ProofVerifyError::Production(
                        ProductionProofVerifyError::MixedCircuitVersion,
                    ));
                }
            }
            let public_input_digest = validity_proof_public_input_digest(proof)?;
            verify_stwo_plonky2_proof(&proof.proof_bytes, public_input_digest)?;
            Ok(())
        }
    }
}

/// One `None` per transaction when no Ethereum signed envelope is present.
pub fn eth_signed_raws_for_txs(txs_len: usize) -> Vec<Option<Vec<u8>>> {
    vec![None; txs_len]
}

/// Execute `txs` on top of `state`, compute roots, and assemble a `Block`.
/// Caller supplies `parent_qc_hash` (see [`crate::qc`]).
pub fn execute_and_build_block(
    chain_id: u64,
    height: u64,
    view: u64,
    parent_hash: Hash256,
    parent_qc_hash: Hash256,
    proposer: [u8; 32],
    timestamp_ms: u64,
    gas_limit: u64,
    state: &mut State,
    txs: Vec<Transaction>,
    eth_signed_raw: Vec<Option<Vec<u8>>>,
) -> Result<Block, BuildBlockError> {
    execute_and_build_zone_block(
        chain_id,
        height,
        view,
        parent_hash,
        parent_qc_hash,
        proposer,
        timestamp_ms,
        gas_limit,
        state,
        txs,
        eth_signed_raw,
        MASTERCHAIN_ZONE_NAMESPACE,
    )
}

/// Execute `txs` for one execution zone, commit its canonical payload into that zone's DA namespace,
/// and assemble a `Block`.
pub fn execute_and_build_zone_block(
    chain_id: u64,
    height: u64,
    view: u64,
    parent_hash: Hash256,
    parent_qc_hash: Hash256,
    proposer: [u8; 32],
    timestamp_ms: u64,
    gas_limit: u64,
    state: &mut State,
    txs: Vec<Transaction>,
    eth_signed_raw: Vec<Option<Vec<u8>>>,
    zone_namespace: ExecutionZoneNamespace,
) -> Result<Block, BuildBlockError> {
    if eth_signed_raw.len() != txs.len() {
        return Err(BuildBlockError::EthRawLenMismatch {
            txs: txs.len(),
            got: eth_signed_raw.len(),
        });
    }
    let mut budget_sum = 0u64;
    for tx in &txs {
        let g = fractal_core::tx_gas_limit(tx)?;
        budget_sum = budget_sum.checked_add(g).ok_or(ExecError::GasOverflow)?;
    }
    if budget_sum > gas_limit {
        return Err(ExecError::GasLimitExceeded.into());
    }
    let parent_state_root = state_root(state)?;
    let mut evm = fractal_evm::RevmEngine::default();
    let gas_used = fractal_core::apply_block_with_evm(state, &txs, &mut evm)?;
    debug_assert!(gas_used <= budget_sum);
    let sr = state_root(state)?;
    let tx_root = ordered_tx_root(&txs)?;
    let feature_set = block_feature_set(&txs);
    let tx_receipts = mixed_execution_tx_receipts(state, &txs)?;
    let receipt_root = tx_receipt_root(&tx_receipts)?;
    let native_event_root = [0u8; 32];
    let evm_log_root = block_evm_log_root(state, &txs)?;
    let da_payload = borsh::to_vec(&txs)?;
    let da_sidecar = build_da_sidecar(&da_payload, zone_namespace, DEFAULT_DA_SHARE_SIZE)?;
    let da_root = da_root(&da_sidecar);
    let da_gas_used = da_gas_for_sidecar(&da_sidecar);
    let da_fee_paid = da_fee_for_gas(da_gas_used);
    let header = BlockHeader {
        version: 1,
        chain_id,
        height,
        view,
        parent_hash,
        parent_qc_hash,
        proposer,
        timestamp_ms,
        parent_state_root,
        state_root: sr,
        tx_root,
        receipt_root,
        native_event_root,
        evm_log_root,
        zone_namespace,
        da_root,
        da_bytes: da_sidecar.original_len,
        da_share_count: da_sidecar.shares.len() as u32,
        da_gas_used,
        da_fee_paid,
        gas_used,
        gas_limit,
        feature_set,
        extra: [0u8; 32],
    };
    Ok(Block {
        header,
        transactions: txs,
        eth_signed_raw,
        da_sidecar,
    })
}

struct SampleRng {
    state: u64,
}

impl SampleRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fractal_core::{
        Account, NativeCall, OnChainTaskReceipt, PayoutEntry, SettleBatchPayload, State,
        Transaction, TxBody, VmKind,
    };

    fn addr(byte0: u8, byte1: u8) -> [u8; 20] {
        let mut out = [0u8; 20];
        out[0] = byte0;
        out[1] = byte1;
        out
    }

    #[test]
    fn tx_root_deterministic() {
        let tx = Transaction {
            signer: [1u8; 20],
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let a = ordered_tx_root(std::slice::from_ref(&tx)).unwrap();
        let b = ordered_tx_root(std::slice::from_ref(&tx)).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn legacy_block_encoding_is_unchanged_by_payload_view() {
        let block = Block {
            header: BlockHeader {
                version: 1,
                chain_id: 41,
                height: 1,
                view: 2,
                parent_hash: [1u8; 32],
                parent_qc_hash: [2u8; 32],
                proposer: [3u8; 32],
                timestamp_ms: 123,
                parent_state_root: [4u8; 32],
                state_root: [5u8; 32],
                tx_root: [6u8; 32],
                receipt_root: [7u8; 32],
                native_event_root: [8u8; 32],
                evm_log_root: [9u8; 32],
                zone_namespace: DEFAULT_DA_NAMESPACE,
                da_root: [10u8; 32],
                da_bytes: 11,
                da_share_count: 12,
                da_gas_used: 13,
                da_fee_paid: 14,
                gas_used: 15,
                gas_limit: 16,
                feature_set: ExecutionFeatureSetV1::empty(),
                extra: [0u8; 32],
            },
            transactions: vec![Transaction {
                signer: [1u8; 20],
                nonce: 0,
                vm: VmKind::Native,
                body: TxBody::Native(NativeCall::NoOp),
            }],
            eth_signed_raw: vec![None],
            da_sidecar: DaSidecar {
                namespace: DEFAULT_DA_NAMESPACE,
                original_len: 0,
                share_size: DEFAULT_DA_SHARE_SIZE,
                data_share_count: 0,
                parity_share_count: 0,
                shares: Vec::new(),
            },
        };
        let before = borsh::to_vec(&block).unwrap();
        assert!(matches!(
            block.payload(),
            BlockPayload::FullTransactions { .. }
        ));
        assert_eq!(block.payload_kind(), BlockPayloadKind::FullTransactions);
        let after = borsh::to_vec(&block).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn build_block_updates_state_root() {
        let mut st = State::default();
        let addr = [9u8; 20];
        st.accounts.insert(
            addr,
            Account {
                nonce: 0,
                balance: 1_000_000,
            },
        );
        let tx = Transaction {
            signer: addr,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let parent = [7u8; 32];
        let block = execute_and_build_block(
            41,
            1,
            0,
            parent,
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            vec![tx],
            eth_signed_raws_for_txs(1),
        )
        .unwrap();
        assert_eq!(block.header.height, 1);
        assert_ne!(block.header.state_root, [0u8; 32]);
    }

    #[cfg(not(feature = "dev-digest"))]
    #[test]
    fn production_build_rejects_dev_digest_discriminant() {
        assert!(ValidityProofSystem::try_from_slice(&[0]).is_err());
        assert_eq!(
            ValidityProofSystem::try_from_slice(&[1]).unwrap(),
            ValidityProofSystem::StwoPlonky2
        );
        assert_eq!(
            borsh::to_vec(&ValidityProofSystem::StwoPlonky2).unwrap(),
            vec![1]
        );
    }

    #[cfg(feature = "dev-digest")]
    #[test]
    fn dev_digest_feature_preserves_explicit_discriminants() {
        assert_eq!(
            borsh::to_vec(&ValidityProofSystem::DevDigest).unwrap(),
            vec![0]
        );
        assert_eq!(
            borsh::to_vec(&ValidityProofSystem::StwoPlonky2).unwrap(),
            vec![1]
        );
    }

    #[cfg(feature = "dev-digest")]
    #[test]
    fn dev_digest_runtime_gate_rejects_production_like_configs() {
        assert!(dev_digest_allowed_for_runtime(Some("local"), Some("dev")));
        assert!(!dev_digest_allowed_for_runtime(
            Some("mainnet"),
            Some("dev")
        ));
        assert!(!dev_digest_allowed_for_runtime(
            Some("testnet"),
            Some("prod")
        ));
        assert!(!dev_digest_allowed_for_runtime(None, Some("production")));
    }

    #[test]
    #[cfg(feature = "dev-digest")]
    fn dev_digest_proof_verifies_against_block_public_inputs() {
        let mut st = State::default();
        let addr = [9u8; 20];
        st.accounts.insert(
            addr,
            Account {
                nonce: 0,
                balance: 1_000_000,
            },
        );
        let tx = Transaction {
            signer: addr,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            vec![tx],
            eth_signed_raws_for_txs(1),
        )
        .unwrap();
        let mut proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            timestamp_ms: block.header.timestamp_ms,
            parent_state_root: block.header.parent_state_root,
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            receipt_root: block.header.receipt_root,
            native_event_root: block.header.native_event_root,
            evm_log_root: block.header.evm_log_root,
            gas_used: block.header.gas_used,
            zone_namespace: block.header.zone_namespace,
            da_root: block.header.da_root,
            circuit_version: CircuitVersion::DevMixedV1,
            coverage_manifest_digest: coverage_manifest_digest(
                &coverage_manifest_for_circuit_version(CircuitVersion::DevMixedV1),
            )
            .unwrap(),
            feature_set: block.header.feature_set,
            proof_system: ValidityProofSystem::DevDigest,
            proof_bytes: Vec::new(),
        };
        proof.proof_bytes = validity_proof_public_input_digest(&proof).unwrap().to_vec();

        verify_block_validity_proof(&block, &proof).expect("proof verifies");
    }

    #[test]
    fn mixed_execution_witness_binds_header_public_inputs_and_receipts() {
        let mut st = State::default();
        let native_signer = [9u8; 20];
        let evm_signer = [8u8; 20];
        st.accounts.insert(
            native_signer,
            Account {
                nonce: 0,
                balance: 1_000_000,
            },
        );
        st.accounts.insert(
            evm_signer,
            Account {
                nonce: 0,
                balance: 1_000_000,
            },
        );
        let txs = vec![
            Transaction {
                signer: native_signer,
                nonce: 0,
                vm: VmKind::Native,
                body: TxBody::Native(NativeCall::NoOp),
            },
            Transaction {
                signer: evm_signer,
                nonce: 0,
                vm: VmKind::Evm,
                body: TxBody::Transfer {
                    to: [7u8; 20],
                    amount: 10,
                },
            },
        ];
        let pre_state = st.clone();
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            txs,
            eth_signed_raws_for_txs(2),
        )
        .unwrap();

        let witness =
            mixed_execution_witness_from_state_transition(&block, &pre_state, &st).unwrap();
        assert_eq!(witness.version, MIXED_EXECUTION_WITNESS_V1);
        assert_eq!(
            witness.public_inputs,
            mixed_execution_public_inputs_from_block(&block).unwrap()
        );
        assert_eq!(
            witness.pre_state_commitment.scheme,
            STATE_COMMITMENT_SCHEME_V1
        );
        assert_eq!(
            witness.post_state_commitment.scheme,
            STATE_COMMITMENT_SCHEME_V1
        );
        assert_ne!(
            witness.pre_state_commitment.unified_root,
            witness.post_state_commitment.unified_root
        );
        assert!(witness.state_accesses.iter().any(|row| matches!(
            row.kind,
            StateAccessKindV1::ReadWrite | StateAccessKindV1::Write
        )));
        assert_eq!(witness.tx_receipts.len(), 2);
        assert_eq!(
            tx_receipt_root(&witness.tx_receipts).unwrap(),
            block.header.receipt_root
        );
        assert_eq!(witness.gas_sum, block.header.gas_used);
        assert_eq!(witness.native_trace_rows.len(), 1);
        assert_eq!(witness.native_trace_rows[0].tx_index, 0);
        assert_eq!(
            witness.native_trace_rows[0].gas_used,
            witness.tx_receipts[0].gas_used
        );
        assert_eq!(witness.native_trace_rows[0].signer_pre_nonce, 0);
        assert_eq!(witness.native_trace_rows[0].signer_post_nonce, 1);
        assert_eq!(
            witness.native_trace_rows[0].native_event_root,
            block.header.native_event_root
        );
        assert!(!witness.native_trace_rows[0].state_access_indices.is_empty());
        assert_eq!(witness.evm_trace_rows.len(), 1);
        assert_eq!(witness.evm_trace_rows[0].tx_index, 1);
        assert_eq!(witness.evm_trace_rows[0].kind, EvmTraceKindV1::Transfer);
        assert_eq!(
            witness.evm_trace_rows[0].gas_used,
            witness.tx_receipts[1].gas_used
        );
        assert_eq!(
            witness.evm_trace_rows[0].post_evm_root,
            witness.post_state_commitment.evm_root
        );
        assert_eq!(
            witness.evm_zkvm_surface.covered_features.bits
                & (FEATURE_EVM_TRANSFER | FEATURE_EVM_CALL | FEATURE_EVM_CREATE),
            FEATURE_EVM_TRANSFER | FEATURE_EVM_CALL | FEATURE_EVM_CREATE
        );
        assert_eq!(
            keccak256(&borsh::to_vec(&witness).unwrap()),
            keccak256(&borsh::to_vec(&witness).unwrap())
        );
    }

    #[test]
    fn witness_records_evm_to_native_precompile_dispatch_rows() {
        let signer = addr(0x11, 0x22);
        let to_precompile = addr(0xfc, 0x01);
        let tx = Transaction {
            signer,
            nonce: 0,
            vm: VmKind::Evm,
            body: TxBody::EvmCall {
                to: to_precompile,
                value: 0,
                calldata: borsh::to_vec(&NativeCall::NoOp).unwrap(),
                gas_limit: 1_000_000,
            },
        };

        let (block, witness) = build_replay_witness_for_txs(vec![tx], &[signer]);
        assert!(block.header.feature_set.bits & FEATURE_EVM_TO_NATIVE_PRECOMPILE != 0);
        assert_eq!(witness.precompile_dispatch_rows.len(), 1);
        let row = &witness.precompile_dispatch_rows[0];
        assert_eq!(row.tx_index, 0);
        assert_eq!(row.caller, signer);
        assert_eq!(row.precompile_address, to_precompile);
        assert_eq!(row.native_opcode, 0x01);
        assert_eq!(row.decoded_call, NativeCall::NoOp);
        assert_eq!(
            row.calldata_hash,
            keccak256(&borsh::to_vec(&NativeCall::NoOp).unwrap())
        );
        assert_eq!(witness.evm_trace_rows.len(), 1);
        assert_eq!(witness.gas_sum, block.header.gas_used);
    }

    #[test]
    fn proof_eligibility_rejects_uncovered_and_unsupported_features() {
        let evm_transfer = Transaction {
            signer: [8u8; 20],
            nonce: 0,
            vm: VmKind::Evm,
            body: TxBody::Transfer {
                to: [7u8; 20],
                amount: 10,
            },
        };
        assert_eq!(
            verify_transactions_eligible_for_circuit(
                std::slice::from_ref(&evm_transfer),
                CircuitVersion::NativeStateTransitionV1
            ),
            Err(ProofEligibilityError::EvmTxInNativeCircuit(0))
        );

        let precompile_dispatch = Transaction {
            signer: [8u8; 20],
            nonce: 0,
            vm: VmKind::Evm,
            body: TxBody::EvmCall {
                to: addr(0xfc, 0x01),
                value: 0,
                calldata: borsh::to_vec(&NativeCall::NoOp).unwrap(),
                gas_limit: 1_000_000,
            },
        };
        assert_eq!(
            verify_transactions_eligible_for_circuit(
                std::slice::from_ref(&precompile_dispatch),
                CircuitVersion::NativeStateTransitionV1
            ),
            Err(ProofEligibilityError::PrecompileDispatchInNativeCircuit(0))
        );

        let value_call = Transaction {
            signer: [8u8; 20],
            nonce: 0,
            vm: VmKind::Evm,
            body: TxBody::EvmCall {
                to: [6u8; 20],
                value: 1,
                calldata: Vec::new(),
                gas_limit: 1_000_000,
            },
        };
        assert_eq!(
            verify_transactions_eligible_for_circuit(
                std::slice::from_ref(&value_call),
                CircuitVersion::MixedStateTransitionV1
            ),
            Err(ProofEligibilityError::UnsupportedEvmValue(0))
        );

        let reserved_call = Transaction {
            signer: [8u8; 20],
            nonce: 0,
            vm: VmKind::Evm,
            body: TxBody::EvmCall {
                to: addr(0xfc, 0xff),
                value: 0,
                calldata: Vec::new(),
                gas_limit: 1_000_000,
            },
        };
        assert_eq!(
            verify_transactions_eligible_for_circuit(
                std::slice::from_ref(&reserved_call),
                CircuitVersion::MixedStateTransitionV1
            ),
            Err(ProofEligibilityError::UnsupportedEvmCallTarget(0))
        );
    }

    fn native_transition_witness(mut witness: MixedExecutionWitnessV1) -> MixedExecutionWitnessV1 {
        witness.public_inputs.circuit_version = CircuitVersion::NativeStateTransitionV1;
        witness.public_inputs.coverage_manifest_digest = coverage_manifest_digest(
            &coverage_manifest_for_circuit_version(CircuitVersion::NativeStateTransitionV1),
        )
        .unwrap();
        witness
    }

    fn native_recursive_block_proof(
        block: &Block,
        witness: &MixedExecutionWitnessV1,
    ) -> BlockValidityProof {
        let mut proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            timestamp_ms: block.header.timestamp_ms,
            parent_state_root: block.header.parent_state_root,
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            receipt_root: block.header.receipt_root,
            native_event_root: block.header.native_event_root,
            evm_log_root: block.header.evm_log_root,
            gas_used: block.header.gas_used,
            zone_namespace: block.header.zone_namespace,
            da_root: block.header.da_root,
            circuit_version: CircuitVersion::NativeStateTransitionV1,
            coverage_manifest_digest: coverage_manifest_digest(
                &coverage_manifest_for_circuit_version(CircuitVersion::NativeStateTransitionV1),
            )
            .unwrap(),
            feature_set: block.header.feature_set,
            proof_system: ValidityProofSystem::StwoPlonky2,
            proof_bytes: Vec::new(),
        };
        let statement = native_state_transition_statement_v1(witness).unwrap();
        proof.proof_bytes = borsh::to_vec(
            &native_recursive_proof_envelope_v1(statement, &proof, [0x44; 32]).unwrap(),
        )
        .unwrap();
        proof
    }

    fn mixed_transition_witness(mut witness: MixedExecutionWitnessV1) -> MixedExecutionWitnessV1 {
        witness.public_inputs.circuit_version = CircuitVersion::MixedStateTransitionV1;
        witness.public_inputs.coverage_manifest_digest = coverage_manifest_digest(
            &coverage_manifest_for_circuit_version(CircuitVersion::MixedStateTransitionV1),
        )
        .unwrap();
        witness
    }

    fn mixed_evm_zkvm_block_proof(
        block: &Block,
        witness: &MixedExecutionWitnessV1,
    ) -> BlockValidityProof {
        let mut proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            timestamp_ms: block.header.timestamp_ms,
            parent_state_root: block.header.parent_state_root,
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            receipt_root: block.header.receipt_root,
            native_event_root: block.header.native_event_root,
            evm_log_root: block.header.evm_log_root,
            gas_used: block.header.gas_used,
            zone_namespace: block.header.zone_namespace,
            da_root: block.header.da_root,
            circuit_version: CircuitVersion::MixedStateTransitionV1,
            coverage_manifest_digest: coverage_manifest_digest(
                &coverage_manifest_for_circuit_version(CircuitVersion::MixedStateTransitionV1),
            )
            .unwrap(),
            feature_set: block.header.feature_set,
            proof_system: ValidityProofSystem::StwoPlonky2,
            proof_bytes: Vec::new(),
        };
        proof.proof_bytes = borsh::to_vec(&evm_zkvm_proof_envelope_v1(witness).unwrap()).unwrap();
        proof
    }

    fn mixed_aggregate_block_proof(
        block: &Block,
        witness: &MixedExecutionWitnessV1,
    ) -> BlockValidityProof {
        let mut proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            timestamp_ms: block.header.timestamp_ms,
            parent_state_root: block.header.parent_state_root,
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            receipt_root: block.header.receipt_root,
            native_event_root: block.header.native_event_root,
            evm_log_root: block.header.evm_log_root,
            gas_used: block.header.gas_used,
            zone_namespace: block.header.zone_namespace,
            da_root: block.header.da_root,
            circuit_version: CircuitVersion::MixedStateTransitionV1,
            coverage_manifest_digest: coverage_manifest_digest(
                &coverage_manifest_for_circuit_version(CircuitVersion::MixedStateTransitionV1),
            )
            .unwrap(),
            feature_set: block.header.feature_set,
            proof_system: ValidityProofSystem::StwoPlonky2,
            proof_bytes: Vec::new(),
        };
        proof.proof_bytes =
            borsh::to_vec(&mixed_intrablock_aggregate_proof_envelope_v1(witness).unwrap()).unwrap();
        proof
    }

    #[test]
    fn phase_f_selects_risc_zero_revm_surface_and_budget() {
        let surface = evm_zkvm_surface_v1();
        assert_eq!(surface.zkvm_choice, ZkVmChoiceV1::RiscZeroRevmTransitionV1);
        assert_eq!(surface.zkvm_target, "risc0-fractal-revm-transition-v1");
        assert_eq!(surface.revm_crate_version, "38.0.0");
        assert!(surface.covered_features.bits & FEATURE_EVM_TRANSFER != 0);
        assert!(surface.covered_features.bits & FEATURE_EVM_CALL != 0);
        assert!(surface.covered_features.bits & FEATURE_EVM_CREATE != 0);
        assert!(surface.covered_features.bits & FEATURE_EVM_TO_NATIVE_PRECOMPILE != 0);
        assert_eq!(surface.uncovered_features, ExecutionFeatureSetV1::empty());

        let budget = evm_mixed_proving_budget_v1();
        assert_eq!(budget.zkvm_choice, ZkVmChoiceV1::RiscZeroRevmTransitionV1);
        assert_eq!(
            budget.batching_mode,
            MixedProofBatchingModeV1::BatchedFallback
        );
        assert!(budget.mixed_block_latency_target_ms > budget.native_block_latency_target_ms);
        assert!(budget.max_proof_final_lag_ms >= budget.mixed_block_latency_target_ms);
    }

    #[test]
    fn evm_zkvm_fixture_proves_revm_transition_surface() {
        let signer = addr(0x81, 0x82);
        let tx = Transaction {
            signer,
            nonce: 0,
            vm: VmKind::Evm,
            body: TxBody::Transfer {
                to: [7u8; 20],
                amount: 10,
            },
        };
        let (block, witness) = build_replay_witness_for_txs(vec![tx], &[signer]);
        let witness = mixed_transition_witness(witness);
        let statement = evm_zkvm_transition_statement_v1(&witness).unwrap();
        assert_eq!(
            statement.zkvm_choice,
            ZkVmChoiceV1::RiscZeroRevmTransitionV1
        );
        assert_eq!(
            statement.public_input_digest,
            keccak256(&borsh::to_vec(&witness.public_inputs).unwrap())
        );
        assert_eq!(
            statement.witness_digest,
            mixed_execution_witness_digest(&witness).unwrap()
        );
        assert_eq!(statement.unified_post_state_root, block.header.state_root);
        assert_eq!(statement.evm_log_root, block.header.evm_log_root);
        assert_eq!(statement.evm_trace_rows, 1);

        let proof = mixed_evm_zkvm_block_proof(&block, &witness);
        verify_block_validity_proof(&block, &proof).expect("mixed EVM zkVM fixture verifies");
    }

    #[test]
    fn evm_zkvm_fixture_rejects_tampering_and_wrong_circuit() {
        let signer = addr(0x83, 0x84);
        let tx = Transaction {
            signer,
            nonce: 0,
            vm: VmKind::Evm,
            body: TxBody::Transfer {
                to: [7u8; 20],
                amount: 10,
            },
        };
        let (block, witness) = build_replay_witness_for_txs(vec![tx], &[signer]);
        let witness = mixed_transition_witness(witness);
        let proof = mixed_evm_zkvm_block_proof(&block, &witness);

        let mut envelope = StwoPlonky2ProofEnvelope::try_from_slice(&proof.proof_bytes).unwrap();
        let StwoPlonky2ProofEnvelope::EvmZkVmFixtureV1 { fixture } = &mut envelope else {
            panic!("evm zkvm fixture envelope");
        };
        fixture.statement.evm_log_root[0] ^= 0x01;
        let mut tampered = proof.clone();
        tampered.proof_bytes = borsh::to_vec(&envelope).unwrap();
        assert!(matches!(
            verify_block_validity_proof(&block, &tampered),
            Err(ProofVerifyError::Production(
                ProductionProofVerifyError::EvmZkVmFixture
            ))
        ));

        let mut wrong_circuit = proof.clone();
        wrong_circuit.circuit_version = CircuitVersion::DevMixedV1;
        wrong_circuit.coverage_manifest_digest = coverage_manifest_digest(
            &coverage_manifest_for_circuit_version(CircuitVersion::DevMixedV1),
        )
        .unwrap();
        assert!(matches!(
            verify_block_validity_proof(&block, &wrong_circuit),
            Err(ProofVerifyError::Production(
                ProductionProofVerifyError::MixedCircuitVersion
            ))
        ));
    }

    #[test]
    fn mixed_aggregate_fixture_proves_native_and_evm_block() {
        let native_signer = addr(0x85, 0x86);
        let evm_signer = addr(0x87, 0x88);
        let native_tx = Transaction {
            signer: native_signer,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let evm_tx = Transaction {
            signer: evm_signer,
            nonce: 0,
            vm: VmKind::Evm,
            body: TxBody::Transfer {
                to: [7u8; 20],
                amount: 10,
            },
        };
        let (block, witness) =
            build_replay_witness_for_txs(vec![native_tx, evm_tx], &[native_signer, evm_signer]);
        let witness = mixed_transition_witness(witness);
        let fixture = mixed_intrablock_aggregate_fixture_v1(&witness).unwrap();
        assert_eq!(
            fixture.circuit_version,
            CircuitVersion::MixedStateTransitionV1
        );
        assert_eq!(fixture.native_component_statement.native_trace_rows, 1);
        assert_eq!(fixture.evm_zkvm_fixture.statement.evm_trace_rows, 1);
        assert_eq!(
            fixture.witness_digest,
            mixed_execution_witness_digest(&witness).unwrap()
        );

        let proof = mixed_aggregate_block_proof(&block, &witness);
        verify_block_validity_proof(&block, &proof).expect("mixed aggregate fixture verifies");
    }

    #[test]
    fn mixed_aggregate_fixture_reflects_evm_to_native_precompile_dispatch() {
        let signer = addr(0x89, 0x8a);
        let tx = Transaction {
            signer,
            nonce: 0,
            vm: VmKind::Evm,
            body: TxBody::EvmCall {
                to: addr(0xfc, 0x01),
                value: 0,
                calldata: borsh::to_vec(&NativeCall::NoOp).unwrap(),
                gas_limit: 1_000_000,
            },
        };
        let (block, witness) = build_replay_witness_for_txs(vec![tx], &[signer]);
        let witness = mixed_transition_witness(witness);
        let fixture = mixed_intrablock_aggregate_fixture_v1(&witness).unwrap();
        assert_eq!(fixture.native_component_statement.native_trace_rows, 0);
        assert_eq!(
            fixture.native_component_statement.precompile_dispatch_rows,
            1
        );
        assert_eq!(fixture.evm_zkvm_fixture.statement.evm_trace_rows, 1);
        assert!(
            fixture.native_component_statement.feature_set.bits & FEATURE_EVM_TO_NATIVE_PRECOMPILE
                != 0
        );
        assert_eq!(
            fixture.native_component_statement.public_input_digest,
            fixture.evm_zkvm_fixture.statement.public_input_digest
        );
        assert_eq!(
            fixture.native_component_statement.witness_digest,
            fixture.evm_zkvm_fixture.statement.witness_digest
        );

        let proof = mixed_aggregate_block_proof(&block, &witness);
        verify_block_validity_proof(&block, &proof)
            .expect("mixed aggregate precompile fixture verifies");
    }

    #[test]
    fn mixed_aggregate_fixture_rejects_component_tampering() {
        let native_signer = addr(0x8b, 0x8c);
        let evm_signer = addr(0x8d, 0x8e);
        let native_tx = Transaction {
            signer: native_signer,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let evm_tx = Transaction {
            signer: evm_signer,
            nonce: 0,
            vm: VmKind::Evm,
            body: TxBody::Transfer {
                to: [7u8; 20],
                amount: 10,
            },
        };
        let (block, witness) =
            build_replay_witness_for_txs(vec![native_tx, evm_tx], &[native_signer, evm_signer]);
        let witness = mixed_transition_witness(witness);
        let proof = mixed_aggregate_block_proof(&block, &witness);
        let mut envelope = StwoPlonky2ProofEnvelope::try_from_slice(&proof.proof_bytes).unwrap();
        let StwoPlonky2ProofEnvelope::MixedIntraBlockAggregateFixtureV1 { fixture } = &mut envelope
        else {
            panic!("mixed aggregate fixture envelope");
        };
        fixture.native_component_statement.native_trace_root[0] ^= 0x01;
        let mut tampered = proof.clone();
        tampered.proof_bytes = borsh::to_vec(&envelope).unwrap();
        assert!(matches!(
            verify_block_validity_proof(&block, &tampered),
            Err(ProofVerifyError::Production(
                ProductionProofVerifyError::MixedAggregateFixture
            ))
        ));
    }

    fn science_receipt(id: u8, requester: [u8; 20]) -> OnChainTaskReceipt {
        OnChainTaskReceipt {
            receipt_id: [id; 32],
            job_id: [id.saturating_add(1); 32],
            requester,
            worker: 1,
            verifier: 2,
            artifact_root: [id.saturating_add(2); 32],
            output_hash: [id.saturating_add(3); 32],
            score: 97,
            payout_amount: 250,
            verifier_fee: 5,
            protocol_fee: 2,
            final_status: 1,
            finalized_at: 1_000,
            schema_version: 1,
        }
    }

    #[test]
    fn native_state_transition_air_fixture_proves_native_science_work_block() {
        let signer = addr(0x33, 0x44);
        let receipt = science_receipt(0x51, signer);
        let payout = PayoutEntry {
            index: 0,
            account: signer,
            amount: 250,
        };
        let batch = SettleBatchPayload {
            batch_id: [0x71; 32],
            operator: signer,
            receipts: vec![receipt.clone()],
            payout_entries: vec![payout.clone()],
            submitted_at: 1_001,
            operator_sig: [0u8; 64],
        };
        let txs = vec![
            Transaction {
                signer,
                nonce: 0,
                vm: VmKind::Native,
                body: TxBody::Native(NativeCall::RegisterAgent {
                    operator: signer,
                    pubkey: [0x11; 32],
                    kind: 1,
                    metadata_uri: "science://dataset/register".to_owned(),
                }),
            },
            Transaction {
                signer,
                nonce: 1,
                vm: VmKind::Native,
                body: TxBody::Native(NativeCall::WalletTaskReceiptAnchorV1 {
                    commitment: [0x61; 32],
                    receipt_witness: Vec::new(),
                }),
            },
            Transaction {
                signer,
                nonce: 2,
                vm: VmKind::Native,
                body: TxBody::Native(NativeCall::SettleBatch(batch)),
            },
            Transaction {
                signer,
                nonce: 3,
                vm: VmKind::Native,
                body: TxBody::Native(NativeCall::ClaimPayout {
                    batch_id: [0x71; 32],
                    account: signer,
                    amount: payout.amount,
                    leaf_index: payout.index,
                    proof: Vec::new(),
                }),
            },
        ];
        let (block, witness) = build_replay_witness_for_txs(txs, &[signer]);
        let witness = native_transition_witness(witness);

        assert!(block.header.feature_set.contains_only(
            coverage_manifest_for_circuit_version(CircuitVersion::NativeStateTransitionV1)
                .covered_features
        ));
        let columns = native_state_transition_trace_columns_v1(&witness).unwrap();
        assert_eq!(columns.len(), 4);
        assert_eq!(columns[0].columns[1], 0x01);
        assert_eq!(columns[1].columns[1], 0x0e);
        assert_eq!(columns[2].columns[1], 0x05);
        assert_eq!(columns[3].columns[1], 0x06);

        let statement = native_state_transition_statement_v1(&witness).unwrap();
        assert_eq!(statement.trace_rows, 4);
        assert_eq!(statement.gas_used, block.header.gas_used);
        assert_eq!(statement.receipt_root, block.header.receipt_root);
        assert_eq!(statement.native_event_root, block.header.native_event_root);
        assert_eq!(
            statement.witness_digest,
            mixed_execution_witness_digest(&witness).unwrap()
        );
        assert_ne!(statement.fiat_shamir_transcript_digest, [0u8; 32]);
        assert_ne!(statement.native_subtrie_access_digest, [0u8; 32]);

        let fixture = prove_native_state_transition_fixture_v1(&witness).unwrap();
        verify_native_state_transition_fixture_v1(&fixture, &witness).unwrap();
    }

    #[test]
    fn native_state_transition_air_rejects_evm_execution_and_dispatch_rows() {
        let signer = addr(0x55, 0x66);
        let evm_tx = Transaction {
            signer,
            nonce: 0,
            vm: VmKind::Evm,
            body: TxBody::Transfer {
                to: [7u8; 20],
                amount: 10,
            },
        };
        let (_, witness) = build_replay_witness_for_txs(vec![evm_tx], &[signer]);
        let witness = native_transition_witness(witness);
        assert!(matches!(
            native_state_transition_statement_v1(&witness),
            Err(NativeStateTransitionAirError::EvmExecutionPresent)
        ));

        let precompile_tx = Transaction {
            signer,
            nonce: 0,
            vm: VmKind::Evm,
            body: TxBody::EvmCall {
                to: addr(0xfc, 0x01),
                value: 0,
                calldata: borsh::to_vec(&NativeCall::NoOp).unwrap(),
                gas_limit: 1_000_000,
            },
        };
        let (_, witness) = build_replay_witness_for_txs(vec![precompile_tx], &[signer]);
        let witness = native_transition_witness(witness);
        assert!(matches!(
            native_state_transition_statement_v1(&witness),
            Err(NativeStateTransitionAirError::EvmExecutionPresent)
                | Err(NativeStateTransitionAirError::PrecompileDispatchPresent)
        ));
    }

    #[test]
    fn native_recursive_fixture_verifies_and_rejects_mismatches() {
        let signer = addr(0x71, 0x72);
        let tx = Transaction {
            signer,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let (block, witness) = build_replay_witness_for_txs(vec![tx], &[signer]);
        let witness = native_transition_witness(witness);
        let proof = native_recursive_block_proof(&block, &witness);

        verify_block_validity_proof(&block, &proof).expect("native recursive fixture verifies");

        let mut envelope = StwoPlonky2ProofEnvelope::try_from_slice(&proof.proof_bytes).unwrap();
        let StwoPlonky2ProofEnvelope::NativeRecursiveFixtureV1 { statement, .. } = &mut envelope
        else {
            panic!("native recursive fixture envelope");
        };
        statement.public_input_digest[0] ^= 0x01;
        let mut tampered = proof.clone();
        tampered.proof_bytes = borsh::to_vec(&envelope).unwrap();
        assert!(matches!(
            verify_block_validity_proof(&block, &tampered),
            Err(ProofVerifyError::Production(
                ProductionProofVerifyError::NativeRecursiveFixture
            ))
        ));

        let mut stale = proof.clone();
        stale.circuit_version = CircuitVersion::DevMixedV1;
        stale.coverage_manifest_digest = coverage_manifest_digest(
            &coverage_manifest_for_circuit_version(CircuitVersion::DevMixedV1),
        )
        .unwrap();
        let statement = native_state_transition_statement_v1(&witness).unwrap();
        stale.proof_bytes = borsh::to_vec(
            &native_recursive_proof_envelope_v1(statement, &stale, [0x44; 32]).unwrap(),
        )
        .unwrap();
        assert!(matches!(
            verify_block_validity_proof(&block, &stale),
            Err(ProofVerifyError::Production(
                ProductionProofVerifyError::NativeRecursiveFixture
            ))
        ));
    }

    #[test]
    fn native_recursive_fixture_rejects_evm_containing_block_under_native_coverage() {
        let signer = addr(0x73, 0x74);
        let tx = Transaction {
            signer,
            nonce: 0,
            vm: VmKind::Evm,
            body: TxBody::Transfer {
                to: [7u8; 20],
                amount: 10,
            },
        };
        let (block, _) = build_replay_witness_for_txs(vec![tx], &[signer]);
        let proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            timestamp_ms: block.header.timestamp_ms,
            parent_state_root: block.header.parent_state_root,
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            receipt_root: block.header.receipt_root,
            native_event_root: block.header.native_event_root,
            evm_log_root: block.header.evm_log_root,
            gas_used: block.header.gas_used,
            zone_namespace: block.header.zone_namespace,
            da_root: block.header.da_root,
            circuit_version: CircuitVersion::NativeStateTransitionV1,
            coverage_manifest_digest: coverage_manifest_digest(
                &coverage_manifest_for_circuit_version(CircuitVersion::NativeStateTransitionV1),
            )
            .unwrap(),
            feature_set: block.header.feature_set,
            proof_system: ValidityProofSystem::StwoPlonky2,
            proof_bytes: vec![1],
        };
        assert!(matches!(
            verify_block_validity_proof(&block, &proof),
            Err(ProofVerifyError::CircuitCoverage)
        ));
    }

    #[test]
    fn native_inter_block_chain_enforces_post_to_parent_state_root() {
        let signer = addr(0x75, 0x76);
        let mut pre_state = state_with_accounts(&[signer]);
        let block1_pre = pre_state.clone();
        let tx1 = Transaction {
            signer,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let block1 = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut pre_state,
            vec![tx1],
            eth_signed_raws_for_txs(1),
        )
        .unwrap();
        let block2_pre = pre_state.clone();
        let tx2 = Transaction {
            signer,
            nonce: 1,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let block2 = execute_and_build_block(
            41,
            2,
            0,
            header_hash(&block1.header).unwrap(),
            [0u8; 32],
            [0u8; 32],
            2_000,
            60_000_000,
            &mut pre_state,
            vec![tx2],
            eth_signed_raws_for_txs(1),
        )
        .unwrap();
        let witness1 = native_transition_witness(
            mixed_execution_witness_from_replay(&block1, &block1_pre).unwrap(),
        );
        let witness2 = native_transition_witness(
            mixed_execution_witness_from_replay(&block2, &block2_pre).unwrap(),
        );
        let proof1 = native_recursive_block_proof(&block1, &witness1);
        let proof2 = native_recursive_block_proof(&block2, &witness2);

        verify_native_inter_block_chain_v1(&proof1, &proof2).expect("linked native proofs");
        let mut broken = proof2.clone();
        broken.parent_state_root = [9u8; 32];
        assert!(matches!(
            verify_native_inter_block_chain_v1(&proof1, &broken),
            Err(ProductionProofVerifyError::NativeInterBlockChain)
        ));
    }

    fn state_with_accounts(addresses: &[[u8; 20]]) -> State {
        let mut st = State::default();
        for address in addresses {
            st.accounts.insert(
                *address,
                Account {
                    nonce: 0,
                    balance: 1_000_000,
                },
            );
        }
        st
    }

    fn build_replay_witness_for_txs(
        txs: Vec<Transaction>,
        signers: &[[u8; 20]],
    ) -> (Block, MixedExecutionWitnessV1) {
        let pre_state = state_with_accounts(signers);
        let mut execution_state = pre_state.clone();
        let tx_count = txs.len();
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut execution_state,
            txs,
            eth_signed_raws_for_txs(tx_count),
        )
        .unwrap();
        let witness = mixed_execution_witness_from_replay(&block, &pre_state).unwrap();
        (block, witness)
    }

    #[test]
    fn replaying_same_block_produces_identical_witness_digest() {
        let signer = [9u8; 20];
        let tx = Transaction {
            signer,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let pre_state = state_with_accounts(&[signer]);
        let mut execution_state = pre_state.clone();
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut execution_state,
            vec![tx],
            eth_signed_raws_for_txs(1),
        )
        .unwrap();

        let a = mixed_execution_witness_from_replay(&block, &pre_state).unwrap();
        let b = mixed_execution_witness_from_replay(&block, &pre_state).unwrap();
        assert_eq!(borsh::to_vec(&a).unwrap(), borsh::to_vec(&b).unwrap());
        assert_eq!(
            mixed_execution_witness_digest(&a).unwrap(),
            mixed_execution_witness_digest(&b).unwrap()
        );
    }

    #[test]
    fn transaction_reordering_changes_witness_digest() {
        let native_signer = [9u8; 20];
        let evm_signer = [8u8; 20];
        let native_tx = Transaction {
            signer: native_signer,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let evm_tx = Transaction {
            signer: evm_signer,
            nonce: 0,
            vm: VmKind::Evm,
            body: TxBody::Transfer {
                to: [7u8; 20],
                amount: 10,
            },
        };
        let signers = [native_signer, evm_signer];
        let (_, ordered) =
            build_replay_witness_for_txs(vec![native_tx.clone(), evm_tx.clone()], &signers);
        let (_, reordered) = build_replay_witness_for_txs(vec![evm_tx, native_tx], &signers);

        assert_ne!(
            mixed_execution_witness_digest(&ordered).unwrap(),
            mixed_execution_witness_digest(&reordered).unwrap()
        );
    }

    #[test]
    fn native_evm_and_mixed_blocks_produce_valid_witness_shapes() {
        let native_signer = [9u8; 20];
        let evm_signer = [8u8; 20];
        let native_tx = Transaction {
            signer: native_signer,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let evm_tx = Transaction {
            signer: evm_signer,
            nonce: 0,
            vm: VmKind::Evm,
            body: TxBody::Transfer {
                to: [7u8; 20],
                amount: 10,
            },
        };

        let (native_block, native_witness) =
            build_replay_witness_for_txs(vec![native_tx.clone()], &[native_signer]);
        assert!(native_block.header.feature_set.contains_only(
            coverage_manifest_for_circuit_version(CircuitVersion::NativeStateTransitionV1)
                .covered_features
        ));
        assert_eq!(native_witness.transactions.len(), 1);
        assert_eq!(native_witness.tx_receipts.len(), 1);
        assert_eq!(
            native_witness.public_inputs.feature_set,
            native_block.header.feature_set
        );

        let (evm_block, evm_witness) =
            build_replay_witness_for_txs(vec![evm_tx.clone()], &[evm_signer]);
        assert!(evm_block.header.feature_set.bits & FEATURE_EVM_TRANSFER != 0);
        assert_eq!(evm_witness.transactions.len(), 1);
        assert_eq!(evm_witness.tx_receipts.len(), 1);

        let (mixed_block, mixed_witness) =
            build_replay_witness_for_txs(vec![native_tx, evm_tx], &[native_signer, evm_signer]);
        assert!(mixed_block.header.feature_set.bits & FEATURE_NATIVE_TX != 0);
        assert!(mixed_block.header.feature_set.bits & FEATURE_EVM_TRANSFER != 0);
        assert_eq!(mixed_witness.transactions.len(), 2);
        assert_eq!(mixed_witness.tx_receipts.len(), 2);
        assert_eq!(
            mixed_execution_witness_metadata(
                &mixed_witness,
                WitnessRetentionPolicyV1::MetadataOnly
            )
            .unwrap()
            .witness_digest,
            mixed_execution_witness_digest(&mixed_witness).unwrap()
        );
    }

    #[test]
    #[cfg(feature = "dev-digest")]
    fn dev_digest_proof_rejects_timestamp_mismatch() {
        let mut st = State::default();
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
        )
        .unwrap();
        let mut proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            timestamp_ms: block.header.timestamp_ms + 1,
            parent_state_root: block.header.parent_state_root,
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            receipt_root: block.header.receipt_root,
            native_event_root: block.header.native_event_root,
            evm_log_root: block.header.evm_log_root,
            gas_used: block.header.gas_used,
            zone_namespace: block.header.zone_namespace,
            da_root: block.header.da_root,
            circuit_version: CircuitVersion::DevMixedV1,
            coverage_manifest_digest: coverage_manifest_digest(
                &coverage_manifest_for_circuit_version(CircuitVersion::DevMixedV1),
            )
            .unwrap(),
            feature_set: block.header.feature_set,
            proof_system: ValidityProofSystem::StwoPlonky2,
            proof_bytes: Vec::new(),
        };
        proof.proof_bytes = validity_proof_public_input_digest(&proof).unwrap().to_vec();

        assert!(matches!(
            verify_block_validity_proof(&block, &proof),
            Err(ProofVerifyError::Timestamp)
        ));
    }

    #[test]
    #[cfg(feature = "dev-digest")]
    fn dev_digest_proof_rejects_every_public_input_root_mismatch() {
        let signer = [0xA1; 20];
        let mut st = State::default();
        st.accounts.insert(
            signer,
            Account {
                nonce: 0,
                balance: 1_000,
            },
        );
        let tx = Transaction {
            signer,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            vec![tx],
            eth_signed_raws_for_txs(1),
        )
        .unwrap();
        let valid_proof = || {
            let mut proof = BlockValidityProof {
                chain_id: block.header.chain_id,
                height: block.header.height,
                block_hash: header_hash(&block.header).unwrap(),
                timestamp_ms: block.header.timestamp_ms,
                parent_state_root: block.header.parent_state_root,
                state_root: block.header.state_root,
                tx_root: block.header.tx_root,
                receipt_root: block.header.receipt_root,
                native_event_root: block.header.native_event_root,
                evm_log_root: block.header.evm_log_root,
                gas_used: block.header.gas_used,
                zone_namespace: block.header.zone_namespace,
                da_root: block.header.da_root,
                circuit_version: CircuitVersion::DevMixedV1,
                coverage_manifest_digest: coverage_manifest_digest(
                    &coverage_manifest_for_circuit_version(CircuitVersion::DevMixedV1),
                )
                .unwrap(),
                feature_set: block.header.feature_set,
                proof_system: ValidityProofSystem::DevDigest,
                proof_bytes: Vec::new(),
            };
            proof.proof_bytes = validity_proof_public_input_digest(&proof).unwrap().to_vec();
            proof
        };

        let mut cases: Vec<(&str, BlockValidityProof, ProofVerifyError)> = Vec::new();

        let mut proof = valid_proof();
        proof.parent_state_root[0] ^= 0x01;
        cases.push((
            "parent_state_root bit flip",
            proof,
            ProofVerifyError::ParentStateRoot,
        ));

        let mut proof = valid_proof();
        proof.state_root[0] ^= 0x01;
        cases.push(("state_root bit flip", proof, ProofVerifyError::StateRoot));

        let mut proof = valid_proof();
        proof.tx_root[0] ^= 0x01;
        cases.push(("tx_root bit flip", proof, ProofVerifyError::TxRoot));

        let mut proof = valid_proof();
        proof.da_root[0] ^= 0x01;
        cases.push(("da_root bit flip", proof, ProofVerifyError::DaRoot));

        let mut proof = valid_proof();
        proof.receipt_root[0] ^= 0x01;
        cases.push((
            "receipt_root bit flip",
            proof,
            ProofVerifyError::ReceiptRoot,
        ));

        let mut proof = valid_proof();
        proof.native_event_root[0] ^= 0x01;
        cases.push((
            "native_event_root bit flip",
            proof,
            ProofVerifyError::NativeEventRoot,
        ));

        let mut proof = valid_proof();
        proof.evm_log_root[0] ^= 0x01;
        cases.push(("evm_log_root bit flip", proof, ProofVerifyError::EvmLogRoot));

        let mut proof = valid_proof();
        proof.zone_namespace = *b"badroot!";
        cases.push((
            "DA namespace mismatch",
            proof,
            ProofVerifyError::ZoneNamespace,
        ));

        let mut proof = valid_proof();
        proof.tx_root = block.header.da_root;
        cases.push((
            "cross-root confusion tx_root<-da_root",
            proof,
            ProofVerifyError::TxRoot,
        ));

        let mut proof = valid_proof();
        proof.da_root = block.header.tx_root;
        cases.push((
            "cross-root confusion da_root<-tx_root",
            proof,
            ProofVerifyError::DaRoot,
        ));

        let mut proof = valid_proof();
        proof.parent_state_root = [0xAA; 32];
        cases.push((
            "stale parent_state_root replay",
            proof,
            ProofVerifyError::ParentStateRoot,
        ));

        let mut proof = valid_proof();
        proof.coverage_manifest_digest[0] ^= 0x01;
        cases.push((
            "coverage manifest metadata",
            proof,
            ProofVerifyError::CoverageManifest,
        ));

        for (name, proof, expected) in cases {
            let err = verify_block_validity_proof(&block, &proof)
                .expect_err("tampered proof must reject");
            assert_eq!(
                std::mem::discriminant(&err),
                std::mem::discriminant(&expected),
                "{name} rejected with {err:?}, expected {expected:?}"
            );
        }

        let mut truncated = valid_proof();
        truncated.proof_bytes.truncate(16);
        assert!(matches!(
            verify_block_validity_proof(&block, &truncated),
            Err(ProofVerifyError::BadDevDigest)
        ));
    }

    #[test]
    #[cfg(feature = "dev-digest")]
    fn native_circuit_coverage_rejects_evm_feature_set() {
        let mut st = State::default();
        let signer = [8u8; 20];
        st.accounts.insert(
            signer,
            Account {
                nonce: 0,
                balance: 1_000_000,
            },
        );
        let tx = Transaction {
            signer,
            nonce: 0,
            vm: VmKind::Evm,
            body: TxBody::Transfer {
                to: [7u8; 20],
                amount: 10,
            },
        };
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            vec![tx],
            eth_signed_raws_for_txs(1),
        )
        .unwrap();
        assert!(!block.header.feature_set.contains_only(
            coverage_manifest_for_circuit_version(CircuitVersion::NativeStateTransitionV1)
                .covered_features
        ));

        let mut proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            timestamp_ms: block.header.timestamp_ms,
            parent_state_root: block.header.parent_state_root,
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            receipt_root: block.header.receipt_root,
            native_event_root: block.header.native_event_root,
            evm_log_root: block.header.evm_log_root,
            gas_used: block.header.gas_used,
            zone_namespace: block.header.zone_namespace,
            da_root: block.header.da_root,
            circuit_version: CircuitVersion::NativeStateTransitionV1,
            coverage_manifest_digest: coverage_manifest_digest(
                &coverage_manifest_for_circuit_version(CircuitVersion::NativeStateTransitionV1),
            )
            .unwrap(),
            feature_set: block.header.feature_set,
            proof_system: ValidityProofSystem::StwoPlonky2,
            proof_bytes: vec![1],
        };
        proof.proof_bytes = validity_proof_public_input_digest(&proof).unwrap().to_vec();

        assert!(matches!(
            verify_block_validity_proof(&block, &proof),
            Err(ProofVerifyError::CircuitCoverage)
        ));
    }

    #[test]
    fn stwo_plonky2_proof_rejects_malformed_envelope() {
        let mut st = State::default();
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
        )
        .unwrap();
        let proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            timestamp_ms: block.header.timestamp_ms,
            parent_state_root: block.header.parent_state_root,
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            receipt_root: block.header.receipt_root,
            native_event_root: block.header.native_event_root,
            evm_log_root: block.header.evm_log_root,
            gas_used: block.header.gas_used,
            zone_namespace: block.header.zone_namespace,
            da_root: block.header.da_root,
            circuit_version: CircuitVersion::DevMixedV1,
            coverage_manifest_digest: coverage_manifest_digest(
                &coverage_manifest_for_circuit_version(CircuitVersion::DevMixedV1),
            )
            .unwrap(),
            feature_set: block.header.feature_set,
            proof_system: ValidityProofSystem::StwoPlonky2,
            proof_bytes: vec![1, 2, 3],
        };

        assert!(matches!(
            verify_block_validity_proof(&block, &proof),
            Err(ProofVerifyError::Production(
                ProductionProofVerifyError::MalformedEnvelope
            ))
        ));
    }

    #[test]
    fn stwo_plonky2_proof_rejects_invalid_verifier_data() {
        let mut st = State::default();
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
        )
        .unwrap();
        let proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            timestamp_ms: block.header.timestamp_ms,
            parent_state_root: block.header.parent_state_root,
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            receipt_root: block.header.receipt_root,
            native_event_root: block.header.native_event_root,
            evm_log_root: block.header.evm_log_root,
            gas_used: block.header.gas_used,
            zone_namespace: block.header.zone_namespace,
            da_root: block.header.da_root,
            circuit_version: CircuitVersion::DevMixedV1,
            coverage_manifest_digest: coverage_manifest_digest(
                &coverage_manifest_for_circuit_version(CircuitVersion::DevMixedV1),
            )
            .unwrap(),
            feature_set: block.header.feature_set,
            proof_system: ValidityProofSystem::StwoPlonky2,
            proof_bytes: borsh::to_vec(&StwoPlonky2ProofEnvelope::Plonky2PoseidonGoldilocksV1 {
                verifier_circuit_data: vec![9, 9, 9],
                proof_with_public_inputs: vec![1, 2, 3],
                compressed: false,
            })
            .unwrap(),
        };

        assert!(matches!(
            verify_block_validity_proof(&block, &proof),
            Err(ProofVerifyError::Production(
                ProductionProofVerifyError::Plonky2VerifierData
            ))
        ));
    }

    #[test]
    fn stwo_air_adapter_and_recursive_fixture_bind_public_inputs() {
        let mut st = State::default();
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
        )
        .unwrap();
        let proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            timestamp_ms: block.header.timestamp_ms,
            parent_state_root: block.header.parent_state_root,
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            receipt_root: block.header.receipt_root,
            native_event_root: block.header.native_event_root,
            evm_log_root: block.header.evm_log_root,
            gas_used: block.header.gas_used,
            zone_namespace: block.header.zone_namespace,
            da_root: block.header.da_root,
            circuit_version: CircuitVersion::DevMixedV1,
            coverage_manifest_digest: coverage_manifest_digest(
                &coverage_manifest_for_circuit_version(CircuitVersion::DevMixedV1),
            )
            .unwrap(),
            feature_set: block.header.feature_set,
            proof_system: ValidityProofSystem::StwoPlonky2,
            proof_bytes: vec![1],
        };

        let public_input_digest = validity_proof_public_input_digest(&proof).unwrap();
        let adapter = stwo_execution_air_adapter_v1(&proof).unwrap();
        assert_eq!(adapter.public_input_digest, public_input_digest);
        assert_eq!(
            adapter.public_input_limbs,
            stwo_plonky2_public_input_limbs(&public_input_digest)
        );
        let fixture = canonical_recursive_proof_fixture_v1(&adapter, [3u8; 32]).unwrap();
        assert_eq!(
            fixture.stwo_air_adapter_digest,
            stwo_execution_air_adapter_digest(&adapter).unwrap()
        );
        assert_eq!(fixture.public_input_digest, public_input_digest);
        assert_eq!(fixture.public_input_limbs, adapter.public_input_limbs);
    }

    #[test]
    fn stwo_air_fixture_path_validates_binding_then_fails_closed() {
        let mut st = State::default();
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
        )
        .unwrap();
        let mut proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            timestamp_ms: block.header.timestamp_ms,
            parent_state_root: block.header.parent_state_root,
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            receipt_root: block.header.receipt_root,
            native_event_root: block.header.native_event_root,
            evm_log_root: block.header.evm_log_root,
            gas_used: block.header.gas_used,
            zone_namespace: block.header.zone_namespace,
            da_root: block.header.da_root,
            circuit_version: CircuitVersion::DevMixedV1,
            coverage_manifest_digest: coverage_manifest_digest(
                &coverage_manifest_for_circuit_version(CircuitVersion::DevMixedV1),
            )
            .unwrap(),
            feature_set: block.header.feature_set,
            proof_system: ValidityProofSystem::StwoPlonky2,
            proof_bytes: Vec::new(),
        };
        let adapter = stwo_execution_air_adapter_v1(&proof).unwrap();
        let fixture = canonical_recursive_proof_fixture_v1(&adapter, [3u8; 32]).unwrap();
        proof.proof_bytes = borsh::to_vec(&StwoPlonky2ProofEnvelope::StwoV1 {
            air_adapter: adapter,
            recursive_fixture: fixture,
            proof_bytes: vec![9],
        })
        .unwrap();

        assert!(matches!(
            verify_block_validity_proof(&block, &proof),
            Err(ProofVerifyError::Production(
                ProductionProofVerifyError::StwoAdapterUnavailable
            ))
        ));
    }

    #[test]
    fn stwo_air_fixture_rejects_wrong_public_inputs_before_verifier() {
        let mut st = State::default();
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
        )
        .unwrap();
        let mut proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            timestamp_ms: block.header.timestamp_ms,
            parent_state_root: block.header.parent_state_root,
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            receipt_root: block.header.receipt_root,
            native_event_root: block.header.native_event_root,
            evm_log_root: block.header.evm_log_root,
            gas_used: block.header.gas_used,
            zone_namespace: block.header.zone_namespace,
            da_root: block.header.da_root,
            circuit_version: CircuitVersion::DevMixedV1,
            coverage_manifest_digest: coverage_manifest_digest(
                &coverage_manifest_for_circuit_version(CircuitVersion::DevMixedV1),
            )
            .unwrap(),
            feature_set: block.header.feature_set,
            proof_system: ValidityProofSystem::StwoPlonky2,
            proof_bytes: Vec::new(),
        };
        let mut adapter = stwo_execution_air_adapter_v1(&proof).unwrap();
        adapter.public_input_digest = [4u8; 32];
        let fixture = canonical_recursive_proof_fixture_v1(&adapter, [3u8; 32]).unwrap();
        proof.proof_bytes = borsh::to_vec(&StwoPlonky2ProofEnvelope::StwoV1 {
            air_adapter: adapter,
            recursive_fixture: fixture,
            proof_bytes: vec![9],
        })
        .unwrap();

        assert!(matches!(
            verify_block_validity_proof(&block, &proof),
            Err(ProofVerifyError::Production(
                ProductionProofVerifyError::PublicInputDigest
            ))
        ));
    }

    #[test]
    #[cfg(feature = "dev-digest")]
    fn dev_digest_proof_rejects_wrong_state_root() {
        let mut st = State::default();
        let addr = [9u8; 20];
        st.accounts.insert(
            addr,
            Account {
                nonce: 0,
                balance: 1,
            },
        );
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
        )
        .unwrap();
        let proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            timestamp_ms: block.header.timestamp_ms,
            parent_state_root: block.header.parent_state_root,
            state_root: [9u8; 32],
            tx_root: block.header.tx_root,
            receipt_root: block.header.receipt_root,
            native_event_root: block.header.native_event_root,
            evm_log_root: block.header.evm_log_root,
            gas_used: block.header.gas_used,
            zone_namespace: block.header.zone_namespace,
            da_root: block.header.da_root,
            circuit_version: CircuitVersion::DevMixedV1,
            coverage_manifest_digest: coverage_manifest_digest(
                &coverage_manifest_for_circuit_version(CircuitVersion::DevMixedV1),
            )
            .unwrap(),
            feature_set: block.header.feature_set,
            proof_system: ValidityProofSystem::StwoPlonky2,
            proof_bytes: vec![1],
        };

        assert!(matches!(
            verify_block_validity_proof(&block, &proof),
            Err(ProofVerifyError::StateRoot)
        ));
    }

    #[test]
    fn block_header_commits_to_da_sidecar() {
        let mut st = State::default();
        let addr = [9u8; 20];
        st.accounts.insert(
            addr,
            Account {
                nonce: 0,
                balance: 1,
            },
        );
        let tx = Transaction {
            signer: addr,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            vec![tx],
            eth_signed_raws_for_txs(1),
        )
        .unwrap();

        verify_da_sidecar(&block.header, &block.da_sidecar).expect("da sidecar verifies");
        assert_eq!(block.header.zone_namespace, DEFAULT_DA_NAMESPACE);
        assert_eq!(block.da_sidecar.namespace, block.header.zone_namespace);
        assert_eq!(block.header.da_root, da_root(&block.da_sidecar));
        assert!(block.header.da_bytes > 0);
        assert!(block.header.da_share_count > 0);
        assert_eq!(
            block.header.da_gas_used,
            da_gas_for_sidecar(&block.da_sidecar)
        );
        assert_eq!(
            block.header.da_fee_paid,
            da_fee_for_gas(block.header.da_gas_used)
        );
    }

    #[test]
    fn da_sidecar_rejects_wrong_da_fee_accounting() {
        let mut st = State::default();
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
        )
        .unwrap();
        let mut bad = block.header.clone();
        bad.da_fee_paid = bad.da_fee_paid.saturating_add(1);

        assert!(verify_da_sidecar(&bad, &block.da_sidecar).is_err());
    }

    #[test]
    fn fee_policy_exposes_separate_cost_categories() {
        let policy = default_fee_policy();

        assert_eq!(
            FeePolicyV1::cost_categories().map(FeeCostCategory::as_str),
            ["da_bytes", "proof_verify", "shared_state_execution"]
        );
        assert_eq!(policy.da_bytes_fee(512), 512);
        assert_eq!(policy.proof_verify_fee(32), 10_032);
        assert_eq!(policy.shared_state_execution_fee(21_000), 21_000);
    }

    #[test]
    fn fee_policy_keeps_da_fee_separate_from_execution_gas() {
        let mut st = State::default();
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
        )
        .unwrap();
        let policy = default_fee_policy();

        assert_eq!(block.header.gas_used, 0);
        assert_eq!(
            block.header.da_fee_paid,
            policy.da_gas_fee(block.header.da_gas_used)
        );
        assert_eq!(policy.shared_state_execution_fee(block.header.gas_used), 0);
        assert!(policy.proof_verify_fee(32) > 0);
    }

    #[test]
    fn da_payload_reconstructs_canonical_transaction_bytes() {
        let tx = Transaction {
            signer: [1u8; 20],
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let payload = borsh::to_vec(&vec![tx]).unwrap();
        let sidecar =
            build_da_sidecar(&payload, DEFAULT_DA_NAMESPACE, 7).expect("DA sidecar fixture");

        let reconstructed = reconstruct_da_payload(&sidecar).expect("reconstruct");
        assert_eq!(reconstructed, payload);
    }

    #[test]
    fn zone_blob_da_is_independent_of_base_chain_transaction_list() {
        let tx = Transaction {
            signer: [1u8; 20],
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let tx_list_bytes = borsh::to_vec(&vec![tx]).unwrap();
        let blob_payload = b"zone proof input blob".to_vec();
        let blob = ZoneBlobDaV1 {
            namespace: *b"zone0001",
            payload: blob_payload.clone(),
            share_size: 8,
            sampling: DaSamplingParamsV1 {
                seed: 41,
                sample_count: 8,
                min_samples: 4,
            },
        };
        let (sidecar, commitment) = build_zone_blob_da_sidecar(&blob).unwrap();

        assert_ne!(blob_payload, tx_list_bytes);
        assert_eq!(commitment.namespace, *b"zone0001");
        assert_eq!(commitment.byte_count, blob_payload.len() as u64);
        assert_eq!(reconstruct_da_payload(&sidecar).unwrap(), blob_payload);
    }

    #[test]
    fn zone_blob_da_header_commitment_binds_sampling_params() {
        let blob = ZoneBlobDaV1 {
            namespace: *b"zone0001",
            payload: b"proof updates".to_vec(),
            share_size: 8,
            sampling: DaSamplingParamsV1 {
                seed: 99,
                sample_count: 8,
                min_samples: 4,
            },
        };
        let (sidecar, commitment) = build_zone_blob_da_sidecar(&blob).unwrap();
        let payload_root = BlockPayload::ProofUpdates(Vec::new())
            .payload_root()
            .unwrap();
        let mut header = BlockHeader {
            version: 1,
            chain_id: 41,
            height: 1,
            view: 0,
            parent_hash: [0u8; 32],
            parent_qc_hash: [0u8; 32],
            proposer: [0u8; 32],
            timestamp_ms: 1_000,
            parent_state_root: [0u8; 32],
            state_root: [0u8; 32],
            tx_root: [0u8; 32],
            receipt_root: [0u8; 32],
            native_event_root: [0u8; 32],
            evm_log_root: [0u8; 32],
            zone_namespace: commitment.namespace,
            da_root: commitment.da_root,
            da_bytes: commitment.byte_count,
            da_share_count: commitment.share_count,
            da_gas_used: da_gas_for_sidecar(&sidecar),
            da_fee_paid: da_fee_for_gas(da_gas_for_sidecar(&sidecar)),
            gas_used: 0,
            gas_limit: 60_000_000,
            feature_set: ExecutionFeatureSetV1::empty(),
            extra: proof_ingestion_header_extra(payload_root, &commitment).unwrap(),
        };

        verify_zone_blob_da_header(&header, &sidecar, &commitment, payload_root)
            .expect("zone blob header verifies");

        let mut tampered = commitment.clone();
        tampered.sampling.seed = tampered.sampling.seed.saturating_add(1);
        assert!(matches!(
            verify_zone_blob_da_header(&header, &sidecar, &tampered, payload_root),
            Err(DaVerifyError::Root)
        ));

        header.da_share_count = header.da_share_count.saturating_add(1);
        assert!(matches!(
            verify_zone_blob_da_header(&header, &sidecar, &commitment, payload_root),
            Err(DaVerifyError::ShareCount)
        ));
    }

    #[test]
    fn da_sidecar_adds_parity_shares() {
        let sidecar = build_da_sidecar(b"abcdefghijklmnopqrstuvwxyz", DEFAULT_DA_NAMESPACE, 7)
            .expect("DA sidecar fixture");

        assert_eq!(sidecar.data_share_count, 4);
        assert_eq!(sidecar.parity_share_count, 4);
        assert_eq!(sidecar.shares.len(), 8);
        assert!(sidecar
            .shares
            .iter()
            .take(sidecar.data_share_count as usize)
            .all(|s| !s.is_parity));
        assert!(sidecar
            .shares
            .iter()
            .skip(sidecar.data_share_count as usize)
            .all(|s| s.is_parity));
    }

    #[test]
    fn da_payload_reconstructs_with_missing_data_shares() {
        let payload = b"abcdefghijklmnopqrstuvwxyz0123456789";
        let sidecar =
            build_da_sidecar(payload, DEFAULT_DA_NAMESPACE, 8).expect("DA sidecar fixture");
        let available = sidecar
            .shares
            .iter()
            .filter(|s| s.index != 0 && s.index != 2)
            .cloned();

        let reconstructed =
            reconstruct_da_payload_from_shares(&sidecar, available).expect("reconstruct");
        assert_eq!(reconstructed, payload);
    }

    #[test]
    fn da_payload_rejects_insufficient_reconstruction_shares() {
        let payload = b"abcdefghijklmnopqrstuvwxyz0123456789";
        let sidecar =
            build_da_sidecar(payload, DEFAULT_DA_NAMESPACE, 8).expect("DA sidecar fixture");
        let available = sidecar
            .shares
            .iter()
            .take((sidecar.data_share_count - 1) as usize)
            .cloned();

        assert!(matches!(
            reconstruct_da_payload_from_shares(&sidecar, available),
            Err(DaVerifyError::InsufficientShares)
        ));
    }

    #[test]
    fn da_sampling_rejects_tampered_share() {
        let payload = b"abcdefghijklmnopqrstuvwxyz";
        let mut sidecar =
            build_da_sidecar(payload, DEFAULT_DA_NAMESPACE, 64).expect("DA sidecar fixture");
        let root = da_root(&sidecar);
        sidecar.shares[0].data[0] ^= 0xff;

        assert!(matches!(
            verify_da_samples(&sidecar, root, DEFAULT_DA_NAMESPACE, 41, 8),
            Err(DaVerifyError::Root) | Err(DaVerifyError::ShareCommitment)
        ));
    }

    #[test]
    fn da_sampling_rejects_tampered_parity_share() {
        let payload = b"abcdefghijklmnopqrstuvwxyz";
        let mut sidecar =
            build_da_sidecar(payload, DEFAULT_DA_NAMESPACE, 8).expect("DA sidecar fixture");
        let root = da_root(&sidecar);
        let parity_idx = sidecar.data_share_count as usize;
        sidecar.shares[parity_idx].data[0] ^= 0xff;

        assert!(matches!(
            verify_da_samples(&sidecar, root, DEFAULT_DA_NAMESPACE, 41, 16),
            Err(DaVerifyError::ShareCommitment)
        ));
    }

    #[test]
    fn zone_block_uses_supplied_da_namespace() {
        let mut st = State::default();
        let namespace = *b"zone0001";
        let block = execute_and_build_zone_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
            namespace,
        )
        .unwrap();

        assert_eq!(block.header.zone_namespace, namespace);
        assert_eq!(block.da_sidecar.namespace, namespace);
        verify_da_sidecar(&block.header, &block.da_sidecar).expect("zone da sidecar verifies");
    }

    #[test]
    fn da_sidecar_rejects_namespace_mismatch() {
        let mut st = State::default();
        let mut block = execute_and_build_zone_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
            *b"zone0001",
        )
        .unwrap();
        block.da_sidecar.namespace = *b"zone0002";

        assert!(matches!(
            verify_da_sidecar(&block.header, &block.da_sidecar),
            Err(DaVerifyError::Namespace)
        ));
    }

    #[test]
    fn da_sampling_rejects_namespace_mismatch() {
        let payload = b"abcdefghijklmnopqrstuvwxyz";
        let sidecar = build_da_sidecar(payload, *b"zone0001", 64).expect("DA sidecar fixture");
        let root = da_root(&sidecar);

        assert!(matches!(
            verify_da_samples(&sidecar, root, *b"zone0002", 41, 8),
            Err(DaVerifyError::Namespace)
        ));
    }

    #[test]
    fn da_sampling_receipt_verifies_without_reconstruction() {
        let payload = b"abcdefghijklmnopqrstuvwxyz0123456789";
        let sidecar =
            build_da_sidecar(payload, DEFAULT_DA_NAMESPACE, 8).expect("DA sidecar fixture");
        let root = da_root(&sidecar);
        let receipt = build_da_sampling_receipt(&sidecar, root, 41, 8).expect("sampling receipt");

        verify_da_sampling_receipt(&receipt, root, DEFAULT_DA_NAMESPACE, 4)
            .expect("receipt verifies");
    }

    #[test]
    fn da_sampling_receipt_rejects_tampered_index() {
        let payload = b"abcdefghijklmnopqrstuvwxyz0123456789";
        let sidecar =
            build_da_sidecar(payload, DEFAULT_DA_NAMESPACE, 8).expect("DA sidecar fixture");
        let root = da_root(&sidecar);
        let mut receipt =
            build_da_sampling_receipt(&sidecar, root, 41, 8).expect("sampling receipt");
        receipt.samples[0].index = receipt.samples[0].index.saturating_add(1);

        assert!(matches!(
            verify_da_sampling_receipt(&receipt, root, DEFAULT_DA_NAMESPACE, 4),
            Err(DaVerifyError::ShareIndex)
        ));
    }

    #[test]
    fn da_sampling_receipt_rejects_tampered_commitment() {
        let payload = b"abcdefghijklmnopqrstuvwxyz0123456789";
        let sidecar =
            build_da_sidecar(payload, DEFAULT_DA_NAMESPACE, 8).expect("DA sidecar fixture");
        let root = da_root(&sidecar);
        let mut receipt =
            build_da_sampling_receipt(&sidecar, root, 41, 8).expect("sampling receipt");
        receipt.samples[0].commitment[0] ^= 0xff;

        assert!(matches!(
            verify_da_sampling_receipt(&receipt, root, DEFAULT_DA_NAMESPACE, 4),
            Err(DaVerifyError::ShareCommitment)
        ));
    }

    #[test]
    fn da_sampling_receipt_rejects_insufficient_samples() {
        let payload = b"abcdefghijklmnopqrstuvwxyz0123456789";
        let sidecar =
            build_da_sidecar(payload, DEFAULT_DA_NAMESPACE, 8).expect("DA sidecar fixture");
        let root = da_root(&sidecar);
        let receipt = build_da_sampling_receipt(&sidecar, root, 41, 2).expect("sampling receipt");

        assert!(matches!(
            verify_da_sampling_receipt(&receipt, root, DEFAULT_DA_NAMESPACE, 4),
            Err(DaVerifyError::InsufficientSamples)
        ));
    }

    #[test]
    fn da_sampling_receipt_does_not_replace_full_reconstruction() {
        let payload = b"abcdefghijklmnopqrstuvwxyz0123456789";
        let sidecar =
            build_da_sidecar(payload, DEFAULT_DA_NAMESPACE, 8).expect("DA sidecar fixture");
        let root = da_root(&sidecar);
        let receipt = build_da_sampling_receipt(&sidecar, root, 41, 4).expect("sampling receipt");

        verify_da_sampling_receipt(&receipt, root, DEFAULT_DA_NAMESPACE, 4)
            .expect("receipt verifies");
        assert_eq!(
            reconstruct_da_payload(&sidecar).expect("full reconstruction still works"),
            payload
        );
    }

    #[test]
    #[cfg(feature = "dev-digest")]
    fn proof_rejects_wrong_da_root() {
        let mut st = State::default();
        let addr = [9u8; 20];
        st.accounts.insert(
            addr,
            Account {
                nonce: 0,
                balance: 1,
            },
        );
        let block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
        )
        .unwrap();
        let proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            timestamp_ms: block.header.timestamp_ms,
            parent_state_root: block.header.parent_state_root,
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            receipt_root: block.header.receipt_root,
            native_event_root: block.header.native_event_root,
            evm_log_root: block.header.evm_log_root,
            gas_used: block.header.gas_used,
            zone_namespace: block.header.zone_namespace,
            da_root: [9u8; 32],
            circuit_version: CircuitVersion::DevMixedV1,
            coverage_manifest_digest: coverage_manifest_digest(
                &coverage_manifest_for_circuit_version(CircuitVersion::DevMixedV1),
            )
            .unwrap(),
            feature_set: block.header.feature_set,
            proof_system: ValidityProofSystem::StwoPlonky2,
            proof_bytes: vec![1],
        };

        assert!(matches!(
            verify_block_validity_proof(&block, &proof),
            Err(ProofVerifyError::DaRoot)
        ));
    }

    #[test]
    #[cfg(feature = "dev-digest")]
    fn proof_rejects_tampered_zone_blob_da_sidecar() {
        let mut st = State::default();
        let mut block = execute_and_build_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
        )
        .unwrap();
        let payload_root = BlockPayload::ProofUpdates(Vec::new())
            .payload_root()
            .unwrap();
        let blob = ZoneBlobDaV1 {
            namespace: block.header.zone_namespace,
            payload: b"zone proof blob".to_vec(),
            share_size: 8,
            sampling: DaSamplingParamsV1 {
                seed: 41,
                sample_count: 8,
                min_samples: 4,
            },
        };
        let (sidecar, commitment) = build_zone_blob_da_sidecar(&blob).unwrap();
        block.header.da_root = commitment.da_root;
        block.header.da_bytes = commitment.byte_count;
        block.header.da_share_count = commitment.share_count;
        block.header.da_gas_used = da_gas_for_sidecar(&sidecar);
        block.header.da_fee_paid = da_fee_for_gas(block.header.da_gas_used);
        block.header.extra = proof_ingestion_header_extra(payload_root, &commitment).unwrap();
        block.da_sidecar = sidecar;
        let proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            timestamp_ms: block.header.timestamp_ms,
            parent_state_root: block.header.parent_state_root,
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            receipt_root: block.header.receipt_root,
            native_event_root: block.header.native_event_root,
            evm_log_root: block.header.evm_log_root,
            gas_used: block.header.gas_used,
            zone_namespace: block.header.zone_namespace,
            da_root: block.header.da_root,
            circuit_version: CircuitVersion::DevMixedV1,
            coverage_manifest_digest: coverage_manifest_digest(
                &coverage_manifest_for_circuit_version(CircuitVersion::DevMixedV1),
            )
            .unwrap(),
            feature_set: block.header.feature_set,
            proof_system: ValidityProofSystem::StwoPlonky2,
            proof_bytes: vec![1],
        };
        block.da_sidecar.shares[0].data[0] ^= 0xff;

        assert!(matches!(
            verify_block_validity_proof(&block, &proof),
            Err(ProofVerifyError::DataAvailability)
        ));
    }

    #[test]
    #[cfg(feature = "dev-digest")]
    fn proof_rejects_wrong_zone_namespace() {
        let mut st = State::default();
        let block = execute_and_build_zone_block(
            41,
            1,
            0,
            [7u8; 32],
            [0u8; 32],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            Vec::new(),
            eth_signed_raws_for_txs(0),
            *b"zone0001",
        )
        .unwrap();
        let proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            timestamp_ms: block.header.timestamp_ms,
            parent_state_root: block.header.parent_state_root,
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            receipt_root: block.header.receipt_root,
            native_event_root: block.header.native_event_root,
            evm_log_root: block.header.evm_log_root,
            gas_used: block.header.gas_used,
            zone_namespace: *b"zone0002",
            da_root: block.header.da_root,
            circuit_version: CircuitVersion::DevMixedV1,
            coverage_manifest_digest: coverage_manifest_digest(
                &coverage_manifest_for_circuit_version(CircuitVersion::DevMixedV1),
            )
            .unwrap(),
            feature_set: block.header.feature_set,
            proof_system: ValidityProofSystem::StwoPlonky2,
            proof_bytes: vec![1],
        };

        assert!(matches!(
            verify_block_validity_proof(&block, &proof),
            Err(ProofVerifyError::ZoneNamespace)
        ));
    }
}
