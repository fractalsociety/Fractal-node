//! 256-bit sparse Merkle trie for capability revocation (`docs/wallet.md` §4.6, §25.2).
//!
//! Keys are `cap_id` bytes (MSB-first bit order). Empty leaves use a domain-separated default;
//! revoked leaves store `revocation_leaf_commitment(cap_id, entry)`.

use std::collections::BTreeMap;

use borsh::{BorshDeserialize, BorshSerialize};

use crate::merkle::MerkleRoot;

/// Key path length in bits (= 32-byte `cap_id`).
pub const SMT_KEY_BITS: usize = 256;

fn hash_node(left: &[u8; 32], right: &[u8; 32]) -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"fractal/wallet/smt_node/v1");
    h.update(left);
    h.update(right);
    *h.finalize().as_bytes()
}

/// Default leaf when a `cap_id` is not revoked.
#[must_use]
pub fn empty_leaf_hash() -> [u8; 32] {
    let mut h = blake3::Hasher::new();
    h.update(b"fractal/wallet/smt_empty_leaf/v1");
    *h.finalize().as_bytes()
}

/// Precomputed root of an empty subtree at `depth` (0 = tree root, 256 = leaf tier).
fn empty_subtree_at(depth: usize) -> [u8; 32] {
    debug_assert!(depth <= SMT_KEY_BITS);
    let mut h = empty_leaf_hash();
    for _ in depth..SMT_KEY_BITS {
        h = hash_node(&h, &h);
    }
    h
}

/// Root of an empty revocation SMT.
pub fn empty_tree_root() -> MerkleRoot {
    empty_subtree_at(0)
}

#[must_use]
pub fn bit_at(key: &[u8; 32], depth: usize) -> bool {
    debug_assert!(depth < SMT_KEY_BITS);
    let byte = key[depth / 8];
    let bit_in_byte = 7 - (depth % 8);
    (byte >> bit_in_byte) & 1 == 1
}

fn subtree_root(leaves: &[( [u8; 32], [u8; 32] )], depth: usize) -> [u8; 32] {
    if leaves.is_empty() {
        return empty_subtree_at(depth);
    }
    if depth == SMT_KEY_BITS {
        debug_assert_eq!(leaves.len(), 1);
        return leaves[0].1;
    }
    let mut left = Vec::new();
    let mut right = Vec::new();
    for item in leaves {
        if bit_at(&item.0, depth) {
            right.push(*item);
        } else {
            left.push(*item);
        }
    }
    hash_node(
        &subtree_root(&left, depth + 1),
        &subtree_root(&right, depth + 1),
    )
}

/// Sparse Merkle trie over revoked `cap_id` → leaf commitment.
#[derive(Clone, Debug, Default)]
pub struct RevocationSparseTrie {
    leaves: BTreeMap<[u8; 32], [u8; 32]>,
}

impl RevocationSparseTrie {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.leaves.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.leaves.is_empty()
    }

    pub fn insert(&mut self, cap_id: [u8; 32], leaf_commitment: [u8; 32]) {
        self.leaves.insert(cap_id, leaf_commitment);
    }

    #[must_use]
    pub fn root(&self) -> MerkleRoot {
        let entries: Vec<_> = self.leaves.iter().map(|(k, v)| (*k, *v)).collect();
        subtree_root(&entries, 0)
    }

    #[must_use]
    pub fn contains_key(&self, cap_id: &[u8; 32]) -> bool {
        self.leaves.contains_key(cap_id)
    }

    pub fn non_membership_proof(&self, cap_id: &[u8; 32]) -> Option<SmtNonMembershipProof> {
        if self.leaves.contains_key(cap_id) {
            return None;
        }
        let all: Vec<_> = self.leaves.iter().map(|(k, v)| (*k, *v)).collect();
        let mut active = all;
        let mut siblings = Vec::new();
        for depth in 0..SMT_KEY_BITS {
            let here_bit = bit_at(cap_id, depth);
            let mut same = Vec::new();
            let mut other = Vec::new();
            for item in &active {
                if bit_at(&item.0, depth) == here_bit {
                    same.push(*item);
                } else {
                    other.push(*item);
                }
            }
            let sib_hash = subtree_root(&other, depth + 1);
            if sib_hash != empty_subtree_at(depth + 1) {
                siblings.push((depth as u8, sib_hash));
            }
            active = same;
        }
        Some(SmtNonMembershipProof {
            cap_id: *cap_id,
            siblings,
        })
    }

    pub fn membership_proof(&self, cap_id: &[u8; 32]) -> Option<SmtMembershipProof> {
        let leaf = *self.leaves.get(cap_id)?;
        let all: Vec<_> = self.leaves.iter().map(|(k, v)| (*k, *v)).collect();
        let mut active: Vec<_> = all;
        let mut siblings = Vec::new();
        for depth in 0..SMT_KEY_BITS {
            let here_bit = bit_at(cap_id, depth);
            let mut same = Vec::new();
            let mut other = Vec::new();
            for item in &active {
                if bit_at(&item.0, depth) == here_bit {
                    same.push(*item);
                } else {
                    other.push(*item);
                }
            }
            let sib_hash = subtree_root(&other, depth + 1);
            if sib_hash != empty_subtree_at(depth + 1) {
                siblings.push((depth as u8, sib_hash));
            }
            active = same;
        }
        Some(SmtMembershipProof {
            cap_id: *cap_id,
            leaf,
            siblings,
        })
    }
}

