//! Execution shard routing + masterchain coordination types (`docs/prd.md` §6, §7.10, M10).
//!
//! Track A (monolith): `shard_count == 1` and `shard_id == 0` — all txs accepted, headers tag shard 0.
//! Track B: `FRACTAL_SHARD_COUNT` > 1 and each node runs one `FRACTAL_SHARD_ID`.

use borsh::{BorshDeserialize, BorshSerialize};
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
    pub state_root: Hash256,
    pub message_root: Hash256,
    pub proof_digest: Hash256,
    pub prover: [u8; 20],
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

        let updated = registry
            .submit_proof_final_update(ZoneProofFinalUpdateV1 {
                zone_id: 100,
                zone_block_height: 7,
                state_root: [0x22; 32],
                message_root: [0x33; 32],
                proof_digest: [0x44; 32],
                prover: [0x55; 20],
            })
            .expect("proof-final update");

        assert_eq!(updated.latest_proof_final_height, 7);
        assert_eq!(updated.latest_state_root, [0x22; 32]);
        assert_eq!(updated.latest_message_root, [0x33; 32]);
        assert_eq!(registry.zone(100).unwrap().latest_proof_final_height, 7);
        assert_eq!(
            registry.submit_proof_final_update(ZoneProofFinalUpdateV1 {
                zone_id: 100,
                zone_block_height: 7,
                state_root: [0x66; 32],
                message_root: [0x77; 32],
                proof_digest: [0x88; 32],
                prover: [0x55; 20],
            }),
            Err(ExecutionZoneError::StaleZoneUpdate {
                height: 7,
                current: 7,
            })
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
}
