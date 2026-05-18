//! Typed borsh records for PRD §10.3 **`cf_blocks`**, **`cf_tx_index`**, **`cf_receipts`**, **`cf_state`**.

use borsh::{BorshDeserialize, BorshSerialize};
use fractal_consensus::Block;
use fractal_core::{EvmLog, State};
use fractal_crypto::hash::keccak256;
use fractal_shard::{MasterchainBlockV1, ShardAnchor};

pub const STORED_RECORD_V1: u8 = 1;

/// Key prefix: block by canonical height (`cf_blocks`).
pub const KEY_PREFIX_BLOCK_BY_HEIGHT: u8 = 0x01;
/// Key prefix: height lookup from block header hash (`cf_blocks`).
pub const KEY_PREFIX_BLOCK_HASH_TO_HEIGHT: u8 = 0x02;
/// Key prefix: full execution state after block `height` (`cf_state`).
pub const KEY_PREFIX_STATE_AT_HEIGHT: u8 = 0x03;
/// Key prefix: Ethereum account MPT root after block `height` (`cf_state`).
pub const KEY_PREFIX_EVM_ACCOUNT_MPT_ROOT_AT_HEIGHT: u8 = 0x04;
/// Key prefix: Ethereum MPT node body by `keccak256(rlp)` (`cf_state`).
pub const KEY_PREFIX_EVM_MPT_NODE: u8 = 0x05;
/// Key prefix: chunked snapshot v2 manifest (`cf_snapshots`).
pub const KEY_PREFIX_SNAPSHOT_V2_MANIFEST: u8 = 0x20;
/// Key prefix: chunked snapshot v2 state payload chunk (`cf_snapshots`).
pub const KEY_PREFIX_SNAPSHOT_V2_STATE_CHUNK: u8 = 0x21;
/// Key prefix: native VM event row (`cf_native_events`).
pub const KEY_PREFIX_NATIVE_EVENT: u8 = 0x10;
/// Key prefix: shard anchor at block height (`cf_shard_anchors`).
pub const KEY_PREFIX_SHARD_ANCHOR: u8 = 0x11;
/// Key prefix: masterchain block by height (`cf_masterchain`).
pub const KEY_PREFIX_MASTERCHAIN_BLOCK: u8 = 0x01;

/// Big-endian block height (used by `checkpoint_proofs`, `cf_snapshots`).
#[inline]
pub fn height_be_key(height: u64) -> [u8; 8] {
    height.to_be_bytes()
}

/// Height-keyed CF row namespaced per shard when `shard_count > 1`.
#[inline]
pub fn scoped_height_key(shard_id: u32, shard_count: u32, height: u64) -> Vec<u8> {
    scope_storage_key(shard_id, shard_count, &height_be_key(height))
}

/// When `shard_count > 1`, prefix application keys with big-endian `shard_id` (M10).
#[inline]
pub fn scope_storage_key(shard_id: u32, shard_count: u32, key: &[u8]) -> Vec<u8> {
    if shard_count <= 1 {
        return key.to_vec();
    }
    let mut out = Vec::with_capacity(4 + key.len());
    out.extend_from_slice(&shard_id.to_be_bytes());
    out.extend_from_slice(key);
    out
}

#[inline]
pub fn shard_anchor_key(shard_id: u32, block_height: u64) -> [u8; 13] {
    let mut k = [0u8; 13];
    k[0] = KEY_PREFIX_SHARD_ANCHOR;
    k[1..5].copy_from_slice(&shard_id.to_be_bytes());
    k[5..13].copy_from_slice(&block_height.to_be_bytes());
    k
}

#[inline]
pub fn masterchain_block_key(height: u64) -> [u8; 9] {
    let mut k = [0u8; 9];
    k[0] = KEY_PREFIX_MASTERCHAIN_BLOCK;
    k[1..].copy_from_slice(&height.to_be_bytes());
    k
}

#[inline]
pub fn block_by_height_key(height: u64) -> [u8; 9] {
    let mut k = [0u8; 9];
    k[0] = KEY_PREFIX_BLOCK_BY_HEIGHT;
    k[1..].copy_from_slice(&height.to_be_bytes());
    k
}

#[inline]
pub fn block_hash_to_height_key(header_hash: &[u8; 32]) -> [u8; 33] {
    let mut k = [0u8; 33];
    k[0] = KEY_PREFIX_BLOCK_HASH_TO_HEIGHT;
    k[1..].copy_from_slice(header_hash);
    k
}

#[inline]
pub fn state_at_height_key(height: u64) -> [u8; 9] {
    let mut k = [0u8; 9];
    k[0] = KEY_PREFIX_STATE_AT_HEIGHT;
    k[1..].copy_from_slice(&height.to_be_bytes());
    k
}

#[inline]
pub fn evm_mpt_root_at_height_key(height: u64) -> [u8; 9] {
    let mut k = [0u8; 9];
    k[0] = KEY_PREFIX_EVM_ACCOUNT_MPT_ROOT_AT_HEIGHT;
    k[1..].copy_from_slice(&height.to_be_bytes());
    k
}

#[inline]
pub fn evm_mpt_node_key(node_hash: [u8; 32]) -> [u8; 33] {
    let mut k = [0u8; 33];
    k[0] = KEY_PREFIX_EVM_MPT_NODE;
    k[1..].copy_from_slice(&node_hash);
    k
}

#[inline]
pub fn snapshot_v2_manifest_key(height: u64) -> [u8; 9] {
    let mut k = [0u8; 9];
    k[0] = KEY_PREFIX_SNAPSHOT_V2_MANIFEST;
    k[1..].copy_from_slice(&height.to_be_bytes());
    k
}

