//! Merkle-commitment package.
//!
//! Builds a Merkle root and inclusion proofs over decision-trace observation
//! hashes. Leaves are the decoded bytes of each `observation_hash`; internal
//! nodes are SHA-256 of `left || right`, duplicating the last node on odd levels.

use crate::protocol::{EvidenceBundle, Hash};

/// Inclusion proof for a decision-trace observation hash.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InclusionProof {
    /// Zero-based leaf index in the original evidence trace list.
    pub index: usize,
    /// Sibling hashes from leaf level upward.
    pub siblings: Vec<Hash>,
}

/// Return the Merkle root over all decision-trace observation hashes.
pub fn root(evidence: &EvidenceBundle) -> Hash {
    let Some(leaves) = leaf_bytes(evidence) else {
        return empty_root();
    };
    root_from_nodes(leaves)
}

/// Return an inclusion proof for `index`, or `None` when the index or leaves are invalid.
pub fn prove(evidence: &EvidenceBundle, index: usize) -> Option<InclusionProof> {
    let mut level = leaf_bytes(evidence)?;
    if index >= level.len() {
        return None;
    }

    let mut proof_index = index;
    let mut siblings = Vec::new();
    while level.len() > 1 {
        let sibling_index = if proof_index.is_multiple_of(2) {
            (proof_index + 1).min(level.len() - 1)
        } else {
            proof_index - 1
        };
        siblings.push(Hash(hex::encode(&level[sibling_index])));
        level = next_level(&level);
        proof_index /= 2;
    }

    Some(InclusionProof { index, siblings })
}

/// Verify that `leaf` is included under `root` by `proof`.
pub fn verify(leaf: &Hash, proof: &InclusionProof, root: &Hash) -> bool {
    let Some(mut current) = decode_hash(leaf) else {
        return false;
    };

    let mut index = proof.index;
    for sibling in &proof.siblings {
        let Some(sibling_bytes) = decode_hash(sibling) else {
            return false;
        };
        current = if index.is_multiple_of(2) {
            parent_hash(&current, &sibling_bytes)
        } else {
            parent_hash(&sibling_bytes, &current)
        };
        index /= 2;
    }

    Hash(hex::encode(current)) == *root
}

fn leaf_bytes(evidence: &EvidenceBundle) -> Option<Vec<Vec<u8>>> {
    evidence
        .decision_traces
        .iter()
        .map(|trace| decode_hash(&trace.observation_hash))
        .collect()
}

fn root_from_nodes(mut level: Vec<Vec<u8>>) -> Hash {
    if level.is_empty() {
        return empty_root();
    }
    while level.len() > 1 {
        level = next_level(&level);
    }
    Hash(hex::encode(&level[0]))
}

fn next_level(level: &[Vec<u8>]) -> Vec<Vec<u8>> {
    level
        .chunks(2)
        .map(|chunk| {
            let left = &chunk[0];
            let right = chunk.get(1).unwrap_or(&chunk[0]);
            parent_hash(left, right)
        })
        .collect()
}

fn parent_hash(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut input = Vec::with_capacity(left.len() + right.len());
    input.extend_from_slice(left);
    input.extend_from_slice(right);
    fractal_crypto::sha256(&input).to_vec()
}

fn decode_hash(hash: &Hash) -> Option<Vec<u8>> {
    hex::decode(&hash.0).ok().filter(|bytes| bytes.len() == 32)
}

fn empty_root() -> Hash {
    Hash::new(b"")
}
