//! Ethereum-compatible account + per-account storage Merkle Patricia Trie helpers.
//!
//! Uses [`alloy_trie`] for canonical roots; builds trie nodes for RocksDB (`cf_state` MPT subkeys).

use std::collections::BTreeMap;

use alloy_primitives::{Address as EthAddress, B256, U256, keccak256};
use alloy_rlp::Encodable;
use alloy_trie::{
    EMPTY_ROOT_HASH, KECCAK_EMPTY, Nibbles, TrieAccount, TrieMask,
    nodes::{BranchNodeRef, ExtensionNodeRef, LeafNodeRef, RlpNode},
    root,
};
use fractal_core::State;

use crate::chain_records::{evm_mpt_node_key, evm_mpt_root_at_height_key};

#[inline]
fn finalize_node_rlp(rlp: &mut Vec<u8>, db: &mut BTreeMap<[u8; 32], Vec<u8>>) -> RlpNode {
    if rlp.len() >= 32 {
        let h = keccak256(rlp.as_slice());
        db.insert(h.into(), rlp.clone());
    }
    RlpNode::from_rlp(rlp.as_slice())
}

fn root_bytes(node: &RlpNode) -> [u8; 32] {
    if let Some(h) = node.as_hash() {
        h.into()
    } else {
        keccak256(node.as_slice()).into()
    }
}

fn storage_root_for_addr(state: &State, addr: &[u8; 20]) -> B256 {
    let mut slots: Vec<(B256, U256)> = state
        .evm_storage
        .iter()
        .filter(|((a, _), _)| a == addr)
        .map(|((_, slot), val)| {
            let key = B256::from_slice(slot);
            (key, U256::from_be_slice(val))
        })
        .collect();
    if slots.is_empty() {
        return EMPTY_ROOT_HASH;
    }
    slots.sort_unstable_by_key(|(k, _)| *k);
    root::storage_root(slots)
}

fn trie_account_for(state: &State, addr: &[u8; 20], acct: &fractal_core::Account) -> TrieAccount {
    let storage_root = storage_root_for_addr(state, addr);
    let code_hash = state
        .evm_code
        .get(addr)
        .map(|c| keccak256(c))
        .unwrap_or(KECCAK_EMPTY);
    TrieAccount {
        nonce: acct.nonce,
        balance: U256::from(acct.balance),
        storage_root,
        code_hash,
    }
}

/// Sorted account trie leaves: keccak(address) unpacked to nibbles → RLP(account).
fn collect_sorted_leaves(state: &State) -> Vec<(Nibbles, Vec<u8>)> {
    let mut v = Vec::with_capacity(state.accounts.len());
    for (addr, acct) in state.accounts.iter() {
        let h = keccak256(EthAddress::from_slice(addr));
        let ta = trie_account_for(state, addr, acct);
        let mut rlp = Vec::new();
        ta.encode(&mut rlp);
        v.push((Nibbles::unpack(h), rlp));
    }
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

fn common_prefix_len(entries: &[(Nibbles, Vec<u8>)]) -> usize {
    if entries.is_empty() {
        return 0;
    }
    let first = &entries[0].0;
    let mut i = 0usize;
    'outer: while i < first.len() {
        let n = first.get(i).expect("len bound");
        for (k, _) in entries.iter().skip(1) {
            if i >= k.len() || k.get(i) != Some(n) {
                break 'outer;
            }
        }
        i += 1;
    }
    i
}

