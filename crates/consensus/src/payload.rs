//! Versioned block payload contract for proof-ingestion work.
//!
//! The existing [`crate::Block`] wire shape remains the legacy full-transaction
//! encoding. This module defines the forward-compatible payload enum that newer
//! block modes will commit through a versioned payload root.

use borsh::{BorshDeserialize, BorshSerialize};
use fractal_core::{OwnedObjectCertificate, OwnedObjectVersion, Transaction};
use fractal_crypto::hash::{keccak256, Hash256};
use std::collections::BTreeSet;

use crate::{CircuitVersion, ExecutionFeatureSetV1};

pub const PAYLOAD_ROOT_DOMAIN: &[u8] = b"fractal:block-payload-root:v1";
pub const PAYLOAD_LEAF_DOMAIN: &[u8] = b"fractal:block-payload-leaf:v1";
pub const PROOF_UPDATE_LEAF_DOMAIN: &[u8] = b"fractal:proof-update-leaf:v1";
pub const CERTIFICATE_BATCH_LEAF_DOMAIN: &[u8] = b"fractal:certificate-batch-leaf:v1";
pub const CERTIFICATE_BATCH_ROOT_DOMAIN: &[u8] = b"fractal:certificate-batch-root:v1";
pub const RLVR_PROOF_LEAF_DOMAIN: &[u8] = b"fractal:rlvr-proof-leaf:v1";
pub const RLVR_PROOFS_ROOT_DOMAIN: &[u8] = b"fractal:rlvr-proofs-root:v1";

/// Versioned-root tag byte for the RLVR proofs commitment (distinct from the
/// `BlockPayloadKind` tags so RLVR proofs get their own domain without adding a
/// new payload kind).
pub const RLVR_PROOFS_ROOT_TAG: u8 = 4;

/// Proof-ingestion update committed by a base-chain block.
///
/// This is intentionally local to `fractal-consensus` so the payload contract
/// does not depend on shard orchestration crates. Follow-up tasks can add
/// conversions from shard-local proof-final update types.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ZoneProofUpdateV1 {
    pub zone_id: u64,
    pub height: u64,
    pub parent_root: Hash256,
    pub new_root: Hash256,
    pub tx_root: Hash256,
    pub da_root: Hash256,
    pub message_root: Hash256,
    pub forced_inclusion_root: Hash256,
    pub circuit_version: CircuitVersion,
    pub feature_set: ExecutionFeatureSetV1,
    pub proof_digest: Hash256,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct OwnedObjectCertificateBatchV1 {
    pub certificates: Vec<OwnedObjectCertificate>,
}

/// Which kind of RLVR proof a commitment binds (RLVR-040 proof types, mirrored
/// locally so the consensus payload contract does not depend on the RLVR crate).
#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum RlvrProofTypeTag {
    ProofOfRoute,
    ProofOfEval,
    ProofOfTraining,
}

impl RlvrProofTypeTag {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProofOfRoute => "proof_of_route",
            Self::ProofOfEval => "proof_of_eval",
            Self::ProofOfTraining => "proof_of_training",
        }
    }

    #[must_use]
    pub const fn tag(self) -> u8 {
        match self {
            Self::ProofOfRoute => 0,
            Self::ProofOfEval => 1,
            Self::ProofOfTraining => 2,
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "proof_of_route" | "proof-of-route" | "route" => Some(Self::ProofOfRoute),
            "proof_of_eval" | "proof-of-eval" | "eval" => Some(Self::ProofOfEval),
            "proof_of_training" | "proof-of-training" | "training" => Some(Self::ProofOfTraining),
            _ => None,
        }
    }
}

