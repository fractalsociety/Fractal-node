//! PRD §10.3 column families + M9 `checkpoint_proofs` in a single RocksDB directory.
//!
//! Key layouts for application data are intentionally minimal here: higher layers own encoding.
//! Documented conventions (non-enforced):
//! - **`cf_state`** — `StoredStateAtHeightV1` at `0x03||height`; Ethereum account MPT root at `0x04||height`; MPT node RLP at `0x05||node_hash`.
//! - **`cf_blocks`** — block bytes keyed by convention e.g. `b"h/" || height_be` or `b"H/" || block_hash`.
//! - **`cf_tx_index`** — `tx_hash → (block_height, index)` encoded by owner.
//! - **`cf_receipts`** — receipt blobs keyed by tx hash or `(block, index)`.
//! - **`cf_native_events`** — indexed native log payloads.
//! - **`cf_mempool`** — backup of pending tx blobs (primary remains in-memory on the node).
//! - **`cf_consensus`** — votes, QCs, view metadata (encoding TBD).
//! - **`cf_snapshots`** — periodic fast-sync bundles; typically `height_be: [u8;8] → opaque blob`.
//! - **`checkpoint_proofs`** — `height_be: [u8;8] → borsh(PersistedCheckpointProofV1)` (M9).

use std::path::Path;

use borsh::BorshDeserialize;
use fractal_consensus::{Block, FormedQc, Vote, header_hash};
use fractal_core::{State, VmKind};
use fractal_crypto::hash::keccak256;
use rocksdb::{ColumnFamily, ColumnFamilyDescriptor, DB, Options};
use thiserror::Error;

use crate::chain_records::{
    STORED_RECORD_V1, StoredBlockV1, StoredMasterchainBlockV1, StoredNativeEventV1,
    StoredReceiptV1, StoredShardAnchorV1, StoredStateAtHeightV1, StoredTxIndexV1,
    block_by_height_key, block_hash_to_height_key, evm_mpt_root_at_height_key, logs_bloom_256,
    masterchain_block_key, native_event_key, scope_storage_key, shard_anchor_key,
    snapshot_v2_manifest_key, snapshot_v2_state_chunk_key, state_at_height_key,
};
use fractal_shard::{MasterchainBlockV1, ShardAnchor};

/// PRD §10.3 — execution state trie (future MPT).
pub const CF_STATE: &str = "cf_state";
/// Canonical blocks by height / hash (keying convention up to indexer / node).
pub const CF_BLOCKS: &str = "cf_blocks";
/// Transaction hash → block location.
pub const CF_TX_INDEX: &str = "cf_tx_index";
/// Execution receipts / outcomes.
pub const CF_RECEIPTS: &str = "cf_receipts";
/// Native VM event log index.
pub const CF_NATIVE_EVENTS: &str = "cf_native_events";
/// Mempool backup (node keeps primary mempool in RAM).
pub const CF_MEMPOOL: &str = "cf_mempool";
/// HotStuff-2 / PBFT auxiliary persistence (votes, QCs, views).
pub const CF_CONSENSUS: &str = "cf_consensus";
/// State / chain snapshots for fast sync (PRD §10.4).
pub const CF_SNAPSHOTS: &str = "cf_snapshots";
/// Async STWO checkpoint proofs (PRD §7.8 / M9); not in §10.3 table but shares the same DB.
pub const CF_CHECKPOINT_PROOFS: &str = "checkpoint_proofs";
/// Shard → masterchain anchors (`docs/prd.md` §7.10, M10+).
pub const CF_SHARD_ANCHORS: &str = "cf_shard_anchors";
/// Masterchain coordination blocks (M11 sketch; local ledger on shard nodes).
pub const CF_MASTERCHAIN: &str = "cf_masterchain";

