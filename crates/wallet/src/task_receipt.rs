//! TaskReceipt + full ToolReceipt (`docs/wallet.md` §9.1–9.2).

use borsh::{BorshDeserialize, BorshSerialize};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use thiserror::Error;

use crate::market::provider_id_from_public_key;
use crate::merkle;
use crate::types::{
    Amount, IntentId, ProviderId, PublicKey, ReceiptId, TaskId, TeeType, TimestampMs, ToolClass,
};

/// Class-specific metering (§9.1); unused fields are zero.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct MeteringRecord {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub wall_duration_ms: u64,
    pub bytes_metered: u64,
}

/// TEE attestation payload (§9.1); `quote` is opaque bytes (Phase 1 stub).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TeeAttestation {
    pub tee_type: TeeType,
    pub quote: Vec<u8>,
}

/// Provider-signed payload (everything except `receipt_id` and signatures).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ToolReceiptBody {
    pub intent_id: IntentId,
    pub task_id: TaskId,
    pub agent_session: PublicKey,
    pub provider_id: ProviderId,
    pub tool_class: ToolClass,
    pub payload_commitment: [u8; 32],
    pub output_commitment: [u8; 32],
    pub output_pointer: String,
    pub metering: MeteringRecord,
    pub cost: Amount,
    pub started_at: TimestampMs,
    pub completed_at: TimestampMs,
    pub attestation: Option<TeeAttestation>,
}

/// What the agent signs when acknowledging delivery (subset binding to `receipt_id`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ToolReceiptAgentAckBody {
    pub receipt_id: ReceiptId,
    pub intent_id: IntentId,
    pub task_id: TaskId,
    pub output_commitment: [u8; 32],
    pub cost: Amount,
}

/// Full tool receipt (§9.1). `receipt_id` **must** equal [`derive_tool_receipt_id`] of
/// `(body.intent_id, provider_sig)`.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ToolReceipt {
    pub receipt_id: ReceiptId,
    pub body: ToolReceiptBody,
    pub provider_sig: [u8; 64],
    pub agent_ack_sig: Option<[u8; 64]>,
}

