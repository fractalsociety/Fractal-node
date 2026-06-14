//! Masterchain anchor ledger (PRD §7.10, M10/M11).

use borsh::{BorshDeserialize, BorshSerialize};
use fractal_crypto::{Hash256, keccak256};
use fractal_proof_aggregator::{
    AggregatorError, GlobalZkStatementV1, Plonky2ProofBundleV1, SubmissionError,
    VerifiedStwoStatementV1, dedupe_submissions, proof_submission_from_checkpoint_digest,
    prove_and_aggregate, prove_and_aggregate_verified, validate_proof_submission,
};
use fractal_shard::{
    CrossShardMessageV1, MasterchainBlockV1, ProofSubmissionV1, ShardAnchor,
    masterchain_block_from_anchors_and_messages, shard_anchor_from_header,
};

pub type ZoneId = u64;

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
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

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
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

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ZoneProofFinalUpdateV1 {
    pub zone_id: ZoneId,
    pub zone_block_height: u64,
    pub state_root: Hash256,
    pub message_root: Hash256,
    pub proof_digest: Hash256,
    pub prover: [u8; 20],
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, BorshSerialize, BorshDeserialize)]
pub struct AsyncCrossZoneMessageV1 {
    pub from_zone: ZoneId,
    pub to_zone: ZoneId,
    pub nonce: u64,
    pub payload_hash: Hash256,
    pub payload: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ForcedInclusionRequestV1 {
    pub zone_id: ZoneId,
    pub requester: [u8; 20],
    pub request_id: Hash256,
    pub tx_hash: Hash256,
    pub payload: Vec<u8>,
    pub submitted_at_masterchain_height: u64,
    pub deadline_masterchain_height: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ForcedInclusionEventV1 {
    pub version: u8,
    pub request: ForcedInclusionRequestV1,
    pub included_at_masterchain_height: u64,
    pub sequencer_late_by_blocks: u64,
}

impl ForcedInclusionEventV1 {
    pub const VERSION: u8 = 1;
}

/// Invalid-proof handling policy for the future mandatory-proof regime (§7.8).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProofSlashingPolicyV1 {
    pub enabled: bool,
    /// Require a verified STWO public statement for every accepted `ProofSubmissionV1`.
    pub require_verified_stwo: bool,
    /// Informational bond amount for the downstream slashing executor / economics layer.
    pub slash_amount_wei: u128,
}

impl Default for ProofSlashingPolicyV1 {
    fn default() -> Self {
        Self {
            enabled: false,
            require_verified_stwo: false,
            slash_amount_wei: 0,
        }
    }
}

/// Slashable evidence emitted when a prover submits an invalid proof statement.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct InvalidProofSlashEventV1 {
    pub version: u8,
    pub masterchain_height: u64,
    pub prover: [u8; 20],
    pub shard_id: u32,
    pub start_block: u64,
    pub end_block: u64,
    pub proof_digest: Hash256,
    pub reason_code: u8,
    pub evidence_hash: Hash256,
    pub slash_amount_wei: u128,
    pub executed: bool,
    pub burned_bond_wei: u128,
    pub bond_before_wei: u128,
    pub bond_after_wei: u128,
    pub prover_active_after: bool,
}

impl InvalidProofSlashEventV1 {
    pub const VERSION: u8 = 1;
}

pub const INVALID_PROOF_EMPTY_DIGEST: u8 = 1;
pub const INVALID_PROOF_BAD_RANGE: u8 = 2;
pub const INVALID_PROOF_UNKNOWN_SHARD: u8 = 3;
pub const INVALID_PROOF_RANGE_EXCEEDS_ANCHOR: u8 = 4;
pub const INVALID_PROOF_DUPLICATE: u8 = 5;
pub const INVALID_PROOF_MISSING_VERIFIED_STWO: u8 = 6;
pub const INVALID_PROOF_AGGREGATOR_REJECTED: u8 = 7;

/// Permissionless prover market admission controls.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProverMarketParamsV1 {
    pub version: u8,
    pub enabled: bool,
    pub require_registered_identity: bool,
    pub min_identity_bond_wei: u128,
    pub max_pending_submissions_per_prover: u32,
    pub max_range_blocks: u64,
}

impl ProverMarketParamsV1 {
    pub const VERSION: u8 = 1;

