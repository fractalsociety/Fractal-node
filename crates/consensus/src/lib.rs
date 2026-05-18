//! HotStuff-2–oriented block types (`docs/prd.md` §7.3, §18 M2/M7).
//!
//! [`Block`] carries an embedded parent [`QuorumCertificate`] + signer indices (M7-d-6); `header.parent_qc_hash`
//! is always `keccak256(borsh(parent_qc))`. Execution uses revm + native txs; header hashing is deterministic.
//!
//! [`qc`] defines quorum certificate hashing and `parent_qc_hash` / embedded parent QC bundles
//! (`docs/prd.md` §18 M7, M7-d-6).
//!
//! [`validators`] holds static validator sets and view-based leader ids (`docs/prd.md` §18 M7-b).
//!
//! [`vote`] holds per-validator HotStuff-2 vote wire types (`docs/prd.md` §18 M7-d-3).
//!
//! [`timeout`] holds view-timeout wire types for pacemaker / view advance (`docs/prd.md` §7.4, M7-f).

use borsh::{BorshDeserialize, BorshSerialize};
use fractal_core::{state_root, ExecError, State, Transaction};
use fractal_crypto::hash::{keccak256, Hash256};
use thiserror::Error;

pub mod hyperbft;
pub mod hyperbft_pipeline;
pub mod optimistic;
pub mod qc;
pub mod timeout;
pub mod validators;
pub mod vote;

pub use fractal_core::Transaction as Tx;
pub use qc::{
    genesis_parent_qc, hash_qc, is_genesis_parent_qc, singleton_qc_certifying, QuorumCertificate,
};
pub use validators::{ValidatorEntry, ValidatorId, ValidatorSet};
pub use timeout::{
    high_qc_rank, verify_formed_timeout_cert, FormedTimeoutCert, RecordTimeoutOutcome, Timeout,
    TimeoutError, TimeoutPool, TimeoutSignBody,
};
pub use hyperbft::{
    parent_qc_bundle, resolve_parent_qc, CertifiedParent, ConsensusMode, HyperBftConfig,
    HyperBftPipeline, ParentQcResolution,
};
pub use hyperbft_pipeline::{
    PipelineSlot, ThreeStagePipeline, ThreeStageTickSummary,
};
pub use optimistic::OptimisticExecution;
pub use vote::{
    verify_formed_qc, FormedQc, RecordVoteOutcome, Vote, VoteError, VotePool, VoteSignBody,
};
pub use fractal_bft_wire::{
    verify_consensus_misbehavior_evidence, ConsensusMisbehaviorEvidenceV1, MisbehaviorError,
    MisbehaviorKind,
};
pub use fractal_core::BlockFinalizeContext;

#[derive(Debug, Error)]
pub enum BuildBlockError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Exec(#[from] ExecError),
    #[error("eth_signed_raw length {got} != transactions length {txs}")]
    EthRawLenMismatch { txs: usize, got: usize },
}

/// Legacy floor gas per tx (EVM transfer); native txs use [`fractal_core::intrinsic_gas`].
pub const MIN_TX_GAS: u64 = 21_000;

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct BlockHeader {
    pub version: u16,
    pub chain_id: u64,
    pub height: u64,
    pub view: u64,
    pub parent_hash: Hash256,
    /// Parent QC hash (HotStuff-2): `keccak256(borsh(QC))` certifying the parent block header.
    /// First real block uses [`crate::genesis_parent_qc`]; see [`crate::qc`].
    pub parent_qc_hash: Hash256,
    pub proposer: [u8; 32],
    pub timestamp_ms: u64,
    pub state_root: Hash256,
    pub tx_root: Hash256,
    pub gas_used: u64,
    pub gas_limit: u64,
    /// Execution shard (`docs/prd.md` §7.9 / M10). Track A monolith uses `0`.
    pub shard_id: u32,
    pub extra: [u8; 32],
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
    /// Parallel to `transactions`: optional original EIP-1559 bytes (`keccak256` = RPC tx hash).
    /// Followers replay this to populate `NodeInner::eth_signed_raw` / hash maps like the producer.
    pub eth_signed_raw: Vec<Option<Vec<u8>>>,
    /// QC certifying the **parent** of this block (`header.height - 1`), i.e. the chain tip before
    /// this block applied. Height-1 blocks use [`crate::genesis_parent_qc`] with empty signers.
    /// M7-d-6: aggregate signature is verified against `parent_qc_signer_indices` on sync.
    pub parent_qc: QuorumCertificate,
    /// Validators that participated in `parent_qc.aggregate_sig` (indices into the active set).
    pub parent_qc_signer_indices: Vec<u32>,
}

fn tx_hash(tx: &Transaction) -> Result<Hash256, std::io::Error> {
    Ok(keccak256(&borsh::to_vec(tx)?))
}