/// Compact SMT non-membership witness (sparse siblings only).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SmtNonMembershipProof {
    pub cap_id: [u8; 32],
    pub siblings: Vec<(u8, [u8; 32])>,
}

/// Compact SMT membership witness for a revoked ancestor (`cascade == false`).
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct SmtMembershipProof {
    pub cap_id: [u8; 32],
    pub leaf: [u8; 32],
    pub siblings: Vec<(u8, [u8; 32])>,
}

fn sibling_at(proof: &[(u8, [u8; 32])], depth: usize) -> [u8; 32] {
    proof
        .iter()
        .find(|(d, _)| *d as usize == depth)
        .map(|(_, h)| *h)
        .unwrap_or_else(|| empty_subtree_at(depth + 1))
}

fn compute_root_from_path(
    cap_id: &[u8; 32],
    depth: usize,
    leaf_hash: [u8; 32],
    siblings: &[(u8, [u8; 32])],
) -> [u8; 32] {
    if depth == SMT_KEY_BITS {
        return leaf_hash;
    }
    let here = bit_at(cap_id, depth);
    let sib = sibling_at(siblings, depth);
    let child = compute_root_from_path(cap_id, depth + 1, leaf_hash, siblings);
    if here {
        hash_node(&sib, &child)
    } else {
        hash_node(&child, &sib)
    }
}

#[must_use]
pub fn verify_non_membership(root: &MerkleRoot, proof: &SmtNonMembershipProof) -> bool {
    let leaf = empty_leaf_hash();
    let computed = compute_root_from_path(&proof.cap_id, 0, leaf, &proof.siblings);
    &computed == root
}

#[must_use]
pub fn verify_membership(root: &MerkleRoot, proof: &SmtMembershipProof) -> bool {
    let computed = compute_root_from_path(&proof.cap_id, 0, proof.leaf, &proof.siblings);
    &computed == root
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(id: u8) -> ([u8; 32], [u8; 32]) {
        let mut k = [0u8; 32];
        k[0] = id;
        let mut v = [0u8; 32];
        v[0] = id.wrapping_add(100);
        (k, v)
    }

    #[test]
    fn empty_root_matches_precomputed() {
        let trie = RevocationSparseTrie::new();
        assert_eq!(trie.root(), empty_tree_root());
    }

    #[test]
    fn single_leaf_non_membership_roundtrip() {
        let mut trie = RevocationSparseTrie::new();
        trie.insert(leaf(1).0, leaf(1).1);
        let root = trie.root();
        let target = {
            let mut k = [0u8; 32];
            k[0] = 2;
            k
        };
        let proof = trie.non_membership_proof(&target).unwrap();
        assert!(verify_non_membership(&root, &proof));
    }

    #[test]
    fn membership_roundtrip() {
        let mut trie = RevocationSparseTrie::new();
        let (k, v) = leaf(5);
        trie.insert(k, v);
        let root = trie.root();
        let proof = trie.membership_proof(&k).unwrap();
        assert!(verify_membership(&root, &proof));
    }

    #[test]
    fn two_leaves_deterministic_root() {
        let mut trie = RevocationSparseTrie::new();
        trie.insert(leaf(1).0, leaf(1).1);
        trie.insert(leaf(3).0, leaf(3).1);
        let r1 = trie.root();
        let mut trie2 = RevocationSparseTrie::new();
        trie2.insert(leaf(3).0, leaf(3).1);
        trie2.insert(leaf(1).0, leaf(1).1);
        assert_eq!(r1, trie2.root());
    }
}