    #[must_use]
    pub fn disabled() -> Self {
        Self {
            version: Self::VERSION,
            enabled: false,
            require_registered_identity: false,
            min_identity_bond_wei: 0,
            max_pending_submissions_per_prover: u32::MAX,
            max_range_blocks: u64::MAX,
        }
    }

    #[must_use]
    pub fn devnet(min_identity_bond_wei: u128) -> Self {
        Self {
            version: Self::VERSION,
            enabled: true,
            require_registered_identity: true,
            min_identity_bond_wei,
            max_pending_submissions_per_prover: 8,
            max_range_blocks: 10_000,
        }
    }
}

impl Default for ProverMarketParamsV1 {
    fn default() -> Self {
        Self::disabled()
    }
}

/// Masterchain-native prover identity row. The bond is an anti-spam escrow tracked
/// by the coordination ledger; production settlement can mirror it into FRAC state.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProverIdentityV1 {
    pub version: u8,
    pub prover: [u8; 20],
    pub bond_wei: u128,
    pub registered_at_masterchain_height: u64,
    pub active: bool,
}

impl ProverIdentityV1 {
    pub const VERSION: u8 = 1;
}

/// Treasury-backed prover reward parameters (PRD §6.2.4).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProverEconomicsParamsV1 {
    pub version: u8,
    pub enabled: bool,
    pub treasury: [u8; 20],
    /// Maximum reward per covered shard block before lag decay.
    pub base_reward_per_block_wei: u128,
    /// Lag value where the reward decays to half of the maximum.
    pub lag_half_life_seconds: u32,
}

impl ProverEconomicsParamsV1 {
    pub const VERSION: u8 = 1;

    #[must_use]
    pub fn disabled() -> Self {
        Self {
            version: Self::VERSION,
            enabled: false,
            treasury: [0u8; 20],
            base_reward_per_block_wei: 0,
            lag_half_life_seconds: 1,
        }
    }

    #[must_use]
    pub fn devnet(treasury: [u8; 20]) -> Self {
        Self {
            version: Self::VERSION,
            enabled: true,
            treasury,
            base_reward_per_block_wei: 1_000_000_000_000,
            lag_half_life_seconds: 60,
        }
    }
}

impl Default for ProverEconomicsParamsV1 {
    fn default() -> Self {
        Self::disabled()
    }
}

/// Ledger event for a treasury payout credited to an accepted proof prover.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ProverRewardEventV1 {
    pub version: u8,
    pub masterchain_height: u64,
    pub prover: [u8; 20],
    pub shard_id: u32,
    pub start_block: u64,
    pub end_block: u64,
    pub lag_seconds: u32,
    pub covered_blocks: u64,
    pub reward_wei: u128,
    pub treasury: [u8; 20],
    pub treasury_balance_after_wei: u128,
}

impl ProverRewardEventV1 {
    pub const VERSION: u8 = 1;
}

