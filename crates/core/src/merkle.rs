//! Keccak binary Merkle tree (same pairing rule as `fractal-consensus` tx root).

use fractal_crypto::Hash256;
use fractal_crypto::hash::keccak256;

fn hash_pair(left: &Hash256, right: &Hash256) -> Hash256 {
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(left);
    buf[32..].copy_from_slice(right);
    keccak256(&buf)
}

/// Merkle root over ordered leaves (empty → zero hash).
pub fn merkle_root(leaves: &[Hash256]) -> Hash256 {
    if leaves.is_empty() {
        return [0u8; 32];
    }
    let mut level: Vec<Hash256> = leaves.to_vec();
    while level.len() > 1 {
        let mut next = Vec::with_capacity((level.len() + 1) / 2);
        let mut i = 0;
        while i < level.len() {
            if i + 1 < level.len() {
                next.push(hash_pair(&level[i], &level[i + 1]));
                i += 2;
            } else {
                next.push(hash_pair(&level[i], &level[i]));
                i += 1;
            }
        }
        level = next;
    }
    level[0]
}

/// Verify inclusion of `leaf` at `index` (0-based leaf order) against `root`.
pub fn verify_merkle_proof(
    root: Hash256,
    leaf: Hash256,
    mut index: usize,
    proof: &[Hash256],
) -> bool {
    let mut node = leaf;
    for sib in proof {
        let (l, r) = if index % 2 == 0 {
            (&node, sib)
        } else {
            (sib, &node)
        };
        node = hash_pair(l, r);
        index /= 2;
    }
    node == root
}

/// Build sibling path for `leaves[index]` (tree built like [`merkle_root`]).
pub fn merkle_proof(leaves: &[Hash256], index: usize) -> Option<Vec<Hash256>> {
    if leaves.is_empty() || index >= leaves.len() {
        return None;
    }
    let mut level: Vec<Hash256> = leaves.to_vec();
    let mut idx = index;
    let mut proof = Vec::new();
    while level.len() > 1 {
        let sibling_idx = if idx % 2 == 0 { idx + 1 } else { idx - 1 };
        let sib = if sibling_idx < level.len() {
            level[sibling_idx]
        } else {
            level[idx]
        };
        proof.push(sib);
        let mut next = Vec::with_capacity((level.len() + 1) / 2);
        let mut i = 0;
        while i < level.len() {
            if i + 1 < level.len() {
                next.push(hash_pair(&level[i], &level[i + 1]));
                i += 2;
            } else {
                next.push(hash_pair(&level[i], &level[i]));
                i += 1;
            }
        }
        idx /= 2;
        level = next;
    }
    Some(proof)
}
