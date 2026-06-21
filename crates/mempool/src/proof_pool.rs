//! Proof-update pool for the proof-ingestion lane.
//!
//! This pool is intentionally separate from the transaction mempool: proof
//! updates are keyed by `(zone_id, height)` and do not consume transaction gas
//! or participate in EIP-1559 transaction selection.

use std::collections::BTreeMap;

use fractal_consensus::ZoneProofUpdateV1;
use fractal_crypto::hash::Hash256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProofUpdateKey {
    pub zone_id: u64,
    pub height: u64,
}

impl From<&ZoneProofUpdateV1> for ProofUpdateKey {
    fn from(update: &ZoneProofUpdateV1) -> Self {
        Self {
            zone_id: update.zone_id,
            height: update.height,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PooledProofUpdate {
    pub update: ZoneProofUpdateV1,
    pub max_priority_fee: u128,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProofPoolConflictPolicy {
    /// Reject conflicts and only increment conflict metrics.
    Reject,
    /// Reject conflicts and retain both updates as evidence for later handling.
    RetainEvidence,
}

impl Default for ProofPoolConflictPolicy {
    fn default() -> Self {
        Self::Reject
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProofUpdateConflict {
    pub key: ProofUpdateKey,
    pub existing_digest: Hash256,
    pub conflicting_digest: Hash256,
    pub existing: ZoneProofUpdateV1,
    pub conflicting: ZoneProofUpdateV1,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProofPoolMetrics {
    pub pending_total: usize,
    pub inserted_total: u64,
    pub evicted_total: u64,
    pub drained_total: u64,
    pub conflict_total: u64,
    pub retained_conflicts: usize,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ProofPoolError {
    #[error("proof update conflicts with existing update for zone {key:?}")]
    Conflict { key: ProofUpdateKey },
}

#[derive(Clone, Debug, Default)]
pub struct ProofPool {
    pending: BTreeMap<ProofUpdateKey, PooledProofUpdate>,
    conflicts: Vec<ProofUpdateConflict>,
    metrics: ProofPoolMetrics,
    conflict_policy: ProofPoolConflictPolicy,
}

impl ProofPool {
    #[must_use]
    pub fn new(conflict_policy: ProofPoolConflictPolicy) -> Self {
        Self {
            conflict_policy,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    #[must_use]
    pub fn metrics(&self) -> ProofPoolMetrics {
        let mut metrics = self.metrics.clone();
        metrics.pending_total = self.pending.len();
        metrics.retained_conflicts = self.conflicts.len();
        metrics
    }

    #[must_use]
    pub fn conflicts(&self) -> &[ProofUpdateConflict] {
        &self.conflicts
    }

    #[must_use]
    pub fn get(&self, key: ProofUpdateKey) -> Option<&PooledProofUpdate> {
        self.pending.get(&key)
    }

    pub fn insert(&mut self, update: PooledProofUpdate) -> Result<(), ProofPoolError> {
        let key = ProofUpdateKey::from(&update.update);
        if let Some(existing) = self.pending.get(&key) {
            if existing.update == update.update {
                return Ok(());
            }

            self.metrics.conflict_total = self.metrics.conflict_total.saturating_add(1);
            if self.conflict_policy == ProofPoolConflictPolicy::RetainEvidence {
                self.conflicts.push(ProofUpdateConflict {
                    key,
                    existing_digest: existing.update.proof_digest,
                    conflicting_digest: update.update.proof_digest,
                    existing: existing.update.clone(),
                    conflicting: update.update,
                });
            }
            return Err(ProofPoolError::Conflict { key });
        }

        self.pending.insert(key, update);
        self.metrics.inserted_total = self.metrics.inserted_total.saturating_add(1);
        Ok(())
    }

    pub fn remove(&mut self, key: ProofUpdateKey) -> Option<PooledProofUpdate> {
        let removed = self.pending.remove(&key);
        if removed.is_some() {
            self.metrics.evicted_total = self.metrics.evicted_total.saturating_add(1);
        }
        removed
    }

    pub fn drain_ready(&mut self, max_updates: usize) -> Vec<ZoneProofUpdateV1> {
        let mut ready = self.pending.values().cloned().collect::<Vec<_>>();
        ready.sort_by(|a, b| {
            b.max_priority_fee
                .cmp(&a.max_priority_fee)
                .then_with(|| ProofUpdateKey::from(&a.update).cmp(&ProofUpdateKey::from(&b.update)))
        });

        let keys = ready
            .iter()
            .take(max_updates)
            .map(|p| ProofUpdateKey::from(&p.update))
            .collect::<Vec<_>>();
        let mut drained = Vec::with_capacity(keys.len());
        for key in keys {
            if let Some(update) = self.pending.remove(&key) {
                drained.push(update.update);
            }
        }
        self.metrics.drained_total = self
            .metrics
            .drained_total
            .saturating_add(drained.len() as u64);
        drained
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fractal_consensus::{CircuitVersion, ExecutionFeatureSetV1};

    fn update(zone_id: u64, height: u64, digest_byte: u8) -> ZoneProofUpdateV1 {
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
            proof_digest: [digest_byte; 32],
        }
    }

    fn pooled(zone_id: u64, height: u64, digest_byte: u8, tip: u128) -> PooledProofUpdate {
        PooledProofUpdate {
            update: update(zone_id, height, digest_byte),
            max_priority_fee: tip,
        }
    }

    #[test]
    fn stores_and_removes_by_zone_height() {
        let mut pool = ProofPool::default();
        pool.insert(pooled(7, 9, 1, 10)).unwrap();

        let key = ProofUpdateKey {
            zone_id: 7,
            height: 9,
        };
        assert!(pool.get(key).is_some());
        assert_eq!(pool.metrics().pending_total, 1);

        let removed = pool.remove(key).expect("removed");
        assert_eq!(removed.update.proof_digest, [1u8; 32]);
        assert!(pool.is_empty());
        assert_eq!(pool.metrics().evicted_total, 1);
    }

    #[test]
    fn rejects_conflicting_same_key_update() {
        let mut pool = ProofPool::default();
        pool.insert(pooled(7, 9, 1, 10)).unwrap();

        assert_eq!(
            pool.insert(pooled(7, 9, 2, 10)),
            Err(ProofPoolError::Conflict {
                key: ProofUpdateKey {
                    zone_id: 7,
                    height: 9,
                }
            })
        );
        assert_eq!(pool.metrics().conflict_total, 1);
        assert!(pool.conflicts().is_empty());
    }

    #[test]
    fn retain_evidence_policy_keeps_conflict_pair() {
        let mut pool = ProofPool::new(ProofPoolConflictPolicy::RetainEvidence);
        pool.insert(pooled(7, 9, 1, 10)).unwrap();
        assert!(pool.insert(pooled(7, 9, 2, 10)).is_err());

        assert_eq!(pool.conflicts().len(), 1);
        let conflict = &pool.conflicts()[0];
        assert_eq!(conflict.existing_digest, [1u8; 32]);
        assert_eq!(conflict.conflicting_digest, [2u8; 32]);
        assert_eq!(pool.metrics().retained_conflicts, 1);
    }

    #[test]
    fn drains_by_priority_and_updates_metrics() {
        let mut pool = ProofPool::default();
        pool.insert(pooled(1, 1, 1, 1)).unwrap();
        pool.insert(pooled(2, 1, 2, 9)).unwrap();
        pool.insert(pooled(3, 1, 3, 3)).unwrap();

        let drained = pool.drain_ready(2);

        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].proof_digest, [2u8; 32]);
        assert_eq!(drained[1].proof_digest, [3u8; 32]);
        assert_eq!(pool.len(), 1);
        assert_eq!(pool.metrics().inserted_total, 3);
        assert_eq!(pool.metrics().drained_total, 2);
    }

    #[test]
    fn duplicate_update_is_idempotent() {
        let mut pool = ProofPool::default();
        let update = pooled(7, 9, 1, 10);
        pool.insert(update.clone()).unwrap();
        pool.insert(update).unwrap();

        assert_eq!(pool.len(), 1);
        assert_eq!(pool.metrics().inserted_total, 1);
        assert_eq!(pool.metrics().conflict_total, 0);
    }
}