#[inline]
pub fn snapshot_v2_state_chunk_key(height: u64, chunk_index: u32) -> [u8; 13] {
    let mut k = [0u8; 13];
    k[0] = KEY_PREFIX_SNAPSHOT_V2_STATE_CHUNK;
    k[1..9].copy_from_slice(&height.to_be_bytes());
    k[9..13].copy_from_slice(&chunk_index.to_be_bytes());
    k
}

/// Key: `height_be || tx_index_be` under [`KEY_PREFIX_NATIVE_EVENT`].
#[inline]
pub fn native_event_key(height: u64, tx_index: u32) -> [u8; 13] {
    let mut k = [0u8; 13];
    k[0] = KEY_PREFIX_NATIVE_EVENT;
    k[1..9].copy_from_slice(&height.to_be_bytes());
    k[9..13].copy_from_slice(&tx_index.to_be_bytes());
    k
}

/// **`cf_blocks`** value: versioned full block (`borsh` includes consensus `Block`).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct StoredBlockV1 {
    pub version: u8,
    pub block: Block,
}

/// **`cf_tx_index`** value: transaction hash (key) → block location.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct StoredTxIndexV1 {
    pub version: u8,
    pub block_height: u64,
    pub tx_index: u32,
}

/// **`cf_receipts`** value: execution outcome for one transaction (RPC tx hash as key).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct StoredReceiptV1 {
    pub version: u8,
    pub block_height: u64,
    pub tx_index: u32,
    /// Sum of this tx’s gas plus all prior txs in the same block (`eth` cumulative style within block).
    pub cumulative_gas_used: u64,
    pub success: bool,
    pub logs_bloom: [u8; 256],
    pub logs: Vec<EvmLog>,
}

/// **`cf_state`** value: execution [`State`] immediately after committing `height`.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct StoredStateAtHeightV1 {
    pub version: u8,
    pub height: u64,
    pub state: State,
}

/// **`cf_state`**: Ethereum account MPT root (see [`crate::evm_accounts_mpt`]).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct StoredEvmAccountMptRootV1 {
    pub version: u8,
    pub height: u64,
    pub root: [u8; 32],
}

/// **`cf_native_events`**: one row per committed native [`Transaction`](fractal_core::Transaction).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct StoredNativeEventV1 {
    pub version: u8,
    pub block_height: u64,
    pub tx_index: u32,
    pub rpc_tx_hash: [u8; 32],
    pub tx_borsh: Vec<u8>,
}

/// **`cf_shard_anchors`** value (`docs/prd.md` §7.10.3).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct StoredShardAnchorV1 {
    pub version: u8,
    pub anchor: ShardAnchor,
}

/// **`cf_masterchain`** value (coordination layer sketch).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct StoredMasterchainBlockV1 {
    pub version: u8,
    pub block: MasterchainBlockV1,
}

/// Ethereum-style 2048-bit log bloom (same as `fractal_rpc::logs_bloom_256`).
#[must_use]
pub fn logs_bloom_256(evm_logs: &[EvmLog]) -> [u8; 256] {
    let mut bloom = [0u8; 256];
    for log in evm_logs {
        bloom_add(&mut bloom, &log.address);
        for t in &log.topics {
            bloom_add(&mut bloom, t);
        }
    }
    bloom
}

fn bloom_add(bloom: &mut [u8; 256], data: &[u8]) {
    let h = keccak256(data);
    let v1 = 1u8 << (h[1] & 0x7);
    let v2 = 1u8 << (h[3] & 0x7);
    let v3 = 1u8 << (h[5] & 0x7);
    let u16be = |a: usize| u16::from_be_bytes([h[a], h[a + 1]]);
    let idx = |pair_start: usize| -> usize {
        256usize - (((u16be(pair_start) & 0x7ff) >> 3) as usize) - 1
    };
    let i1 = idx(0);
    let i2 = idx(2);
    let i3 = idx(4);
    bloom[i1] |= v1;
    bloom[i2] |= v2;
    bloom[i3] |= v3;
}

#[cfg(test)]
mod key_tests {
    use super::*;

    #[test]
    fn scoped_keys_prefix_shard_when_multi_shard() {
        let k = block_by_height_key(7);
        assert_eq!(scope_storage_key(0, 1, &k), k);
        let scoped = scope_storage_key(3, 4, &k);
        assert_eq!(&scoped[0..4], 3u32.to_be_bytes());
        assert_eq!(&scoped[4..], k);
    }

    #[test]
    fn scoped_height_keys_differ_per_shard() {
        let a = scoped_height_key(0, 2, 100);
        let b = scoped_height_key(1, 2, 100);
        assert_ne!(a, b);
        assert_eq!(scoped_height_key(0, 1, 100), height_be_key(100).to_vec());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fractal_consensus::{Block, BlockHeader, genesis_parent_qc};

    #[test]
    fn stored_block_borsh_roundtrip() {
        let block = Block {
            header: BlockHeader {
                version: 1u16,
                chain_id: 41,
                height: 1,
                view: 0,
                parent_hash: [7u8; 32],
                parent_qc_hash: [8u8; 32],
                proposer: [9u8; 32],
                timestamp_ms: 1,
                state_root: [0xab; 32],
                tx_root: [0xbc; 32],
                gas_used: 21_000,
                gas_limit: 30_000_000,
                shard_id: 0,
                extra: [0u8; 32],
            },
            transactions: vec![],
            parent_qc: genesis_parent_qc(),
            parent_qc_signer_indices: vec![],
            eth_signed_raw: vec![],
        };
        let s = StoredBlockV1 {
            version: STORED_RECORD_V1,
            block: block.clone(),
        };
        let v = borsh::to_vec(&s).expect("encode");
        let d: StoredBlockV1 = borsh::from_slice(&v).expect("decode");
        assert_eq!(d, s);
    }
}
