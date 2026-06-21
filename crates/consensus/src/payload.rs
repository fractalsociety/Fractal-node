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

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum BlockPayloadItem {
    Transaction {
        transaction: Transaction,
        eth_signed_raw: Option<Vec<u8>>,
    },
    ProofUpdate(ZoneProofUpdateV1),
    CertificateBatch(OwnedObjectCertificateBatchV1),
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
    if let BlockPayloadItem::CertificateBatch(batch) = item {
        let batch_root = certificate_batch_root(batch)?;
        let mut bytes = PAYLOAD_LEAF_DOMAIN.to_vec();
        bytes.extend_from_slice(b"certificate_batch");
        bytes.extend_from_slice(&batch_root);
        return Ok(keccak256(&bytes));
    }
    let mut bytes = PAYLOAD_LEAF_DOMAIN.to_vec();
    bytes.extend_from_slice(&borsh::to_vec(item)?);
    Ok(keccak256(&bytes))
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

        let mut cases = Vec::new();

        let mut changed = base.clone();
        changed.zone_id += 1;
        cases.push(changed);

        let mut changed = base.clone();
        changed.parent_root = [9u8; 32];
        cases.push(changed);

        let mut changed = base.clone();
        changed.new_root = [9u8; 32];
        cases.push(changed);

        let mut changed = base.clone();
        changed.da_root = [9u8; 32];
        cases.push(changed);

        let mut changed = base.clone();
        changed.message_root = [9u8; 32];
        cases.push(changed);

        let mut changed = base.clone();
        changed.forced_inclusion_root = [9u8; 32];
        cases.push(changed);

        let mut changed = base.clone();
        changed.circuit_version = CircuitVersion::MixedStateTransitionV1;
        cases.push(changed);

        let mut changed = base.clone();
        changed.proof_digest = [7u8; 32];
        cases.push(changed);

        for changed in cases {
            assert_ne!(
                base_root,
                proof_updates_root(std::slice::from_ref(&changed)).unwrap()
            );
        }
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
}