/// All column families opened together ([`FractalRocksDb::open`]).
pub const ALL_COLUMN_FAMILIES: &[&str] = &[
    CF_STATE,
    CF_BLOCKS,
    CF_TX_INDEX,
    CF_RECEIPTS,
    CF_NATIVE_EVENTS,
    CF_MEMPOOL,
    CF_CONSENSUS,
    CF_SNAPSHOTS,
    CF_CHECKPOINT_PROOFS,
    CF_SHARD_ANCHORS,
    CF_MASTERCHAIN,
];

fn cf_descriptors() -> Vec<ColumnFamilyDescriptor> {
    ALL_COLUMN_FAMILIES
        .iter()
        .map(|name| ColumnFamilyDescriptor::new(*name, Options::default()))
        .collect()
}

#[derive(Debug, Error)]
pub enum RocksStoreError {
    #[error("rocksdb: {0}")]
    Rocks(#[from] rocksdb::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("unknown column family: {0}")]
    UnknownColumnFamily(String),
}

/// Single RocksDB handle with every PRD §10.3 family plus `checkpoint_proofs`.
#[derive(Clone)]
pub struct FractalRocksDb {
    db: std::sync::Arc<DB>,
}

impl std::fmt::Debug for FractalRocksDb {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("FractalRocksDb { .. }")
    }
}

