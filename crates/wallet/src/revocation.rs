//! Revocation set + Merkle root / proofs (`docs/wallet.md` §4.6, §12.3).

use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::BTreeMap;
use thiserror::Error;

use crate::capability::CapabilityId;
use crate::merkle::MerkleRoot;
use crate::smt::{self, RevocationSparseTrie, SmtMembershipProof, SmtNonMembershipProof};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RevocationEntry {
    pub revoked_at_ms: u64,
    pub reason_code: u8,
    pub cascade: bool,
}

/// Leaf commitment in the revocation tree: `BLAKE3(domain || cap_id || borsh(entry))`.
#[must_use]
pub fn revocation_leaf_commitment(cap_id: &CapabilityId, entry: &RevocationEntry) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"fractal/wallet/revocation_leaf/v1");
    h.update(cap_id);
    h.update(
        &borsh::to_vec(entry).expect("RevocationEntry borsh"),
    );
    *h.finalize().as_bytes()
}

#[derive(Clone, Debug, Default)]
pub struct RevocationSet {
    inner: BTreeMap<CapabilityId, RevocationEntry>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RevocationError {
    #[error("capability already revoked")]
    Duplicate,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RevocationProofError {
    #[error("capability is revoked")]
    Revoked,
    #[error("ancestor revoked with cascade")]
    CascadeRevoked,
    #[error("revocation merkle root mismatch")]
    RootMismatch,
    #[error("non-membership proof invalid")]
    NonMembershipInvalid,
    #[error("membership proof invalid")]
    MembershipInvalid,
    #[error("membership entry has cascade=true")]
    CascadeNotAllowed,
}

/// Witness for one ancestor: absent from set, or present with `cascade == false`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum RevocationAncestorWitness {
    NotRevoked(SmtNonMembershipProof),
    RevokedNonCascade {
        entry: RevocationEntry,
        membership: SmtMembershipProof,
    },
}

/// Provider-facing bundle for verify-time non-revocation (`docs/wallet.md` §4.6 (c), §25.2 compact).
///
/// Phase 2: `revoked_leaf_count` + neighbor Merkle witnesses only (no full `revoked_cap_ids` list).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RevocationVerifyProof {
    pub revocation_root: MerkleRoot,
    /// Number of revoked capabilities in the tree at proof-build time (4 bytes vs N×32 witness list).
    pub revoked_leaf_count: u32,
    pub cap_id: CapabilityId,
    pub cap_not_revoked: SmtNonMembershipProof,
    pub ancestors: Vec<(CapabilityId, RevocationAncestorWitness)>,
}

impl RevocationVerifyProof {
    /// Borsh size of this proof (for provider latency budgets).
    #[must_use]
    pub fn encoded_len(&self) -> usize {
        borsh::to_vec(self).map(|v| v.len()).unwrap_or(0)
    }
}

impl RevocationSet {
    pub fn revoke(
        &mut self,
        cap_id: CapabilityId,
        entry: RevocationEntry,
    ) -> Result<(), RevocationError> {
        if self.inner.contains_key(&cap_id) {
            return Err(RevocationError::Duplicate);
        }
        self.inner.insert(cap_id, entry);
        Ok(())
    }

    pub fn get(&self, cap_id: &CapabilityId) -> Option<&RevocationEntry> {
        self.inner.get(cap_id)
    }

    pub fn from_entries(
        entries: impl IntoIterator<Item = (CapabilityId, RevocationEntry)>,
    ) -> Self {
        Self {
            inner: entries.into_iter().collect(),
        }
    }

    pub fn sorted_cap_ids(&self) -> Vec<CapabilityId> {
        self.inner.keys().copied().collect()
    }

    fn sparse_trie(&self) -> RevocationSparseTrie {
        let mut trie = RevocationSparseTrie::new();
        for (id, e) in &self.inner {
            trie.insert(*id, revocation_leaf_commitment(id, e));
        }
        trie
    }

    /// Sparse Merkle trie root over revoked capabilities (`docs/wallet.md` §4.6 SMT).
    pub fn root(&self) -> MerkleRoot {
        self.sparse_trie().root()
    }

    pub fn non_membership_proof(&self, cap_id: &CapabilityId) -> Option<SmtNonMembershipProof> {
        self.sparse_trie().non_membership_proof(cap_id)
    }

