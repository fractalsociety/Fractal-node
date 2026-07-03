//! RLVR-046 / RLVR-050 hook: the committed-proof index.
//!
//! [`RlvrProofPool`](super::RlvrProofPool) only holds **pending** proofs (not yet
//! included in a block). Once a block includes an RLVR proof, the block-inclusion
//! path (RLVR-050) records it here, keyed by `proof_hash`, alongside the block
//! that committed it. [`fractal_get_rlvr_proof`](crate::fractal_get_rlvr_proof)
//! (RLVR-046) then answers both "is this proof pending?" and "which block
//! committed this proof?" without ever returning raw trace data.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use serde::{Deserialize, Serialize};

use crate::{RlvrError, RlvrProofObject, RlvrProofType};

/// Lifecycle status reported by `fractal_getRlvrProof`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RlvrProofStatus {
    Pending,
    Committed,
    NotFound,
}

impl RlvrProofStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Committed => "committed",
            Self::NotFound => "not_found",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().trim_matches('"') {
            "pending" | "Pending" => Some(Self::Pending),
            "committed" | "Committed" => Some(Self::Committed),
            "not_found" | "NotFound" => Some(Self::NotFound),
            _ => None,
        }
    }
}

/// The block that committed an RLVR proof. `block_hash` is a 64-char hex hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RlvrProofBlockReference {
    pub block_height: u64,
    pub block_hash: String,
}

impl RlvrProofBlockReference {
    pub fn validate(&self) -> Result<(), RlvrError> {
        if self.block_height == 0 {
            return Err(RlvrError::Config(
                "rlvr proof block_reference.block_height must be greater than zero".into(),
            ));
        }
        validate_hex_hash("block_reference.block_hash", &self.block_hash)
    }
}

/// A committed proof plus the block that committed it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommittedRlvrProof {
    pub proof: RlvrProofObject,
    pub block: RlvrProofBlockReference,
    /// Wall-clock ms at which the proof was committed (set by the caller).
    pub committed_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RlvrCommittedProofIndexMetrics {
    pub committed_total: usize,
    pub proof_of_route_total: usize,
    pub proof_of_eval_total: usize,
    pub proof_of_training_total: usize,
}

/// Index of RLVR proofs that have been included in a block, keyed by
/// `proof_hash`. The in-memory companion to the pending [`RlvrProofPool`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RlvrCommittedProofIndex {
    committed: BTreeMap<String, CommittedRlvrProof>,
    #[serde(default)]
    by_proof_type: BTreeMap<String, BTreeSet<String>>,
    #[serde(default)]
    by_adapter_hash: BTreeMap<String, BTreeSet<String>>,
    #[serde(default)]
    by_route_policy_hash: BTreeMap<String, BTreeSet<String>>,
    #[serde(default)]
    metrics: RlvrCommittedProofIndexMetrics,
}