#[derive(Debug, Error)]
pub enum ChainPersistError {
    #[error(transparent)]
    Rocks(#[from] RocksStoreError),
    #[error("borsh: {0}")]
    Borsh(#[from] std::io::Error),
    #[error("tx rpc hash len {got} != tx count {expected}")]
    TxRpcHashCount { expected: usize, got: usize },
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct PrunedExecutionRows {
    pub blocks: usize,
    pub block_hash_indexes: usize,
    pub tx_indexes: usize,
    pub receipts: usize,
    pub native_events: usize,
    pub state_rows: usize,
}

impl FractalRocksDb {
    /// Opens or creates the database and all column families (existing data dirs get new CFs added).
    pub fn open(path: &Path) -> Result<Self, RocksStoreError> {
        std::fs::create_dir_all(path)?;
        let mut db_opts = Options::default();
        db_opts.create_if_missing(true);
        db_opts.create_missing_column_families(true);
        let db = DB::open_cf_descriptors(&db_opts, path, cf_descriptors())?;
        Ok(Self {
            db: std::sync::Arc::new(db),
        })
    }

    fn cf(&self, name: &str) -> Result<&ColumnFamily, RocksStoreError> {
        self.db
            .cf_handle(name)
            .ok_or_else(|| RocksStoreError::UnknownColumnFamily(name.to_string()))
    }

    /// Opaque put for any known PRD / checkpoint column family.
    pub fn put_raw(&self, cf: &str, key: &[u8], val: &[u8]) -> Result<(), RocksStoreError> {
        let handle = self.cf(cf)?;
        self.db.put_cf(handle, key, val)?;
        Ok(())
    }

    /// Opaque get for any known column family.
    pub fn get_raw(&self, cf: &str, key: &[u8]) -> Result<Option<Vec<u8>>, RocksStoreError> {
        let handle = self.cf(cf)?;
        Ok(self.db.get_cf(handle, key)?)
    }

    /// Delete a raw key from a known column family; returns whether a key existed.
    pub fn delete_raw(&self, cf: &str, key: &[u8]) -> Result<bool, RocksStoreError> {
        let handle = self.cf(cf)?;
        let existed = self.db.get_cf(handle, key)?.is_some();
        if existed {
            self.db.delete_cf(handle, key)?;
        }
        Ok(existed)
    }

    // --- M9 checkpoint proofs (shard-scoped when `shard_count > 1`) ---

    pub fn put_proof_blob(
        &self,
        shard_id: u32,
        shard_count: u32,
        height: u64,
        blob: &[u8],
    ) -> Result<(), RocksStoreError> {
        let key = crate::chain_records::scoped_height_key(shard_id, shard_count, height);
        self.put_raw(CF_CHECKPOINT_PROOFS, &key, blob)
    }

    pub fn get_proof_blob(
        &self,
        shard_id: u32,
        shard_count: u32,
        height: u64,
    ) -> Result<Option<Vec<u8>>, RocksStoreError> {
        let key = crate::chain_records::scoped_height_key(shard_id, shard_count, height);
        self.get_raw(CF_CHECKPOINT_PROOFS, &key)
    }

    /// Delete a checkpoint proof row; returns whether a key existed.
    pub fn delete_proof_blob(
        &self,
        shard_id: u32,
        shard_count: u32,
        height: u64,
    ) -> Result<bool, RocksStoreError> {
        let handle = self.cf(CF_CHECKPOINT_PROOFS)?;
        let key = crate::chain_records::scoped_height_key(shard_id, shard_count, height);
        let existed = self.db.get_cf(handle, &key)?.is_some();
        if existed {
            self.db.delete_cf(handle, &key)?;
        }
        Ok(existed)
    }

    /// **`cf_snapshots`**: store a fast-sync or checkpoint blob at `height`.
    pub fn put_snapshot_blob(
        &self,
        shard_id: u32,
        shard_count: u32,
        height: u64,
        blob: &[u8],
    ) -> Result<(), RocksStoreError> {
        let key = crate::chain_records::scoped_height_key(shard_id, shard_count, height);
        self.put_raw(CF_SNAPSHOTS, &key, blob)
    }

    pub fn get_snapshot_blob(
        &self,
        shard_id: u32,
        shard_count: u32,
        height: u64,
    ) -> Result<Option<Vec<u8>>, RocksStoreError> {
        let key = crate::chain_records::scoped_height_key(shard_id, shard_count, height);
        self.get_raw(CF_SNAPSHOTS, &key)
    }

    pub fn put_snapshot_v2_manifest(
        &self,
        shard_id: u32,
        shard_count: u32,
        height: u64,
        manifest: &[u8],
    ) -> Result<(), RocksStoreError> {
        let key = scope_storage_key(shard_id, shard_count, &snapshot_v2_manifest_key(height));
        self.put_raw(CF_SNAPSHOTS, &key, manifest)
    }

    pub fn get_snapshot_v2_manifest(
        &self,
        shard_id: u32,
        shard_count: u32,
        height: u64,
    ) -> Result<Option<Vec<u8>>, RocksStoreError> {
        let key = scope_storage_key(shard_id, shard_count, &snapshot_v2_manifest_key(height));
        self.get_raw(CF_SNAPSHOTS, &key)
    }

    pub fn put_snapshot_v2_state_chunk(
        &self,
        shard_id: u32,
        shard_count: u32,
        height: u64,
        chunk_index: u32,
        chunk: &[u8],
    ) -> Result<(), RocksStoreError> {
        let key = scope_storage_key(
            shard_id,
            shard_count,
            &snapshot_v2_state_chunk_key(height, chunk_index),
        );
        self.put_raw(CF_SNAPSHOTS, &key, chunk)
    }

    pub fn get_snapshot_v2_state_chunk(
        &self,
        shard_id: u32,
        shard_count: u32,
        height: u64,
        chunk_index: u32,
    ) -> Result<Option<Vec<u8>>, RocksStoreError> {
        let key = scope_storage_key(
            shard_id,
            shard_count,
            &snapshot_v2_state_chunk_key(height, chunk_index),
        );
        self.get_raw(CF_SNAPSHOTS, &key)
    }

    /// **`cf_blocks`**, **`cf_tx_index`**, **`cf_receipts`** for one height (no **`cf_state`** row).
    pub fn persist_block_indexes_v1(
        &self,
        block: &Block,
        state: &State,
        tx_rpc_hashes: &[[u8; 32]],
        shard_count: u32,
    ) -> Result<(), ChainPersistError> {
        let n = block.transactions.len();
        if tx_rpc_hashes.len() != n {
            return Err(ChainPersistError::TxRpcHashCount {
                expected: n,
                got: tx_rpc_hashes.len(),
            });
        }
        let h = block.header.height;
        let sid = block.header.shard_id;
        let sb = StoredBlockV1 {
            version: STORED_RECORD_V1,
            block: block.clone(),
        };
        let blk_h = scope_storage_key(sid, shard_count, &block_by_height_key(h));
        self.put_raw(CF_BLOCKS, &blk_h, &borsh::to_vec(&sb)?)?;
        let hh = header_hash(&block.header)?;
        let blk_hash = scope_storage_key(sid, shard_count, &block_hash_to_height_key(&hh));
        self.put_raw(CF_BLOCKS, &blk_hash, &h.to_be_bytes())?;

        let mut cumulative_gas = 0u64;
        for (i, tx) in block.transactions.iter().enumerate() {
            let raw = borsh::to_vec(tx)?;
            let ih = keccak256(&raw);
            let rpc_h = tx_rpc_hashes[i];
            cumulative_gas =
                cumulative_gas.saturating_add(state.evm_tx_gas_used.get(&ih).copied().unwrap_or(0));
            let logs = state.evm_tx_logs.get(&ih).cloned().unwrap_or_default();
            let bloom = logs_bloom_256(&logs);
            let success = state.evm_tx_success.get(&ih).copied().unwrap_or(true);
            let rec = StoredReceiptV1 {
                version: STORED_RECORD_V1,
                block_height: h,
                tx_index: i as u32,
                cumulative_gas_used: cumulative_gas,
                success,
                logs_bloom: bloom,
                logs,
            };
            let txi = StoredTxIndexV1 {
                version: STORED_RECORD_V1,
                block_height: h,
                tx_index: i as u32,
            };
            let tx_k = scope_storage_key(sid, shard_count, &rpc_h);
            self.put_raw(CF_TX_INDEX, &tx_k, &borsh::to_vec(&txi)?)?;
            self.put_raw(CF_RECEIPTS, &tx_k, &borsh::to_vec(&rec)?)?;
            if tx.vm == VmKind::Native {
                let ev = StoredNativeEventV1 {
                    version: STORED_RECORD_V1,
                    block_height: h,
                    tx_index: i as u32,
                    rpc_tx_hash: rpc_h,
                    tx_borsh: raw,
                };
                let ev_k = scope_storage_key(sid, shard_count, &native_event_key(h, i as u32));
                self.put_raw(CF_NATIVE_EVENTS, &ev_k, &borsh::to_vec(&ev)?)?;
            }
        }
        Ok(())
    }

    /// Delete height-scoped execution rows written by [`Self::persist_block_commit_v1`].
    ///
    /// Content-addressed EVM MPT nodes are intentionally retained because they may be shared by
    /// retained state roots; height-keyed state snapshots and roots are dropped.
    pub fn prune_execution_height_v1(
        &self,
        shard_id: u32,
        shard_count: u32,
        height: u64,
    ) -> Result<PrunedExecutionRows, ChainPersistError> {
        let mut out = PrunedExecutionRows::default();
        let block_key = scope_storage_key(shard_id, shard_count, &block_by_height_key(height));
        let stored_block = match self.get_raw(CF_BLOCKS, &block_key)? {
            Some(raw) => Some(StoredBlockV1::try_from_slice(&raw)?),
            None => None,
        };

        if let Some(stored) = stored_block.as_ref() {
            let hh = header_hash(&stored.block.header)?;
            let hash_key = scope_storage_key(shard_id, shard_count, &block_hash_to_height_key(&hh));
            if self.delete_raw(CF_BLOCKS, &hash_key)? {
                out.block_hash_indexes += 1;
            }

            for (i, tx) in stored.block.transactions.iter().enumerate() {
                let raw = borsh::to_vec(tx)?;
                let ih = keccak256(&raw);
                let rpc_h = if let Some(Some(eth_raw)) = stored.block.eth_signed_raw.get(i) {
                    let eh = keccak256(eth_raw);
                    if eh != ih { eh } else { ih }
                } else {
                    ih
                };
                let tx_key = scope_storage_key(shard_id, shard_count, &rpc_h);
                if self.delete_raw(CF_TX_INDEX, &tx_key)? {
                    out.tx_indexes += 1;
                }
                if self.delete_raw(CF_RECEIPTS, &tx_key)? {
                    out.receipts += 1;
                }
                if tx.vm == VmKind::Native {
                    let ev_key = scope_storage_key(
                        shard_id,
                        shard_count,
                        &native_event_key(height, i as u32),
                    );
                    if self.delete_raw(CF_NATIVE_EVENTS, &ev_key)? {
                        out.native_events += 1;
                    }
                }
            }
        }

        if self.delete_raw(CF_BLOCKS, &block_key)? {
            out.blocks += 1;
        }
        let state_key = scope_storage_key(shard_id, shard_count, &state_at_height_key(height));
        if self.delete_raw(CF_STATE, &state_key)? {
            out.state_rows += 1;
        }
        let mpt_root_key =
            scope_storage_key(shard_id, shard_count, &evm_mpt_root_at_height_key(height));
        if self.delete_raw(CF_STATE, &mpt_root_key)? {
            out.state_rows += 1;
        }
        Ok(out)
    }

    /// Execution state committed after block `height` (**`cf_state`**: full `borsh(State)` snapshot).
    pub fn persist_state_at_height_v1(
        &self,
        shard_id: u32,
        shard_count: u32,
        height: u64,
        state: &State,
    ) -> Result<(), ChainPersistError> {
        let ss = StoredStateAtHeightV1 {
            version: STORED_RECORD_V1,
            height,
            state: state.clone(),
        };
        let st_k = scope_storage_key(shard_id, shard_count, &state_at_height_key(height));
        self.put_raw(CF_STATE, &st_k, &borsh::to_vec(&ss)?)?;
        crate::evm_accounts_mpt::persist_evm_account_mpt_to_cf_state(
            self,
            shard_id,
            shard_count,
            height,
            state,
        )?;
        Ok(())
    }

    /// Persist a shard anchor (`cf_shard_anchors`).
    pub fn persist_shard_anchor_v1(&self, anchor: &ShardAnchor) -> Result<(), ChainPersistError> {
        let rec = StoredShardAnchorV1 {
            version: STORED_RECORD_V1,
            anchor: anchor.clone(),
        };
        self.put_raw(
            CF_SHARD_ANCHORS,
            &shard_anchor_key(anchor.shard_id, anchor.block_height),
            &borsh::to_vec(&rec)?,
        )?;
        Ok(())
    }

    pub fn get_shard_anchor_v1(
        &self,
        shard_id: u32,
        block_height: u64,
    ) -> Result<Option<ShardAnchor>, ChainPersistError> {
        let Some(raw) =
            self.get_raw(CF_SHARD_ANCHORS, &shard_anchor_key(shard_id, block_height))?
        else {
            return Ok(None);
        };
        let rec = StoredShardAnchorV1::try_from_slice(&raw)?;
        Ok(Some(rec.anchor))
    }

    /// Persist a masterchain block (`cf_masterchain`).
    pub fn persist_masterchain_block_v1(
        &self,
        block: &MasterchainBlockV1,
    ) -> Result<(), ChainPersistError> {
        let rec = StoredMasterchainBlockV1 {
            version: STORED_RECORD_V1,
            block: block.clone(),
        };
        self.put_raw(
            CF_MASTERCHAIN,
            &masterchain_block_key(block.height),
            &borsh::to_vec(&rec)?,
        )?;
        Ok(())
    }

    pub fn get_masterchain_block_v1(
        &self,
        height: u64,
    ) -> Result<Option<MasterchainBlockV1>, ChainPersistError> {
        let raw = match self.get_raw(CF_MASTERCHAIN, &masterchain_block_key(height))? {
            Some(b) => b,
            None => return Ok(None),
        };
        let rec = StoredMasterchainBlockV1::try_from_slice(&raw)?;
        Ok(Some(rec.block))
    }

    /// Backup a pending transaction's wire bytes (`cf_mempool`); primary mempool stays in RAM.
    pub fn mempool_put_backup_v1(
        &self,
        shard_id: u32,
        shard_count: u32,
        rpc_tx_hash: &[u8; 32],
        raw_tx: &[u8],
    ) -> Result<(), RocksStoreError> {
        let key = scope_storage_key(shard_id, shard_count, rpc_tx_hash);
        self.put_raw(CF_MEMPOOL, &key, raw_tx)
    }

    pub fn mempool_delete_backup(
        &self,
        shard_id: u32,
        shard_count: u32,
        rpc_tx_hash: &[u8; 32],
    ) -> Result<(), RocksStoreError> {
        let h = self.cf(CF_MEMPOOL)?;
        let key = scope_storage_key(shard_id, shard_count, rpc_tx_hash);
        self.db.delete_cf(h, &key)?;
        Ok(())
    }

    /// Persist a verified vote (`cf_consensus`).
    pub fn persist_consensus_vote_v1(
        &self,
        shard_id: u32,
        shard_count: u32,
        vote: &Vote,
    ) -> Result<(), ChainPersistError> {
        let mut inner = Vec::with_capacity(1 + 8 + 8 + 32 + 4);
        inner.push(0x01);
        inner.extend(vote.view.to_be_bytes());
        inner.extend(vote.height.to_be_bytes());
        inner.extend_from_slice(&vote.header_hash);
        inner.extend(vote.validator_index.to_be_bytes());
        let k = scope_storage_key(shard_id, shard_count, &inner);
        self.put_raw(CF_CONSENSUS, &k, &borsh::to_vec(vote)?)?;
        Ok(())
    }

    /// Persist a formed quorum certificate (`cf_consensus`).
    pub fn persist_consensus_formed_qc_v1(
        &self,
        shard_id: u32,
        shard_count: u32,
        formed: &FormedQc,
    ) -> Result<(), ChainPersistError> {
        let mut inner = Vec::with_capacity(1 + 8 + 8 + 32);
        inner.push(0x02);
        inner.extend(formed.qc.view.to_be_bytes());
        inner.extend(formed.qc.block_height.to_be_bytes());
        inner.extend_from_slice(&formed.qc.block_header_hash);
        let k = scope_storage_key(shard_id, shard_count, &inner);
        self.put_raw(CF_CONSENSUS, &k, &borsh::to_vec(formed)?)?;
        Ok(())
    }

    /// Persist one committed block: **`cf_blocks`**, **`cf_tx_index`**, **`cf_receipts`**, **`cf_state`**.
    ///
    /// `tx_rpc_hashes[i]` is the JSON-RPC transaction hash for `block.transactions[i]`. `state` is
    /// execution state **after** this block.
    pub fn persist_block_commit_v1(
        &self,
        block: &Block,
        state: &State,
        tx_rpc_hashes: &[[u8; 32]],
        shard_count: u32,
    ) -> Result<(), ChainPersistError> {
        let sid = block.header.shard_id;
        self.persist_block_indexes_v1(block, state, tx_rpc_hashes, shard_count)?;
        self.persist_state_at_height_v1(sid, shard_count, block.header.height, state)?;
        Ok(())
    }
}

/// Backward-compatible name for [`FractalRocksDb`] (M9 proof persistence only used these APIs).
pub type RocksCheckpointProofStore = FractalRocksDb;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persist_empty_block_commit_roundtrip() {
        use fractal_consensus::{Block, BlockHeader, genesis_parent_qc};
        use fractal_core::State;

        let dir =
            std::env::temp_dir().join(format!("fractal_rocks_persist_blk_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = FractalRocksDb::open(&dir).expect("open");
        let state = State::default();
        let block = Block {
            header: BlockHeader {
                version: 1,
                chain_id: 41,
                height: 1,
                view: 0,
                parent_hash: [7u8; 32],
                parent_qc_hash: [8u8; 32],
                proposer: [9u8; 32],
                timestamp_ms: 1,
                state_root: fractal_core::state_root(&state).expect("sr"),
                tx_root: fractal_consensus::ordered_tx_root(&[]).expect("tr"),
                gas_used: 0,
                gas_limit: 30_000_000,
                shard_id: 0,
                extra: [0u8; 32],
            },
            transactions: vec![],
            parent_qc: genesis_parent_qc(),
            parent_qc_signer_indices: vec![],
            eth_signed_raw: vec![],
        };
        db.persist_block_commit_v1(&block, &state, &[], 1)
            .expect("persist");
        let raw = db
            .get_raw(CF_BLOCKS, &block_by_height_key(1))
            .expect("get")
            .expect("row");
        let got: crate::chain_records::StoredBlockV1 = borsh::from_slice(&raw).expect("decode");
        assert_eq!(got.block.header.height, 1);
        let st = db
            .get_raw(CF_STATE, &state_at_height_key(1))
            .expect("get st")
            .expect("st row");
        let got_st: crate::chain_records::StoredStateAtHeightV1 =
            borsh::from_slice(&st).expect("decode st");
        assert_eq!(got_st.height, 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn opens_all_column_families() {
        let dir = std::env::temp_dir().join(format!("fractal_rocks_all_cf_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = FractalRocksDb::open(&dir).expect("open");
        for name in ALL_COLUMN_FAMILIES {
            db.cf(name).unwrap_or_else(|_| panic!("missing CF {name}"));
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn raw_put_get_round_trip_each_prd_cf() {
        let dir = std::env::temp_dir().join(format!(
            "fractal_rocks_prd_roundtrip_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        let db = FractalRocksDb::open(&dir).expect("open");
        for (i, name) in ALL_COLUMN_FAMILIES.iter().enumerate() {
            let key = format!("k{i}").into_bytes();
            let val = format!("v-{name}").into_bytes();
            db.put_raw(name, &key, &val).expect("put");
            assert_eq!(db.get_raw(name, &key).expect("get"), Some(val));
        }
        drop(db);
        let db2 = FractalRocksDb::open(&dir).expect("reopen");
        assert_eq!(
            db2.get_raw(CF_STATE, b"k0").expect("get"),
            Some(b"v-cf_state".to_vec())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn checkpoint_proof_round_trip_alias() {
        let dir =
            std::env::temp_dir().join(format!("fractal_rocks_cp_proof_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = RocksCheckpointProofStore::open(&dir).expect("open");
        db.put_proof_blob(0, 1, 7, b"hello-rocksdb").expect("put");
        assert_eq!(
            db.get_proof_blob(0, 1, 7).expect("get"),
            Some(b"hello-rocksdb".to_vec())
        );
        assert_eq!(db.get_proof_blob(0, 1, 8).expect("get2"), None);
        drop(db);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn snapshot_blob_helpers() {
        let dir = std::env::temp_dir().join(format!("fractal_rocks_snap_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = FractalRocksDb::open(&dir).expect("open");
        db.put_snapshot_blob(0, 1, 100_000, b"snap-v1-bytes")
            .expect("snap put");
        assert_eq!(
            db.get_snapshot_blob(0, 1, 100_000).expect("get"),
            Some(b"snap-v1-bytes".to_vec())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn checkpoint_proofs_isolated_per_shard_in_one_db() {
        let dir =
            std::env::temp_dir().join(format!("fractal_rocks_cp_shard_ns_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db = FractalRocksDb::open(&dir).expect("open");
        db.put_proof_blob(0, 2, 42, b"shard0-proof").expect("put0");
        db.put_proof_blob(1, 2, 42, b"shard1-proof").expect("put1");
        assert_eq!(
            db.get_proof_blob(0, 2, 42).expect("get0"),
            Some(b"shard0-proof".to_vec())
        );
        assert_eq!(
            db.get_proof_blob(1, 2, 42).expect("get1"),
            Some(b"shard1-proof".to_vec())
        );
        assert!(db.delete_proof_blob(0, 2, 42).expect("del"));
        assert!(db.get_proof_blob(0, 2, 42).expect("gone").is_none());
        assert!(db.get_proof_blob(1, 2, 42).expect("still").is_some());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
