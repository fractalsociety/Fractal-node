//! Deterministic binary Merkle tree over sorted leaf commitments (`docs/wallet.md` §4.6 — audit proofs).
//!
//! Phase 1 uses a **sorted-leaf Merkle tree** (deterministic root). A full sparse Merkle trie can replace
//! this module later without changing wallet semantics at the edges.

pub type MerkleRoot = [u8; 32];

fn hash_pair(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"node");
    h.update(left);
    h.update(right);
    *h.finalize().as_bytes()
}

fn hash_leaf(data: &[u8; 32]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"leaf");
    h.update(data);
    *h.finalize().as_bytes()
}

fn next_level(level: &[[u8; 32]]) -> Vec<[u8; 32]> {
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
    next
}

/// Root of an empty revocation set.
pub const EMPTY_ROOT: MerkleRoot = [0u8; 32];

pub fn root_from_sorted_leaves(leaves: &[[u8; 32]]) -> MerkleRoot {
    if leaves.is_empty() {
        return EMPTY_ROOT;
    }
    let mut level: Vec<[u8; 32]> = leaves.iter().map(|l| hash_leaf(l)).collect();
    while level.len() > 1 {
        level = next_level(&level);
    }
    level[0]
}

/// Merkle root over **already-hashed** sorted leaves (e.g. `BLAKE3(borsh(summary))` per tool receipt).
pub fn root_from_sorted_commitments(sorted: &[[u8; 32]]) -> MerkleRoot {
    if sorted.is_empty() {
        return EMPTY_ROOT;
    }
    let mut level = sorted.to_vec();
    while level.len() > 1 {
        level = next_level(&level);
    }
    level[0]
}

/// Inclusion proof for `leaf` in the sorted `leaves` multiset (binary search index).
pub fn merkle_proof(leaves: &[[u8; 32]], leaf: &[u8; 32]) -> Option<(usize, Vec<[u8; 32]>)> {
    let idx = leaves.binary_search(leaf).ok()?;
    let mut level: Vec<[u8; 32]> = leaves.iter().map(|l| hash_leaf(l)).collect();
    let mut i = idx;
    let mut path = Vec::new();
    while level.len() > 1 {
        let sibling_i = if i % 2 == 0 {
            if i + 1 < level.len() {
                i + 1
            } else {
                i
            }
        } else {
            i - 1
        };
        path.push(level[sibling_i]);
        level = next_level(&level);
        i /= 2;
    }
    Some((idx, path))
}

pub fn verify_membership(root: &MerkleRoot, leaf: &[u8; 32], mut idx: usize, path: &[[u8; 32]]) -> bool {
    if root == &EMPTY_ROOT {
        return false;
    }
    let mut h = hash_leaf(leaf);
    for sib in path {
        h = if idx % 2 == 0 {
            hash_pair(&h, sib)
        } else {
            hash_pair(sib, &h)
        };
        idx /= 2;
    }
    &h == root
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merkle_roundtrip() {
        let mut leaves = vec![[1u8; 32], [2u8; 32], [3u8; 32]];
        leaves.sort();
        let root = root_from_sorted_leaves(&leaves);
        let (i, path) = merkle_proof(&leaves, &[2u8; 32]).unwrap();
        assert!(verify_membership(&root, &[2u8; 32], i, &path));
    }
}