/// In-memory ring of recent masterchain blocks + latest anchor per shard.
#[derive(Clone, Debug, Default)]
pub struct MasterchainLedger {
    pub masterchain_height: u64,
    pub blocks: Vec<MasterchainBlockV1>,
    pub latest_anchors: std::collections::BTreeMap<u32, ShardAnchor>,
    pub pending_validity_proofs: Vec<ProofSubmissionV1>,
    pub pending_cross_shard_messages: Vec<CrossShardMessageV1>,
    pub pending_stwo_by_height: std::collections::BTreeMap<u64, [u8; 32]>,
    pub pending_stwo_ranges: std::collections::BTreeMap<(u64, u64), [u8; 32]>,
    pub pending_verified_stwo_statements:
        std::collections::BTreeMap<(u32, u64, u64, [u8; 32]), VerifiedStwoStatementV1>,
    pub last_plonky2_bundle: Option<Plonky2ProofBundleV1>,
    pub proof_slashing_policy: ProofSlashingPolicyV1,
    pub invalid_proof_slash_events: Vec<InvalidProofSlashEventV1>,
    pub prover_market: ProverMarketParamsV1,
    pub prover_identities: std::collections::BTreeMap<[u8; 20], ProverIdentityV1>,
    pub prover_economics: ProverEconomicsParamsV1,
    pub prover_reward_credits_wei: std::collections::BTreeMap<[u8; 20], u128>,
    pub prover_reward_events: Vec<ProverRewardEventV1>,
    pub treasury_balance_wei: u128,
    pub execution_zones: std::collections::BTreeMap<ZoneId, ExecutionZoneRecordV1>,
    pub pending_cross_zone_messages: Vec<AsyncCrossZoneMessageV1>,
    pub pending_forced_inclusions: Vec<ForcedInclusionRequestV1>,
    pub forced_inclusion_events: Vec<ForcedInclusionEventV1>,
    /// Set when a shard posts a newer anchor; cleared after `seal_round`.
    pub pending_anchor_updates: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum MasterchainError {
    #[error("submission: {0}")]
    Submission(#[from] SubmissionError),
    #[error("aggregator: {0}")]
    Aggregator(#[from] AggregatorError),
    #[error("stale anchor shard={shard_id} height={height} prev={prev}")]
    StaleAnchor {
        shard_id: u32,
        height: u64,
        prev: u64,
    },
    #[error("no anchors to seal")]
    NothingToSeal,
    #[error("verified STWO statement required for proof shard={shard_id} range=[{start},{end}]")]
    MissingVerifiedStwo { shard_id: u32, start: u64, end: u64 },
    #[error("prover identity already registered")]
    ProverAlreadyRegistered,
    #[error("prover identity required")]
    ProverIdentityRequired,
    #[error("prover bond {bond} below minimum {min}")]
    ProverBondTooLow { bond: u128, min: u128 },
    #[error("prover pending submission limit exceeded")]
    ProverPendingLimit,
    #[error("proof range covers {range} blocks, above market max {max}")]
    ProofRangeTooLarge { range: u64, max: u64 },
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

impl MasterchainLedger {
    pub const MAX_BLOCKS: usize = 256;

    pub fn set_proof_slashing_policy(&mut self, policy: ProofSlashingPolicyV1) {
        self.proof_slashing_policy = policy;
    }

    pub fn set_prover_market(&mut self, params: ProverMarketParamsV1) {
        self.prover_market = params;
    }

    pub fn register_prover_identity(
        &mut self,
        prover: [u8; 20],
        bond_wei: u128,
    ) -> Result<ProverIdentityV1, MasterchainError> {
        if self.prover_identities.contains_key(&prover) {
            return Err(MasterchainError::ProverAlreadyRegistered);
        }
        if bond_wei < self.prover_market.min_identity_bond_wei {
            return Err(MasterchainError::ProverBondTooLow {
                bond: bond_wei,
                min: self.prover_market.min_identity_bond_wei,
            });
        }
        let identity = ProverIdentityV1 {
            version: ProverIdentityV1::VERSION,
            prover,
            bond_wei,
            registered_at_masterchain_height: self.masterchain_height,
            active: true,
        };
        self.prover_identities.insert(prover, identity.clone());
        Ok(identity)
    }

    pub fn top_up_prover_bond(
        &mut self,
        prover: [u8; 20],
        amount_wei: u128,
    ) -> Result<ProverIdentityV1, MasterchainError> {
        let identity = self
            .prover_identities
            .get_mut(&prover)
            .ok_or(MasterchainError::ProverIdentityRequired)?;
        identity.bond_wei = identity.bond_wei.saturating_add(amount_wei);
        Ok(identity.clone())
    }

    #[must_use]
    pub fn prover_identity(&self, prover: &[u8; 20]) -> Option<&ProverIdentityV1> {
        self.prover_identities.get(prover)
    }

    pub fn set_prover_economics(&mut self, params: ProverEconomicsParamsV1) {
        self.prover_economics = params;
    }

    pub fn fund_prover_treasury(&mut self, amount_wei: u128) {
        self.treasury_balance_wei = self.treasury_balance_wei.saturating_add(amount_wei);
    }

    #[must_use]
    pub fn invalid_proof_slash_events(&self) -> &[InvalidProofSlashEventV1] {
        &self.invalid_proof_slash_events
    }

    #[must_use]
    pub fn prover_reward_events(&self) -> &[ProverRewardEventV1] {
        &self.prover_reward_events
    }

    #[must_use]
    pub fn prover_reward_credit(&self, prover: &[u8; 20]) -> u128 {
        self.prover_reward_credits_wei
            .get(prover)
            .copied()
            .unwrap_or(0)
    }

    pub fn submit_validity_proof(
        &mut self,
        sub: ProofSubmissionV1,
    ) -> Result<(), MasterchainError> {
        self.validate_prover_market_submission(&sub)?;
        let anchors: Vec<ShardAnchor> = self.latest_anchors.values().cloned().collect();
        if anchors.is_empty() {
            if sub.proof_digest == [0u8; 32] || sub.start_block > sub.end_block {
                let err = if sub.proof_digest == [0u8; 32] {
                    SubmissionError::EmptyDigest
                } else {
                    SubmissionError::InvalidRange
                };
                self.record_invalid_submission(&sub, submission_reason_code(&err));
                return Err(MasterchainError::Submission(err));
            }
        } else {
            if let Err(err) = validate_proof_submission(&sub, &anchors) {
                self.record_invalid_submission(&sub, submission_reason_code(&err));
                return Err(MasterchainError::Submission(err));
            }
        }
        self.pending_validity_proofs.push(sub);
        Ok(())
    }

    pub fn record_stwo_digest(
        &mut self,
        block_height: u64,
        digest: [u8; 32],
    ) -> Result<(), MasterchainError> {
        self.record_stwo_digest_range(block_height, block_height, digest)
    }

    pub fn record_stwo_digest_range(
        &mut self,
        start_block: u64,
        end_block: u64,
        digest: [u8; 32],
    ) -> Result<(), MasterchainError> {
        if digest == [0u8; 32] {
            return Err(MasterchainError::Submission(SubmissionError::EmptyDigest));
        }
        if start_block > end_block {
            return Err(MasterchainError::Submission(SubmissionError::InvalidRange));
        }
        if start_block == end_block {
            self.pending_stwo_by_height.insert(end_block, digest);
        } else {
            self.pending_stwo_ranges
                .insert((start_block, end_block), digest);
        }
        Ok(())
    }

    pub fn record_verified_stwo_statement(
        &mut self,
        stmt: VerifiedStwoStatementV1,
    ) -> Result<(), MasterchainError> {
        self.record_stwo_digest_range(stmt.start_block, stmt.end_block, stmt.proof_digest)?;
        self.pending_verified_stwo_statements.insert(
            (
                stmt.shard_id,
                stmt.start_block,
                stmt.end_block,
                stmt.proof_digest,
            ),
            stmt,
        );
        Ok(())
    }

    pub fn submit_cross_shard_message(&mut self, msg: CrossShardMessageV1) {
        self.pending_cross_shard_messages.push(msg);
        self.pending_anchor_updates = true;
    }

    pub fn create_execution_zone(
        &mut self,
        zone_id: ZoneId,
        creator: [u8; 20],
        metadata: ExecutionZoneMetadataV1,
    ) -> Result<ExecutionZoneRecordV1, MasterchainError> {
        if metadata.forced_inclusion_timeout_masterchain_blocks == 0 {
            return Err(MasterchainError::InvalidForcedInclusionTimeout);
        }
        if self.execution_zones.contains_key(&zone_id) {
            return Err(MasterchainError::ZoneAlreadyExists);
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
        self.execution_zones.insert(zone_id, record.clone());
        self.pending_anchor_updates = true;
        Ok(record)
    }

    pub fn execution_zone(&self, zone_id: ZoneId) -> Option<&ExecutionZoneRecordV1> {
        self.execution_zones.get(&zone_id)
    }

    pub fn submit_zone_proof_final_update(
        &mut self,
        update: ZoneProofFinalUpdateV1,
    ) -> Result<ExecutionZoneRecordV1, MasterchainError> {
        if update.proof_digest == [0u8; 32] {
            return Err(MasterchainError::EmptyZoneProofDigest);
        }
        let zone = self
            .execution_zones
            .get_mut(&update.zone_id)
            .ok_or(MasterchainError::UnknownZone(update.zone_id))?;
        if update.zone_block_height <= zone.latest_proof_final_height {
            return Err(MasterchainError::StaleZoneUpdate {
                height: update.zone_block_height,
                current: zone.latest_proof_final_height,
            });
        }
        zone.latest_proof_final_height = update.zone_block_height;
        zone.latest_state_root = update.state_root;
        zone.latest_message_root = update.message_root;
        self.pending_anchor_updates = true;
        Ok(zone.clone())
    }

    pub fn submit_cross_zone_message(
        &mut self,
        msg: AsyncCrossZoneMessageV1,
    ) -> Result<(), MasterchainError> {
        if !self.execution_zones.contains_key(&msg.from_zone) {
            return Err(MasterchainError::UnknownZone(msg.from_zone));
        }
        if !self.execution_zones.contains_key(&msg.to_zone) {
            return Err(MasterchainError::UnknownZone(msg.to_zone));
        }
        self.pending_cross_zone_messages.push(msg);
        self.pending_anchor_updates = true;
        Ok(())
    }

    pub fn drain_cross_zone_messages_for(
        &mut self,
        to_zone: ZoneId,
    ) -> Result<Vec<AsyncCrossZoneMessageV1>, MasterchainError> {
        if !self.execution_zones.contains_key(&to_zone) {
            return Err(MasterchainError::UnknownZone(to_zone));
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
    ) -> Result<ForcedInclusionRequestV1, MasterchainError> {
        let zone = self
            .execution_zones
            .get(&zone_id)
            .ok_or(MasterchainError::UnknownZone(zone_id))?;
        let request_id = forced_inclusion_request_id(
            zone_id,
            &requester,
            &tx_hash,
            self.masterchain_height,
        );
        if self
            .pending_forced_inclusions
            .iter()
            .chain(
                self.forced_inclusion_events
                    .iter()
                    .map(|event| &event.request),
            )
            .any(|req| req.request_id == request_id)
        {
            return Err(MasterchainError::ForcedInclusionAlreadyExists);
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
        self.pending_anchor_updates = true;
        Ok(request)
    }

    pub fn pending_forced_inclusions(&self) -> &[ForcedInclusionRequestV1] {
        &self.pending_forced_inclusions
    }

    pub fn forced_inclusion_events(&self) -> &[ForcedInclusionEventV1] {
        &self.forced_inclusion_events
    }

    fn flush_stwo_digests_for_anchor(
        &mut self,
        shard_id: u32,
        anchor_height: u64,
        prover: [u8; 20],
    ) {
        let heights: Vec<u64> = self
            .pending_stwo_by_height
            .keys()
            .copied()
            .filter(|h| *h <= anchor_height)
            .collect();
        for h in heights {
            let Some(digest) = self.pending_stwo_by_height.remove(&h) else {
                continue;
            };
            let sub = proof_submission_from_checkpoint_digest(shard_id, h, h, prover, digest, 0);
            if !self
                .pending_validity_proofs
                .iter()
                .any(|p| p.shard_id == shard_id && p.start_block == h && p.end_block == h)
            {
                self.pending_validity_proofs.push(sub);
            }
        }
        let ranges: Vec<(u64, u64)> = self
            .pending_stwo_ranges
            .keys()
            .copied()
            .filter(|(_, end)| *end <= anchor_height)
            .collect();
        for (start, end) in ranges {
            let Some(digest) = self.pending_stwo_ranges.remove(&(start, end)) else {
                continue;
            };
            let sub =
                proof_submission_from_checkpoint_digest(shard_id, start, end, prover, digest, 0);
            if !self
                .pending_validity_proofs
                .iter()
                .any(|p| p.shard_id == shard_id && p.start_block == start && p.end_block == end)
            {
                self.pending_validity_proofs.push(sub);
            }
        }
    }

    /// Accept a shard anchor from RPC (dedicated masterchain) or before local seal.
    pub fn ingest_shard_anchor(&mut self, anchor: ShardAnchor) -> Result<(), MasterchainError> {
        if let Some(prev) = self.latest_anchors.get(&anchor.shard_id) {
            if anchor.block_height < prev.block_height {
                return Err(MasterchainError::StaleAnchor {
                    shard_id: anchor.shard_id,
                    height: anchor.block_height,
                    prev: prev.block_height,
                });
            }
            if anchor.block_height == prev.block_height
                && anchor.state_root == prev.state_root
                && anchor.witness_commitment == prev.witness_commitment
            {
                return Ok(());
            }
        }
        self.latest_anchors.insert(anchor.shard_id, anchor);
        self.pending_anchor_updates = true;
        Ok(())
    }

    /// Seal one masterchain block from current anchors + pending proofs (BFT round).
    pub fn seal_round(
        &mut self,
        prover: [u8; 20],
    ) -> Result<Option<MasterchainBlockV1>, MasterchainError> {
        if self.latest_anchors.is_empty() {
            return Ok(None);
        }
        if !self.pending_anchor_updates
            && self.pending_validity_proofs.is_empty()
            && self.pending_cross_shard_messages.is_empty()
            && self.pending_cross_zone_messages.is_empty()
            && self.pending_forced_inclusions.is_empty()
        {
            return Ok(None);
        }
        let flush_targets: Vec<(u32, u64)> = self
            .latest_anchors
            .values()
            .map(|a| (a.shard_id, a.block_height))
            .collect();
        for (shard_id, height) in flush_targets {
            self.flush_stwo_digests_for_anchor(shard_id, height, prover);
        }
        let anchors: Vec<ShardAnchor> = self.latest_anchors.values().cloned().collect();
        self.masterchain_height = self.masterchain_height.saturating_add(1);
        self.flush_due_forced_inclusions();
        let proofs = std::mem::take(&mut self.pending_validity_proofs);
        let cross_shard_messages = std::mem::take(&mut self.pending_cross_shard_messages);
        let proofs = match dedupe_submissions(&proofs) {
            Ok(proofs) => proofs,
            Err(err) => {
                for p in &proofs {
                    self.record_invalid_submission(p, submission_reason_code(&err));
                }
                return Err(MasterchainError::Submission(err));
            }
        };
        for p in &proofs {
            if let Err(err) = validate_proof_submission(p, &anchors) {
                self.record_invalid_submission(p, submission_reason_code(&err));
                return Err(MasterchainError::Submission(err));
            }
        }
        let mut verified_stwo_statements = Vec::with_capacity(proofs.len());
        for p in &proofs {
            let key = (p.shard_id, p.start_block, p.end_block, p.proof_digest);
            if let Some(stmt) = self.pending_verified_stwo_statements.remove(&key) {
                verified_stwo_statements.push(stmt);
            }
        }
        if self.proof_slashing_policy.enabled
            && self.proof_slashing_policy.require_verified_stwo
            && verified_stwo_statements.len() != proofs.len()
        {
            for p in &proofs {
                if !verified_stwo_statements
                    .iter()
                    .any(|s| s.matches_submission(p))
                {
                    self.record_invalid_submission(p, INVALID_PROOF_MISSING_VERIFIED_STWO);
                    return Err(MasterchainError::MissingVerifiedStwo {
                        shard_id: p.shard_id,
                        start: p.start_block,
                        end: p.end_block,
                    });
                }
            }
        } else if verified_stwo_statements.len() != proofs.len() {
            verified_stwo_statements.clear();
        }
        let global_state_root = fractal_shard::global_state_root_from_anchors(&anchors);
        let (global_zk_root, aggregated) = if proofs.is_empty() {
            ([0u8; 32], None)
        } else if verified_stwo_statements.len() == proofs.len() {
            let aggregated = match prove_and_aggregate_verified(
                self.masterchain_height,
                &global_state_root,
                &proofs,
                &verified_stwo_statements,
            ) {
                Ok(aggregated) => aggregated,
                Err(err) => {
                    for p in &proofs {
                        self.record_invalid_submission(p, INVALID_PROOF_AGGREGATOR_REJECTED);
                    }
                    return Err(MasterchainError::Aggregator(err));
                }
            };
            (aggregated.global_zk_root, Some(aggregated))
        } else {
            let aggregated =
                match prove_and_aggregate(self.masterchain_height, &global_state_root, &proofs) {
                    Ok(aggregated) => aggregated,
                    Err(err) => {
                        for p in &proofs {
                            self.record_invalid_submission(p, INVALID_PROOF_AGGREGATOR_REJECTED);
                        }
                        return Err(MasterchainError::Aggregator(err));
                    }
                };
            (aggregated.global_zk_root, Some(aggregated))
        };
        let mc = masterchain_block_from_anchors_and_messages(
            self.masterchain_height,
            anchors,
            proofs.clone(),
            global_zk_root,
            cross_shard_messages,
        );
        self.credit_accepted_proofs(&proofs);
        self.last_plonky2_bundle = aggregated.map(|agg| {
            Plonky2ProofBundleV1::from_aggregated(
                self.masterchain_height,
                GlobalZkStatementV1 {
                    global_state_root,
                    global_zk_root,
                    validity_proofs: proofs,
                    verified_stwo_statements,
                },
                &agg,
            )
        });
        self.blocks.push(mc.clone());
        if self.blocks.len() > Self::MAX_BLOCKS {
            let drop = self.blocks.len() - Self::MAX_BLOCKS;
            self.blocks.drain(0..drop);
        }
        self.pending_anchor_updates = false;
        Ok(Some(mc))
    }

    fn flush_due_forced_inclusions(&mut self) {
        let height = self.masterchain_height;
        let mut pending = Vec::new();
        for request in self.pending_forced_inclusions.drain(..) {
            if request.deadline_masterchain_height <= height {
                self.forced_inclusion_events.push(ForcedInclusionEventV1 {
                    version: ForcedInclusionEventV1::VERSION,
                    sequencer_late_by_blocks: height
                        .saturating_sub(request.deadline_masterchain_height),
                    included_at_masterchain_height: height,
                    request,
                });
            } else {
                pending.push(request);
            }
        }
        self.pending_forced_inclusions = pending;
    }

    /// Shard-embedded path: ingest + seal in one step.
    pub fn seal_anchor(
        &mut self,
        anchor: ShardAnchor,
        prover: [u8; 20],
    ) -> Result<MasterchainBlockV1, MasterchainError> {
        self.ingest_shard_anchor(anchor)?;
        self.seal_round(prover)?
            .ok_or(MasterchainError::NothingToSeal)
    }

    #[must_use]
    pub fn head(&self) -> Option<&MasterchainBlockV1> {
        self.blocks.last()
    }

    #[must_use]
    pub fn global_zk_root(&self) -> Option<[u8; 32]> {
        self.head().map(|b| b.global_zk_root)
    }

    #[must_use]
    pub fn plonky2_bundle(&self) -> Option<&Plonky2ProofBundleV1> {
        self.last_plonky2_bundle.as_ref()
    }

    #[must_use]
    pub fn anchor_for_shard(&self, shard_id: u32) -> Option<&ShardAnchor> {
        self.latest_anchors.get(&shard_id)
    }

    fn record_invalid_submission(&mut self, sub: &ProofSubmissionV1, reason_code: u8) {
        if !self.proof_slashing_policy.enabled {
            return;
        }
        let evidence_hash = invalid_proof_evidence_hash(
            self.masterchain_height,
            sub,
            reason_code,
            self.proof_slashing_policy.slash_amount_wei,
        );
        if self
            .invalid_proof_slash_events
            .iter()
            .any(|e| e.evidence_hash == evidence_hash)
        {
            return;
        }
        let execution = self.execute_invalid_proof_slash(sub.prover);
        self.invalid_proof_slash_events
            .push(InvalidProofSlashEventV1 {
                version: InvalidProofSlashEventV1::VERSION,
                masterchain_height: self.masterchain_height,
                prover: sub.prover,
                shard_id: sub.shard_id,
                start_block: sub.start_block,
                end_block: sub.end_block,
                proof_digest: sub.proof_digest,
                reason_code,
                evidence_hash,
                slash_amount_wei: self.proof_slashing_policy.slash_amount_wei,
                executed: execution.executed,
                burned_bond_wei: execution.burned_bond_wei,
                bond_before_wei: execution.bond_before_wei,
                bond_after_wei: execution.bond_after_wei,
                prover_active_after: execution.prover_active_after,
            });
    }

    fn execute_invalid_proof_slash(&mut self, prover: [u8; 20]) -> SlashExecutionOutcome {
        let Some(identity) = self.prover_identities.get_mut(&prover) else {
            return SlashExecutionOutcome::default();
        };
        if !identity.active {
            return SlashExecutionOutcome {
                executed: false,
                burned_bond_wei: 0,
                bond_before_wei: identity.bond_wei,
                bond_after_wei: identity.bond_wei,
                prover_active_after: false,
            };
        }
        let before = identity.bond_wei;
        let burned = before.min(self.proof_slashing_policy.slash_amount_wei);
        identity.bond_wei = identity.bond_wei.saturating_sub(burned);
        if identity.bond_wei < self.prover_market.min_identity_bond_wei {
            identity.active = false;
        }
        SlashExecutionOutcome {
            executed: true,
            burned_bond_wei: burned,
            bond_before_wei: before,
            bond_after_wei: identity.bond_wei,
            prover_active_after: identity.active,
        }
    }

    fn validate_prover_market_submission(
        &self,
        sub: &ProofSubmissionV1,
    ) -> Result<(), MasterchainError> {
        if !self.prover_market.enabled {
            return Ok(());
        }
        let range = proof_range_len(sub);
        if range > self.prover_market.max_range_blocks {
            return Err(MasterchainError::ProofRangeTooLarge {
                range,
                max: self.prover_market.max_range_blocks,
            });
        }
        if self.prover_market.require_registered_identity {
            let identity = self
                .prover_identities
                .get(&sub.prover)
                .filter(|id| id.active)
                .ok_or(MasterchainError::ProverIdentityRequired)?;
            if identity.bond_wei < self.prover_market.min_identity_bond_wei {
                return Err(MasterchainError::ProverBondTooLow {
                    bond: identity.bond_wei,
                    min: self.prover_market.min_identity_bond_wei,
                });
            }
        }
        if self.prover_market.max_pending_submissions_per_prover != u32::MAX {
            let pending = self
                .pending_validity_proofs
                .iter()
                .filter(|p| p.prover == sub.prover)
                .count() as u32;
            if pending >= self.prover_market.max_pending_submissions_per_prover {
                return Err(MasterchainError::ProverPendingLimit);
            }
        }
        Ok(())
    }

    fn credit_accepted_proofs(&mut self, proofs: &[ProofSubmissionV1]) {
        if !self.prover_economics.enabled || self.prover_economics.base_reward_per_block_wei == 0 {
            return;
        }
        for proof in proofs {
            let reward = prover_reward_wei(&self.prover_economics, proof);
            if reward == 0 || self.treasury_balance_wei == 0 {
                continue;
            }
            let paid = reward.min(self.treasury_balance_wei);
            self.treasury_balance_wei -= paid;
            let credit = self
                .prover_reward_credits_wei
                .entry(proof.prover)
                .or_default();
            *credit = credit.saturating_add(paid);
            self.prover_reward_events.push(ProverRewardEventV1 {
                version: ProverRewardEventV1::VERSION,
                masterchain_height: self.masterchain_height,
                prover: proof.prover,
                shard_id: proof.shard_id,
                start_block: proof.start_block,
                end_block: proof.end_block,
                lag_seconds: proof.lag_seconds,
                covered_blocks: proof_range_len(proof),
                reward_wei: paid,
                treasury: self.prover_economics.treasury,
                treasury_balance_after_wei: self.treasury_balance_wei,
            });
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct SlashExecutionOutcome {
    executed: bool,
    burned_bond_wei: u128,
    bond_before_wei: u128,
    bond_after_wei: u128,
    prover_active_after: bool,
}

#[must_use]
pub fn submission_reason_code(err: &SubmissionError) -> u8 {
    match err {
        SubmissionError::EmptyDigest => INVALID_PROOF_EMPTY_DIGEST,
        SubmissionError::InvalidRange => INVALID_PROOF_BAD_RANGE,
        SubmissionError::UnknownShard(_) => INVALID_PROOF_UNKNOWN_SHARD,
        SubmissionError::RangeExceedsAnchor { .. } => INVALID_PROOF_RANGE_EXCEEDS_ANCHOR,
        SubmissionError::Duplicate { .. } => INVALID_PROOF_DUPLICATE,
    }
}

#[must_use]
pub fn invalid_proof_evidence_hash(
    masterchain_height: u64,
    sub: &ProofSubmissionV1,
    reason_code: u8,
    slash_amount_wei: u128,
) -> Hash256 {
    #[derive(BorshSerialize)]
    struct Evidence<'a> {
        tag: [u8; 16],
        masterchain_height: u64,
        submission: &'a ProofSubmissionV1,
        reason_code: u8,
        slash_amount_wei: u128,
    }
    let bytes = borsh::to_vec(&Evidence {
        tag: *b"FRAC_BAD_PROOF__",
        masterchain_height,
        submission: sub,
        reason_code,
        slash_amount_wei,
    })
    .expect("invalid proof evidence borsh");
    keccak256(&bytes)
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
pub fn proof_range_len(sub: &ProofSubmissionV1) -> u64 {
    sub.end_block
        .saturating_sub(sub.start_block)
        .saturating_add(1)
}

/// PRD §6.2.4 payout curve:
/// `reward = base_per_block * range_len * half_life / (lag_seconds + half_life)`.
#[must_use]
pub fn prover_reward_wei(params: &ProverEconomicsParamsV1, sub: &ProofSubmissionV1) -> u128 {
    if !params.enabled || params.base_reward_per_block_wei == 0 || sub.start_block > sub.end_block {
        return 0;
    }
    let range_reward = params
        .base_reward_per_block_wei
        .saturating_mul(u128::from(proof_range_len(sub)));
    let half_life = u128::from(params.lag_half_life_seconds.max(1));
    let denominator = u128::from(sub.lag_seconds).saturating_add(half_life);
    range_reward.saturating_mul(half_life) / denominator
}

#[must_use]
pub fn anchor_from_block_header(header: &fractal_consensus::BlockHeader) -> ShardAnchor {
    shard_anchor_from_header(header.shard_id, header)
}
