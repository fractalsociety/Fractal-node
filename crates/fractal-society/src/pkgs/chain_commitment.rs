//! Chain-commitment adapter package.
//!
//! Defines a small commitment interface and a deterministic in-memory adapter
//! useful for tests and local proof-pipeline wiring.

use std::sync::Mutex;

use crate::protocol::{ChainReference, Hash};
use serde::{Deserialize, Serialize};

/// Schema marker for RLMF chain attestation submission records.
pub const RLMF_ATTESTATION_SCHEMA_V1: &str = "rlmf.chain_attestation.v1";

/// Interface for submitting a proof hash to a commitment layer.
pub trait CommitmentAdapter: Send + Sync {
    /// Commit `proof_hash` and return the chain reference.
    fn submit(&self, proof_hash: &Hash) -> crate::Result<ChainReference>;
}

/// Submission envelope for an RLMF commitment hash or commitment root.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RlmfAttestationSubmission {
    /// Stable local attestation identifier.
    pub attestation_id: String,
    /// RLMF job, model, adapter, or receipt subject being attested.
    pub subject_id: String,
    /// Producing system, such as Fractalwork, DataEvol, or Coordinate.
    pub source_system: String,
    /// Single RLMF commitment hash from the canonical schema.
    pub commitment_hash: Hash,
    /// Optional batch/root commitment; submitted instead of `commitment_hash` when present.
    pub commitment_root: Option<Hash>,
    /// Submission schema marker.
    pub schema: String,
}

/// Result of submitting an RLMF attestation through a chain commitment adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RlmfAttestationReceipt {
    /// Original validated submission envelope.
    pub submission: RlmfAttestationSubmission,
    /// Exact hash sent to the chain adapter.
    pub submitted_hash: Hash,
    /// Chain reference returned by the adapter.
    pub chain_reference: ChainReference,
}

impl RlmfAttestationSubmission {
    /// Build a validated RLMF attestation submission from hex commitment strings.
    pub fn new(
        attestation_id: impl Into<String>,
        subject_id: impl Into<String>,
        source_system: impl Into<String>,
        commitment_hash: impl AsRef<str>,
        commitment_root: Option<impl AsRef<str>>,
    ) -> crate::Result<Self> {
        let attestation_id = attestation_id.into();
        let subject_id = subject_id.into();
        let source_system = source_system.into();
        if attestation_id.is_empty() {
            return Err(crate::error::Error::InvalidArtifact(
                "RLMF attestation id is required".to_string(),
            ));
        }
        if subject_id.is_empty() {
            return Err(crate::error::Error::InvalidArtifact(
                "RLMF attestation subject id is required".to_string(),
            ));
        }
        if source_system.is_empty() {
            return Err(crate::error::Error::InvalidArtifact(
                "RLMF attestation source system is required".to_string(),
            ));
        }
        Ok(Self {
            attestation_id,
            subject_id,
            source_system,
            commitment_hash: Hash::from_hex(commitment_hash.as_ref())?,
            commitment_root: commitment_root
                .map(|root| Hash::from_hex(root.as_ref()))
                .transpose()?,
            schema: RLMF_ATTESTATION_SCHEMA_V1.to_string(),
        })
    }

    /// Return the commitment root when present, otherwise the single commitment hash.
    pub fn submitted_hash(&self) -> &Hash {
        self.commitment_root
            .as_ref()
            .unwrap_or(&self.commitment_hash)
    }
}

/// Submit an RLMF attestation through any existing FractalChain commitment adapter.
pub fn submit_rlmf_attestation(
    adapter: &dyn CommitmentAdapter,
    submission: RlmfAttestationSubmission,
) -> crate::Result<RlmfAttestationReceipt> {
    if submission.schema != RLMF_ATTESTATION_SCHEMA_V1 {
        return Err(crate::error::Error::InvalidArtifact(
            "RLMF attestation schema mismatch".to_string(),
        ));
    }
    let submitted_hash = submission.submitted_hash().clone();
    let chain_reference = adapter.submit(&submitted_hash)?;
    Ok(RlmfAttestationReceipt {
        submission,
        submitted_hash,
        chain_reference,
    })
}

/// Deterministic in-memory commitment adapter.
pub struct InMemoryCommitmentAdapter {
    network: String,
    next_block: Mutex<u64>,
}

impl InMemoryCommitmentAdapter {
    /// Create a mock adapter for `network`, allocating blocks from `starting_block`.
    pub fn new(network: impl Into<String>, starting_block: u64) -> Self {
        Self {
            network: network.into(),
            next_block: Mutex::new(starting_block),
        }
    }
}

impl CommitmentAdapter for InMemoryCommitmentAdapter {
    fn submit(&self, proof_hash: &Hash) -> crate::Result<ChainReference> {
        let mut next_block = self
            .next_block
            .lock()
            .expect("in-memory commitment mutex should not be poisoned");
        let block_number = *next_block;
        *next_block = next_block.saturating_add(1);

        Ok(ChainReference {
            network: self.network.clone(),
            transaction_hash: deterministic_tx_hash(proof_hash, block_number),
            block_number,
            finalized: true,
        })
    }
}

fn deterministic_tx_hash(proof_hash: &Hash, block_number: u64) -> String {
    let mut bytes = Vec::with_capacity(proof_hash.0.len() + std::mem::size_of::<u64>());
    bytes.extend_from_slice(proof_hash.0.as_bytes());
    bytes.extend_from_slice(&block_number.to_be_bytes());
    Hash::new(&bytes).0
}
