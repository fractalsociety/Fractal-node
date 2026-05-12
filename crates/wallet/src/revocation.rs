//! Revocation set + Merkle root (`docs/wallet.md` §4.6).

use borsh::{BorshDeserialize, BorshSerialize};
use std::collections::BTreeMap;
use thiserror::Error;

use crate::capability::CapabilityId;
use crate::merkle::{self, MerkleRoot};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RevocationEntry {
    pub revoked_at_ms: u64,
    pub reason_code: u8,
    pub cascade: bool,
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

    /// Merkle root over sorted `cap_id` keys (commitment only — details live off-root in full nodes).
    pub fn root(&self) -> MerkleRoot {
        let keys: Vec<[u8; 32]> = self.inner.keys().copied().collect();
        merkle::root_from_sorted_leaves(&keys)
    }

    pub fn proof_for(&self, cap_id: &CapabilityId) -> Option<Vec<[u8; 32]>> {
        let keys: Vec<[u8; 32]> = self.inner.keys().copied().collect();
        merkle::merkle_proof(&keys, cap_id).map(|(_, p)| p)
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
}