/// RLVR-047: a proof-of-route / proof-of-eval / proof-of-training commitment
/// carried inside a block payload. Mirrors [`ZoneProofUpdateV1`]'s "local to
/// consensus" contract: it binds **only hashes and metadata roots** — never raw
/// prompts, answers, traces, or adapter weights — so it is safe to commit
/// on-chain. The RLVR crate converts its `RlvrProofObject` into this at
/// inclusion time.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct RlvrProofCommitmentV1 {
    pub proof_type: RlvrProofTypeTag,
    /// Hash of the full local RLVR proof object.
    pub proof_hash: Hash256,
    pub trace_hash: Hash256,
    pub route_policy_hash: Hash256,
    pub reward_policy_hash: Hash256,
    pub model_id_hash: Hash256,
    pub adapter_hash: Hash256,
    pub eval_result_hash: Hash256,
    pub timestamp_unix: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum BlockPayloadItem {
    Transaction {
        transaction: Transaction,
        eth_signed_raw: Option<Vec<u8>>,
    },
    ProofUpdate(ZoneProofUpdateV1),
    CertificateBatch(OwnedObjectCertificateBatchV1),
    /// RLVR-047: hash-only proof-of-route/eval/training commitment.
    RlvrProof(RlvrProofCommitmentV1),
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum BlockPayload {
    FullTransactions {
        transactions: Vec<Transaction>,
        eth_signed_raw: Vec<Option<Vec<u8>>>,
    },
    ProofUpdates(Vec<ZoneProofUpdateV1>),
    CertificateBatches(Vec<OwnedObjectCertificateBatchV1>),
    Mixed(Vec<BlockPayloadItem>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockPayloadKind {
    FullTransactions,
    ProofUpdates,
    CertificateBatches,
    Mixed,
}

impl BlockPayloadKind {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FullTransactions => "full_transactions",
            Self::ProofUpdates => "proof_updates",
            Self::CertificateBatches => "certificate_batches",
            Self::Mixed => "mixed",
        }
    }
}

impl BlockPayload {
    #[must_use]
    pub fn kind(&self) -> BlockPayloadKind {
        match self {
            Self::FullTransactions { .. } => BlockPayloadKind::FullTransactions,
            Self::ProofUpdates(_) => BlockPayloadKind::ProofUpdates,
            Self::CertificateBatches(_) => BlockPayloadKind::CertificateBatches,
            Self::Mixed(_) => BlockPayloadKind::Mixed,
        }
    }

    /// Deterministic commitment root for the payload enum.
    pub fn payload_root(&self) -> Result<Hash256, std::io::Error> {
        let leaves = match self {
            Self::FullTransactions {
                transactions,
                eth_signed_raw,
            } => {
                let mut leaves = Vec::with_capacity(transactions.len());
                for (idx, tx) in transactions.iter().enumerate() {
                    let raw = eth_signed_raw.get(idx).cloned().unwrap_or(None);
                    leaves.push(payload_leaf_hash(&BlockPayloadItem::Transaction {
                        transaction: tx.clone(),
                        eth_signed_raw: raw,
                    })?);
                }
                leaves
            }
            Self::ProofUpdates(updates) => {
                return Ok(proof_updates_root(updates)?);
            }
            Self::CertificateBatches(batches) => {
                return Ok(certificate_batches_root(batches)?);
            }
            Self::Mixed(items) => items
                .iter()
                .map(payload_leaf_hash)
                .collect::<Result<Vec<_>, _>>()?,
        };
        Ok(versioned_payload_root(self.kind(), &leaves))
    }
}

#[must_use]
pub fn versioned_payload_root(kind: BlockPayloadKind, leaves: &[Hash256]) -> Hash256 {
    let root = merkle_root_from_hashes(leaves);
    let mut bytes = Vec::with_capacity(PAYLOAD_ROOT_DOMAIN.len() + 1 + 32);
    bytes.extend_from_slice(PAYLOAD_ROOT_DOMAIN);
    bytes.push(kind_tag(kind));
    bytes.extend_from_slice(&root);
    keccak256(&bytes)
}

pub fn payload_leaf_hash(item: &BlockPayloadItem) -> Result<Hash256, std::io::Error> {
    match item {
        BlockPayloadItem::CertificateBatch(batch) => {
            let batch_root = certificate_batch_root(batch)?;
            let mut bytes = PAYLOAD_LEAF_DOMAIN.to_vec();
            bytes.extend_from_slice(b"certificate_batch");
            bytes.extend_from_slice(&batch_root);
            Ok(keccak256(&bytes))
        }
        BlockPayloadItem::RlvrProof(commitment) => rlvr_proof_leaf_hash(commitment),
        _ => {
            let mut bytes = PAYLOAD_LEAF_DOMAIN.to_vec();
            bytes.extend_from_slice(&borsh::to_vec(item)?);
            Ok(keccak256(&bytes))
        }
    }
}

#[derive(BorshSerialize)]
struct ProofUpdateRootLeaf {
    zone_id: u64,
    parent_root: Hash256,
    new_root: Hash256,
    da_root: Hash256,
    message_root: Hash256,
    forced_inclusion_root: Hash256,
    circuit_version: CircuitVersion,
    proof_digest: Hash256,
}

/// Deterministic root over proof-ingestion updates.
///
/// The leaf intentionally binds only the public-input fields required by the
/// proof-ingestion verifier contract. Order is significant.
pub fn proof_updates_root(updates: &[ZoneProofUpdateV1]) -> Result<Hash256, std::io::Error> {
    let leaves = updates
        .iter()
        .map(proof_update_leaf_hash)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(versioned_payload_root(
        BlockPayloadKind::ProofUpdates,
        &leaves,
    ))
}

pub fn proof_update_leaf_hash(update: &ZoneProofUpdateV1) -> Result<Hash256, std::io::Error> {
    let leaf = ProofUpdateRootLeaf {
        zone_id: update.zone_id,
        parent_root: update.parent_root,
        new_root: update.new_root,
        da_root: update.da_root,
        message_root: update.message_root,
        forced_inclusion_root: update.forced_inclusion_root,
        circuit_version: update.circuit_version,
        proof_digest: update.proof_digest,
    };
    let mut bytes = PROOF_UPDATE_LEAF_DOMAIN.to_vec();
    bytes.extend_from_slice(&borsh::to_vec(&leaf)?);
    Ok(keccak256(&bytes))
}

#[must_use]
pub fn certificate_batch_conflicts(batch: &OwnedObjectCertificateBatchV1) -> bool {
    let mut versions = BTreeSet::<OwnedObjectVersion>::new();
    for cert in &batch.certificates {
        for version in &cert.object_versions {
            if !versions.insert(version.clone()) {
                return true;
            }
        }
    }
    false
}

#[derive(BorshSerialize)]
struct CertificateBatchLeaf {
    certificate_hash: Hash256,
    object_versions: Vec<OwnedObjectVersion>,
}

pub fn certificate_batches_root(
    batches: &[OwnedObjectCertificateBatchV1],
) -> Result<Hash256, std::io::Error> {
    let roots = batches
        .iter()
        .map(certificate_batch_root)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(versioned_payload_root(
        BlockPayloadKind::CertificateBatches,
        &roots,
    ))
}

pub fn certificate_batch_root(
    batch: &OwnedObjectCertificateBatchV1,
) -> Result<Hash256, std::io::Error> {
    if certificate_batch_conflicts(batch) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "certificate batch contains duplicate object version",
        ));
    }
    let leaves = batch
        .certificates
        .iter()
        .map(certificate_batch_leaf_hash)
        .collect::<Result<Vec<_>, _>>()?;
    let root = merkle_root_from_hashes(&leaves);
    let mut bytes = Vec::with_capacity(CERTIFICATE_BATCH_ROOT_DOMAIN.len() + 32);
    bytes.extend_from_slice(CERTIFICATE_BATCH_ROOT_DOMAIN);
    bytes.extend_from_slice(&root);
    Ok(keccak256(&bytes))
}

