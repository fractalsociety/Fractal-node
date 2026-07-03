//! Hash-only Proof of Route, Proof of Eval, and Proof of Training chain integration.

pub mod committed;
pub mod dispute;

pub use committed::{
    CommittedRlvrProof, RlvrCommittedProofIndex, RlvrCommittedProofIndexMetrics,
    RlvrProofBlockReference, RlvrProofStatus,
};
pub use dispute::{
    RlvrDisputeRecord, RlvrDisputeStore, RlvrDisputeStoreMetrics, RlvrDisputeTarget,
};

use std::collections::BTreeMap;

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};

use crate::{hash_bytes, scan_privacy_tags, RlvrError, TraceHashCommitment};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RlvrProofType {
    ProofOfRoute,
    ProofOfEval,
    ProofOfTraining,
}

impl RlvrProofType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ProofOfRoute => "ProofOfRoute",
            Self::ProofOfEval => "ProofOfEval",
            Self::ProofOfTraining => "ProofOfTraining",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RlvrProofObject {
    pub proof_type: RlvrProofType,
    pub trace_hash: String,
    pub redacted_trace_hash: String,
    pub verifier_outputs_hash: String,
    pub rubric_hash: Option<String>,
    pub reward_policy_hash: String,
    pub reward_vector_hash: String,
    pub route_policy_hash: String,
    pub router_policy_hash: String,
    pub model_id_hash: String,
    pub adapter_hash: Option<String>,
    pub eval_hash: Option<String>,
    pub eval_result_hash: Option<String>,
    pub timestamp: u64,
    pub timestamp_ms: u64,
    pub node_id: Option<String>,
    pub node_public_key: Option<String>,
    pub node_signature: String,
}

#[derive(Serialize)]
struct CanonicalProofObjectPayload<'a> {
    proof_type: RlvrProofType,
    trace_hash: &'a str,
    redacted_trace_hash: &'a str,
    verifier_outputs_hash: &'a str,
    rubric_hash: Option<&'a str>,
    reward_policy_hash: &'a str,
    reward_vector_hash: &'a str,
    route_policy_hash: &'a str,
    router_policy_hash: &'a str,
    model_id_hash: &'a str,
    adapter_hash: Option<&'a str>,
    eval_hash: Option<&'a str>,
    eval_result_hash: Option<&'a str>,
    timestamp: u64,
    timestamp_ms: u64,
    node_id: Option<&'a str>,
    node_public_key: Option<&'a str>,
    node_signature: &'a str,
}

