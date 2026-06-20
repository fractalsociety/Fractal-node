//! Chain-commitment adapter package.
//!
//! Defines a small commitment interface and a deterministic in-memory adapter
//! useful for tests and local proof-pipeline wiring.

use std::sync::Mutex;

use crate::protocol::{ChainReference, Hash};

/// Interface for submitting a proof hash to a commitment layer.
pub trait CommitmentAdapter: Send + Sync {
    /// Commit `proof_hash` and return the chain reference.
    fn submit(&self, proof_hash: &Hash) -> crate::Result<ChainReference>;
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
