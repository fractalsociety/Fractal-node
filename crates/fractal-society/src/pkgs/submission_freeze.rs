//! Submission-freeze package.
//!
//! Freezes a candidate submission into a canonical manifest hash.

use serde::{Deserialize, Serialize};

use crate::protocol::Hash;

/// Candidate submission identity, frozen by hashes plus attempt number.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Submission {
    /// Frozen agent manifest hash.
    pub agent_hash: Hash,
    /// Frozen protocol hash.
    pub protocol_hash: Hash,
    /// Frozen dataset hash.
    pub dataset_hash: Hash,
    /// Frozen environment hash.
    pub env_hash: Hash,
    /// Submission attempt number.
    pub attempt: u32,
}

impl Submission {
    /// Create a new submission manifest input.
    pub fn new(
        agent_hash: Hash,
        protocol_hash: Hash,
        dataset_hash: Hash,
        env_hash: Hash,
        attempt: u32,
    ) -> Self {
        Self {
            agent_hash,
            protocol_hash,
            dataset_hash,
            env_hash,
            attempt,
        }
    }

    /// Return the canonical tamper-evident manifest hash for this submission.
    pub fn manifest_hash(&self) -> crate::Result<Hash> {
        Hash::of(self)
    }
}
