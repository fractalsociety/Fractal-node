//! Execution shard routing + masterchain coordination types (`docs/prd.md` §6, §7.10, M10).
//!
//! Track A (monolith): `shard_count == 1` and `shard_id == 0` — all txs accepted, headers tag shard 0.
//! Track B: `FRACTAL_SHARD_COUNT` > 1 and each node runs one `FRACTAL_SHARD_ID`.

use borsh::{BorshDeserialize, BorshSerialize};
use fractal_consensus::{
    coverage_manifest_digest, coverage_manifest_for_circuit_version, CircuitVersion,
    ExecutionFeatureSetV1,
};
use fractal_core::{
    forced_inclusion_penalty_wei, sequencer_reward_wei, SequencerRewardParams, SequencerWorkReceipt,
};
use fractal_core::{merkle_proof, merkle_root, verify_merkle_proof};
use fractal_crypto::hash::{keccak256, Hash256};
use std::collections::BTreeMap;
use thiserror::Error;

/// Logical execution shard (0 .. shard_count-1).
pub type ShardId = u32;

/// Default shard for the monolithic testnet (Track A).
pub const DEFAULT_SHARD_ID: ShardId = 0;

/// PRD design default before multi-process shard fleet is deployed.
pub const DEFAULT_SHARD_COUNT: u32 = 10;

/// Env: number of execution shards (`1` = monolith only).
pub const ENV_SHARD_COUNT: &str = "FRACTAL_SHARD_COUNT";

/// Env: this process serves shard `N` (clamped to `shard_count - 1`).
pub const ENV_SHARD_ID: &str = "FRACTAL_SHARD_ID";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShardTopology {
    pub shard_count: u32,
}

impl ShardTopology {
    /// `shard_count` from [`ENV_SHARD_COUNT`], default **1** (monolith).
    #[must_use]
    pub fn from_env() -> Self {
        let shard_count = std::env::var(ENV_SHARD_COUNT)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .filter(|&n| n >= 1)
            .unwrap_or(1);
        Self { shard_count }
    }

    /// This node's shard from [`ENV_SHARD_ID`], default **0**, clamped to valid range.
    #[must_use]
    pub fn node_shard_id_from_env(&self) -> ShardId {
        let raw = std::env::var(ENV_SHARD_ID)
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .unwrap_or(DEFAULT_SHARD_ID);
        raw.min(self.shard_count.saturating_sub(1))
    }

    #[must_use]
    pub fn is_monolith(&self) -> bool {
        self.shard_count <= 1
    }
}

#[derive(Debug, Error)]
pub enum ShardRoutingError {
    #[error("shard_id {shard_id} >= shard_count {shard_count}")]
    InvalidShardId { shard_id: ShardId, shard_count: u32 },
    #[error("transaction home shard {home} does not match node shard {node}")]
    WrongShard { home: ShardId, node: ShardId },
}

/// `keccak256(signer)[0..4] mod shard_count` — deterministic home shard for an account.
#[must_use]
pub fn home_shard_for_address(signer: &[u8; 20], shard_count: u32) -> ShardId {
    home_shard_for_bytes(signer, shard_count)
}

/// Agent / capability id (32 bytes) → home shard.
#[must_use]
pub fn home_shard_for_agent_id(agent_id: &[u8; 32], shard_count: u32) -> ShardId {
    home_shard_for_bytes(agent_id, shard_count)
}

#[inline]
fn home_shard_for_bytes(key: &[u8], shard_count: u32) -> ShardId {
    if shard_count <= 1 {
        return DEFAULT_SHARD_ID;
    }
    let h = keccak256(key);
    let n = u32::from_be_bytes([h[0], h[1], h[2], h[3]]);
    n % shard_count
}

/// Home shard for a transaction (signer address).
#[must_use]
pub fn home_shard_for_signer(signer: &[u8; 20], shard_count: u32) -> ShardId {
    home_shard_for_address(signer, shard_count)
}

/// Whether this node should accept a tx for its mempool.
#[must_use]
pub fn accepts_transaction(
    signer: &[u8; 20],
    node_shard_id: ShardId,
    topology: &ShardTopology,
) -> bool {
    if topology.is_monolith() {
        return node_shard_id == DEFAULT_SHARD_ID;
    }
    home_shard_for_signer(signer, topology.shard_count) == node_shard_id
}

/// Validate block header shard tag vs this node.
pub fn validate_block_shard(
    header_shard_id: ShardId,
    node_shard_id: ShardId,
    topology: &ShardTopology,
) -> Result<(), ShardRoutingError> {
    if header_shard_id >= topology.shard_count {
        return Err(ShardRoutingError::InvalidShardId {
            shard_id: header_shard_id,
            shard_count: topology.shard_count,
        });
    }
    if topology.is_monolith() {
        if header_shard_id != DEFAULT_SHARD_ID {
            return Err(ShardRoutingError::InvalidShardId {
                shard_id: header_shard_id,
                shard_count: 1,
            });
        }
        return Ok(());
    }
    if header_shard_id != node_shard_id {
        return Err(ShardRoutingError::WrongShard {
            home: header_shard_id,
            node: node_shard_id,
        });
    }
    Ok(())
}

/// Reject txs routed to another shard (for RPC error strings).
pub fn check_accepts_transaction(
    signer: &[u8; 20],
    node_shard_id: ShardId,
    topology: &ShardTopology,
) -> Result<(), ShardRoutingError> {
    if accepts_transaction(signer, node_shard_id, topology) {
        return Ok(());
    }
    let home = home_shard_for_signer(signer, topology.shard_count);
    Err(ShardRoutingError::WrongShard {
        home,
        node: node_shard_id,
    })
}

// --- Masterchain wire types (Track B; not yet executed on-chain) ---

/// Default shard blocks between masterchain anchors (`docs/prd.md` §7.10.2).
pub const DEFAULT_ANCHOR_INTERVAL: u64 = 100;

/// Shard state anchor posted to masterchain.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ShardAnchor {
    pub shard_id: ShardId,
    pub block_height: u64,
    pub state_root: Hash256,
    pub witness_commitment: Hash256,
}

/// Tier-1 STWO proof metadata (full proof bytes stored off-chain / in submission).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ProofSubmissionV1 {
    pub shard_id: ShardId,
    pub start_block: u64,
    pub end_block: u64,
    pub prover: [u8; 20],
    pub lag_seconds: u32,
    /// Digest of STWO artifact or placeholder until wired.
    pub proof_digest: Hash256,
}

/// Masterchain block body sketch (`docs/prd.md` §7.10.3).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct MasterchainBlockV1 {
    pub height: u64,
    pub shard_anchors: Vec<ShardAnchor>,
    pub validity_proofs: Vec<ProofSubmissionV1>,
    pub zone_proof_commitments: Vec<ZoneProofCommitmentV1>,
    pub global_state_root: Hash256,
    pub global_zk_root: Hash256,
    pub cross_shard_messages: Vec<CrossShardMessageV1>,
}

/// Cross-shard agent message routed at anchor cadence.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CrossShardMessageV1 {
    pub from_shard: ShardId,
    pub to_shard: ShardId,
    pub payload_hash: Hash256,
    /// Opaque destination payload. Shard nodes currently interpret this as `borsh(NativeCall)`.
    pub payload: Vec<u8>,
}