pub fn certificate_batch_leaf_hash(
    certificate: &OwnedObjectCertificate,
) -> Result<Hash256, std::io::Error> {
    let leaf = CertificateBatchLeaf {
        certificate_hash: certificate
            .certificate_hash()
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?,
        object_versions: certificate.object_versions.clone(),
    };
    let mut bytes = CERTIFICATE_BATCH_LEAF_DOMAIN.to_vec();
    bytes.extend_from_slice(&borsh::to_vec(&leaf)?);
    Ok(keccak256(&bytes))
}

#[derive(BorshSerialize)]
struct RlvrProofRootLeaf {
    proof_type: u8,
    proof_hash: Hash256,
    trace_hash: Hash256,
    route_policy_hash: Hash256,
    reward_policy_hash: Hash256,
    model_id_hash: Hash256,
    adapter_hash: Hash256,
    eval_result_hash: Hash256,
    timestamp_unix: u64,
}

/// Hash a single RLVR proof commitment into a payload leaf. Binds every hash
/// field, the proof-type tag, and the timestamp — never raw trace content.
pub fn rlvr_proof_leaf_hash(commitment: &RlvrProofCommitmentV1) -> Result<Hash256, std::io::Error> {
    let leaf = RlvrProofRootLeaf {
        proof_type: commitment.proof_type.tag(),
        proof_hash: commitment.proof_hash,
        trace_hash: commitment.trace_hash,
        route_policy_hash: commitment.route_policy_hash,
        reward_policy_hash: commitment.reward_policy_hash,
        model_id_hash: commitment.model_id_hash,
        adapter_hash: commitment.adapter_hash,
        eval_result_hash: commitment.eval_result_hash,
        timestamp_unix: commitment.timestamp_unix,
    };
    let mut bytes = RLVR_PROOF_LEAF_DOMAIN.to_vec();
    bytes.extend_from_slice(&borsh::to_vec(&leaf)?);
    Ok(keccak256(&bytes))
}

/// Deterministic commitment root over a batch of RLVR proofs. Uses its own
/// domain + tag (`RLVR_PROOFS_ROOT_TAG`) so it is distinct from the
/// proof-update and certificate-batch roots, and is the hook RLVR-048 binds into
/// a header extension. Order is significant.
pub fn rlvr_proofs_root(commitments: &[RlvrProofCommitmentV1]) -> Result<Hash256, std::io::Error> {
    let leaves = commitments
        .iter()
        .map(rlvr_proof_leaf_hash)
        .collect::<Result<Vec<_>, _>>()?;
    let root = merkle_root_from_hashes(&leaves);
    let mut bytes = Vec::with_capacity(RLVR_PROOFS_ROOT_DOMAIN.len() + 1 + 32);
    bytes.extend_from_slice(RLVR_PROOFS_ROOT_DOMAIN);
    bytes.push(RLVR_PROOFS_ROOT_TAG);
    bytes.extend_from_slice(&root);
    Ok(keccak256(&bytes))
}

