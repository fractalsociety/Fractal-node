//! HotStuff-2–oriented block types for **singleton** (`n = 1`, `f = 0`) production (`docs/prd.md` §7.3, §18 M2).
//!
//! Full vote aggregation / libp2p gossip lands in later milestones; this crate freezes the
//! on-disk / wire shape and deterministic header hashing for the execution pipeline.
//!
//! [`qc`] defines quorum certificate hashing and the Phase-1 singleton `parent_qc_hash` chain
//! (`docs/prd.md` §18 M7-a).
//!
//! [`validators`] holds static validator sets and view-based leader ids (`docs/prd.md` §18 M7-b).

use borsh::{BorshDeserialize, BorshSerialize};
use fractal_core::{state_root, ExecError, State, Transaction};
use fractal_crypto::hash::{keccak256, Hash256};
use thiserror::Error;

pub mod qc;
pub mod validators;

pub use fractal_core::Transaction as Tx;
pub use qc::{
    expected_parent_qc_for_parent_header, genesis_parent_qc, hash_qc, next_parent_qc_hash_after_commit,
    singleton_qc_certifying, QuorumCertificate,
};
pub use validators::{ValidatorId, ValidatorSet};

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
    pub extra: [u8; 32],
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Block {
    pub header: BlockHeader,
    pub transactions: Vec<Transaction>,
    /// Parallel to `transactions`: optional original EIP-1559 bytes (`keccak256` = RPC tx hash).
    /// Followers replay this to populate `NodeInner::eth_signed_raw` / hash maps like the producer.
    pub eth_signed_raw: Vec<Option<Vec<u8>>>,
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
/// Caller supplies `parent_qc_hash` (see [`crate::qc`]).
pub fn execute_and_build_block(
    chain_id: u64,
    height: u64,
    view: u64,
    parent_hash: Hash256,
    parent_qc_hash: Hash256,
    proposer: [u8; 32],
    timestamp_ms: u64,
    gas_limit: u64,
    state: &mut State,
    txs: Vec<Transaction>,
    eth_signed_raw: Vec<Option<Vec<u8>>>,
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
    debug_assert!(gas_used <= budget_sum);
    let sr = state_root(state)?;
    let tx_root = ordered_tx_root(&txs)?;
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
        extra: [0u8; 32],
    };
    Ok(Block {
        header,
        transactions: txs,
        eth_signed_raw,
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
        let block = execute_and_build_block(
            41, 1, 0, parent, [0u8; 32], [0u8; 32], 1_000, 60_000_000, &mut st, vec![tx], eth_signed_raws_for_txs(1),
        )
        .unwrap();
        assert_eq!(block.header.height, 1);
        assert_ne!(block.header.state_root, [0u8; 32]);
    }
}