    /// Direct revoke on `cap_id`, or cascade revoke from an ancestor on `ancestor_chain` (closest root → leaf).
    pub fn is_revoked(&self, cap_id: &CapabilityId, ancestor_chain: &[CapabilityId]) -> bool {
        if self.inner.contains_key(cap_id) {
            return true;
        }
        for a in ancestor_chain {
            if let Some(e) = self.inner.get(a) {
                if e.cascade {
                    return true;
                }
            }
        }
        false
    }

    /// Build a verify bundle for `cap_id` + `ancestor_chain` (root → leaf order).
    pub fn build_verify_proof(
        &self,
        cap_id: CapabilityId,
        ancestor_chain: &[CapabilityId],
    ) -> Result<RevocationVerifyProof, RevocationProofError> {
        if self.is_revoked(&cap_id, ancestor_chain) {
            if self.inner.contains_key(&cap_id) {
                return Err(RevocationProofError::Revoked);
            }
            return Err(RevocationProofError::CascadeRevoked);
        }
        let trie = self.sparse_trie();
        let cap_not_revoked = trie
            .non_membership_proof(&cap_id)
            .ok_or(RevocationProofError::Revoked)?;
        let mut ancestors = Vec::with_capacity(ancestor_chain.len());
        for &aid in ancestor_chain {
            if let Some(entry) = self.inner.get(&aid) {
                if entry.cascade {
                    return Err(RevocationProofError::CascadeRevoked);
                }
                let membership = trie
                    .membership_proof(&aid)
                    .ok_or(RevocationProofError::MembershipInvalid)?;
                ancestors.push((
                    aid,
                    RevocationAncestorWitness::RevokedNonCascade {
                        entry: entry.clone(),
                        membership,
                    },
                ));
            } else {
                let proof = trie
                    .non_membership_proof(&aid)
                    .ok_or(RevocationProofError::NonMembershipInvalid)?;
                ancestors.push((aid, RevocationAncestorWitness::NotRevoked(proof)));
            }
        }
        Ok(RevocationVerifyProof {
            revocation_root: trie.root(),
            revoked_leaf_count: self.inner.len() as u32,
            cap_id,
            cap_not_revoked,
            ancestors,
        })
    }

    /// Verify capability is not revoked using a proof against `expected_root`.
    pub fn verify_not_revoked(
        expected_root: &MerkleRoot,
        cap_id: &CapabilityId,
        proof: &RevocationVerifyProof,
    ) -> Result<(), RevocationProofError> {
        if proof.revocation_root != *expected_root {
            return Err(RevocationProofError::RootMismatch);
        }
        if proof.cap_id != *cap_id {
            return Err(RevocationProofError::NonMembershipInvalid);
        }
        verify_proof_bundle(proof)
    }
}

/// Verify cryptographic non-revocation bundle (§4.6 (c) sparse Merkle trie).
pub fn verify_proof_bundle(proof: &RevocationVerifyProof) -> Result<(), RevocationProofError> {
    if proof.cap_not_revoked.cap_id != proof.cap_id {
        return Err(RevocationProofError::NonMembershipInvalid);
    }
    if !smt::verify_non_membership(&proof.revocation_root, &proof.cap_not_revoked) {
        return Err(RevocationProofError::NonMembershipInvalid);
    }

    for (aid, witness) in &proof.ancestors {
        match witness {
            RevocationAncestorWitness::NotRevoked(nm) => {
                if nm.cap_id != *aid {
                    return Err(RevocationProofError::NonMembershipInvalid);
                }
                if !smt::verify_non_membership(&proof.revocation_root, nm) {
                    return Err(RevocationProofError::NonMembershipInvalid);
                }
            }
            RevocationAncestorWitness::RevokedNonCascade { entry, membership } => {
                if entry.cascade {
                    return Err(RevocationProofError::CascadeNotAllowed);
                }
                if membership.cap_id != *aid {
                    return Err(RevocationProofError::MembershipInvalid);
                }
                let leaf = revocation_leaf_commitment(aid, entry);
                if membership.leaf != leaf {
                    return Err(RevocationProofError::MembershipInvalid);
                }
                if !smt::verify_membership(&proof.revocation_root, membership) {
                    return Err(RevocationProofError::MembershipInvalid);
                }
            }
        }
    }
    Ok(())
}