#[derive(Serialize)]
struct UnsignedCanonicalProofObjectPayload<'a> {
    proof_type: RlvrProofType,
    trace_hash: &'a str,
    redacted_trace_hash: &'a str,
    verifier_outputs_hash: &'a str,
    rubric_hash: Option<&'a str>,
    reward_policy_hash: &'a str,
    reward_vector_hash: &'a str,
    route_policy_hash: &'a str,
    router_policy_hash: &'a str,
    model_id_hash: &'a str,
    adapter_hash: Option<&'a str>,
    eval_hash: Option<&'a str>,
    eval_result_hash: Option<&'a str>,
    timestamp: u64,
    timestamp_ms: u64,
    node_id: Option<&'a str>,
    node_public_key: Option<&'a str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NodeSigningKey {
    pub node_id: String,
    pub public_key: String,
    secret_seed: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RlvrProofPoolMetrics {
    pub pending_total: usize,
    pub inserted_total: u64,
    pub duplicate_total: u64,
    pub proof_of_route_total: usize,
    pub proof_of_eval_total: usize,
    pub proof_of_training_total: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RlvrPooledProof {
    pub proof_hash: String,
    pub proof: RlvrProofObject,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RlvrProofPool {
    pending: BTreeMap<String, RlvrProofObject>,
    metrics: RlvrProofPoolMetrics,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RlvrProofBlockPayloadItem {
    pub proof_hash: String,
    pub proof_json: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RlvrAcceptedProofState {
    accepted: BTreeMap<String, RlvrProofObject>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RlvrBlockApplyReport {
    pub accepted_count: usize,
    pub accepted_hashes: Vec<String>,
}

impl RlvrProofObject {
    pub fn from_trace_commitment(
        proof_type: RlvrProofType,
        commitment: &TraceHashCommitment,
        reward_policy_hash: impl Into<String>,
        route_policy_hash: impl Into<String>,
        model_id_hash: impl Into<String>,
        timestamp_ms: u64,
        node_signature: impl Into<String>,
    ) -> Self {
        let route_policy_hash = route_policy_hash.into();
        Self {
            proof_type,
            trace_hash: commitment.trace_hash.clone(),
            redacted_trace_hash: commitment.redacted_trace_hash.clone(),
            verifier_outputs_hash: commitment.verifier_outputs_hash.clone(),
            rubric_hash: None,
            reward_policy_hash: reward_policy_hash.into(),
            reward_vector_hash: commitment.reward_vector_hash.clone(),
            route_policy_hash: route_policy_hash.clone(),
            router_policy_hash: route_policy_hash,
            model_id_hash: model_id_hash.into(),
            adapter_hash: None,
            eval_hash: None,
            eval_result_hash: None,
            timestamp: timestamp_ms,
            timestamp_ms,
            node_id: None,
            node_public_key: None,
            node_signature: node_signature.into(),
        }
    }

    pub fn with_rubric_hash(mut self, rubric_hash: impl Into<String>) -> Self {
        self.rubric_hash = Some(rubric_hash.into());
        self
    }

    pub fn with_adapter_hash(mut self, adapter_hash: impl Into<String>) -> Self {
        self.adapter_hash = Some(adapter_hash.into());
        self
    }

    pub fn with_eval_result_hash(mut self, eval_result_hash: impl Into<String>) -> Self {
        let eval_result_hash = eval_result_hash.into();
        self.eval_hash = Some(eval_result_hash.clone());
        self.eval_result_hash = Some(eval_result_hash);
        self
    }

    pub fn sign_with_node_key(mut self, key: &NodeSigningKey) -> Result<Self, RlvrError> {
        key.validate()?;
        self.node_id = Some(key.node_id.clone());
        self.node_public_key = Some(key.public_key.clone());
        let signature: Signature = key.signing_key().sign(&self.unsigned_canonical_bytes()?);
        self.node_signature = hex::encode(signature.to_bytes());
        self.verify_node_signature()?;
        Ok(self)
    }

    pub fn verify_node_signature(&self) -> Result<(), RlvrError> {
        self.validate_hash_only()?;
        let node_id = self.node_id.as_deref().ok_or_else(|| {
            RlvrError::Config("rlvr proof node_id is required for signature verification".into())
        })?;
        if node_id.trim().is_empty() {
            return Err(RlvrError::Config(
                "rlvr proof node_id cannot be empty".into(),
            ));
        }
        let public_key = self.node_public_key.as_deref().ok_or_else(|| {
            RlvrError::Config(
                "rlvr proof node_public_key is required for signature verification".into(),
            )
        })?;
        let verifying_key = VerifyingKey::from_bytes(&decode_32("node_public_key", public_key)?)
            .map_err(|err| RlvrError::Config(format!("invalid node_public_key: {err}")))?;
        let signature = Signature::from_bytes(&decode_64("node_signature", &self.node_signature)?);
        verifying_key
            .verify(&self.unsigned_canonical_bytes()?, &signature)
            .map_err(|err| RlvrError::Config(format!("invalid node signature: {err}")))
    }

    pub fn validate_hash_only(&self) -> Result<(), RlvrError> {
        for (name, value) in self.hash_fields() {
            validate_hex_hash(name, value)?;
        }
        if let Some(rubric_hash) = &self.rubric_hash {
            validate_hex_hash("rubric_hash", rubric_hash)?;
        }
        if let Some(adapter_hash) = &self.adapter_hash {
            validate_hex_hash("adapter_hash", adapter_hash)?;
        }
        if let Some(eval_hash) = &self.eval_hash {
            validate_hex_hash("eval_hash", eval_hash)?;
        }
        if let Some(eval_result_hash) = &self.eval_result_hash {
            validate_hex_hash("eval_result_hash", eval_result_hash)?;
        }
        match self.proof_type {
            RlvrProofType::ProofOfRoute => {}
            RlvrProofType::ProofOfEval => {
                if self.eval_result_hash.is_none() {
                    return Err(RlvrError::Config(
                        "ProofOfEval requires eval_result_hash".into(),
                    ));
                }
            }
            RlvrProofType::ProofOfTraining => {
                if self.adapter_hash.is_none() {
                    return Err(RlvrError::Config(
                        "ProofOfTraining requires adapter_hash".into(),
                    ));
                }
            }
        }
        if self.timestamp == 0 {
            return Err(RlvrError::Config(
                "rlvr proof timestamp must be greater than zero".into(),
            ));
        }
        if self.timestamp_ms == 0 {
            return Err(RlvrError::Config(
                "rlvr proof timestamp_ms must be greater than zero".into(),
            ));
        }
        if self.node_signature.trim().is_empty() {
            return Err(RlvrError::Config(
                "rlvr proof node_signature cannot be empty".into(),
            ));
        }
        if let Some(node_id) = &self.node_id {
            if node_id.trim().is_empty() {
                return Err(RlvrError::Config(
                    "rlvr proof node_id cannot be empty".into(),
                ));
            }
            validate_chain_safe_text("node_id", node_id)?;
        }
        if let Some(node_public_key) = &self.node_public_key {
            let _ = decode_32("node_public_key", node_public_key)?;
        }
        validate_chain_safe_text("node_signature", &self.node_signature)?;
        Ok(())
    }

    pub fn unsigned_canonical_bytes(&self) -> Result<Vec<u8>, RlvrError> {
        self.validate_hash_only()?;
        serde_json::to_vec(&UnsignedCanonicalProofObjectPayload {
            proof_type: self.proof_type,
            trace_hash: &self.trace_hash,
            redacted_trace_hash: &self.redacted_trace_hash,
            verifier_outputs_hash: &self.verifier_outputs_hash,
            rubric_hash: self.rubric_hash.as_deref(),
            reward_policy_hash: &self.reward_policy_hash,
            reward_vector_hash: &self.reward_vector_hash,
            route_policy_hash: &self.route_policy_hash,
            router_policy_hash: &self.router_policy_hash,
            model_id_hash: &self.model_id_hash,
            adapter_hash: self.adapter_hash.as_deref(),
            eval_hash: self.eval_hash.as_deref(),
            eval_result_hash: self.eval_result_hash.as_deref(),
            timestamp: self.timestamp,
            timestamp_ms: self.timestamp_ms,
            node_id: self.node_id.as_deref(),
            node_public_key: self.node_public_key.as_deref(),
        })
        .map_err(RlvrError::from)
    }

    pub fn canonical_bytes(&self) -> Result<Vec<u8>, RlvrError> {
        self.validate_hash_only()?;
        serde_json::to_vec(&CanonicalProofObjectPayload {
            proof_type: self.proof_type,
            trace_hash: &self.trace_hash,
            redacted_trace_hash: &self.redacted_trace_hash,
            verifier_outputs_hash: &self.verifier_outputs_hash,
            rubric_hash: self.rubric_hash.as_deref(),
            reward_policy_hash: &self.reward_policy_hash,
            reward_vector_hash: &self.reward_vector_hash,
            route_policy_hash: &self.route_policy_hash,
            router_policy_hash: &self.router_policy_hash,
            model_id_hash: &self.model_id_hash,
            adapter_hash: self.adapter_hash.as_deref(),
            eval_hash: self.eval_hash.as_deref(),
            eval_result_hash: self.eval_result_hash.as_deref(),
            timestamp: self.timestamp,
            timestamp_ms: self.timestamp_ms,
            node_id: self.node_id.as_deref(),
            node_public_key: self.node_public_key.as_deref(),
            node_signature: &self.node_signature,
        })
        .map_err(RlvrError::from)
    }

    pub fn proof_hash(&self) -> Result<String, RlvrError> {
        Ok(hash_bytes(&self.canonical_bytes()?))
    }

    pub fn stable_hash(&self) -> Result<String, RlvrError> {
        self.proof_hash()
    }

    fn hash_fields(&self) -> [(&'static str, &str); 8] {
        [
            ("trace_hash", &self.trace_hash),
            ("redacted_trace_hash", &self.redacted_trace_hash),
            ("verifier_outputs_hash", &self.verifier_outputs_hash),
            ("reward_policy_hash", &self.reward_policy_hash),
            ("reward_vector_hash", &self.reward_vector_hash),
            ("route_policy_hash", &self.route_policy_hash),
            ("router_policy_hash", &self.router_policy_hash),
            ("model_id_hash", &self.model_id_hash),
        ]
    }

    pub fn serialized_len(&self) -> Result<usize, RlvrError> {
        Ok(self.canonical_bytes()?.len())
    }
}

impl NodeSigningKey {
    pub fn from_seed(
        node_id: impl Into<String>,
        seed: impl AsRef<[u8]>,
    ) -> Result<Self, RlvrError> {
        let node_id = node_id.into();
        if node_id.trim().is_empty() {
            return Err(RlvrError::Config(
                "node signing key id cannot be empty".into(),
            ));
        }
        let seed_hash = blake3::hash(seed.as_ref());
        let secret_seed = *seed_hash.as_bytes();
        let signing_key = SigningKey::from_bytes(&secret_seed);
        let public_key = hex::encode(signing_key.verifying_key().to_bytes());
        Ok(Self {
            node_id,
            public_key,
            secret_seed,
        })
    }

    pub fn validate(&self) -> Result<(), RlvrError> {
        if self.node_id.trim().is_empty() {
            return Err(RlvrError::Config(
                "node signing key id cannot be empty".into(),
            ));
        }
        validate_chain_safe_text("node_id", &self.node_id)?;
        let _ = decode_32("node_public_key", &self.public_key)?;
        Ok(())
    }

    fn signing_key(&self) -> SigningKey {
        SigningKey::from_bytes(&self.secret_seed)
    }
}

impl RlvrProofPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn metrics(&self) -> &RlvrProofPoolMetrics {
        &self.metrics
    }

    pub fn get(&self, proof_hash: &str) -> Option<&RlvrProofObject> {
        self.pending.get(proof_hash)
    }

    pub fn insert(&mut self, proof: RlvrProofObject) -> Result<String, RlvrError> {
        proof.verify_node_signature()?;
        let proof_hash = proof.proof_hash()?;
        if self.pending.contains_key(&proof_hash) {
            self.metrics.duplicate_total += 1;
            return Err(RlvrError::Config(format!(
                "rlvr proof pool already contains proof_hash {proof_hash}"
            )));
        }
        self.pending.insert(proof_hash.clone(), proof);
        self.metrics.inserted_total += 1;
        self.refresh_metrics();
        Ok(proof_hash)
    }

    pub fn remove(&mut self, proof_hash: &str) -> Option<RlvrProofObject> {
        let removed = self.pending.remove(proof_hash);
        if removed.is_some() {
            self.refresh_metrics();
        }
        removed
    }

    pub fn drain_ready(&mut self, max_proofs: usize) -> Vec<RlvrPooledProof> {
        let proof_hashes = self
            .pending
            .keys()
            .take(max_proofs)
            .cloned()
            .collect::<Vec<_>>();
        let mut drained = Vec::with_capacity(proof_hashes.len());
        for proof_hash in proof_hashes {
            if let Some(proof) = self.pending.remove(&proof_hash) {
                drained.push(RlvrPooledProof { proof_hash, proof });
            }
        }
        if !drained.is_empty() {
            self.refresh_metrics();
        }
        drained
    }

    pub fn list(&self) -> Vec<RlvrPooledProof> {
        self.pending
            .iter()
            .map(|(proof_hash, proof)| RlvrPooledProof {
                proof_hash: proof_hash.clone(),
                proof: proof.clone(),
            })
            .collect()
    }

    fn refresh_metrics(&mut self) {
        self.metrics.pending_total = self.pending.len();
        self.metrics.proof_of_route_total = self
            .pending
            .values()
            .filter(|proof| proof.proof_type == RlvrProofType::ProofOfRoute)
            .count();
        self.metrics.proof_of_eval_total = self
            .pending
            .values()
            .filter(|proof| proof.proof_type == RlvrProofType::ProofOfEval)
            .count();
        self.metrics.proof_of_training_total = self
            .pending
            .values()
            .filter(|proof| proof.proof_type == RlvrProofType::ProofOfTraining)
            .count();
    }
}

impl RlvrAcceptedProofState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.accepted.len()
    }

    pub fn is_empty(&self) -> bool {
        self.accepted.is_empty()
    }

    pub fn get(&self, proof_hash: &str) -> Option<&RlvrProofObject> {
        self.accepted.get(proof_hash)
    }

    pub fn list(&self) -> Vec<RlvrPooledProof> {
        self.accepted
            .iter()
            .map(|(proof_hash, proof)| RlvrPooledProof {
                proof_hash: proof_hash.clone(),
                proof: proof.clone(),
            })
            .collect()
    }
}

pub fn apply_rlvr_proof_block_payload(
    state: &mut RlvrAcceptedProofState,
    payload: &[RlvrProofBlockPayloadItem],
) -> Result<RlvrBlockApplyReport, RlvrError> {
    let mut verified = Vec::with_capacity(payload.len());
    for (idx, item) in payload.iter().enumerate() {
        validate_hex_hash("rlvr_block_payload.proof_hash", &item.proof_hash)?;
        let proof: RlvrProofObject = serde_json::from_slice(&item.proof_json).map_err(|err| {
            RlvrError::Config(format!(
                "malformed RLVR proof payload at index {idx}: {err}"
            ))
        })?;
        proof.verify_node_signature()?;
        let computed_hash = proof.proof_hash()?;
        if computed_hash != item.proof_hash {
            return Err(RlvrError::Config(format!(
                "rlvr proof payload hash mismatch at index {idx}: expected {}, computed {}",
                item.proof_hash, computed_hash
            )));
        }
        verified.push((computed_hash, proof));
    }

    for (proof_hash, proof) in &verified {
        state.accepted.insert(proof_hash.clone(), proof.clone());
    }
    Ok(RlvrBlockApplyReport {
        accepted_count: verified.len(),
        accepted_hashes: verified
            .into_iter()
            .map(|(proof_hash, _proof)| proof_hash)
            .collect(),
    })
}

fn validate_hex_hash(name: &str, value: &str) -> Result<(), RlvrError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RlvrError::Config(format!(
            "{name} must be a 64-character hex hash"
        )));
    }
    Ok(())
}

