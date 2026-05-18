//! Deterministic binary Merkle tree over sorted leaf commitments (`docs/wallet.md` §4.6 — audit proofs).
//!
//! Sorted-leaf Merkle helpers (task receipts and legacy tests). **Revocation** uses [`crate::smt`].

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

/// Inclusion proof for leaf at `leaf_index` in a sorted commitment tree (indexed by `cap_id` order).
pub fn merkle_proof_commitment_at(
    sorted_commitments: &[[u8; 32]],
    leaf_index: usize,
) -> Option<Vec<[u8; 32]>> {
    if leaf_index >= sorted_commitments.len() {
        return None;
    }
    let mut level = sorted_commitments.to_vec();
    let mut i = leaf_index;
    let mut path = Vec::new();
    while level.len() > 1 {
        let sibling_i = if i % 2 == 0 {
            if i + 1 < level.len() { i + 1 } else { i }
        } else {
            i - 1
        };
        path.push(level[sibling_i]);
        level = next_level(&level);
        i /= 2;
    }
    Some(path)
}

pub fn verify_membership_commitment(
    root: &MerkleRoot,
    commitment: &[u8; 32],
    idx: usize,
    path: &[[u8; 32]],
) -> bool {
    if root == &EMPTY_ROOT {
        return false;
    }
    let mut h = *commitment;
    let mut i = idx;
    for sib in path {
        h = if i % 2 == 0 {
            hash_pair(&h, sib)
        } else {
            hash_pair(sib, &h)
        };
        i /= 2;
    }
    &h == root
}

/// Non-membership witness for a sorted leaf multiset (`cap_id` not present).
#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct SortedNonMembershipProof {
    pub cap_id: [u8; 32],
    pub insertion_index: usize,
    pub left: Option<NeighborWitness>,
    pub right: Option<NeighborWitness>,
}

#[derive(Clone, Debug, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct NeighborWitness {
    pub cap_id: [u8; 32],
    pub leaf_index: usize,
    pub leaf_commitment: [u8; 32],
    pub path: Vec<[u8; 32]>,
}

/// Build a non-membership proof for `cap_id` against sorted **leaf commitments** (not raw keys).
pub fn non_membership_proof_commitments(
    sorted_commitments: &[[u8; 32]],
    sorted_cap_ids: &[[u8; 32]],
    cap_id: &[u8; 32],
) -> Option<SortedNonMembershipProof> {
    debug_assert_eq!(sorted_commitments.len(), sorted_cap_ids.len());
    let insertion_index = sorted_cap_ids.partition_point(|k| k < cap_id);
    if insertion_index < sorted_cap_ids.len() && sorted_cap_ids[insertion_index] == *cap_id {
        return None;
    }
    let left = if insertion_index > 0 {
        let idx = insertion_index - 1;
        let path = merkle_proof_commitment_at(sorted_commitments, idx)?;
        Some(NeighborWitness {
            cap_id: sorted_cap_ids[idx],
            leaf_index: idx,
            leaf_commitment: sorted_commitments[idx],
            path,
        })
    } else {
        None
    };
    let right = if insertion_index < sorted_cap_ids.len() {
        let idx = insertion_index;
        let path = merkle_proof_commitment_at(sorted_commitments, idx)?;
        Some(NeighborWitness {
            cap_id: sorted_cap_ids[idx],
            leaf_index: idx,
            leaf_commitment: sorted_commitments[idx],
            path,
        })
    } else {
        None
    };
    Some(SortedNonMembershipProof {
        cap_id: *cap_id,
        insertion_index,
        left,
        right,
    })
}

/// Verify `cap_id` is absent from the tree committed by `root`.
pub fn verify_non_membership_commitments(
    root: &MerkleRoot,
    sorted_cap_ids: &[[u8; 32]],
    proof: &SortedNonMembershipProof,
) -> bool {
    if root == &EMPTY_ROOT {
        return true;
    }
    if sorted_cap_ids.binary_search(&proof.cap_id).is_ok() {
        return false;
    }
    if proof.insertion_index != sorted_cap_ids.partition_point(|k| k < &proof.cap_id) {
        return false;
    }
    verify_non_membership_commitments_compact(root, sorted_cap_ids.len() as u32, proof)
        && verify_neighbor_ordering(sorted_cap_ids, proof)
}