/// `receipt_id = BLAKE3(intent_id || provider_sig)` (§9.1).
pub fn derive_tool_receipt_id(intent_id: &IntentId, provider_sig: &[u8; 64]) -> ReceiptId {
    let mut buf = [0u8; 96];
    buf[..32].copy_from_slice(intent_id);
    buf[32..].copy_from_slice(provider_sig);
    *blake3::hash(&buf).as_bytes()
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ToolReceiptVerifyError {
    #[error("invalid provider public key")]
    BadProviderKey,
    #[error("provider_id does not match public key")]
    ProviderIdMismatch,
    #[error("invalid provider signature")]
    BadProviderSig,
    #[error("encode error")]
    Encode,
    #[error("receipt_id does not match BLAKE3(intent_id || provider_sig)")]
    ReceiptIdMismatch,
    #[error("invalid agent public key")]
    BadAgentKey,
    #[error("missing agent_ack_sig")]
    MissingAgentAck,
    #[error("invalid agent ack signature")]
    BadAgentAckSig,
}

impl ToolReceipt {
    /// Provider signs `borsh(ToolReceiptBody)`; `receipt_id` is derived from `intent_id` and that signature.
    pub fn sign_new(body: ToolReceiptBody, provider_sk: &SigningKey) -> Result<Self, std::io::Error> {
        let msg = borsh::to_vec(&body)?;
        let provider_sig = provider_sk.sign(&msg).to_bytes();
        let receipt_id = derive_tool_receipt_id(&body.intent_id, &provider_sig);
        Ok(Self {
            receipt_id,
            body,
            provider_sig,
            agent_ack_sig: None,
        })
    }

    pub fn verify_provider(&self, provider_pk: &PublicKey) -> Result<(), ToolReceiptVerifyError> {
        if provider_id_from_public_key(provider_pk) != self.body.provider_id {
            return Err(ToolReceiptVerifyError::ProviderIdMismatch);
        }
        let vk = VerifyingKey::from_bytes(provider_pk).map_err(|_| ToolReceiptVerifyError::BadProviderKey)?;
        let sig = Signature::from_bytes(&self.provider_sig);
        let msg = borsh::to_vec(&self.body).map_err(|_| ToolReceiptVerifyError::Encode)?;
        vk.verify(&msg, &sig).map_err(|_| ToolReceiptVerifyError::BadProviderSig)?;
        let expected = derive_tool_receipt_id(&self.body.intent_id, &self.provider_sig);
        if expected != self.receipt_id {
            return Err(ToolReceiptVerifyError::ReceiptIdMismatch);
        }
        Ok(())
    }

    /// Agent acknowledges this receipt (signs [`ToolReceiptAgentAckBody`]).
    pub fn sign_agent_ack(&self, agent_sk: &SigningKey) -> Result<[u8; 64], std::io::Error> {
        let ack = ToolReceiptAgentAckBody {
            receipt_id: self.receipt_id,
            intent_id: self.body.intent_id,
            task_id: self.body.task_id,
            output_commitment: self.body.output_commitment,
            cost: self.body.cost,
        };
        let msg = borsh::to_vec(&ack)?;
        Ok(agent_sk.sign(&msg).to_bytes())
    }

    pub fn with_agent_ack(mut self, sig: [u8; 64]) -> Self {
        self.agent_ack_sig = Some(sig);
        self
    }

    pub fn verify_agent_ack(&self) -> Result<(), ToolReceiptVerifyError> {
        let Some(agent_sig) = &self.agent_ack_sig else {
            return Err(ToolReceiptVerifyError::MissingAgentAck);
        };
        let vk =
            VerifyingKey::from_bytes(&self.body.agent_session).map_err(|_| ToolReceiptVerifyError::BadAgentKey)?;
        let sig = Signature::from_bytes(agent_sig);
        let ack = ToolReceiptAgentAckBody {
            receipt_id: self.receipt_id,
            intent_id: self.body.intent_id,
            task_id: self.body.task_id,
            output_commitment: self.body.output_commitment,
            cost: self.body.cost,
        };
        let msg = borsh::to_vec(&ack).map_err(|_| ToolReceiptVerifyError::Encode)?;
        vk.verify(&msg, &sig).map_err(|_| ToolReceiptVerifyError::BadAgentAckSig)?;
        Ok(())
    }
}

/// Leaf commitment for Merkle inclusion over full §9.1 receipts: `BLAKE3(borsh(ToolReceipt))`.
pub fn tool_receipt_leaf_commitment(r: &ToolReceipt) -> [u8; 32] {
    let bytes = borsh::to_vec(r).expect("ToolReceipt borsh");
    *blake3::hash(&bytes).as_bytes()
}

/// Merkle root over `BLAKE3(borsh(receipt))` leaves, sorted by commitment bytes (deterministic).
pub fn tool_receipt_root(receipts: &[ToolReceipt]) -> [u8; 32] {
    let mut commits: Vec<[u8; 32]> = receipts.iter().map(tool_receipt_leaf_commitment).collect();
    commits.sort();
    merkle::root_from_sorted_commitments(&commits)
}

pub fn verify_tool_receipt_costs(receipts: &[ToolReceipt], expected_total: Amount) -> bool {
    receipts.iter().map(|r| r.body.cost).sum::<Amount>() == expected_total
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TaskReceipt {
    pub task_id: TaskId,
    pub agent_session: PublicKey,
    pub artifact_commitment: [u8; 32],
    pub artifact_pointer: String,
    pub tool_receipt_root: [u8; 32],
    pub total_tool_cost: Amount,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TaskReceiptBuildError {
    #[error("tool cost sum mismatch")]
    CostMismatch,
    #[error("tool receipt root mismatch")]
    RootMismatch,
    #[error("tool receipt task_id does not match task receipt")]
    TaskIdMismatch,
    #[error("tool receipt agent_session does not match task receipt")]
    AgentSessionMismatch,
}

pub fn build_task_receipt(
    task_id: TaskId,
    agent_session: PublicKey,
    artifact_commitment: [u8; 32],
    artifact_pointer: String,
    tool_receipts: &[ToolReceipt],
    expected_total: Amount,
    claimed_root: [u8; 32],
) -> Result<TaskReceipt, TaskReceiptBuildError> {
    for r in tool_receipts {
        if r.body.task_id != task_id {
            return Err(TaskReceiptBuildError::TaskIdMismatch);
        }
        if r.body.agent_session != agent_session {
            return Err(TaskReceiptBuildError::AgentSessionMismatch);
        }
    }
    if !verify_tool_receipt_costs(tool_receipts, expected_total) {
        return Err(TaskReceiptBuildError::CostMismatch);
    }
    let root = tool_receipt_root(tool_receipts);
    if root != claimed_root {
        return Err(TaskReceiptBuildError::RootMismatch);
    }
    Ok(TaskReceipt {
        task_id,
        agent_session,
        artifact_commitment,
        artifact_pointer,
        tool_receipt_root: root,
        total_tool_cost: expected_total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn sample_body(
        intent_id: IntentId,
        task_id: TaskId,
        agent: &SigningKey,
        provider: &SigningKey,
        cost: Amount,
    ) -> ToolReceiptBody {
        let provider_pk = provider.verifying_key().to_bytes();
        ToolReceiptBody {
            intent_id,
            task_id,
            agent_session: agent.verifying_key().to_bytes(),
            provider_id: provider_id_from_public_key(&provider_pk),
            tool_class: ToolClass::Browser,
            payload_commitment: [0xabu8; 32],
            output_commitment: [0xcdu8; 32],
            output_pointer: "da://blob/1".into(),
            metering: MeteringRecord {
                input_tokens: 1,
                output_tokens: 2,
                wall_duration_ms: 100,
                bytes_metered: 500,
            },
            cost,
            started_at: 1_000,
            completed_at: 2_000,
            attestation: None,
        }
    }

    #[test]
    fn receipt_id_is_blake3_of_intent_and_sig() {
        let mut rng = OsRng;
        let agent = SigningKey::generate(&mut rng);
        let provider = SigningKey::generate(&mut rng);
        let body = sample_body([1u8; 32], 7, &agent, &provider, 10);
        let r = ToolReceipt::sign_new(body, &provider).unwrap();
        let expected = derive_tool_receipt_id(&r.body.intent_id, &r.provider_sig);
        assert_eq!(r.receipt_id, expected);
    }

    #[test]
    fn provider_verify_round_trip() {
        let mut rng = OsRng;
        let agent = SigningKey::generate(&mut rng);
        let provider = SigningKey::generate(&mut rng);
        let provider_pk = provider.verifying_key().to_bytes();
        let body = sample_body([2u8; 32], 8, &agent, &provider, 20);
        let r = ToolReceipt::sign_new(body, &provider).unwrap();
        r.verify_provider(&provider_pk).unwrap();
    }

    #[test]
    fn agent_ack_round_trip() {
        let mut rng = OsRng;
        let agent = SigningKey::generate(&mut rng);
        let provider = SigningKey::generate(&mut rng);
        let provider_pk = provider.verifying_key().to_bytes();
        let body = sample_body([3u8; 32], 9, &agent, &provider, 30);
        let r = ToolReceipt::sign_new(body, &provider).unwrap();
        let ack = r.sign_agent_ack(&agent).unwrap();
        let r2 = r.with_agent_ack(ack);
        r2.verify_provider(&provider_pk).unwrap();
        r2.verify_agent_ack().unwrap();
    }

    #[test]
    fn root_and_build() {
        let mut rng = OsRng;
        let agent = SigningKey::generate(&mut rng);
        let p1 = SigningKey::generate(&mut rng);
        let p2 = SigningKey::generate(&mut rng);
        let r1 = ToolReceipt::sign_new(
            sample_body([0x10u8; 32], 9, &agent, &p1, 10),
            &p1,
        )
        .unwrap();
        let r2 = ToolReceipt::sign_new(
            sample_body([0x11u8; 32], 9, &agent, &p2, 20),
            &p2,
        )
        .unwrap();
        let receipts = vec![r1, r2];
        let root = tool_receipt_root(&receipts);
        let tr = build_task_receipt(
            9,
            agent.verifying_key().to_bytes(),
            [5u8; 32],
            "ipfs://x".into(),
            &receipts,
            30,
            root,
        )
        .unwrap();
        assert_eq!(tr.total_tool_cost, 30);
        assert_eq!(tr.tool_receipt_root, root);
    }
}
