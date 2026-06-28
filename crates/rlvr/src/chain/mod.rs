//! Hash-only Proof of Route, Proof of Eval, and Proof of Training chain integration.

use serde::{Deserialize, Serialize};

use crate::{stable_hash, RlvrError, TraceHashCommitment};

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
pub struct RlvrProofObject {
    pub proof_type: RlvrProofType,
    pub trace_hash: String,
    pub redacted_trace_hash: String,
    pub verifier_outputs_hash: String,
    pub reward_policy_hash: String,
    pub reward_vector_hash: String,
    pub route_policy_hash: String,
    pub model_id_hash: String,
    pub adapter_hash: Option<String>,
    pub eval_hash: Option<String>,
    pub timestamp_ms: u64,
    pub node_signature: String,
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
        Self {
            proof_type,
            trace_hash: commitment.trace_hash.clone(),
            redacted_trace_hash: commitment.redacted_trace_hash.clone(),
            verifier_outputs_hash: commitment.verifier_outputs_hash.clone(),
            reward_policy_hash: reward_policy_hash.into(),
            reward_vector_hash: commitment.reward_vector_hash.clone(),
            route_policy_hash: route_policy_hash.into(),
            model_id_hash: model_id_hash.into(),
            adapter_hash: None,
            eval_hash: None,
            timestamp_ms,
            node_signature: node_signature.into(),
        }
    }

    pub fn validate_hash_only(&self) -> Result<(), RlvrError> {
        for (name, value) in self.hash_fields() {
            validate_hex_hash(name, value)?;
        }
        if let Some(adapter_hash) = &self.adapter_hash {
            validate_hex_hash("adapter_hash", adapter_hash)?;
        }
        if let Some(eval_hash) = &self.eval_hash {
            validate_hex_hash("eval_hash", eval_hash)?;
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
        Ok(())
    }

    pub fn stable_hash(&self) -> Result<String, RlvrError> {
        self.validate_hash_only()?;
        stable_hash(self)
    }

    fn hash_fields(&self) -> [(&'static str, &str); 7] {
        [
            ("trace_hash", &self.trace_hash),
            ("redacted_trace_hash", &self.redacted_trace_hash),
            ("verifier_outputs_hash", &self.verifier_outputs_hash),
            ("reward_policy_hash", &self.reward_policy_hash),
            ("reward_vector_hash", &self.reward_vector_hash),
            ("route_policy_hash", &self.route_policy_hash),
            ("model_id_hash", &self.model_id_hash),
        ]
    }

    pub fn serialized_len(&self) -> Result<usize, RlvrError> {
        self.validate_hash_only()?;
        Ok(serde_json::to_vec(self)?.len())
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