fn verify_neighbor_ordering(sorted_cap_ids: &[[u8; 32]], proof: &SortedNonMembershipProof) -> bool {
    if let Some(left) = &proof.left {
        if left.leaf_index >= sorted_cap_ids.len() || sorted_cap_ids[left.leaf_index] >= proof.cap_id
        {
            return false;
        }
    }
    if let Some(right) = &proof.right {
        if right.leaf_index >= sorted_cap_ids.len()
            || sorted_cap_ids[right.leaf_index] <= proof.cap_id
        {
            return false;
        }
    }
    true
}

/// Compact non-membership verify (`docs/wallet.md` §25.2): neighbor Merkle witnesses + `leaf_count` only.
pub fn verify_non_membership_commitments_compact(
    root: &MerkleRoot,
    revoked_leaf_count: u32,
    proof: &SortedNonMembershipProof,
) -> bool {
    if root == &EMPTY_ROOT {
        return proof.left.is_none() && proof.right.is_none() && revoked_leaf_count == 0;
    }
    let n = revoked_leaf_count as usize;
    if n == 0 {
        return false;
    }
    match (&proof.left, &proof.right) {
        (None, None) => return false,
        (None, Some(right)) => {
            if right.leaf_index != 0 || proof.cap_id >= right.cap_id {
                return false;
            }
            if !verify_membership_commitment(
                root,
                &right.leaf_commitment,
                right.leaf_index,
                &right.path,
            ) {
                return false;
            }
        }
        (Some(left), None) => {
            if left.leaf_index + 1 != n || proof.cap_id <= left.cap_id {
                return false;
            }
            if !verify_membership_commitment(
                root,
                &left.leaf_commitment,
                left.leaf_index,
                &left.path,
            ) {
                return false;
            }
        }
        (Some(left), Some(right)) => {
            if left.cap_id >= proof.cap_id
                || proof.cap_id >= right.cap_id
                || left.leaf_index + 1 != right.leaf_index
            {
                return false;
            }
            if !verify_membership_commitment(
                root,
                &left.leaf_commitment,
                left.leaf_index,
                &left.path,
            ) || !verify_membership_commitment(
                root,
                &right.leaf_commitment,
                right.leaf_index,
                &right.path,
            ) {
                return false;
            }
        }
    }
    true
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

    #[test]
    fn non_membership_roundtrip() {
        let ids = vec![[1u8; 32], [3u8; 32], [5u8; 32]];
        let commits: Vec<[u8; 32]> = ids
            .iter()
            .map(|id| hash_leaf(id))
            .collect();
        let root = root_from_sorted_commitments(&commits);
        let proof = non_membership_proof_commitments(&commits, &ids, &[4u8; 32]).unwrap();
        assert!(verify_non_membership_commitments(&root, &ids, &proof));
        assert!(non_membership_proof_commitments(&commits, &ids, &[3u8; 32]).is_none());
    }

    #[test]
    fn compact_verify_matches_full_list_verify() {
        let ids = vec![[1u8; 32], [3u8; 32], [5u8; 32], [7u8; 32]];
        let commits: Vec<[u8; 32]> = ids
            .iter()
            .map(|id| hash_leaf(id))
            .collect();
        let root = root_from_sorted_commitments(&commits);
        let proof = non_membership_proof_commitments(&commits, &ids, &[4u8; 32]).unwrap();
        assert!(verify_non_membership_commitments(&root, &ids, &proof));
        assert!(verify_non_membership_commitments_compact(
            &root,
            ids.len() as u32,
            &proof
        ));
    }

    #[test]
    fn empty_tree_non_membership() {
        let proof = SortedNonMembershipProof {
            cap_id: [9u8; 32],
            insertion_index: 0,
            left: None,
            right: None,
        };
        assert!(verify_non_membership_commitments(&EMPTY_ROOT, &[], &proof));
    }
}