/// Recursive Patricia trie on **hashed** keys (64 nibbles), matching Ethereum rules
/// for extension/branch/leaf. Skips branch **value** slots (no prefix-of-another-key case);
/// that case is astronomically unlikely for keccak outputs.
fn build_trie(entries: &[(Nibbles, Vec<u8>)], db: &mut BTreeMap<[u8; 32], Vec<u8>>) -> RlpNode {
    assert!(!entries.is_empty());
    debug_assert!(
        entries.iter().all(|(k, _)| !k.is_empty()) || entries.len() == 1,
        "MPT branch-with-value (prefix key) not implemented; use root-only path if hit"
    );

    if entries.len() == 1 {
        let (path, val) = &entries[0];
        let leaf = LeafNodeRef::new(path, val.as_slice());
        let mut buf = Vec::new();
        leaf.encode(&mut buf);
        return finalize_node_rlp(&mut buf, db);
    }

    let l = common_prefix_len(entries);
    if l > 0 {
        let shared = entries[0].0.slice(..l);
        let stripped: Vec<_> = entries
            .iter()
            .map(|(k, v)| (k.slice(l..), v.clone()))
            .collect();
        let child = build_trie(&stripped, db);
        let ext = ExtensionNodeRef::new(&shared, child.as_slice());
        let mut buf = Vec::new();
        ext.encode(&mut buf);
        return finalize_node_rlp(&mut buf, db);
    }

    let mut mask = TrieMask::default();
    let mut stack: Vec<RlpNode> = Vec::new();
    for n in 0u8..16u8 {
        let sub: Vec<_> = entries
            .iter()
            .filter(|(k, _)| !k.is_empty() && k.get(0) == Some(n))
            .map(|(k, v)| (k.slice(1..), v.clone()))
            .collect();
        if sub.is_empty() {
            continue;
        }
        mask.set_bit(n);
        stack.push(build_trie(&sub, db));
    }

    let branch_ref = BranchNodeRef::new(&stack, mask);
    let mut buf = Vec::new();
    branch_ref.encode(&mut buf);
    finalize_node_rlp(&mut buf, db)
}

/// Returns Ethereum MPT account trie root and hashed-node payloads (RLP ≥ 32 bytes).
#[must_use]
pub fn evm_account_mpt_root_and_nodes(state: &State) -> ([u8; 32], BTreeMap<[u8; 32], Vec<u8>>) {
    let leaves = collect_sorted_leaves(state);
    if leaves.is_empty() {
        return (EMPTY_ROOT_HASH.into(), BTreeMap::new());
    }

    let expected = {
        let pairs: Vec<(B256, TrieAccount)> = state
            .accounts
            .iter()
            .map(|(addr, acct)| {
                let h = keccak256(EthAddress::from_slice(addr));
                (h, trie_account_for(state, addr, acct))
            })
            .collect();
        root::state_root_unsorted(pairs)
    };

    let mut db = BTreeMap::new();
    let root_node = build_trie(&leaves, &mut db);
    let got = root_bytes(&root_node);
    debug_assert_eq!(
        B256::from(got),
        expected,
        "internal MPT builder root mismatch vs alloy_trie"
    );

    (got, db)
}

/// Write MPT root + node bodies under [`crate::chain_records`] key prefixes in **`cf_state`**.
pub fn persist_evm_account_mpt_to_cf_state(
    db: &crate::fractal_db::FractalRocksDb,
    shard_id: u32,
    shard_count: u32,
    height: u64,
    state: &State,
) -> Result<(), crate::fractal_db::ChainPersistError> {
    use crate::chain_records::scope_storage_key;
    let (root, nodes) = evm_account_mpt_root_and_nodes(state);
    let meta = crate::chain_records::StoredEvmAccountMptRootV1 {
        version: crate::chain_records::STORED_RECORD_V1,
        height,
        root,
    };
    let root_k = scope_storage_key(shard_id, shard_count, &evm_mpt_root_at_height_key(height));
    db.put_raw(crate::fractal_db::CF_STATE, &root_k, &borsh::to_vec(&meta)?)?;
    for (h, rlp) in nodes {
        let node_k = scope_storage_key(shard_id, shard_count, &evm_mpt_node_key(h));
        db.put_raw(crate::fractal_db::CF_STATE, &node_k, &rlp)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use fractal_core::Account;

    #[test]
    fn empty_trie_root_matches_alloy() {
        let st = State::default();
        let (root, nodes) = evm_account_mpt_root_and_nodes(&st);
        assert_eq!(B256::from(root), EMPTY_ROOT_HASH);
        assert!(nodes.is_empty());
    }

    #[test]
    fn single_account_roots_match() {
        let mut st = State::default();
        let a = [1u8; 20];
        st.accounts.insert(
            a,
            Account {
                nonce: 2,
                balance: 1000,
            },
        );
        let leaves = collect_sorted_leaves(&st);
        let mut db = BTreeMap::new();
        let n = build_trie(&leaves, &mut db);
        let r = root_bytes(&n);
        let pairs: Vec<(B256, TrieAccount)> = st
            .accounts
            .iter()
            .map(|(addr, acct)| {
                let h = keccak256(EthAddress::from_slice(addr));
                (h, trie_account_for(&st, addr, acct))
            })
            .collect();
        let want: [u8; 32] = root::state_root_unsorted(pairs).into();
        assert_eq!(r, want);
        let (r2, _) = evm_account_mpt_root_and_nodes(&st);
        assert_eq!(r2, want);
    }
}