fn hash_pair(left: &Hash256, right: &Hash256) -> Hash256 {
    let mut buf = [0u8; 64];
    buf[..32].copy_from_slice(left);
    buf[32..].copy_from_slice(right);
    keccak256(&buf)
}

/// Ordered Merkle root over transaction hashes (matches canonical tx order in the block).
pub fn ordered_tx_root(txs: &[Transaction]) -> Result<Hash256, std::io::Error> {
    if txs.is_empty() {
        return Ok([0u8; 32]);
    }
    let mut level: Vec<Hash256> = txs.iter().map(tx_hash).collect::<Result<_, _>>()?;
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
    Ok(level[0])
}

pub fn header_hash(header: &BlockHeader) -> Result<Hash256, std::io::Error> {
    Ok(keccak256(&borsh::to_vec(header)?))
}

/// One `None` per transaction when no Ethereum signed envelope is present.
pub fn eth_signed_raws_for_txs(txs_len: usize) -> Vec<Option<Vec<u8>>> {
    vec![None; txs_len]
}

/// Execute `txs` on top of `state`, compute roots, and assemble a `Block`.
///
/// `parent_qc` / `parent_qc_signer_indices` are the HotStuff-2 QC bundle for the parent tip;
/// `header.parent_qc_hash` is always `hash_qc(&parent_qc)`.
///
/// When `finalize` is `Some`, [`fractal_core::finalize_block_hooks`] runs after successful execution
/// (unbonding payouts + optional block rewards).
pub fn execute_and_build_block(
    chain_id: u64,
    shard_id: u32,
    height: u64,
    view: u64,
    parent_hash: Hash256,
    parent_qc: QuorumCertificate,
    parent_qc_signer_indices: Vec<u32>,
    proposer: [u8; 32],
    timestamp_ms: u64,
    gas_limit: u64,
    state: &mut State,
    txs: Vec<Transaction>,
    eth_signed_raw: Vec<Option<Vec<u8>>>,
    finalize: Option<BlockFinalizeContext<'_>>,
) -> Result<Block, BuildBlockError> {
    if eth_signed_raw.len() != txs.len() {
        return Err(BuildBlockError::EthRawLenMismatch {
            txs: txs.len(),
            got: eth_signed_raw.len(),
        });
    }
    let mut budget_sum = 0u64;
    for tx in &txs {
        let g = fractal_core::tx_gas_limit(tx)?;
        budget_sum = budget_sum.checked_add(g).ok_or(ExecError::GasOverflow)?;
    }
    if budget_sum > gas_limit {
        return Err(ExecError::GasLimitExceeded.into());
    }
    let mut evm = fractal_evm::RevmEngine::default();
    let gas_used = fractal_core::apply_block_with_evm(state, &txs, &mut evm)?;
    if let Some(ctx) = finalize {
        fractal_core::finalize_block_hooks(state, &ctx)?;
    }
    debug_assert!(gas_used <= budget_sum);
    let sr = state_root(state)?;
    let tx_root = ordered_tx_root(&txs)?;
    let parent_qc_hash = hash_qc(&parent_qc)?;
    let header = BlockHeader {
        version: 1,
        chain_id,
        height,
        view,
        parent_hash,
        parent_qc_hash,
        proposer,
        timestamp_ms,
        state_root: sr,
        tx_root,
        gas_used,
        gas_limit,
        shard_id,
        extra: [0u8; 32],
    };
    Ok(Block {
        header,
        transactions: txs,
        eth_signed_raw,
        parent_qc,
        parent_qc_signer_indices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use fractal_core::{Account, NativeCall, State, Transaction, TxBody, VmKind};

    #[test]
    fn tx_root_deterministic() {
        let tx = Transaction {
            signer: [1u8; 20],
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let a = ordered_tx_root(std::slice::from_ref(&tx)).unwrap();
        let b = ordered_tx_root(std::slice::from_ref(&tx)).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn build_block_updates_state_root() {
        let mut st = State::default();
        let addr = [9u8; 20];
        st.accounts.insert(
            addr,
            Account {
                nonce: 0,
                balance: 1_000_000,
            },
        );
        let tx = Transaction {
            signer: addr,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let parent = [7u8; 32];
        let gq = crate::genesis_parent_qc();
        let block = execute_and_build_block(
            41,
            0,
            1,
            0,
            parent,
            gq,
            vec![],
            [0u8; 32],
            1_000,
            60_000_000,
            &mut st,
            vec![tx],
            eth_signed_raws_for_txs(1),
            None,
        )
        .unwrap();
        assert_eq!(block.header.height, 1);
        assert_ne!(block.header.state_root, [0u8; 32]);
    }
}