fn kind_tag(kind: BlockPayloadKind) -> u8 {
    match kind {
        BlockPayloadKind::FullTransactions => 0,
        BlockPayloadKind::ProofUpdates => 1,
        BlockPayloadKind::CertificateBatches => 2,
        BlockPayloadKind::Mixed => 3,
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use fractal_core::{
        NativeCall, OwnedObjectCertificate, OwnedObjectId, TxBody, VmKind, HARDHAT_DEFAULT_SIGNER_0,
    };
    use proptest::prelude::*;
    use std::collections::BTreeSet;

    fn noop(nonce: u64) -> Transaction {
        Transaction {
            signer: HARDHAT_DEFAULT_SIGNER_0,
            nonce,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        }
    }

    fn update(zone_id: u64, height: u64, byte: u8) -> ZoneProofUpdateV1 {
        ZoneProofUpdateV1 {
            zone_id,
            height,
            parent_root: [1u8; 32],
            new_root: [2u8; 32],
            tx_root: [3u8; 32],
            da_root: [4u8; 32],
            message_root: [5u8; 32],
            forced_inclusion_root: [6u8; 32],
            circuit_version: CircuitVersion::NativeStateTransitionV1,
            feature_set: ExecutionFeatureSetV1::empty(),
            proof_digest: [byte; 32],
        }
    }

    fn certificate(tx_hash_byte: u8, object_byte: u8, version: u64) -> OwnedObjectCertificate {
        OwnedObjectCertificate {
            tx_hash: [tx_hash_byte; 32],
            owner: [7u8; 20],
            signer_nonce: u64::from(tx_hash_byte),
            object_versions: vec![OwnedObjectVersion {
                object_id: OwnedObjectId::Agent(u64::from(object_byte)),
                version,
            }],
            signer_indices: Vec::new(),
            validator_signatures: Vec::new(),
        }
    }

    #[test]
    fn payload_root_is_stable_and_order_sensitive() {
        let payload = BlockPayload::FullTransactions {
            transactions: vec![noop(0), noop(1)],
            eth_signed_raw: vec![None, None],
        };
        assert_eq!(
            payload.payload_root().unwrap(),
            payload.payload_root().unwrap()
        );

        let reordered = BlockPayload::FullTransactions {
            transactions: vec![noop(1), noop(0)],
            eth_signed_raw: vec![None, None],
        };
        assert_ne!(
            payload.payload_root().unwrap(),
            reordered.payload_root().unwrap()
        );
    }

    #[test]
    fn payload_kind_is_reportable() {
        assert_eq!(
            BlockPayload::ProofUpdates(Vec::new()).kind().as_str(),
            "proof_updates"
        );
        assert_eq!(
            BlockPayload::CertificateBatches(Vec::new()).kind().as_str(),
            "certificate_batches"
        );
    }

    #[test]
    fn proof_updates_root_empty_is_versioned() {
        assert_eq!(
            proof_updates_root(&[]).unwrap(),
            BlockPayload::ProofUpdates(Vec::new())
                .payload_root()
                .unwrap()
        );
        assert_ne!(proof_updates_root(&[]).unwrap(), [0u8; 32]);
    }

    #[test]
    fn proof_updates_root_stable_for_single_and_multi() {
        let single = vec![update(1, 10, 9)];
        assert_eq!(
            proof_updates_root(&single).unwrap(),
            proof_updates_root(&single).unwrap()
        );

        let multi = vec![update(1, 10, 9), update(2, 11, 8)];
        assert_eq!(
            proof_updates_root(&multi).unwrap(),
            BlockPayload::ProofUpdates(multi.clone())
                .payload_root()
                .unwrap()
        );
        assert_ne!(
            proof_updates_root(&single).unwrap(),
            proof_updates_root(&multi).unwrap()
        );
    }

    #[test]
    fn proof_updates_root_is_order_sensitive() {
        let ordered = vec![update(1, 10, 9), update(2, 11, 8)];
        let reordered = vec![update(2, 11, 8), update(1, 10, 9)];

        assert_ne!(
            proof_updates_root(&ordered).unwrap(),
            proof_updates_root(&reordered).unwrap()
        );
    }

    #[test]
    fn proof_updates_root_binds_required_fields() {
        let base = update(1, 10, 9);
        let base_root = proof_updates_root(std::slice::from_ref(&base)).unwrap();

        let mut cases = Vec::<(&str, ZoneProofUpdateV1)>::new();

        let mut changed = base.clone();
        changed.zone_id += 1;
        cases.push(("zone_id", changed));

        let mut changed = base.clone();
        changed.parent_root = [9u8; 32];
        cases.push(("parent_root", changed));

        let mut changed = base.clone();
        changed.new_root = [9u8; 32];
        cases.push(("new_root", changed));

        let mut changed = base.clone();
        changed.da_root = [9u8; 32];
        cases.push(("da_root", changed));

        let mut changed = base.clone();
        changed.message_root = [9u8; 32];
        cases.push(("message_root", changed));

        let mut changed = base.clone();
        changed.forced_inclusion_root = [9u8; 32];
        cases.push(("forced_inclusion_root", changed));

        let mut changed = base.clone();
        changed.circuit_version = CircuitVersion::MixedStateTransitionV1;
        cases.push(("circuit_version", changed));

        let mut changed = base.clone();
        changed.proof_digest = [7u8; 32];
        cases.push(("proof_digest", changed));

        for (field, changed) in cases {
            assert_ne!(
                base_root,
                proof_updates_root(std::slice::from_ref(&changed)).unwrap(),
                "proof update root did not bind {field}"
            );
        }
    }

    #[test]
    fn proof_update_root_rejects_swap_bit_flip_stale_and_cross_root_confusion() {
        let first = update(1, 10, 9);
        let second = update(2, 11, 8);
        let root = proof_updates_root(&[first.clone(), second.clone()]).unwrap();

        let swapped = {
            let mut changed = first.clone();
            changed.new_root = first.parent_root;
            changed.parent_root = first.new_root;
            proof_updates_root(&[changed, second.clone()]).unwrap()
        };
        assert_ne!(root, swapped, "field swap must change proof root");

        let bit_flipped = {
            let mut changed = first.clone();
            changed.proof_digest[0] ^= 0x01;
            proof_updates_root(&[changed, second.clone()]).unwrap()
        };
        assert_ne!(root, bit_flipped, "single-bit flip must change proof root");

        let stale_replay = {
            let mut changed = first.clone();
            changed.parent_root = [0xAA; 32];
            proof_updates_root(&[changed, second]).unwrap()
        };
        assert_ne!(
            root, stale_replay,
            "stale parent root replay must change proof root"
        );

        let cert_batch = OwnedObjectCertificateBatchV1 {
            certificates: vec![certificate(1, 1, 10)],
        };
        assert_ne!(
            root,
            certificate_batches_root(&[cert_batch]).unwrap(),
            "proof update root must not be accepted as certificate batch root"
        );
    }

    #[test]
    fn proof_updates_root_does_not_bind_non_required_fields() {
        let base = update(1, 10, 9);
        let mut changed = base.clone();
        changed.height += 1;
        changed.tx_root = [9u8; 32];
        changed.feature_set = ExecutionFeatureSetV1::all_known();

        assert_eq!(
            proof_updates_root(std::slice::from_ref(&base)).unwrap(),
            proof_updates_root(std::slice::from_ref(&changed)).unwrap()
        );
    }

    #[test]
    fn certificate_batches_root_empty_is_versioned() {
        assert_eq!(
            certificate_batches_root(&[]).unwrap(),
            BlockPayload::CertificateBatches(Vec::new())
                .payload_root()
                .unwrap()
        );
        assert_ne!(certificate_batches_root(&[]).unwrap(), [0u8; 32]);
    }

    #[test]
    fn certificate_batch_root_is_deterministic_for_single_and_multi() {
        let single = OwnedObjectCertificateBatchV1 {
            certificates: vec![certificate(1, 1, 10)],
        };
        assert_eq!(
            certificate_batch_root(&single).unwrap(),
            certificate_batch_root(&single).unwrap()
        );

        let multi = OwnedObjectCertificateBatchV1 {
            certificates: vec![certificate(1, 1, 10), certificate(2, 2, 20)],
        };
        assert_eq!(
            certificate_batches_root(std::slice::from_ref(&multi)).unwrap(),
            BlockPayload::CertificateBatches(vec![multi.clone()])
                .payload_root()
                .unwrap()
        );
        assert_ne!(
            certificate_batch_root(&single).unwrap(),
            certificate_batch_root(&multi).unwrap()
        );
    }

    #[test]
    fn certificate_batch_root_binds_certificate_hashes_and_object_versions() {
        let base = OwnedObjectCertificateBatchV1 {
            certificates: vec![certificate(1, 1, 10)],
        };
        let base_root = certificate_batch_root(&base).unwrap();

        let mut changed_hash = base.clone();
        changed_hash.certificates[0].tx_hash = [9u8; 32];
        assert_ne!(
            base_root,
            certificate_batch_root(&changed_hash).unwrap(),
            "certificate hash mutation must change the batch root"
        );

        let mut changed_version = base.clone();
        changed_version.certificates[0].object_versions[0].version += 1;
        assert_ne!(
            base_root,
            certificate_batch_root(&changed_version).unwrap(),
            "object version mutation must change the batch root"
        );
    }

    #[test]
    fn certificate_batch_root_rejects_bit_flip_truncation_stale_and_cross_root_confusion() {
        let base = OwnedObjectCertificateBatchV1 {
            certificates: vec![certificate(1, 1, 10), certificate(2, 2, 20)],
        };
        let base_root = certificate_batch_root(&base).unwrap();

        let mut changed_hash = base.clone();
        changed_hash.certificates[0].tx_hash[0] ^= 0x01;
        assert_ne!(base_root, certificate_batch_root(&changed_hash).unwrap());

        let mut truncated = base.clone();
        truncated.certificates[0].validator_signatures.truncate(0);
        truncated.certificates[0].signer_indices.truncate(0);
        truncated.certificates[0].object_versions.truncate(0);
        assert_ne!(base_root, certificate_batch_root(&truncated).unwrap());

        let mut stale = base.clone();
        stale.certificates[0].object_versions[0].version = stale.certificates[0].object_versions[0]
            .version
            .saturating_sub(1);
        assert_ne!(base_root, certificate_batch_root(&stale).unwrap());

        assert_ne!(
            certificate_batches_root(std::slice::from_ref(&base)).unwrap(),
            proof_updates_root(&[update(1, 10, 9)]).unwrap(),
            "certificate batch root must not be accepted as proof update root"
        );
    }

    #[test]
    fn certificate_batch_root_rejects_duplicate_object_versions() {
        let dup_version = OwnedObjectVersion {
            object_id: OwnedObjectId::Agent(7),
            version: 3,
        };
        let mut first = certificate(1, 1, 10);
        first.object_versions = vec![dup_version.clone()];
        let mut second = certificate(2, 2, 20);
        second.object_versions = vec![dup_version];
        let batch = OwnedObjectCertificateBatchV1 {
            certificates: vec![first, second],
        };

        assert!(certificate_batch_conflicts(&batch));
        assert_eq!(
            certificate_batch_root(&batch).unwrap_err().kind(),
            std::io::ErrorKind::InvalidData
        );
        assert_eq!(
            BlockPayload::CertificateBatches(vec![batch])
                .payload_root()
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::InvalidData
        );
    }

    fn hash_from(seed: u8, salt: u8) -> Hash256 {
        let mut out = [0u8; 32];
        for (idx, byte) in out.iter_mut().enumerate() {
            *byte = seed.wrapping_add(salt).wrapping_add(idx as u8);
        }
        out
    }

    fn generated_update(seed: u8, height: u64) -> ZoneProofUpdateV1 {
        ZoneProofUpdateV1 {
            zone_id: u64::from(seed) + 1,
            height,
            parent_root: hash_from(seed, 1),
            new_root: hash_from(seed, 2),
            tx_root: hash_from(seed, 3),
            da_root: hash_from(seed, 4),
            message_root: hash_from(seed, 5),
            forced_inclusion_root: hash_from(seed, 6),
            circuit_version: if seed % 2 == 0 {
                CircuitVersion::NativeStateTransitionV1
            } else {
                CircuitVersion::MixedStateTransitionV1
            },
            feature_set: ExecutionFeatureSetV1::empty(),
            proof_digest: hash_from(seed, 7),
        }
    }

    fn generated_cert(seed: u8, idx: usize) -> OwnedObjectCertificate {
        certificate(seed, idx as u8, u64::from(seed) + idx as u64 + 1)
    }

    fn generated_zone_blob_commitment(
        seed: u8,
        sample_count: u32,
    ) -> crate::ZoneBlobDaCommitmentV1 {
        crate::ZoneBlobDaCommitmentV1 {
            namespace: *b"zoneprop",
            da_root: hash_from(seed, 9),
            byte_count: u64::from(seed) * 17 + 1,
            share_count: u32::from(seed % 16) + 1,
            share_size: 256 + u32::from(seed),
            sampling: crate::DaSamplingParamsV1 {
                seed: u64::from(seed) * 257,
                sample_count,
                min_samples: sample_count.min(4),
            },
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(64))]

        #[test]
        fn prop_proof_update_roots_are_deterministic_ordered_and_mutation_resistant(
            seeds in prop::collection::vec(any::<u8>(), 1..8)
        ) {
            let updates: Vec<_> = seeds
                .iter()
                .enumerate()
                .map(|(idx, seed)| generated_update(*seed, idx as u64 + 1))
                .collect();
            let root = proof_updates_root(&updates).unwrap();
            prop_assert_eq!(root, proof_updates_root(&updates).unwrap());
            prop_assert_eq!(root, BlockPayload::ProofUpdates(updates.clone()).payload_root().unwrap());

            let mut mutated = updates.clone();
            mutated[0].proof_digest[0] ^= 0x01;
            prop_assert_ne!(root, proof_updates_root(&mutated).unwrap());

            if updates.len() > 1 {
                let mut reordered = updates.clone();
                reordered.swap(0, updates.len() - 1);
                if proof_update_leaf_hash(&updates[0]).unwrap()
                    != proof_update_leaf_hash(&updates[updates.len() - 1]).unwrap()
                {
                    prop_assert_ne!(root, proof_updates_root(&reordered).unwrap());
                }
            }
        }

        #[test]
        fn prop_certificate_batch_roots_are_deterministic_ordered_and_mutation_resistant(
            seeds in prop::collection::vec(any::<u8>(), 1..8)
        ) {
            let certificates: Vec<_> = seeds
                .iter()
                .enumerate()
                .map(|(idx, seed)| generated_cert(*seed, idx))
                .collect();
            let batch = OwnedObjectCertificateBatchV1 { certificates };
            let root = certificate_batch_root(&batch).unwrap();
            prop_assert_eq!(root, certificate_batch_root(&batch).unwrap());
            prop_assert_eq!(
                certificate_batches_root(std::slice::from_ref(&batch)).unwrap(),
                BlockPayload::CertificateBatches(vec![batch.clone()]).payload_root().unwrap()
            );

            let mut mutated = batch.clone();
            mutated.certificates[0].tx_hash[0] ^= 0x01;
            prop_assert_ne!(root, certificate_batch_root(&mutated).unwrap());

            if batch.certificates.len() > 1 {
                let mut reordered = batch.clone();
                reordered.certificates.swap(0, batch.certificates.len() - 1);
                prop_assert_ne!(root, certificate_batch_root(&reordered).unwrap());
            }
        }

        #[test]
        fn prop_full_and_mixed_payload_roots_are_deterministic_and_ordered(
            nonces in prop::collection::vec(0u64..1_000_000, 1..8)
        ) {
            let transactions: Vec<_> = nonces.iter().copied().map(noop).collect();
            let payload = BlockPayload::FullTransactions {
                eth_signed_raw: vec![None; transactions.len()],
                transactions: transactions.clone(),
            };
            let root = payload.payload_root().unwrap();
            prop_assert_eq!(root, payload.payload_root().unwrap());

            let mixed_items: Vec<_> = transactions
                .iter()
                .cloned()
                .map(|transaction| BlockPayloadItem::Transaction {
                    transaction,
                    eth_signed_raw: None,
                })
                .collect();
            prop_assert_eq!(
                root,
                BlockPayload::FullTransactions {
                    eth_signed_raw: vec![None; transactions.len()],
                    transactions: transactions.clone(),
                }
                .payload_root()
                .unwrap()
            );
            prop_assert_ne!(
                root,
                BlockPayload::Mixed(mixed_items.clone()).payload_root().unwrap()
            );

            let mut mutated = transactions.clone();
            mutated[0].nonce = mutated[0].nonce.wrapping_add(1);
            prop_assert_ne!(
                root,
                BlockPayload::FullTransactions {
                    eth_signed_raw: vec![None; mutated.len()],
                    transactions: mutated,
                }
                .payload_root()
                .unwrap()
            );

            if transactions.len() > 1 {
                let mut reordered = transactions.clone();
                reordered.swap(0, transactions.len() - 1);
                if transactions[0] != transactions[transactions.len() - 1] {
                    prop_assert_ne!(
                        root,
                        BlockPayload::FullTransactions {
                            eth_signed_raw: vec![None; reordered.len()],
                            transactions: reordered,
                        }
                        .payload_root()
                        .unwrap()
                    );
                }
            }
        }

        #[test]
        fn prop_zone_blob_da_commitment_hash_is_deterministic_and_mutation_resistant(
            seed in any::<u8>(),
            sample_count in 1u32..64
        ) {
            let commitment = generated_zone_blob_commitment(seed, sample_count);
            let root = crate::zone_blob_da_commitment_hash(&commitment).unwrap();
            prop_assert_eq!(root, crate::zone_blob_da_commitment_hash(&commitment).unwrap());

            let mut mutated = commitment.clone();
            mutated.sampling.sample_count = mutated.sampling.sample_count.saturating_add(1);
            prop_assert_ne!(root, crate::zone_blob_da_commitment_hash(&mutated).unwrap());

            let mut mutated = commitment;
            mutated.da_root[0] ^= 0x01;
            prop_assert_ne!(root, crate::zone_blob_da_commitment_hash(&mutated).unwrap());
        }
    }

    #[test]
    fn generated_root_samples_have_no_trivial_collisions() {
        let mut proof_roots = BTreeSet::new();
        let mut cert_roots = BTreeSet::new();
        let mut da_roots = BTreeSet::new();

        for seed in 0u8..32 {
            proof_roots
                .insert(proof_updates_root(&[generated_update(seed, u64::from(seed))]).unwrap());
            cert_roots.insert(
                certificate_batch_root(&OwnedObjectCertificateBatchV1 {
                    certificates: vec![generated_cert(seed, seed as usize)],
                })
                .unwrap(),
            );
            da_roots.insert(
                crate::zone_blob_da_commitment_hash(&generated_zone_blob_commitment(
                    seed,
                    u32::from(seed % 8) + 1,
                ))
                .unwrap(),
            );
        }

        assert_eq!(proof_roots.len(), 32);
        assert_eq!(cert_roots.len(), 32);
        assert_eq!(da_roots.len(), 32);
    }

    // ----- RLVR-047: proof-of-route payload item -----

    fn rlvr_commitment(seed: u8, proof_type: RlvrProofTypeTag) -> RlvrProofCommitmentV1 {
        RlvrProofCommitmentV1 {
            proof_type,
            proof_hash: hash_from(seed, 1),
            trace_hash: hash_from(seed, 2),
            route_policy_hash: hash_from(seed, 3),
            reward_policy_hash: hash_from(seed, 4),
            model_id_hash: hash_from(seed, 5),
            adapter_hash: hash_from(seed, 6),
            eval_result_hash: hash_from(seed, 7),
            timestamp_unix: u64::from(seed) + 1_700_000_000,
        }
    }

    #[test]
    fn rlvr_proofs_root_is_stable_and_order_sensitive() {
        let one = vec![rlvr_commitment(1, RlvrProofTypeTag::ProofOfRoute)];
        assert_eq!(
            rlvr_proofs_root(&one).unwrap(),
            rlvr_proofs_root(&one).unwrap()
        );

        let multi = vec![
            rlvr_commitment(1, RlvrProofTypeTag::ProofOfRoute),
            rlvr_commitment(2, RlvrProofTypeTag::ProofOfEval),
        ];
        assert_ne!(
            rlvr_proofs_root(&one).unwrap(),
            rlvr_proofs_root(&multi).unwrap()
        );

        let reordered = vec![multi[1].clone(), multi[0].clone()];
        assert_ne!(
            rlvr_proofs_root(&multi).unwrap(),
            rlvr_proofs_root(&reordered).unwrap()
        );
    }

    #[test]
    fn rlvr_proofs_root_empty_is_versioned_and_nonzero() {
        let empty = rlvr_proofs_root(&[]).unwrap();
        assert_ne!(empty, [0u8; 32]);
        assert_eq!(empty, rlvr_proofs_root(&[]).unwrap());
    }

    #[test]
    fn rlvr_proofs_root_binds_every_hash_field_type_and_timestamp() {
        let base = rlvr_commitment(1, RlvrProofTypeTag::ProofOfRoute);
        let base_root = rlvr_proofs_root(std::slice::from_ref(&base)).unwrap();

        let mut cases = Vec::new();
        for (name, mut mutated) in [
            ("proof_hash", base.clone()),
            ("trace_hash", base.clone()),
            ("route_policy_hash", base.clone()),
            ("reward_policy_hash", base.clone()),
            ("model_id_hash", base.clone()),
            ("adapter_hash", base.clone()),
            ("eval_result_hash", base.clone()),
        ] {
            let target = match name {
                "proof_hash" => &mut mutated.proof_hash,
                "trace_hash" => &mut mutated.trace_hash,
                "route_policy_hash" => &mut mutated.route_policy_hash,
                "reward_policy_hash" => &mut mutated.reward_policy_hash,
                "model_id_hash" => &mut mutated.model_id_hash,
                "adapter_hash" => &mut mutated.adapter_hash,
                _ => &mut mutated.eval_result_hash,
            };
            target[0] ^= 0x01;
            cases.push((name, mutated));
        }

        let mut changed_type = base.clone();
        changed_type.proof_type = RlvrProofTypeTag::ProofOfTraining;
        cases.push(("proof_type", changed_type));

        let mut changed_ts = base.clone();
        changed_ts.timestamp_unix = changed_ts.timestamp_unix.wrapping_add(1);
        cases.push(("timestamp_unix", changed_ts));

        for (field, mutated) in cases {
            assert_ne!(
                base_root,
                rlvr_proofs_root(std::slice::from_ref(&mutated.clone())).unwrap(),
                "rlvr proofs root did not bind {field}"
            );
        }
    }

    #[test]
    fn rlvr_proofs_root_is_distinct_from_other_payload_roots() {
        let rlvr = rlvr_proofs_root(&[rlvr_commitment(1, RlvrProofTypeTag::ProofOfRoute)]).unwrap();
        // Cross-root confusion: an RLVR root must never validate as a proof-update
        // or certificate-batch root (different domains + tags).
        assert_ne!(rlvr, proof_updates_root(&[update(1, 10, 9)]).unwrap());
        assert_ne!(
            rlvr,
            certificate_batches_root(&[OwnedObjectCertificateBatchV1 {
                certificates: vec![certificate(1, 1, 10)]
            }])
            .unwrap()
        );
    }

    #[test]
    fn mixed_payload_commits_rlvr_proof_item_and_preserves_other_roots() {
        // Including an RLVR proof item in a Mixed payload changes its root.
        let tx_item = BlockPayloadItem::Transaction {
            transaction: noop(0),
            eth_signed_raw: None,
        };
        let rlvr_item =
            BlockPayloadItem::RlvrProof(rlvr_commitment(1, RlvrProofTypeTag::ProofOfRoute));

        let without = BlockPayload::Mixed(vec![tx_item.clone()])
            .payload_root()
            .unwrap();
        let with = BlockPayload::Mixed(vec![tx_item.clone(), rlvr_item.clone()])
            .payload_root()
            .unwrap();
        assert_ne!(without, with);

        // Reordering changes the Mixed root, and removing returns to the original.
        let reordered = BlockPayload::Mixed(vec![rlvr_item.clone(), tx_item.clone()])
            .payload_root()
            .unwrap();
        assert_ne!(with, reordered);
        assert_eq!(
            without,
            BlockPayload::Mixed(vec![tx_item]).payload_root().unwrap()
        );
    }

    #[test]
    fn rlvr_proof_commitment_carries_only_hashes_and_metadata() {
        // The commitment is a pure hash/metadata record (no raw prompt/answer/
        // trace strings). Its borsh encoding is fixed-size and contains no
        // embedded plaintext, so it is safe to commit on-chain.
        let commitment = rlvr_commitment(3, RlvrProofTypeTag::ProofOfTraining);
        let encoded = borsh::to_vec(&commitment).unwrap();
        // 1 (type tag) + 7*32 (hashes) + 8 (timestamp) = 233 bytes.
        assert_eq!(encoded.len(), 1 + 7 * 32 + 8);
        assert!(!encoded.windows(4).any(|w| w == b"raw_"));
    }

    #[test]
    fn rlvr_proof_type_tag_round_trips() {
        for tag in [
            RlvrProofTypeTag::ProofOfRoute,
            RlvrProofTypeTag::ProofOfEval,
            RlvrProofTypeTag::ProofOfTraining,
        ] {
            assert_eq!(RlvrProofTypeTag::parse(tag.as_str()), Some(tag));
        }
        assert!(RlvrProofTypeTag::parse("nonsense").is_none());
        assert_eq!(
            RlvrProofTypeTag::parse("proof-of-route"),
            Some(RlvrProofTypeTag::ProofOfRoute)
        );
    }
}
