//! PRD §10.4 chain snapshots.
//!
//! v1 is the original trusted full tip bundle. v2 keeps the same consensus metadata but uses a
//! chunked, hash-bound state payload plus an EVM account MPT root so import can verify and persist
//! production-shaped `cf_state` rows before syncing forward.

use borsh::{BorshDeserialize, BorshSerialize};
use fractal_consensus::{Block, QuorumCertificate, ValidatorEntry};
use fractal_core::State;
use fractal_crypto::Hash256;
use fractal_mempool::BaseFeeParams;
use fractal_proof_aggregator::Plonky2ProofBundleV1;
use fractal_shard::MasterchainBlockV1;

pub const CHAIN_SYNC_SNAPSHOT_V1_VERSION: u8 = 1;
pub const CHAIN_SYNC_SNAPSHOT_V2_VERSION: u8 = 2;
pub const CHAIN_SYNC_PROOF_SNAPSHOT_V1_VERSION: u8 = 3;
pub const SNAPSHOT_V2_STATE_CHUNK_KIND: u8 = 1;
pub const SNAPSHOT_V2_DEFAULT_CHUNK_BYTES: usize = 64 * 1024;

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ChainSyncSnapshotV1 {
    pub version: u8,
    pub chain_id: u64,
    pub shard_id: u32,
    pub shard_count: u32,
    pub height: u64,
    pub view: u64,
    pub head_hash: fractal_crypto::Hash256,
    pub parent_qc_hash: fractal_crypto::Hash256,
    pub high_prepare_qc: QuorumCertificate,
    pub validators: Vec<ValidatorEntry>,
    pub state: State,
    pub blocks: Vec<Block>,
    pub base_fee: u128,
    pub gas_limit: u64,
    pub fee_params: BaseFeeParams,
    pub min_consensus_stake_wei: u128,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ChainSyncSnapshotChunkV1 {
    pub version: u8,
    pub kind: u8,
    pub index: u32,
    pub bytes: Vec<u8>,
    pub hash: Hash256,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ChainSyncSnapshotV2 {
    pub version: u8,
    pub chain_id: u64,
    pub shard_id: u32,
    pub shard_count: u32,
    pub height: u64,
    pub view: u64,
    pub head_hash: Hash256,
    pub parent_qc_hash: Hash256,
    pub high_prepare_qc: QuorumCertificate,
    pub validators: Vec<ValidatorEntry>,
    pub state_root: Hash256,
    pub evm_account_mpt_root: Hash256,
    pub state_borsh_hash: Hash256,
    pub state_len: u64,
    pub state_chunks: Vec<ChainSyncSnapshotChunkV1>,
    /// Full block vector for the current dev node's in-memory chain model. The state payload is
    /// already root-verified; future pruned imports can replace this with proof-chain metadata.
    pub blocks: Vec<Block>,
    pub base_fee: u128,
    pub gas_limit: u64,
    pub fee_params: BaseFeeParams,
    pub min_consensus_stake_wei: u128,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct ChainSyncProofSnapshotV1 {
    pub version: u8,
    pub chain_id: u64,
    pub shard_id: u32,
    pub shard_count: u32,
    pub height: u64,
    pub view: u64,
    pub head_hash: Hash256,
    pub parent_qc_hash: Hash256,
    pub high_prepare_qc: QuorumCertificate,
    pub validators: Vec<ValidatorEntry>,
    pub state_root: Hash256,
    pub evm_account_mpt_root: Hash256,
    pub state_borsh_hash: Hash256,
    pub state_len: u64,
    pub state_chunks: Vec<ChainSyncSnapshotChunkV1>,
    /// Single checkpoint/tip execution block. Historical raw blocks are intentionally omitted;
    /// the masterchain proof chain below verifies the state root at this height.
    pub tip_block: Block,
    pub masterchain_blocks: Vec<MasterchainBlockV1>,
    pub plonky2: Option<Plonky2ProofBundleV1>,
    pub base_fee: u128,
    pub gas_limit: u64,
    pub fee_params: BaseFeeParams,
    pub min_consensus_stake_wei: u128,
}

#[must_use]
pub fn snapshot_v2_chunks(
    kind: u8,
    bytes: &[u8],
    chunk_size: usize,
) -> Vec<ChainSyncSnapshotChunkV1> {
    let chunk_size = chunk_size.max(1);
    bytes
        .chunks(chunk_size)
        .enumerate()
        .map(|(index, chunk)| ChainSyncSnapshotChunkV1 {
            version: CHAIN_SYNC_SNAPSHOT_V2_VERSION,
            kind,
            index: index as u32,
            bytes: chunk.to_vec(),
            hash: fractal_crypto::keccak256(chunk),
        })
        .collect()
}

pub fn reassemble_snapshot_v2_chunks(
    chunks: &[ChainSyncSnapshotChunkV1],
    expected_kind: u8,
    expected_len: u64,
    expected_hash: Hash256,
) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(expected_len as usize);
    for (expected_index, chunk) in chunks.iter().enumerate() {
        if chunk.version != CHAIN_SYNC_SNAPSHOT_V2_VERSION {
            return Err(format!("chunk version {}", chunk.version));
        }
        if chunk.kind != expected_kind {
            return Err(format!("chunk kind {} != {}", chunk.kind, expected_kind));
        }
        if chunk.index != expected_index as u32 {
            return Err(format!(
                "chunk index {} != expected {}",
                chunk.index, expected_index
            ));
        }
        let got = fractal_crypto::keccak256(&chunk.bytes);
        if got != chunk.hash {
            return Err(format!("chunk {} hash mismatch", chunk.index));
        }
        out.extend_from_slice(&chunk.bytes);
    }
    if out.len() as u64 != expected_len {
        return Err(format!(
            "state length {} != expected {}",
            out.len(),
            expected_len
        ));
    }
    let got = fractal_crypto::keccak256(&out);
    if got != expected_hash {
        return Err("state payload hash mismatch".into());
    }
    Ok(out)
}