/// Full capability check: signature + optional time + revocation proof (`docs/wallet.md` §4.6).
pub fn verify_capability_with_revocation(
    token: &crate::capability::CapabilityToken,
    now_ms: u64,
    revocation_root: &MerkleRoot,
    ancestor_chain: &[CapabilityId],
    revocation_proof: &RevocationVerifyProof,
) -> Result<(), CapabilityRevocationVerifyError> {
    token
        .verify()
        .map_err(CapabilityRevocationVerifyError::Capability)?;
    token
        .verify_autonomous_tool_mask()
        .map_err(CapabilityRevocationVerifyError::Capability)?;
    if now_ms > 0 {
        token
            .verify_time(now_ms)
            .map_err(CapabilityRevocationVerifyError::Capability)?;
    }
    RevocationSet::verify_not_revoked(revocation_root, &token.body.cap_id, revocation_proof)?;
    for aid in ancestor_chain {
        if !revocation_proof
            .ancestors
            .iter()
            .any(|(id, _)| id == aid)
        {
            return Err(CapabilityRevocationVerifyError::AncestorChainMismatch);
        }
    }
    Ok(())
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CapabilityRevocationVerifyError {
    #[error(transparent)]
    Capability(#[from] crate::capability::CapabilityVerifyError),
    #[error(transparent)]
    Revocation(#[from] RevocationProofError),
    #[error("revocation proof missing ancestor from capability chain")]
    AncestorChainMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cascade_revokes_descendant_line() {
        let mut s = RevocationSet::default();
        let root = [7u8; 32];
        s.revoke(
            root,
            RevocationEntry {
                revoked_at_ms: 1,
                reason_code: 0,
                cascade: true,
            },
        )
        .unwrap();
        let child = [9u8; 32];
        assert!(s.is_revoked(&child, &[root]));
        assert!(!s.is_revoked(&child, &[]));
    }

    #[test]
    fn verify_proof_roundtrip_non_revoked() {
        let mut s = RevocationSet::default();
        let parent = [1u8; 32];
        s.revoke(
            parent,
            RevocationEntry {
                revoked_at_ms: 1,
                reason_code: 0,
                cascade: false,
            },
        )
        .unwrap();
        let child = [2u8; 32];
        let root = s.root();
        let proof = s.build_verify_proof(child, &[parent]).unwrap();
        RevocationSet::verify_not_revoked(&root, &child, &proof).unwrap();
    }

    #[test]
    fn verify_proof_rejects_cascade() {
        let mut s = RevocationSet::default();
        let parent = [1u8; 32];
        s.revoke(
            parent,
            RevocationEntry {
                revoked_at_ms: 1,
                reason_code: 0,
                cascade: true,
            },
        )
        .unwrap();
        let child = [2u8; 32];
        assert_eq!(
            s.build_verify_proof(child, &[parent]),
            Err(RevocationProofError::CascadeRevoked)
        );
    }

    #[test]
    fn smt_root_differs_from_legacy_sorted_merkle() {
        let mut s = RevocationSet::default();
        s.revoke(
            [1u8; 32],
            RevocationEntry {
                revoked_at_ms: 1,
                reason_code: 0,
                cascade: false,
            },
        )
        .unwrap();
        let smt_root = s.root();
        let sorted: Vec<_> = s
            .inner
            .iter()
            .map(|(id, e)| revocation_leaf_commitment(id, e))
            .collect();
        let legacy = crate::merkle::root_from_sorted_commitments(&sorted);
        assert_ne!(smt_root, legacy);
    }

    #[test]
    fn compact_proof_omits_full_revoked_id_list() {
        let mut s = RevocationSet::default();
        for i in 0..64u8 {
            s.revoke(
                [i; 32],
                RevocationEntry {
                    revoked_at_ms: i as u64,
                    reason_code: 0,
                    cascade: false,
                },
            )
            .unwrap();
        }
        let target = [0xff; 32];
        let proof = s.build_verify_proof(target, &[]).unwrap();
        assert_eq!(proof.revoked_leaf_count, 64);
        let bytes = borsh::to_vec(&proof).unwrap();
        assert!(
            bytes.len() < 64 * 32,
            "compact proof should not embed 64 cap ids ({} bytes)",
            bytes.len()
        );
        RevocationSet::verify_not_revoked(&proof.revocation_root, &target, &proof).unwrap();
    }

    #[test]
    fn leaf_commitment_root_changes_with_cascade() {
        let mut a = RevocationSet::default();
        let id = [1u8; 32];
        a.revoke(
            id,
            RevocationEntry {
                revoked_at_ms: 1,
                reason_code: 0,
                cascade: false,
            },
        )
        .unwrap();
        let r0 = a.root();
        let mut b = RevocationSet::default();
        b.revoke(
            id,
            RevocationEntry {
                revoked_at_ms: 1,
                reason_code: 0,
                cascade: true,
            },
        )
        .unwrap();
        assert_ne!(r0, b.root());
    }
}
