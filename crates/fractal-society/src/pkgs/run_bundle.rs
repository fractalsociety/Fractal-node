//! Run-bundle package.
//!
//! Assembles portable run-bundle identifiers and a tamper-evident bundle hash.

use serde::{Deserialize, Serialize};

use crate::protocol::Hash;

/// Portable hashes and agent identity for a completed run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunBundle {
    /// Run manifest hash.
    pub run_manifest_hash: Hash,
    /// Evidence bundle hash.
    pub evidence_hash: Hash,
    /// Scorecard hash.
    pub scorecard_hash: Hash,
    /// Proof manifest hash.
    pub proof_hash: Hash,
    /// Agent identifier.
    pub agent_id: String,
}

impl RunBundle {
    /// Create a new portable run bundle.
    pub fn new(
        run_manifest_hash: Hash,
        evidence_hash: Hash,
        scorecard_hash: Hash,
        proof_hash: Hash,
        agent_id: impl Into<String>,
    ) -> Self {
        Self {
            run_manifest_hash,
            evidence_hash,
            scorecard_hash,
            proof_hash,
            agent_id: agent_id.into(),
        }
    }

    /// Return the canonical tamper-evident hash of this run bundle.
    pub fn bundle_hash(&self) -> crate::Result<Hash> {
        Hash::of(self)
    }
}