pub type ZoneId = u64;

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecutionZoneMetadataV1 {
    pub version: u8,
    pub proof_system: u8,
    pub da_namespace: [u8; 8],
    pub sequencer_policy: u8,
    pub forced_inclusion_timeout_masterchain_blocks: u64,
}

impl ExecutionZoneMetadataV1 {
    pub const VERSION: u8 = 1;
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ExecutionZoneRecordV1 {
    pub version: u8,
    pub zone_id: ZoneId,
    pub creator: [u8; 20],
    pub metadata: ExecutionZoneMetadataV1,
    pub created_at_masterchain_height: u64,
    pub latest_proof_final_height: u64,
    pub latest_state_root: Hash256,
    pub latest_message_root: Hash256,
}

impl ExecutionZoneRecordV1 {
    pub const VERSION: u8 = 1;
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ZoneProofFinalUpdateV1 {
    pub zone_id: ZoneId,
    pub zone_block_height: u64,
    pub zone_block_hash: Hash256,
    pub state_root: Hash256,
    pub message_root: Hash256,
    pub tx_root: Hash256,
    pub da_root: Hash256,
    pub da_namespace: [u8; 8],
    pub forced_inclusion_root: Hash256,
    pub timestamp_ms: u64,
    pub circuit_version: CircuitVersion,
    pub coverage_manifest_digest: Hash256,
    pub covered_features: ExecutionFeatureSetV1,
    pub feature_set: ExecutionFeatureSetV1,
    pub public_input_digest: Hash256,
    pub source_message_root: Hash256,
    pub required_forced_inclusion_root: Hash256,
    pub da_available: bool,
    pub proof_digest: Hash256,
    pub prover: [u8; 20],
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ZoneBlockHeaderV1 {
    pub version: u8,
    pub zone_id: ZoneId,
    pub height: u64,
    pub parent_zone_block_hash: Hash256,
    pub state_root: Hash256,
    pub message_root: Hash256,
    pub tx_root: Hash256,
    pub da_namespace: [u8; 8],
    pub da_root: Hash256,
    pub forced_inclusion_root: Hash256,
    pub timestamp_ms: u64,
    pub sequencer: [u8; 20],
}

impl ZoneBlockHeaderV1 {
    pub const VERSION: u8 = 1;
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ZoneProofCommitmentV1 {
    pub zone_id: ZoneId,
    pub zone_block_height: u64,
    pub zone_block_hash: Hash256,
    pub state_root: Hash256,
    pub message_root: Hash256,
    pub circuit_version: CircuitVersion,
    pub coverage_manifest_digest: Hash256,
    pub public_input_digest: Hash256,
    pub proof_digest: Hash256,
    pub prover: [u8; 20],
}

impl From<&ZoneProofFinalUpdateV1> for ZoneProofCommitmentV1 {
    fn from(update: &ZoneProofFinalUpdateV1) -> Self {
        Self {
            zone_id: update.zone_id,
            zone_block_height: update.zone_block_height,
            zone_block_hash: update.zone_block_hash,
            state_root: update.state_root,
            message_root: update.message_root,
            circuit_version: update.circuit_version,
            coverage_manifest_digest: update.coverage_manifest_digest,
            public_input_digest: update.public_input_digest,
            proof_digest: update.proof_digest,
            prover: update.prover,
        }
    }
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct AsyncCrossZoneMessageV1 {
    pub from_zone: ZoneId,
    pub to_zone: ZoneId,
    pub nonce: u64,
    pub payload_hash: Hash256,
    pub payload: Vec<u8>,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct CrossZoneMessageInclusionProofV1 {
    pub version: u8,
    pub source_zone: ZoneId,
    pub source_zone_block_height: u64,
    pub message_index: u64,
    pub message: AsyncCrossZoneMessageV1,
    pub message_root: Hash256,
    pub proof_path: Vec<Hash256>,
}

impl CrossZoneMessageInclusionProofV1 {
    pub const VERSION: u8 = 1;
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ForcedInclusionRequestV1 {
    pub zone_id: ZoneId,
    pub requester: [u8; 20],
    pub request_id: Hash256,
    pub tx_hash: Hash256,
    pub payload: Vec<u8>,
    pub submitted_at_masterchain_height: u64,
    pub deadline_masterchain_height: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ForcedInclusionEventV1 {
    pub version: u8,
    pub request: ForcedInclusionRequestV1,
    pub included_at_masterchain_height: u64,
    pub sequencer_late_by_blocks: u64,
}

impl ForcedInclusionEventV1 {
    pub const VERSION: u8 = 1;
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct SequencerEpochWorkV1 {
    pub zone_id: ZoneId,
    pub sequencer: [u8; 20],
    pub zone_blocks: u64,
    pub da_bytes: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct SequencerEpochSettlementV1 {
    pub version: u8,
    pub zone_id: ZoneId,
    pub sequencer: [u8; 20],
    pub reward_wei: u128,
    pub forced_inclusion_penalty_wei: u128,
    pub net_reward_wei: u128,
    pub unpaid_penalty_wei: u128,
    pub forced_inclusion_count: u64,
}

impl SequencerEpochSettlementV1 {
    pub const VERSION: u8 = 1;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ExecutionZoneError {
    #[error("execution zone already exists")]
    ZoneAlreadyExists,
    #[error("execution zone {0} is unknown")]
    UnknownZone(ZoneId),
    #[error("execution zone update height {height} is not newer than current proof-final height {current}")]
    StaleZoneUpdate { height: u64, current: u64 },
    #[error("execution zone proof digest is empty")]
    EmptyZoneProofDigest,
    #[error("forced-inclusion timeout/SLA must be greater than zero")]
    InvalidForcedInclusionTimeout,
    #[error("forced inclusion request already exists")]
    ForcedInclusionAlreadyExists,
    #[error("cross-zone message proof index is out of range")]
    MessageProofIndexOutOfRange,
    #[error("zone proof commitment is missing from masterchain block")]
    ZoneProofNotCommitted,
    #[error("masterchain global zk root does not match zone proof commitments")]
    MasterchainZkRootMismatch,
    #[error("zone proof circuit coverage manifest mismatch")]
    ZoneCoverageManifest,
    #[error("zone proof circuit does not cover zone block feature set")]
    ZoneCircuitCoverage,
    #[error("zone proof DA is unavailable")]
    ZoneDaUnavailable,
    #[error("zone proof public inputs do not bind to zone header")]
    ZonePublicInputs,
    #[error("zone proof forced-inclusion root is not proven")]
    ForcedInclusionRootMismatch,
    #[error("cross-zone source message root is not proven")]
    SourceMessageRootMismatch,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExecutionZoneRegistryV1 {
    pub masterchain_height: u64,
    pub zones: BTreeMap<ZoneId, ExecutionZoneRecordV1>,
    pub pending_cross_zone_messages: Vec<AsyncCrossZoneMessageV1>,
    pub pending_forced_inclusions: Vec<ForcedInclusionRequestV1>,
    pub forced_inclusion_events: Vec<ForcedInclusionEventV1>,
}

impl ExecutionZoneRegistryV1 {
    pub fn create_zone(
        &mut self,
        zone_id: ZoneId,
        creator: [u8; 20],
        metadata: ExecutionZoneMetadataV1,
    ) -> Result<ExecutionZoneRecordV1, ExecutionZoneError> {
        if metadata.forced_inclusion_timeout_masterchain_blocks == 0 {
            return Err(ExecutionZoneError::InvalidForcedInclusionTimeout);
        }
        if self.zones.contains_key(&zone_id) {
            return Err(ExecutionZoneError::ZoneAlreadyExists);
        }
        let record = ExecutionZoneRecordV1 {
            version: ExecutionZoneRecordV1::VERSION,
            zone_id,
            creator,
            metadata,
            created_at_masterchain_height: self.masterchain_height,
            latest_proof_final_height: 0,
            latest_state_root: [0u8; 32],
            latest_message_root: [0u8; 32],
        };
        self.zones.insert(zone_id, record.clone());
        Ok(record)
    }

    #[must_use]
    pub fn zone(&self, zone_id: ZoneId) -> Option<&ExecutionZoneRecordV1> {
        self.zones.get(&zone_id)
    }

    pub fn submit_proof_final_update(
        &mut self,
        update: ZoneProofFinalUpdateV1,
    ) -> Result<ExecutionZoneRecordV1, ExecutionZoneError> {
        if update.proof_digest == [0u8; 32] {
            return Err(ExecutionZoneError::EmptyZoneProofDigest);
        }
        let zone = self
            .zones
            .get_mut(&update.zone_id)
            .ok_or(ExecutionZoneError::UnknownZone(update.zone_id))?;
        verify_zone_update_coverage(&update, &zone.metadata)?;
        verify_zone_update_public_inputs(&update)?;
        if update.zone_block_height <= zone.latest_proof_final_height {
            return Err(ExecutionZoneError::StaleZoneUpdate {
                height: update.zone_block_height,
                current: zone.latest_proof_final_height,
            });
        }
        zone.latest_proof_final_height = update.zone_block_height;
        zone.latest_state_root = update.state_root;
        zone.latest_message_root = update.message_root;
        Ok(zone.clone())
    }

    pub fn submit_verified_proof_final_update(
        &mut self,
        update: ZoneProofFinalUpdateV1,
        masterchain_block: &MasterchainBlockV1,
    ) -> Result<ExecutionZoneRecordV1, ExecutionZoneError> {
        verify_zone_proof_update_against_masterchain(&update, masterchain_block)?;
        self.submit_proof_final_update(update)
    }

    pub fn submit_cross_zone_message(
        &mut self,
        msg: AsyncCrossZoneMessageV1,
    ) -> Result<(), ExecutionZoneError> {
        if !self.zones.contains_key(&msg.from_zone) {
            return Err(ExecutionZoneError::UnknownZone(msg.from_zone));
        }
        if !self.zones.contains_key(&msg.to_zone) {
            return Err(ExecutionZoneError::UnknownZone(msg.to_zone));
        }
        self.pending_cross_zone_messages.push(msg);
        Ok(())
    }

    pub fn drain_cross_zone_messages_for(
        &mut self,
        to_zone: ZoneId,
    ) -> Result<Vec<AsyncCrossZoneMessageV1>, ExecutionZoneError> {
        if !self.zones.contains_key(&to_zone) {
            return Err(ExecutionZoneError::UnknownZone(to_zone));
        }
        self.pending_cross_zone_messages.sort();
        self.pending_cross_zone_messages.dedup();
        let mut delivered = Vec::new();
        let mut pending = Vec::new();
        for msg in self.pending_cross_zone_messages.drain(..) {
            if msg.to_zone == to_zone {
                delivered.push(msg);
            } else {
                pending.push(msg);
            }
        }
        self.pending_cross_zone_messages = pending;
        Ok(delivered)
    }

    pub fn submit_forced_inclusion(
        &mut self,
        zone_id: ZoneId,
        requester: [u8; 20],
        tx_hash: Hash256,
        payload: Vec<u8>,
    ) -> Result<ForcedInclusionRequestV1, ExecutionZoneError> {
        let zone = self
            .zones
            .get(&zone_id)
            .ok_or(ExecutionZoneError::UnknownZone(zone_id))?;
        let request_id =
            forced_inclusion_request_id(zone_id, &requester, &tx_hash, self.masterchain_height);
        if self
            .pending_forced_inclusions
            .iter()
            .chain(
                self.forced_inclusion_events
                    .iter()
                    .map(|event| &event.request),
            )
            .any(|request| request.request_id == request_id)
        {
            return Err(ExecutionZoneError::ForcedInclusionAlreadyExists);
        }
        let request = ForcedInclusionRequestV1 {
            zone_id,
            requester,
            request_id,
            tx_hash,
            payload,
            submitted_at_masterchain_height: self.masterchain_height,
            deadline_masterchain_height: self
                .masterchain_height
                .saturating_add(zone.metadata.forced_inclusion_timeout_masterchain_blocks),
        };
        self.pending_forced_inclusions.push(request.clone());
        Ok(request)
    }

    pub fn advance_masterchain_height(&mut self, height: u64) {
        self.masterchain_height = self.masterchain_height.max(height);
        self.flush_due_forced_inclusions();
    }

    #[must_use]
    pub fn settle_sequencer_epoch(
        &self,
        params: &SequencerRewardParams,
        work: SequencerEpochWorkV1,
    ) -> SequencerEpochSettlementV1 {
        let reward_wei = sequencer_reward_wei(
            params,
            SequencerWorkReceipt {
                zone_blocks: work.zone_blocks,
                da_bytes: work.da_bytes,
            },
        );
        let mut forced_inclusion_count = 0u64;
        let total_forced_inclusion_penalty_wei = self
            .forced_inclusion_events
            .iter()
            .filter(|event| event.request.zone_id == work.zone_id)
            .map(|event| {
                forced_inclusion_count = forced_inclusion_count.saturating_add(1);
                forced_inclusion_penalty_wei(params, event.sequencer_late_by_blocks)
            })
            .fold(0u128, u128::saturating_add);
        let net_reward_wei = reward_wei.saturating_sub(total_forced_inclusion_penalty_wei);
        let unpaid_penalty_wei = total_forced_inclusion_penalty_wei.saturating_sub(reward_wei);
        SequencerEpochSettlementV1 {
            version: SequencerEpochSettlementV1::VERSION,
            zone_id: work.zone_id,
            sequencer: work.sequencer,
            reward_wei,
            forced_inclusion_penalty_wei: total_forced_inclusion_penalty_wei,
            net_reward_wei,
            unpaid_penalty_wei,
            forced_inclusion_count,
        }
    }

    fn flush_due_forced_inclusions(&mut self) {
        let mut pending = Vec::new();
        for request in self.pending_forced_inclusions.drain(..) {
            if request.deadline_masterchain_height <= self.masterchain_height {
                self.forced_inclusion_events.push(ForcedInclusionEventV1 {
                    version: ForcedInclusionEventV1::VERSION,
                    sequencer_late_by_blocks: self
                        .masterchain_height
                        .saturating_sub(request.deadline_masterchain_height),
                    included_at_masterchain_height: self.masterchain_height,
                    request,
                });
            } else {
                pending.push(request);
            }
        }
        self.pending_forced_inclusions = pending;
    }
}

#[must_use]
pub fn forced_inclusion_request_id(
    zone_id: ZoneId,
    requester: &[u8; 20],
    tx_hash: &Hash256,
    submitted_at_masterchain_height: u64,
) -> Hash256 {
    #[derive(BorshSerialize)]
    struct RequestId<'a> {
        tag: [u8; 16],
        zone_id: ZoneId,
        requester: &'a [u8; 20],
        tx_hash: &'a Hash256,
        submitted_at_masterchain_height: u64,
    }
    let bytes = borsh::to_vec(&RequestId {
        tag: *b"FRAC_FORCE_INC__",
        zone_id,
        requester,
        tx_hash,
        submitted_at_masterchain_height,
    })
    .expect("forced inclusion request id borsh");
    keccak256(&bytes)
}

#[must_use]
pub fn cross_zone_message_leaf_hash(message: &AsyncCrossZoneMessageV1) -> Hash256 {
    #[derive(BorshSerialize)]
    struct MessageLeaf<'a> {
        tag: [u8; 16],
        message: &'a AsyncCrossZoneMessageV1,
    }
    let bytes = borsh::to_vec(&MessageLeaf {
        tag: *b"FRAC_XZONE_MSG__",
        message,
    })
    .expect("cross-zone message leaf borsh");
    keccak256(&bytes)
}

#[must_use]
pub fn zone_block_header_hash(header: &ZoneBlockHeaderV1) -> Hash256 {
    #[derive(BorshSerialize)]
    struct ZoneHeaderCommitment<'a> {
        tag: [u8; 16],
        header: &'a ZoneBlockHeaderV1,
    }
    let bytes = borsh::to_vec(&ZoneHeaderCommitment {
        tag: *b"FRAC_ZONE_HDR___",
        header,
    })
    .expect("zone block header commitment borsh");
    keccak256(&bytes)
}

#[must_use]
pub fn zone_proof_final_update_from_header(
    header: &ZoneBlockHeaderV1,
    proof_digest: Hash256,
    prover: [u8; 20],
) -> ZoneProofFinalUpdateV1 {
    zone_proof_final_update_from_header_with_circuit(
        header,
        CircuitVersion::NativeStateTransitionV1,
        ExecutionFeatureSetV1 {
            bits: fractal_consensus::FEATURE_NATIVE_TX
                | fractal_consensus::FEATURE_NATIVE_SHARED_STATE,
        },
        true,
        proof_digest,
        prover,
    )
}

#[must_use]
pub fn zone_proof_public_input_digest(update: &ZoneProofFinalUpdateV1) -> Hash256 {
    #[derive(BorshSerialize)]
    struct ZoneProofPublicInputs<'a> {
        tag: [u8; 16],
        zone_id: ZoneId,
        zone_block_height: u64,
        zone_block_hash: &'a Hash256,
        state_root: &'a Hash256,
        message_root: &'a Hash256,
        tx_root: &'a Hash256,
        da_root: &'a Hash256,
        da_namespace: &'a [u8; 8],
        forced_inclusion_root: &'a Hash256,
        timestamp_ms: u64,
        circuit_version: CircuitVersion,
        coverage_manifest_digest: &'a Hash256,
        feature_set: ExecutionFeatureSetV1,
        source_message_root: &'a Hash256,
        required_forced_inclusion_root: &'a Hash256,
    }
    let bytes = borsh::to_vec(&ZoneProofPublicInputs {
        tag: *b"FRAC_ZONE_PI____",
        zone_id: update.zone_id,
        zone_block_height: update.zone_block_height,
        zone_block_hash: &update.zone_block_hash,
        state_root: &update.state_root,
        message_root: &update.message_root,
        tx_root: &update.tx_root,
        da_root: &update.da_root,
        da_namespace: &update.da_namespace,
        forced_inclusion_root: &update.forced_inclusion_root,
        timestamp_ms: update.timestamp_ms,
        circuit_version: update.circuit_version,
        coverage_manifest_digest: &update.coverage_manifest_digest,
        feature_set: update.feature_set,
        source_message_root: &update.source_message_root,
        required_forced_inclusion_root: &update.required_forced_inclusion_root,
    })
    .expect("zone proof public inputs borsh");
    keccak256(&bytes)
}

#[must_use]
pub fn zone_proof_final_update_from_header_with_circuit(
    header: &ZoneBlockHeaderV1,
    circuit_version: CircuitVersion,
    feature_set: ExecutionFeatureSetV1,
    da_available: bool,
    proof_digest: Hash256,
    prover: [u8; 20],
) -> ZoneProofFinalUpdateV1 {
    let manifest = coverage_manifest_for_circuit_version(circuit_version);
    let coverage_manifest_digest =
        coverage_manifest_digest(&manifest).expect("coverage manifest borsh");
    let mut update = ZoneProofFinalUpdateV1 {
        zone_id: header.zone_id,
        zone_block_height: header.height,
        zone_block_hash: zone_block_header_hash(header),
        state_root: header.state_root,
        message_root: header.message_root,
        tx_root: header.tx_root,
        da_root: header.da_root,
        da_namespace: header.da_namespace,
        forced_inclusion_root: header.forced_inclusion_root,
        timestamp_ms: header.timestamp_ms,
        circuit_version,
        coverage_manifest_digest,
        covered_features: manifest.covered_features,
        feature_set,
        public_input_digest: [0u8; 32],
        source_message_root: header.message_root,
        required_forced_inclusion_root: header.forced_inclusion_root,
        da_available,
        proof_digest,
        prover,
    };
    update.public_input_digest = zone_proof_public_input_digest(&update);
    update
}

#[must_use]
pub fn zone_is_evm_capable(metadata: &ExecutionZoneMetadataV1) -> bool {
    metadata.proof_system >= 2
}

#[must_use]
pub fn required_zone_circuit_version(metadata: &ExecutionZoneMetadataV1) -> CircuitVersion {
    if zone_is_evm_capable(metadata) {
        CircuitVersion::MixedStateTransitionV1
    } else {
        CircuitVersion::NativeStateTransitionV1
    }
}

#[must_use]
pub fn zone_forced_inclusion_root(requests: &[ForcedInclusionRequestV1]) -> Hash256 {
    #[derive(BorshSerialize)]
    struct ForcedInclusionLeaf<'a> {
        tag: [u8; 16],
        request: &'a ForcedInclusionRequestV1,
    }
    let leaves = requests
        .iter()
        .map(|request| {
            keccak256(
                &borsh::to_vec(&ForcedInclusionLeaf {
                    tag: *b"FRAC_FORCE_LEAF_",
                    request,
                })
                .expect("forced inclusion leaf borsh"),
            )
        })
        .collect::<Vec<_>>();
    merkle_root(&leaves)
}

#[must_use]
pub fn forced_inclusion_was_proven(update: &ZoneProofFinalUpdateV1) -> bool {
    update.forced_inclusion_root == update.required_forced_inclusion_root
}

fn verify_zone_update_coverage(
    update: &ZoneProofFinalUpdateV1,
    metadata: &ExecutionZoneMetadataV1,
) -> Result<(), ExecutionZoneError> {
    if update.circuit_version != required_zone_circuit_version(metadata) {
        return Err(ExecutionZoneError::ZoneCircuitCoverage);
    }
    let manifest = coverage_manifest_for_circuit_version(update.circuit_version);
    if update.coverage_manifest_digest
        != coverage_manifest_digest(&manifest)
            .map_err(|_| ExecutionZoneError::ZoneCoverageManifest)?
        || update.covered_features != manifest.covered_features
    {
        return Err(ExecutionZoneError::ZoneCoverageManifest);
    }
    if !update.feature_set.contains_only(manifest.covered_features) {
        return Err(ExecutionZoneError::ZoneCircuitCoverage);
    }
    Ok(())
}

fn verify_zone_update_public_inputs(
    update: &ZoneProofFinalUpdateV1,
) -> Result<(), ExecutionZoneError> {
    if update.public_input_digest != zone_proof_public_input_digest(update) {
        return Err(ExecutionZoneError::ZonePublicInputs);
    }
    if update.source_message_root != update.message_root {
        return Err(ExecutionZoneError::SourceMessageRootMismatch);
    }
    if !forced_inclusion_was_proven(update) {
        return Err(ExecutionZoneError::ForcedInclusionRootMismatch);
    }
    if !update.da_available {
        return Err(ExecutionZoneError::ZoneDaUnavailable);
    }
    Ok(())
}

#[must_use]
pub fn verify_zone_proof_update_matches_header(
    update: &ZoneProofFinalUpdateV1,
    header: &ZoneBlockHeaderV1,
) -> bool {
    update.zone_id == header.zone_id
        && update.zone_block_height == header.height
        && update.zone_block_hash == zone_block_header_hash(header)
        && update.state_root == header.state_root
        && update.message_root == header.message_root
        && update.tx_root == header.tx_root
        && update.da_root == header.da_root
        && update.da_namespace == header.da_namespace
        && update.forced_inclusion_root == header.forced_inclusion_root
        && update.timestamp_ms == header.timestamp_ms
}

#[must_use]
pub fn zone_proof_commitment_hash(commitment: &ZoneProofCommitmentV1) -> Hash256 {
    #[derive(BorshSerialize)]
    struct ZoneProofLeaf<'a> {
        tag: [u8; 16],
        commitment: &'a ZoneProofCommitmentV1,
    }
    let bytes = borsh::to_vec(&ZoneProofLeaf {
        tag: *b"FRAC_ZONE_PROOF_",
        commitment,
    })
    .expect("zone proof commitment borsh");
    keccak256(&bytes)
}

#[must_use]
pub fn zone_proof_root(commitments: &[ZoneProofCommitmentV1]) -> Hash256 {
    let mut sorted = commitments.to_vec();
    sorted.sort_by_key(|c| borsh::to_vec(c).expect("zone proof commitment borsh"));
    let leaves = sorted
        .iter()
        .map(zone_proof_commitment_hash)
        .collect::<Vec<_>>();
    merkle_root(&leaves)
}

pub fn verify_zone_proof_update_against_masterchain(
    update: &ZoneProofFinalUpdateV1,
    masterchain_block: &MasterchainBlockV1,
) -> Result<(), ExecutionZoneError> {
    if update.proof_digest == [0u8; 32] {
        return Err(ExecutionZoneError::EmptyZoneProofDigest);
    }
    let expected_root = zone_proof_root(&masterchain_block.zone_proof_commitments);
    if masterchain_block.global_zk_root != expected_root {
        return Err(ExecutionZoneError::MasterchainZkRootMismatch);
    }
    let commitment = ZoneProofCommitmentV1::from(update);
    if !masterchain_block
        .zone_proof_commitments
        .iter()
        .any(|c| c == &commitment)
    {
        return Err(ExecutionZoneError::ZoneProofNotCommitted);
    }
    Ok(())
}

#[must_use]
pub fn cross_zone_message_root(messages: &[AsyncCrossZoneMessageV1]) -> Hash256 {
    let leaves = messages
        .iter()
        .map(cross_zone_message_leaf_hash)
        .collect::<Vec<_>>();
    merkle_root(&leaves)
}

pub fn build_cross_zone_message_inclusion_proof(
    source_zone: ZoneId,
    source_zone_block_height: u64,
    messages: &[AsyncCrossZoneMessageV1],
    message_index: usize,
) -> Result<CrossZoneMessageInclusionProofV1, ExecutionZoneError> {
    let message = messages
        .get(message_index)
        .cloned()
        .ok_or(ExecutionZoneError::MessageProofIndexOutOfRange)?;
    if message.from_zone != source_zone {
        return Err(ExecutionZoneError::UnknownZone(source_zone));
    }
    let leaves = messages
        .iter()
        .map(cross_zone_message_leaf_hash)
        .collect::<Vec<_>>();
    let proof_path = merkle_proof(&leaves, message_index)
        .ok_or(ExecutionZoneError::MessageProofIndexOutOfRange)?;
    Ok(CrossZoneMessageInclusionProofV1 {
        version: CrossZoneMessageInclusionProofV1::VERSION,
        source_zone,
        source_zone_block_height,
        message_index: message_index as u64,
        message,
        message_root: merkle_root(&leaves),
        proof_path,
    })
}

#[must_use]
pub fn verify_cross_zone_message_inclusion_proof(
    proof: &CrossZoneMessageInclusionProofV1,
    expected_message_root: Hash256,
) -> bool {
    if proof.version != CrossZoneMessageInclusionProofV1::VERSION {
        return false;
    }
    if proof.message.from_zone != proof.source_zone {
        return false;
    }
    if proof.message_root != expected_message_root {
        return false;
    }
    verify_merkle_proof(
        proof.message_root,
        cross_zone_message_leaf_hash(&proof.message),
        proof.message_index as usize,
        &proof.proof_path,
    )
}

/// Env: shard blocks between masterchain anchors (`0` = disabled on monolith).
pub const ENV_ANCHOR_INTERVAL: &str = "FRACTAL_ANCHOR_INTERVAL";

/// Anchor cadence from env; monolith defaults to **disabled** unless explicitly set.
#[must_use]
pub fn anchor_interval_from_env(topology: &ShardTopology) -> u64 {
    let raw = std::env::var(ENV_ANCHOR_INTERVAL).ok();
    if let Some(s) = raw {
        let v = s.trim().parse::<u64>().unwrap_or(0);
        return v;
    }
    if topology.is_monolith() {
        0
    } else {
        DEFAULT_ANCHOR_INTERVAL
    }
}

/// Whether this committed shard height should emit a [`ShardAnchor`].
#[must_use]
pub fn should_emit_anchor_at_height(height: u64, anchor_interval: u64) -> bool {
    anchor_interval > 0 && height > 0 && height.is_multiple_of(anchor_interval)
}

/// Commitment to witness data for async provers (§7.10.3).
#[must_use]
pub fn witness_commitment_for_anchor(
    shard_id: ShardId,
    block_height: u64,
    state_root: &Hash256,
    tx_root: &Hash256,
) -> Hash256 {
    let mut buf = Vec::with_capacity(4 + 8 + 64);
    buf.extend_from_slice(&shard_id.to_be_bytes());
    buf.extend_from_slice(&block_height.to_be_bytes());
    buf.extend_from_slice(state_root);
    buf.extend_from_slice(tx_root);
    keccak256(&buf)
}

/// Build anchor from a committed block header (state already finalized).
#[must_use]
pub fn shard_anchor_from_header(
    shard_id: ShardId,
    header: &fractal_consensus::BlockHeader,
) -> ShardAnchor {
    let witness_commitment =
        witness_commitment_for_anchor(shard_id, header.height, &header.state_root, &header.tx_root);
    ShardAnchor {
        shard_id,
        block_height: header.height,
        state_root: header.state_root,
        witness_commitment,
    }
}

/// Merkle-ish aggregate over shard roots for a masterchain block (sorted by `shard_id`).
#[must_use]
pub fn global_state_root_from_anchors(anchors: &[ShardAnchor]) -> Hash256 {
    if anchors.is_empty() {
        return [0u8; 32];
    }
    let mut sorted: Vec<&ShardAnchor> = anchors.iter().collect();
    sorted.sort_by_key(|a| a.shard_id);
    let mut buf = Vec::with_capacity(sorted.len() * (4 + 8 + 32));
    for a in sorted {
        buf.extend_from_slice(&a.shard_id.to_be_bytes());
        buf.extend_from_slice(&a.block_height.to_be_bytes());
        buf.extend_from_slice(&a.state_root);
    }
    keccak256(&buf)
}

#[must_use]
pub fn ordered_cross_shard_messages(messages: &[CrossShardMessageV1]) -> Vec<CrossShardMessageV1> {
    let mut out = messages.to_vec();
    out.sort_by_key(|m| (m.from_shard, m.to_shard, m.payload_hash));
    out.dedup_by_key(|m| (m.from_shard, m.to_shard, m.payload_hash));
    out
}

/// Assemble a masterchain block from shard anchors at an anchor cadence tick.
#[must_use]
pub fn masterchain_block_from_anchors(
    masterchain_height: u64,
    shard_anchors: Vec<ShardAnchor>,
    validity_proofs: Vec<ProofSubmissionV1>,
    global_zk_root: Hash256,
) -> MasterchainBlockV1 {
    let global_state_root = global_state_root_from_anchors(&shard_anchors);
    MasterchainBlockV1 {
        height: masterchain_height,
        shard_anchors,
        validity_proofs,
        zone_proof_commitments: Vec::new(),
        global_state_root,
        global_zk_root,
        cross_shard_messages: Vec::new(),
    }
}

#[must_use]
pub fn masterchain_block_from_anchors_and_zone_proofs(
    masterchain_height: u64,
    shard_anchors: Vec<ShardAnchor>,
    validity_proofs: Vec<ProofSubmissionV1>,
    zone_proof_commitments: Vec<ZoneProofCommitmentV1>,
) -> MasterchainBlockV1 {
    let global_state_root = global_state_root_from_anchors(&shard_anchors);
    let global_zk_root = zone_proof_root(&zone_proof_commitments);
    MasterchainBlockV1 {
        height: masterchain_height,
        shard_anchors,
        validity_proofs,
        zone_proof_commitments,
        global_state_root,
        global_zk_root,
        cross_shard_messages: Vec::new(),
    }
}

/// Assemble a masterchain block with explicit cross-shard message delivery order.
#[must_use]
pub fn masterchain_block_from_anchors_and_messages(
    masterchain_height: u64,
    shard_anchors: Vec<ShardAnchor>,
    validity_proofs: Vec<ProofSubmissionV1>,
    global_zk_root: Hash256,
    cross_shard_messages: Vec<CrossShardMessageV1>,
) -> MasterchainBlockV1 {
    let mut block = masterchain_block_from_anchors(
        masterchain_height,
        shard_anchors,
        validity_proofs,
        global_zk_root,
    );
    block.cross_shard_messages = ordered_cross_shard_messages(&cross_shard_messages);
    block
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zone_metadata(timeout_blocks: u64, namespace: [u8; 8]) -> ExecutionZoneMetadataV1 {
        ExecutionZoneMetadataV1 {
            version: ExecutionZoneMetadataV1::VERSION,
            proof_system: 1,
            da_namespace: namespace,
            sequencer_policy: 1,
            forced_inclusion_timeout_masterchain_blocks: timeout_blocks,
        }
    }

    fn zone_header(zone_id: ZoneId, height: u64) -> ZoneBlockHeaderV1 {
        ZoneBlockHeaderV1 {
            version: ZoneBlockHeaderV1::VERSION,
            zone_id,
            height,
            parent_zone_block_hash: [0x10; 32],
            state_root: [0x22; 32],
            message_root: [0x33; 32],
            tx_root: [0x44; 32],
            da_namespace: *b"zone0001",
            da_root: [0x55; 32],
            forced_inclusion_root: [0x66; 32],
            timestamp_ms: 1_000,
            sequencer: [0x77; 20],
        }
    }

    #[test]
    fn monolith_only_shard_zero() {
        let topo = ShardTopology { shard_count: 1 };
        let a = [1u8; 20];
        assert_eq!(home_shard_for_address(&a, 1), 0);
        assert!(accepts_transaction(&a, 0, &topo));
        assert!(!accepts_transaction(&a, 1, &topo));
    }

    #[test]
    fn routing_splits_by_signer() {
        let topo = ShardTopology { shard_count: 4 };
        let s0 = [0u8; 20];
        let s1 = [1u8; 20];
        let h0 = home_shard_for_signer(&s0, 4);
        let h1 = home_shard_for_signer(&s1, 4);
        assert!(h0 < 4 && h1 < 4);
        assert_eq!(accepts_transaction(&s0, h0, &topo), true);
        assert_eq!(accepts_transaction(&s0, h1, &topo), h0 == h1);
    }

    #[test]
    fn validate_block_shard_rejects_mismatch() {
        let topo = ShardTopology { shard_count: 4 };
        assert!(validate_block_shard(2, 2, &topo).is_ok());
        assert!(validate_block_shard(3, 2, &topo).is_err());
    }

    #[test]
    fn anchor_interval_and_global_root() {
        assert!(!should_emit_anchor_at_height(99, 100));
        assert!(should_emit_anchor_at_height(100, 100));
        let a0 = ShardAnchor {
            shard_id: 0,
            block_height: 100,
            state_root: [1u8; 32],
            witness_commitment: [2u8; 32],
        };
        let a1 = ShardAnchor {
            shard_id: 1,
            block_height: 100,
            state_root: [3u8; 32],
            witness_commitment: [4u8; 32],
        };
        let g = global_state_root_from_anchors(&[a1.clone(), a0.clone()]);
        assert_ne!(g, [0u8; 32]);
        assert_eq!(
            global_state_root_from_anchors(&[a0, a1]),
            g,
            "order independent"
        );
    }

    #[test]
    fn zone_creation_and_proof_final_update() {
        let mut registry = ExecutionZoneRegistryV1::default();
        let metadata = zone_metadata(3, *b"zone0001");

        let zone = registry
            .create_zone(100, [0x11; 20], metadata.clone())
            .expect("zone created");

        assert_eq!(zone.zone_id, 100);
        assert_eq!(zone.metadata, metadata);
        assert_eq!(zone.latest_proof_final_height, 0);

        let header = zone_header(100, 7);
        let update = zone_proof_final_update_from_header(&header, [0x44; 32], [0x55; 20]);
        let updated = registry
            .submit_proof_final_update(update.clone())
            .expect("proof-final update");

        assert_eq!(updated.latest_proof_final_height, 7);
        assert_eq!(updated.latest_state_root, [0x22; 32]);
        assert_eq!(updated.latest_message_root, [0x33; 32]);
        assert_eq!(registry.zone(100).unwrap().latest_proof_final_height, 7);
        assert_eq!(
            registry.submit_proof_final_update(update),
            Err(ExecutionZoneError::StaleZoneUpdate {
                height: 7,
                current: 7,
            })
        );
    }

    #[test]
    fn zone_block_header_commitment_binds_roots_and_da_fields() {
        let header = zone_header(100, 8);
        let update = zone_proof_final_update_from_header(&header, [0x44; 32], [0x55; 20]);
        assert!(verify_zone_proof_update_matches_header(&update, &header));

        let mut tampered = header.clone();
        tampered.da_root = [0x99; 32];
        assert_ne!(
            zone_block_header_hash(&header),
            zone_block_header_hash(&tampered)
        );
        assert!(!verify_zone_proof_update_matches_header(&update, &tampered));

        let mut tampered = header.clone();
        tampered.forced_inclusion_root = [0xAA; 32];
        assert_ne!(
            zone_block_header_hash(&header),
            zone_block_header_hash(&tampered)
        );
    }

    #[test]
    fn zone_proof_final_update_verifies_against_masterchain_commitment() {
        let mut registry = ExecutionZoneRegistryV1::default();
        registry
            .create_zone(100, [0x11; 20], zone_metadata(3, *b"zone0001"))
            .unwrap();
        let header = zone_header(100, 8);
        let update = zone_proof_final_update_from_header(&header, [0x44; 32], [0x55; 20]);
        let masterchain = masterchain_block_from_anchors_and_zone_proofs(
            3,
            Vec::new(),
            Vec::new(),
            vec![ZoneProofCommitmentV1::from(&update)],
        );

        assert!(verify_zone_proof_update_against_masterchain(&update, &masterchain).is_ok());
        let zone = registry
            .submit_verified_proof_final_update(update, &masterchain)
            .expect("verified proof-final update");

        assert_eq!(zone.latest_proof_final_height, 8);
        assert_eq!(zone.latest_state_root, [0x22; 32]);
        assert_eq!(zone.latest_message_root, [0x33; 32]);
        assert_eq!(
            masterchain.global_zk_root,
            zone_proof_root(&masterchain.zone_proof_commitments)
        );
    }

    #[test]
    fn zone_update_requires_coverage_matching_zone_capability() {
        let mut registry = ExecutionZoneRegistryV1::default();
        let mut metadata = zone_metadata(3, *b"zoneevm1");
        metadata.proof_system = 2;
        registry.create_zone(101, [0x11; 20], metadata).unwrap();
        let header = zone_header(101, 9);
        let native_update = zone_proof_final_update_from_header(&header, [0x44; 32], [0x55; 20]);

        assert_eq!(
            registry.submit_proof_final_update(native_update),
            Err(ExecutionZoneError::ZoneCircuitCoverage)
        );

        let mixed_update = zone_proof_final_update_from_header_with_circuit(
            &header,
            CircuitVersion::MixedStateTransitionV1,
            ExecutionFeatureSetV1 {
                bits: fractal_consensus::FEATURE_NATIVE_TX | fractal_consensus::FEATURE_EVM_CALL,
            },
            true,
            [0x45; 32],
            [0x55; 20],
        );
        let updated = registry
            .submit_proof_final_update(mixed_update)
            .expect("mixed coverage accepted for EVM-capable zone");
        assert_eq!(updated.latest_proof_final_height, 9);
    }

    #[test]
    fn zone_update_rejects_unavailable_da_and_uncovered_features() {
        let mut registry = ExecutionZoneRegistryV1::default();
        registry
            .create_zone(102, [0x11; 20], zone_metadata(3, *b"zone0102"))
            .unwrap();
        let header = zone_header(102, 9);
        let unavailable_da = zone_proof_final_update_from_header_with_circuit(
            &header,
            CircuitVersion::NativeStateTransitionV1,
            ExecutionFeatureSetV1 {
                bits: fractal_consensus::FEATURE_NATIVE_TX,
            },
            false,
            [0x46; 32],
            [0x55; 20],
        );
        assert_eq!(
            registry.submit_proof_final_update(unavailable_da),
            Err(ExecutionZoneError::ZoneDaUnavailable)
        );

        let uncovered = zone_proof_final_update_from_header_with_circuit(
            &header,
            CircuitVersion::NativeStateTransitionV1,
            ExecutionFeatureSetV1 {
                bits: fractal_consensus::FEATURE_NATIVE_TX | fractal_consensus::FEATURE_EVM_CALL,
            },
            true,
            [0x47; 32],
            [0x55; 20],
        );
        assert_eq!(
            registry.submit_proof_final_update(uncovered),
            Err(ExecutionZoneError::ZoneCircuitCoverage)
        );
    }

    #[test]
    fn zone_update_requires_proven_message_and_forced_inclusion_roots() {
        let mut registry = ExecutionZoneRegistryV1::default();
        registry
            .create_zone(103, [0x11; 20], zone_metadata(3, *b"zone0103"))
            .unwrap();
        let header = zone_header(103, 9);
        let mut bad_message_root =
            zone_proof_final_update_from_header(&header, [0x48; 32], [0x55; 20]);
        bad_message_root.source_message_root = [0x99; 32];
        bad_message_root.public_input_digest = zone_proof_public_input_digest(&bad_message_root);
        assert_eq!(
            registry.submit_proof_final_update(bad_message_root),
            Err(ExecutionZoneError::SourceMessageRootMismatch)
        );

        let mut bad_forced_root =
            zone_proof_final_update_from_header(&header, [0x49; 32], [0x55; 20]);
        bad_forced_root.required_forced_inclusion_root = [0xAA; 32];
        bad_forced_root.public_input_digest = zone_proof_public_input_digest(&bad_forced_root);
        assert_eq!(
            registry.submit_proof_final_update(bad_forced_root),
            Err(ExecutionZoneError::ForcedInclusionRootMismatch)
        );
    }

    #[test]
    fn cross_zone_message_delivery_requires_proven_source_message_root() {
        let messages = vec![
            AsyncCrossZoneMessageV1 {
                from_zone: 104,
                to_zone: 105,
                nonce: 1,
                payload_hash: [0xAA; 32],
                payload: vec![0xAA],
            },
            AsyncCrossZoneMessageV1 {
                from_zone: 104,
                to_zone: 106,
                nonce: 2,
                payload_hash: [0xBB; 32],
                payload: vec![0xBB],
            },
        ];
        let message_root = cross_zone_message_root(&messages);
        let proof = build_cross_zone_message_inclusion_proof(104, 9, &messages, 0).unwrap();
        let mut header = zone_header(104, 9);
        header.message_root = message_root;
        let update = zone_proof_final_update_from_header(&header, [0x50; 32], [0x55; 20]);

        assert!(verify_cross_zone_message_inclusion_proof(
            &proof,
            update.source_message_root
        ));
    }

    #[test]
    fn zone_proof_final_update_rejects_missing_or_bad_masterchain_commitment() {
        let header = zone_header(100, 8);
        let update = zone_proof_final_update_from_header(&header, [0x44; 32], [0x55; 20]);
        let empty_masterchain =
            masterchain_block_from_anchors_and_zone_proofs(3, Vec::new(), Vec::new(), Vec::new());

        assert_eq!(
            verify_zone_proof_update_against_masterchain(&update, &empty_masterchain),
            Err(ExecutionZoneError::ZoneProofNotCommitted)
        );

        let mut bad_root = masterchain_block_from_anchors_and_zone_proofs(
            3,
            Vec::new(),
            Vec::new(),
            vec![ZoneProofCommitmentV1::from(&update)],
        );
        bad_root.global_zk_root = [0x99; 32];

        assert_eq!(
            verify_zone_proof_update_against_masterchain(&update, &bad_root),
            Err(ExecutionZoneError::MasterchainZkRootMismatch)
        );

        let mut tampered_update = update.clone();
        tampered_update.message_root = [0xAA; 32];
        let masterchain = masterchain_block_from_anchors_and_zone_proofs(
            3,
            Vec::new(),
            Vec::new(),
            vec![ZoneProofCommitmentV1::from(&update)],
        );
        assert_eq!(
            verify_zone_proof_update_against_masterchain(&tampered_update, &masterchain),
            Err(ExecutionZoneError::ZoneProofNotCommitted)
        );
    }

    #[test]
    fn async_cross_zone_message_delivery_is_ordered_and_deduped() {
        let mut registry = ExecutionZoneRegistryV1::default();
        registry
            .create_zone(1, [0x11; 20], zone_metadata(3, *b"zone0001"))
            .unwrap();
        registry
            .create_zone(2, [0x22; 20], zone_metadata(3, *b"zone0002"))
            .unwrap();

        let msg_b = AsyncCrossZoneMessageV1 {
            from_zone: 1,
            to_zone: 2,
            nonce: 2,
            payload_hash: [0xBB; 32],
            payload: vec![0xBB],
        };
        let msg_a = AsyncCrossZoneMessageV1 {
            from_zone: 1,
            to_zone: 2,
            nonce: 1,
            payload_hash: [0xAA; 32],
            payload: vec![0xAA],
        };
        registry.submit_cross_zone_message(msg_b.clone()).unwrap();
        registry.submit_cross_zone_message(msg_a.clone()).unwrap();
        registry.submit_cross_zone_message(msg_a.clone()).unwrap();

        let delivered = registry.drain_cross_zone_messages_for(2).unwrap();

        assert_eq!(delivered, vec![msg_a, msg_b]);
        assert!(registry
            .drain_cross_zone_messages_for(2)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn cross_zone_message_inclusion_proof_verifies_against_message_root() {
        let messages = vec![
            AsyncCrossZoneMessageV1 {
                from_zone: 1,
                to_zone: 2,
                nonce: 1,
                payload_hash: [0xAA; 32],
                payload: vec![0xAA],
            },
            AsyncCrossZoneMessageV1 {
                from_zone: 1,
                to_zone: 3,
                nonce: 2,
                payload_hash: [0xBB; 32],
                payload: vec![0xBB],
            },
            AsyncCrossZoneMessageV1 {
                from_zone: 1,
                to_zone: 4,
                nonce: 3,
                payload_hash: [0xCC; 32],
                payload: vec![0xCC],
            },
        ];
        let message_root = cross_zone_message_root(&messages);
        let proof = build_cross_zone_message_inclusion_proof(1, 9, &messages, 1)
            .expect("message inclusion proof");

        assert_eq!(proof.version, CrossZoneMessageInclusionProofV1::VERSION);
        assert_eq!(proof.source_zone, 1);
        assert_eq!(proof.source_zone_block_height, 9);
        assert_eq!(proof.message_index, 1);
        assert_eq!(proof.message_root, message_root);
        assert!(verify_cross_zone_message_inclusion_proof(
            &proof,
            message_root
        ));
    }

    #[test]
    fn cross_zone_message_inclusion_proof_rejects_tampering() {
        let messages = vec![
            AsyncCrossZoneMessageV1 {
                from_zone: 7,
                to_zone: 8,
                nonce: 1,
                payload_hash: [0x11; 32],
                payload: vec![1],
            },
            AsyncCrossZoneMessageV1 {
                from_zone: 7,
                to_zone: 8,
                nonce: 2,
                payload_hash: [0x22; 32],
                payload: vec![2],
            },
        ];
        let message_root = cross_zone_message_root(&messages);
        let mut proof = build_cross_zone_message_inclusion_proof(7, 4, &messages, 0).unwrap();
        proof.message.payload = vec![9];

        assert!(!verify_cross_zone_message_inclusion_proof(
            &proof,
            message_root
        ));

        let proof = build_cross_zone_message_inclusion_proof(7, 4, &messages, 0).unwrap();
        assert!(!verify_cross_zone_message_inclusion_proof(
            &proof, [0xFF; 32]
        ));
        assert_eq!(
            build_cross_zone_message_inclusion_proof(7, 4, &messages, 9),
            Err(ExecutionZoneError::MessageProofIndexOutOfRange)
        );
    }

    #[test]
    fn forced_inclusion_materializes_after_sequencer_censorship_sla() {
        let mut registry = ExecutionZoneRegistryV1::default();
        registry
            .create_zone(9, [0x11; 20], zone_metadata(2, *b"zone0009"))
            .unwrap();

        let request = registry
            .submit_forced_inclusion(9, [0xAA; 20], [0xCC; 32], vec![1, 2, 3])
            .expect("forced inclusion request");

        assert_eq!(request.submitted_at_masterchain_height, 0);
        assert_eq!(request.deadline_masterchain_height, 2);
        assert_eq!(registry.pending_forced_inclusions.len(), 1);

        registry.advance_masterchain_height(1);
        assert!(registry.forced_inclusion_events.is_empty());
        assert_eq!(registry.pending_forced_inclusions.len(), 1);

        registry.advance_masterchain_height(2);
        assert!(registry.pending_forced_inclusions.is_empty());
        assert_eq!(
            registry.forced_inclusion_events,
            vec![ForcedInclusionEventV1 {
                version: ForcedInclusionEventV1::VERSION,
                request,
                included_at_masterchain_height: 2,
                sequencer_late_by_blocks: 0,
            }]
        );
    }

    #[test]
    fn sequencer_epoch_settlement_applies_forced_inclusion_penalties() {
        let mut registry = ExecutionZoneRegistryV1::default();
        registry
            .create_zone(9, [0x11; 20], zone_metadata(1, *b"zone0009"))
            .unwrap();
        registry
            .submit_forced_inclusion(9, [0xAA; 20], [0xCC; 32], vec![1, 2, 3])
            .unwrap();
        registry.advance_masterchain_height(3);
        assert_eq!(
            registry.forced_inclusion_events[0].sequencer_late_by_blocks,
            2
        );
        let params = SequencerRewardParams {
            enabled: true,
            treasury: [0x55; 20],
            base_reward_per_zone_block_wei: 100,
            da_byte_reward_wei: 1,
            forced_inclusion_penalty_wei: 250,
            late_forced_inclusion_penalty_per_block_wei: 25,
        };

        let settlement = registry.settle_sequencer_epoch(
            &params,
            SequencerEpochWorkV1 {
                zone_id: 9,
                sequencer: [0x44; 20],
                zone_blocks: 5,
                da_bytes: 100,
            },
        );

        assert_eq!(settlement.reward_wei, 600);
        assert_eq!(settlement.forced_inclusion_penalty_wei, 300);
        assert_eq!(settlement.net_reward_wei, 300);
        assert_eq!(settlement.unpaid_penalty_wei, 0);
        assert_eq!(settlement.forced_inclusion_count, 1);
    }
}