impl RlvrCommittedProofIndex {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.committed.len()
    }

    pub fn is_empty(&self) -> bool {
        self.committed.is_empty()
    }

    pub fn metrics(&self) -> &RlvrCommittedProofIndexMetrics {
        &self.metrics
    }

    pub fn contains(&self, proof_hash: &str) -> bool {
        self.committed.contains_key(proof_hash)
    }

    pub fn get(&self, proof_hash: &str) -> Option<&CommittedRlvrProof> {
        self.committed.get(proof_hash)
    }

    pub fn status(&self, proof_hash: &str) -> RlvrProofStatus {
        if self.contains(proof_hash) {
            RlvrProofStatus::Committed
        } else {
            RlvrProofStatus::NotFound
        }
    }

    pub fn proof_hashes_by_type(&self, proof_type: RlvrProofType) -> Vec<String> {
        self.hashes_from_index(&self.by_proof_type, proof_type.as_str())
    }

    pub fn proof_hashes_by_adapter_hash(&self, adapter_hash: &str) -> Vec<String> {
        self.hashes_from_index(&self.by_adapter_hash, adapter_hash)
    }

    pub fn proof_hashes_by_route_policy_hash(&self, route_policy_hash: &str) -> Vec<String> {
        self.hashes_from_index(&self.by_route_policy_hash, route_policy_hash)
    }

    /// Record a proof as committed by `block`. Validates the proof signature,
    /// asserts `proof_hash` matches `proof.proof_hash()`, and rejects duplicates.
    pub fn insert(
        &mut self,
        proof_hash: &str,
        proof: RlvrProofObject,
        block: RlvrProofBlockReference,
        committed_at_ms: u64,
    ) -> Result<(), RlvrError> {
        validate_hex_hash("proof_hash", proof_hash)?;
        block.validate()?;
        if committed_at_ms == 0 {
            return Err(RlvrError::Config(
                "rlvr committed proof committed_at_ms must be greater than zero".into(),
            ));
        }
        proof.verify_node_signature()?;
        let computed = proof.proof_hash()?;
        if computed != proof_hash {
            return Err(RlvrError::Config(format!(
                "committed proof_hash {proof_hash} does not match proof.proof_hash() {computed}"
            )));
        }
        if self.committed.contains_key(proof_hash) {
            return Err(RlvrError::Config(format!(
                "committed rlvr proof index already contains proof_hash {proof_hash}"
            )));
        }
        self.committed.insert(
            proof_hash.into(),
            CommittedRlvrProof {
                proof,
                block,
                committed_at_ms,
            },
        );
        self.refresh_indexes_and_metrics();
        Ok(())
    }

    pub fn remove(&mut self, proof_hash: &str) -> Option<CommittedRlvrProof> {
        let removed = self.committed.remove(proof_hash);
        if removed.is_some() {
            self.refresh_indexes_and_metrics();
        }
        removed
    }

    pub fn list(&self) -> Vec<CommittedRlvrProof> {
        self.committed.values().cloned().collect()
    }

    pub fn save_to_path(&self, path: impl AsRef<Path>) -> Result<(), RlvrError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self, RlvrError> {
        let bytes = fs::read(path)?;
        let mut index: Self = serde_json::from_slice(&bytes)?;
        index.revalidate_committed_proofs()?;
        index.refresh_indexes_and_metrics();
        Ok(index)
    }

    fn hashes_from_index(
        &self,
        index: &BTreeMap<String, BTreeSet<String>>,
        key: &str,
    ) -> Vec<String> {
        index
            .get(key)
            .map(|hashes| hashes.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn revalidate_committed_proofs(&self) -> Result<(), RlvrError> {
        for (proof_hash, entry) in &self.committed {
            validate_hex_hash("proof_hash", proof_hash)?;
            entry.block.validate()?;
            if entry.committed_at_ms == 0 {
                return Err(RlvrError::Config(
                    "rlvr committed proof committed_at_ms must be greater than zero".into(),
                ));
            }
            entry.proof.verify_node_signature()?;
            let computed = entry.proof.proof_hash()?;
            if computed != *proof_hash {
                return Err(RlvrError::Config(format!(
                    "committed proof_hash {proof_hash} does not match proof.proof_hash() {computed}"
                )));
            }
        }
        Ok(())
    }

    fn refresh_indexes_and_metrics(&mut self) {
        self.by_proof_type.clear();
        self.by_adapter_hash.clear();
        self.by_route_policy_hash.clear();
        for (proof_hash, entry) in &self.committed {
            self.by_proof_type
                .entry(entry.proof.proof_type.as_str().into())
                .or_default()
                .insert(proof_hash.clone());
            if let Some(adapter_hash) = &entry.proof.adapter_hash {
                self.by_adapter_hash
                    .entry(adapter_hash.clone())
                    .or_default()
                    .insert(proof_hash.clone());
            }
            self.by_route_policy_hash
                .entry(entry.proof.route_policy_hash.clone())
                .or_default()
                .insert(proof_hash.clone());
        }

        self.metrics.committed_total = self.committed.len();
        self.metrics.proof_of_route_total = self
            .committed
            .values()
            .filter(|entry| entry.proof.proof_type == RlvrProofType::ProofOfRoute)
            .count();
        self.metrics.proof_of_eval_total = self
            .committed
            .values()
            .filter(|entry| entry.proof.proof_type == RlvrProofType::ProofOfEval)
            .count();
        self.metrics.proof_of_training_total = self
            .committed
            .values()
            .filter(|entry| entry.proof.proof_type == RlvrProofType::ProofOfTraining)
            .count();
    }
}

fn validate_hex_hash(name: &str, value: &str) -> Result<(), RlvrError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RlvrError::Config(format!(
            "{name} must be a 64-character hex hash"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hash_bytes, NodeSigningKey, TraceHashCommitment};

    fn block_ref(height: u64) -> RlvrProofBlockReference {
        RlvrProofBlockReference {
            block_height: height,
            block_hash: hash_bytes(format!("block-{height}").as_bytes()),
        }
    }

    fn signed_proof() -> RlvrProofObject {
        let key = NodeSigningKey::from_seed("node-1", b"committed node seed").unwrap();
        RlvrProofObject::from_trace_commitment(
            RlvrProofType::ProofOfRoute,
            &commitment_fixture(),
            hash_bytes(b"reward-policy"),
            hash_bytes(b"router-policy"),
            hash_bytes(b"model-id"),
            42,
            "unsigned",
        )
        .with_rubric_hash(hash_bytes(b"rubric"))
        .sign_with_node_key(&key)
        .unwrap()
    }

    fn signed_training_proof() -> RlvrProofObject {
        let key = NodeSigningKey::from_seed("node-1", b"committed node seed").unwrap();
        RlvrProofObject::from_trace_commitment(
            RlvrProofType::ProofOfTraining,
            &commitment_fixture(),
            hash_bytes(b"reward-policy"),
            hash_bytes(b"router-policy"),
            hash_bytes(b"model-id"),
            43,
            "unsigned",
        )
        .with_rubric_hash(hash_bytes(b"rubric"))
        .with_adapter_hash(hash_bytes(b"adapter"))
        .with_eval_result_hash(hash_bytes(b"eval-result"))
        .sign_with_node_key(&key)
        .unwrap()
    }

    fn commitment_fixture() -> TraceHashCommitment {
        TraceHashCommitment {
            trace_id: "trace-1".into(),
            task_id: "task-1".into(),
            trace_hash: hash_bytes(b"trace"),
            redacted_trace_hash: hash_bytes(b"redacted"),
            verifier_outputs_hash: hash_bytes(b"verifier"),
            reward_vector_hash: hash_bytes(b"reward-vector"),
            privacy_tags: Vec::new(),
        }
    }

    #[test]
    fn status_round_trips_as_string() {
        for status in [
            RlvrProofStatus::Pending,
            RlvrProofStatus::Committed,
            RlvrProofStatus::NotFound,
        ] {
            assert_eq!(RlvrProofStatus::parse(status.as_str()), Some(status));
        }
        assert!(RlvrProofStatus::parse("nonsense").is_none());
    }

    #[test]
    fn block_reference_validates_height_and_hash() {
        block_ref(1).validate().unwrap();
        let mut bad = block_ref(1);
        bad.block_height = 0;
        assert!(bad.validate().is_err());
        let mut bad_hash = block_ref(1);
        bad_hash.block_hash = "not-a-hash".into();
        assert!(bad_hash.validate().is_err());
    }

    #[test]
    fn index_records_committed_proof_with_block_reference_and_metrics() {
        let proof = signed_proof();
        let hash = proof.proof_hash().unwrap();
        let mut index = RlvrCommittedProofIndex::new();

        index
            .insert(&hash, proof.clone(), block_ref(10), 1_700_000_000_000)
            .unwrap();

        assert_eq!(index.len(), 1);
        assert!(index.contains(&hash));
        let entry = index.get(&hash).unwrap();
        assert_eq!(entry.proof.proof_type, RlvrProofType::ProofOfRoute);
        assert_eq!(entry.block.block_height, 10);
        assert_eq!(entry.committed_at_ms, 1_700_000_000_000);
        assert_eq!(index.metrics().committed_total, 1);
        assert_eq!(index.metrics().proof_of_route_total, 1);
    }

    #[test]
    fn index_queries_committed_proofs_by_type_adapter_and_route_policy() {
        let route_proof = signed_proof();
        let route_hash = route_proof.proof_hash().unwrap();
        let training_proof = signed_training_proof();
        let training_hash = training_proof.proof_hash().unwrap();
        let adapter_hash = training_proof.adapter_hash.clone().unwrap();
        let route_policy_hash = route_proof.route_policy_hash.clone();
        let mut index = RlvrCommittedProofIndex::new();

        index
            .insert(&route_hash, route_proof, block_ref(10), 1_700_000_000_000)
            .unwrap();
        index
            .insert(
                &training_hash,
                training_proof,
                block_ref(11),
                1_700_000_000_001,
            )
            .unwrap();

        assert_eq!(index.status(&route_hash), RlvrProofStatus::Committed);
        assert_eq!(
            index.status(&hash_bytes(b"missing")),
            RlvrProofStatus::NotFound
        );
        assert_eq!(
            index.proof_hashes_by_type(RlvrProofType::ProofOfRoute),
            vec![route_hash.clone()]
        );
        assert_eq!(
            index.proof_hashes_by_type(RlvrProofType::ProofOfTraining),
            vec![training_hash.clone()]
        );
        assert_eq!(
            index.proof_hashes_by_adapter_hash(&adapter_hash),
            vec![training_hash.clone()]
        );
        let mut expected_route_policy_hashes = vec![route_hash, training_hash.clone()];
        expected_route_policy_hashes.sort();
        assert_eq!(
            index.proof_hashes_by_route_policy_hash(&route_policy_hash),
            expected_route_policy_hashes
        );
        assert_eq!(index.metrics().committed_total, 2);
        assert_eq!(index.metrics().proof_of_route_total, 1);
        assert_eq!(index.metrics().proof_of_training_total, 1);
    }

    #[test]
    fn index_persists_and_restores_latest_proof_status_after_restart() {
        let route_proof = signed_proof();
        let route_hash = route_proof.proof_hash().unwrap();
        let training_proof = signed_training_proof();
        let training_hash = training_proof.proof_hash().unwrap();
        let adapter_hash = training_proof.adapter_hash.clone().unwrap();
        let route_policy_hash = training_proof.route_policy_hash.clone();
        let mut index = RlvrCommittedProofIndex::new();
        index
            .insert(&route_hash, route_proof, block_ref(10), 1_700_000_000_000)
            .unwrap();
        index
            .insert(
                &training_hash,
                training_proof,
                block_ref(11),
                1_700_000_000_001,
            )
            .unwrap();

        let path = std::env::temp_dir().join(format!(
            "fractal-rlvr-committed-proof-index-{}.json",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        index.save_to_path(&path).unwrap();

        let restored = RlvrCommittedProofIndex::load_from_path(&path).unwrap();

        assert_eq!(restored.status(&route_hash), RlvrProofStatus::Committed);
        assert_eq!(restored.get(&training_hash).unwrap().block.block_height, 11);
        assert_eq!(
            restored.proof_hashes_by_adapter_hash(&adapter_hash),
            vec![training_hash.clone()]
        );
        let mut expected_route_policy_hashes = vec![route_hash, training_hash];
        expected_route_policy_hashes.sort();
        assert_eq!(
            restored.proof_hashes_by_route_policy_hash(&route_policy_hash),
            expected_route_policy_hashes
        );
        assert_eq!(restored.metrics().committed_total, 2);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn index_rejects_mismatched_proof_hash_and_duplicate() {
        let proof = signed_proof();
        let real_hash = proof.proof_hash().unwrap();
        let wrong_hash = hash_bytes(b"different");
        let mut index = RlvrCommittedProofIndex::new();

        let err = index
            .insert(&wrong_hash, proof.clone(), block_ref(10), 1)
            .unwrap_err();
        assert!(err
            .to_string()
            .contains("does not match proof.proof_hash()"));

        index
            .insert(&real_hash, proof.clone(), block_ref(10), 1)
            .unwrap();
        let err = index
            .insert(&real_hash, proof, block_ref(11), 2)
            .unwrap_err();
        assert!(err.to_string().contains("already contains proof_hash"));
        assert_eq!(index.len(), 1);
    }

    #[test]
    fn index_remove_updates_metrics() {
        let proof = signed_proof();
        let hash = proof.proof_hash().unwrap();
        let mut index = RlvrCommittedProofIndex::new();
        index.insert(&hash, proof, block_ref(10), 1).unwrap();

        let removed = index.remove(&hash).unwrap();
        assert_eq!(removed.block.block_height, 10);
        assert!(index.is_empty());
        assert_eq!(index.metrics().committed_total, 0);
        assert_eq!(index.metrics().proof_of_route_total, 0);
    }
}
