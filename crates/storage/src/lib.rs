//! RocksDB-backed persistence (PRD §10 Storage Layer / M9 proofs).
//!
//! [`FractalRocksDb`] opens **all** PRD §10.3 column families in one database:
//! `cf_state`, `cf_blocks`, `cf_tx_index`, `cf_receipts`, `cf_native_events`,
//! `cf_mempool`, `cf_consensus`, `cf_snapshots`, plus **`checkpoint_proofs`**
//! (M9 async STWO artifacts). Use [`FractalRocksDb::put_raw`] / [`FractalRocksDb::get_raw`]
//! for opaque key–value data, or typed helpers like [`FractalRocksDb::persist_block_commit_v1`].

mod chain_records;
pub mod evm_accounts_mpt;
mod fractal_db;

pub use chain_records::{
    STORED_RECORD_V1, StoredBlockV1, StoredEvmAccountMptRootV1, StoredMasterchainBlockV1,
    StoredNativeEventV1, StoredReceiptV1, StoredShardAnchorV1, StoredStateAtHeightV1,
    StoredTxIndexV1, block_by_height_key, block_hash_to_height_key, evm_mpt_node_key,
    evm_mpt_root_at_height_key, height_be_key, logs_bloom_256, masterchain_block_key,
    native_event_key, scope_storage_key, scoped_height_key, shard_anchor_key, state_at_height_key,
};
pub use fractal_db::{
    ALL_COLUMN_FAMILIES, CF_BLOCKS, CF_CHECKPOINT_PROOFS, CF_CONSENSUS, CF_MASTERCHAIN, CF_MEMPOOL,
    CF_NATIVE_EVENTS, CF_RECEIPTS, CF_SHARD_ANCHORS, CF_SNAPSHOTS, CF_STATE, CF_TX_INDEX,
    ChainPersistError, FractalRocksDb, RocksCheckpointProofStore, RocksStoreError,
};