fn decode_32(name: &str, value: &str) -> Result<[u8; 32], RlvrError> {
    let bytes =
        hex::decode(value).map_err(|_| RlvrError::Config(format!("{name} must be hex encoded")))?;
    bytes
        .try_into()
        .map_err(|_| RlvrError::Config(format!("{name} must decode to 32 bytes")))
}

fn decode_64(name: &str, value: &str) -> Result<[u8; 64], RlvrError> {
    let bytes =
        hex::decode(value).map_err(|_| RlvrError::Config(format!("{name} must be hex encoded")))?;
    bytes
        .try_into()
        .map_err(|_| RlvrError::Config(format!("{name} must decode to 64 bytes")))
}

fn validate_chain_safe_text(name: &str, value: &str) -> Result<(), RlvrError> {
    let lower = value.to_ascii_lowercase();
    if scan_privacy_tags(value).is_private
        || lower.contains("raw_prompt")
        || lower.contains("raw_answer")
        || lower.contains("api key")
        || lower.contains("private file")
        || lower.contains("file contents")
        || value.chars().any(char::is_whitespace)
    {
        return Err(RlvrError::Config(format!(
            "{name} must be a compact signature/reference and cannot contain raw user data"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hash_bytes;

    #[test]
    fn proof_schema_supports_route_eval_and_training_without_raw_data() {
        let commitment = commitment_fixture();
        let route_proof = RlvrProofObject::from_trace_commitment(
            RlvrProofType::ProofOfRoute,
            &commitment,
            hash_bytes(b"reward-policy"),
            hash_bytes(b"router-policy"),
            hash_bytes(b"model-id"),
            42,
            "sig-test",
        )
        .with_rubric_hash(hash_bytes(b"rubric"));
        route_proof.validate_hash_only().unwrap();

        let eval_proof = RlvrProofObject::from_trace_commitment(
            RlvrProofType::ProofOfEval,
            &commitment,
            hash_bytes(b"reward-policy"),
            hash_bytes(b"router-policy"),
            hash_bytes(b"model-id"),
            42,
            "sig-test",
        )
        .with_rubric_hash(hash_bytes(b"rubric"))
        .with_eval_result_hash(hash_bytes(b"eval-result"));
        eval_proof.validate_hash_only().unwrap();

        let training_proof = RlvrProofObject::from_trace_commitment(
            RlvrProofType::ProofOfTraining,
            &commitment,
            hash_bytes(b"reward-policy"),
            hash_bytes(b"router-policy"),
            hash_bytes(b"model-id"),
            42,
            "sig-test",
        )
        .with_rubric_hash(hash_bytes(b"rubric"))
        .with_adapter_hash(hash_bytes(b"adapter"))
        .with_eval_result_hash(hash_bytes(b"eval-result"));
        training_proof.validate_hash_only().unwrap();

        let json = serde_json::to_string(&training_proof).unwrap();
        for field in [
            "ProofOfTraining",
            "trace_hash",
            "rubric_hash",
            "reward_policy_hash",
            "router_policy_hash",
            "model_id_hash",
            "adapter_hash",
            "eval_result_hash",
            "timestamp",
            "node_signature",
        ] {
            assert!(json.contains(field), "missing schema field {field}");
        }
        for raw in ["raw prompt", "private answer", "rubric text"] {
            assert!(!json.contains(raw), "proof leaked raw data {raw}");
        }
    }

    #[test]
    fn proof_schema_rejects_missing_type_specific_hashes() {
        let commitment = commitment_fixture();
        let eval_proof = RlvrProofObject::from_trace_commitment(
            RlvrProofType::ProofOfEval,
            &commitment,
            hash_bytes(b"reward-policy"),
            hash_bytes(b"router-policy"),
            hash_bytes(b"model-id"),
            42,
            "sig-test",
        );
        assert!(eval_proof.validate_hash_only().is_err());

        let training_proof = RlvrProofObject::from_trace_commitment(
            RlvrProofType::ProofOfTraining,
            &commitment,
            hash_bytes(b"reward-policy"),
            hash_bytes(b"router-policy"),
            hash_bytes(b"model-id"),
            42,
            "sig-test",
        );
        assert!(training_proof.validate_hash_only().is_err());
    }

    #[test]
    fn proof_hash_uses_canonical_bytes_and_is_deterministic() {
        let proof = full_training_proof();
        let bytes_a = proof.canonical_bytes().unwrap();
        let bytes_b = proof.canonical_bytes().unwrap();

        assert_eq!(bytes_a, bytes_b);
        assert_eq!(proof.proof_hash().unwrap(), hash_bytes(&bytes_a));
        assert_eq!(proof.stable_hash().unwrap(), proof.proof_hash().unwrap());

        let canonical_json = String::from_utf8(bytes_a).unwrap();
        assert!(canonical_json.starts_with("{\"proof_type\":\"ProofOfTraining\""));
        assert!(canonical_json.contains("\"trace_hash\""));
        assert!(canonical_json.contains("\"node_signature\":\"sig-test\""));
    }

    #[test]
    fn proof_hash_changes_when_any_committed_field_changes() {
        let base = full_training_proof();
        let base_hash = base.proof_hash().unwrap();

        let mutations: Vec<(&str, RlvrProofObject)> = vec![
            ("proof_type", {
                let mut proof = base.clone();
                proof.proof_type = RlvrProofType::ProofOfEval;
                proof
            }),
            ("trace_hash", {
                let mut proof = base.clone();
                proof.trace_hash = hash_bytes(b"trace-mutated");
                proof
            }),
            ("redacted_trace_hash", {
                let mut proof = base.clone();
                proof.redacted_trace_hash = hash_bytes(b"redacted-mutated");
                proof
            }),
            ("verifier_outputs_hash", {
                let mut proof = base.clone();
                proof.verifier_outputs_hash = hash_bytes(b"verifier-mutated");
                proof
            }),
            ("rubric_hash", {
                let mut proof = base.clone();
                proof.rubric_hash = Some(hash_bytes(b"rubric-mutated"));
                proof
            }),
            ("reward_policy_hash", {
                let mut proof = base.clone();
                proof.reward_policy_hash = hash_bytes(b"reward-policy-mutated");
                proof
            }),
            ("reward_vector_hash", {
                let mut proof = base.clone();
                proof.reward_vector_hash = hash_bytes(b"reward-vector-mutated");
                proof
            }),
            ("route_policy_hash", {
                let mut proof = base.clone();
                proof.route_policy_hash = hash_bytes(b"route-policy-mutated");
                proof
            }),
            ("router_policy_hash", {
                let mut proof = base.clone();
                proof.router_policy_hash = hash_bytes(b"router-policy-mutated");
                proof
            }),
            ("model_id_hash", {
                let mut proof = base.clone();
                proof.model_id_hash = hash_bytes(b"model-id-mutated");
                proof
            }),
            ("adapter_hash", {
                let mut proof = base.clone();
                proof.adapter_hash = Some(hash_bytes(b"adapter-mutated"));
                proof
            }),
            ("eval_hash", {
                let mut proof = base.clone();
                proof.eval_hash = Some(hash_bytes(b"eval-mutated"));
                proof
            }),
            ("eval_result_hash", {
                let mut proof = base.clone();
                proof.eval_result_hash = Some(hash_bytes(b"eval-result-mutated"));
                proof
            }),
            ("timestamp", {
                let mut proof = base.clone();
                proof.timestamp += 1;
                proof
            }),
            ("timestamp_ms", {
                let mut proof = base.clone();
                proof.timestamp_ms += 1;
                proof
            }),
            ("node_signature", {
                let mut proof = base.clone();
                proof.node_signature = "sig-mutated".into();
                proof
            }),
        ];

        for (field, proof) in mutations {
            proof
                .validate_hash_only()
                .unwrap_or_else(|err| panic!("mutation for {field} should stay valid: {err}"));
            assert_ne!(
                proof.proof_hash().unwrap(),
                base_hash,
                "mutating {field} did not change proof hash"
            );
        }
    }

    #[test]
    fn proof_json_rejects_raw_prompt_field() {
        let mut raw = serde_json::to_value(full_training_proof()).unwrap();
        raw.as_object_mut()
            .unwrap()
            .insert("raw_prompt".into(), "What is my private API key?".into());

        let err = serde_json::from_value::<RlvrProofObject>(raw).unwrap_err();

        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn proof_json_rejects_raw_answer_field() {
        let mut raw = serde_json::to_value(full_training_proof()).unwrap();
        raw.as_object_mut().unwrap().insert(
            "raw_answer".into(),
            "The user's secret answer is 42.".into(),
        );

        let err = serde_json::from_value::<RlvrProofObject>(raw).unwrap_err();

        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn proof_validation_rejects_api_key_smuggled_into_signature() {
        let mut proof = full_training_proof();
        proof.node_signature = "sk-test-super-secret-token-1234567890abcdef".into();

        let err = proof.validate_hash_only().unwrap_err();

        assert!(err.to_string().contains("raw user data"));
    }

    #[test]
    fn proof_validation_rejects_private_file_contents_smuggled_into_signature() {
        let mut proof = full_training_proof();
        proof.node_signature = "private file contents from /Users/alice/tax.pdf".into();

        let err = proof.validate_hash_only().unwrap_err();

        assert!(err.to_string().contains("raw user data"));
    }

    #[test]
    fn proof_can_be_signed_and_verified_with_node_key() {
        let key = NodeSigningKey::from_seed("node-a", b"node-a-secret").unwrap();
        let proof = full_training_proof().sign_with_node_key(&key).unwrap();

        assert_eq!(proof.node_id.as_deref(), Some("node-a"));
        assert_eq!(
            proof.node_public_key.as_deref(),
            Some(key.public_key.as_str())
        );
        assert_eq!(proof.node_signature.len(), 128);
        proof.verify_node_signature().unwrap();

        let json = serde_json::to_string(&proof).unwrap();
        assert!(json.contains("node_id"));
        assert!(json.contains("node_public_key"));
    }

    #[test]
    fn invalid_node_signatures_fail_verification() {
        let key = NodeSigningKey::from_seed("node-a", b"node-a-secret").unwrap();
        let mut proof = full_training_proof().sign_with_node_key(&key).unwrap();
        proof.trace_hash = hash_bytes(b"tampered-trace");
        let err = proof.verify_node_signature().unwrap_err();
        assert!(err.to_string().contains("invalid node signature"));

        let mut proof = full_training_proof().sign_with_node_key(&key).unwrap();
        proof.node_signature = "00".repeat(64);
        let err = proof.verify_node_signature().unwrap_err();
        assert!(err.to_string().contains("invalid node signature"));
    }

    #[test]
    fn signature_verification_requires_node_identity_and_public_key() {
        let proof = full_training_proof();
        let err = proof.verify_node_signature().unwrap_err();
        assert!(err.to_string().contains("node_id"));

        let key = NodeSigningKey::from_seed("node-a", b"node-a-secret").unwrap();
        let mut proof = full_training_proof().sign_with_node_key(&key).unwrap();
        proof.node_public_key = None;
        let err = proof.verify_node_signature().unwrap_err();
        assert!(err.to_string().contains("node_public_key"));
    }

    #[test]
    fn proof_pool_holds_route_eval_and_training_proofs_with_metrics() {
        let key = NodeSigningKey::from_seed("node-a", b"node-a-secret").unwrap();
        let route_proof = full_route_proof().sign_with_node_key(&key).unwrap();
        let eval_proof = full_eval_proof().sign_with_node_key(&key).unwrap();
        let training_proof = full_training_proof().sign_with_node_key(&key).unwrap();

        let mut pool = RlvrProofPool::new();
        let route_hash = pool.insert(route_proof).unwrap();
        let eval_hash = pool.insert(eval_proof).unwrap();
        let training_hash = pool.insert(training_proof).unwrap();

        assert_eq!(pool.len(), 3);
        assert!(!pool.is_empty());
        assert!(pool.get(&route_hash).is_some());
        assert!(pool.get(&eval_hash).is_some());
        assert!(pool.get(&training_hash).is_some());

        let metrics = pool.metrics();
        assert_eq!(metrics.pending_total, 3);
        assert_eq!(metrics.inserted_total, 3);
        assert_eq!(metrics.duplicate_total, 0);
        assert_eq!(metrics.proof_of_route_total, 1);
        assert_eq!(metrics.proof_of_eval_total, 1);
        assert_eq!(metrics.proof_of_training_total, 1);

        let listed_hashes: Vec<_> = pool
            .list()
            .into_iter()
            .map(|pooled| pooled.proof_hash)
            .collect();
        let mut expected_hashes = vec![route_hash, eval_hash, training_hash];
        expected_hashes.sort();
        assert_eq!(listed_hashes, expected_hashes);
    }

    #[test]
    fn proof_pool_rejects_duplicate_proof_hashes() {
        let key = NodeSigningKey::from_seed("node-a", b"node-a-secret").unwrap();
        let proof = full_training_proof().sign_with_node_key(&key).unwrap();

        let mut pool = RlvrProofPool::new();
        pool.insert(proof.clone()).unwrap();
        let err = pool.insert(proof).unwrap_err();

        assert!(err.to_string().contains("already contains proof_hash"));
        assert_eq!(pool.metrics().pending_total, 1);
        assert_eq!(pool.metrics().inserted_total, 1);
        assert_eq!(pool.metrics().duplicate_total, 1);
    }

    #[test]
    fn proof_pool_rejects_invalid_signatures() {
        let key = NodeSigningKey::from_seed("node-a", b"node-a-secret").unwrap();
        let mut proof = full_training_proof().sign_with_node_key(&key).unwrap();
        proof.trace_hash = hash_bytes(b"tampered-trace");

        let mut pool = RlvrProofPool::new();
        let err = pool.insert(proof).unwrap_err();

        assert!(err.to_string().contains("invalid node signature"));
        assert!(pool.is_empty());
        assert_eq!(pool.metrics().pending_total, 0);
        assert_eq!(pool.metrics().inserted_total, 0);
    }

    #[test]
    fn proof_pool_remove_updates_pending_metrics() {
        let key = NodeSigningKey::from_seed("node-a", b"node-a-secret").unwrap();
        let route_proof = full_route_proof().sign_with_node_key(&key).unwrap();

        let mut pool = RlvrProofPool::new();
        let proof_hash = pool.insert(route_proof).unwrap();
        let removed = pool.remove(&proof_hash).unwrap();

        assert_eq!(removed.proof_type, RlvrProofType::ProofOfRoute);
        assert!(pool.is_empty());
        assert_eq!(pool.metrics().pending_total, 0);
        assert_eq!(pool.metrics().proof_of_route_total, 0);
        assert_eq!(pool.metrics().inserted_total, 1);
    }

    #[test]
    fn proof_pool_drain_ready_is_bounded_and_updates_pending_metrics() {
        let key = NodeSigningKey::from_seed("node-a", b"node-a-secret").unwrap();
        let route_proof = full_route_proof().sign_with_node_key(&key).unwrap();
        let eval_proof = full_eval_proof().sign_with_node_key(&key).unwrap();
        let training_proof = full_training_proof().sign_with_node_key(&key).unwrap();

        let mut pool = RlvrProofPool::new();
        pool.insert(route_proof).unwrap();
        pool.insert(eval_proof).unwrap();
        pool.insert(training_proof).unwrap();
        let expected = pool
            .list()
            .into_iter()
            .map(|pooled| pooled.proof_hash)
            .collect::<Vec<_>>();

        let drained = pool.drain_ready(2);
        assert_eq!(drained.len(), 2);
        assert_eq!(drained[0].proof_hash, expected[0]);
        assert_eq!(drained[1].proof_hash, expected[1]);
        assert_eq!(pool.len(), 1);
        assert!(pool.get(&expected[2]).is_some());
        assert_eq!(pool.metrics().pending_total, 1);
        assert_eq!(pool.metrics().inserted_total, 3);
    }

    #[test]
    fn block_apply_accepts_verified_rlvr_proofs_atomically() {
        let key = NodeSigningKey::from_seed("node-a", b"node-a-secret").unwrap();
        let route_proof = full_route_proof().sign_with_node_key(&key).unwrap();
        let eval_proof = full_eval_proof().sign_with_node_key(&key).unwrap();
        let payload = vec![payload_item(&route_proof), payload_item(&eval_proof)];
        let mut state = RlvrAcceptedProofState::new();

        let report = apply_rlvr_proof_block_payload(&mut state, &payload).unwrap();

        assert_eq!(report.accepted_count, 2);
        assert_eq!(state.len(), 2);
        assert!(state.get(&route_proof.proof_hash().unwrap()).is_some());
        assert!(state.get(&eval_proof.proof_hash().unwrap()).is_some());
        assert_eq!(state.list().len(), 2);
    }

    #[test]
    fn block_apply_rejects_hash_mismatch_without_advancing_state() {
        let key = NodeSigningKey::from_seed("node-a", b"node-a-secret").unwrap();
        let proof = full_route_proof().sign_with_node_key(&key).unwrap();
        let mut item = payload_item(&proof);
        item.proof_hash = hash_bytes(b"wrong-proof-hash");
        let mut state = RlvrAcceptedProofState::new();

        let err = apply_rlvr_proof_block_payload(&mut state, &[item]).unwrap_err();

        assert!(err.to_string().contains("hash mismatch"));
        assert!(state.is_empty());
    }

    #[test]
    fn block_apply_rejects_invalid_signature_without_advancing_state() {
        let key = NodeSigningKey::from_seed("node-a", b"node-a-secret").unwrap();
        let mut proof = full_route_proof().sign_with_node_key(&key).unwrap();
        proof.trace_hash = hash_bytes(b"tampered-after-signing");
        let item = RlvrProofBlockPayloadItem {
            proof_hash: hash_bytes(b"not-reached"),
            proof_json: serde_json::to_vec(&proof).unwrap(),
        };
        let mut state = RlvrAcceptedProofState::new();

        let err = apply_rlvr_proof_block_payload(&mut state, &[item]).unwrap_err();

        assert!(err.to_string().contains("invalid node signature"));
        assert!(state.is_empty());
    }

    #[test]
    fn block_apply_rejects_invalid_proof_type_without_advancing_state() {
        let key = NodeSigningKey::from_seed("node-a", b"node-a-secret").unwrap();
        let mut proof = RlvrProofObject::from_trace_commitment(
            RlvrProofType::ProofOfEval,
            &commitment_fixture(),
            hash_bytes(b"reward-policy"),
            hash_bytes(b"router-policy"),
            hash_bytes(b"model-id"),
            42,
            "sig-test",
        )
        .with_rubric_hash(hash_bytes(b"rubric"));
        proof.node_id = Some(key.node_id);
        proof.node_public_key = Some(key.public_key);
        proof.node_signature = "00".repeat(64);
        let item = RlvrProofBlockPayloadItem {
            proof_hash: hash_bytes(b"invalid-proof-type"),
            proof_json: serde_json::to_vec(&proof).unwrap(),
        };
        let mut state = RlvrAcceptedProofState::new();

        let err = apply_rlvr_proof_block_payload(&mut state, &[item]).unwrap_err();

        assert!(err
            .to_string()
            .contains("ProofOfEval requires eval_result_hash"));
        assert!(state.is_empty());
    }

    #[test]
    fn block_apply_rejects_raw_data_fields_without_advancing_state() {
        let key = NodeSigningKey::from_seed("node-a", b"node-a-secret").unwrap();
        let proof = full_route_proof().sign_with_node_key(&key).unwrap();
        let mut raw = serde_json::to_value(&proof).unwrap();
        raw.as_object_mut()
            .unwrap()
            .insert("raw_answer".into(), "private answer".into());
        let item = RlvrProofBlockPayloadItem {
            proof_hash: proof.proof_hash().unwrap(),
            proof_json: serde_json::to_vec(&raw).unwrap(),
        };
        let mut state = RlvrAcceptedProofState::new();

        let err = apply_rlvr_proof_block_payload(&mut state, &[item]).unwrap_err();

        assert!(err.to_string().contains("unknown field"));
        assert!(state.is_empty());
    }

    #[test]
    fn block_apply_rejects_malformed_payload_without_advancing_state() {
        let item = RlvrProofBlockPayloadItem {
            proof_hash: hash_bytes(b"malformed"),
            proof_json: b"{not json".to_vec(),
        };
        let mut state = RlvrAcceptedProofState::new();

        let err = apply_rlvr_proof_block_payload(&mut state, &[item]).unwrap_err();

        assert!(err.to_string().contains("malformed RLVR proof payload"));
        assert!(state.is_empty());
    }

    #[test]
    fn block_apply_is_all_or_nothing_when_later_item_is_invalid() {
        let key = NodeSigningKey::from_seed("node-a", b"node-a-secret").unwrap();
        let valid = full_route_proof().sign_with_node_key(&key).unwrap();
        let mut invalid = full_training_proof().sign_with_node_key(&key).unwrap();
        invalid.node_signature = "00".repeat(64);
        let payload = vec![payload_item(&valid), payload_item(&invalid)];
        let mut state = RlvrAcceptedProofState::new();

        let err = apply_rlvr_proof_block_payload(&mut state, &payload).unwrap_err();

        assert!(err.to_string().contains("invalid node signature"));
        assert!(state.is_empty());
    }

    fn full_route_proof() -> RlvrProofObject {
        RlvrProofObject::from_trace_commitment(
            RlvrProofType::ProofOfRoute,
            &commitment_fixture(),
            hash_bytes(b"reward-policy"),
            hash_bytes(b"router-policy"),
            hash_bytes(b"model-id"),
            42,
            "sig-test",
        )
        .with_rubric_hash(hash_bytes(b"rubric"))
    }

    fn full_eval_proof() -> RlvrProofObject {
        RlvrProofObject::from_trace_commitment(
            RlvrProofType::ProofOfEval,
            &commitment_fixture(),
            hash_bytes(b"reward-policy"),
            hash_bytes(b"router-policy"),
            hash_bytes(b"model-id"),
            42,
            "sig-test",
        )
        .with_rubric_hash(hash_bytes(b"rubric"))
        .with_eval_result_hash(hash_bytes(b"eval-result"))
    }

    fn full_training_proof() -> RlvrProofObject {
        RlvrProofObject::from_trace_commitment(
            RlvrProofType::ProofOfTraining,
            &commitment_fixture(),
            hash_bytes(b"reward-policy"),
            hash_bytes(b"router-policy"),
            hash_bytes(b"model-id"),
            42,
            "sig-test",
        )
        .with_rubric_hash(hash_bytes(b"rubric"))
        .with_adapter_hash(hash_bytes(b"adapter"))
        .with_eval_result_hash(hash_bytes(b"eval-result"))
    }

    fn payload_item(proof: &RlvrProofObject) -> RlvrProofBlockPayloadItem {
        RlvrProofBlockPayloadItem {
            proof_hash: proof.proof_hash().unwrap(),
            proof_json: serde_json::to_vec(proof).unwrap(),
        }
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
}
