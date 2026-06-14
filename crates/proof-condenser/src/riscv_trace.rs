//! Deterministic replay trace for the proof condenser's RISC-V-shaped public witness.
//!
//! This is not a CPU emulator yet. It is the concrete guest execution harness boundary: the
//! condenser replays the finalized block range into canonical begin/tx/end rows, validates header
//! links and transaction roots, and binds the resulting trace root into STWO public inputs.

use borsh::{BorshDeserialize, BorshSerialize};
use fractal_consensus::{Block, header_hash, ordered_tx_root};
use fractal_crypto::hash::keccak256;

use crate::CheckpointJob;

pub const TRACE_MAGIC: &[u8; 8] = b"FRACRV02";

#[derive(Debug, thiserror::Error)]
pub enum RiscvTraceError {
    #[error("empty block range")]
    EmptyRange,
    #[error("non-contiguous block range: got {got}, expected {expected}")]
    NonContiguousRange { got: u64, expected: u64 },
    #[error("parent hash mismatch at height {height}")]
    ParentHashMismatch { height: u64 },
    #[error("tx root mismatch at height {height}")]
    TxRootMismatch { height: u64 },
    #[error("trace encode/hash: {0}")]
    Io(#[from] std::io::Error),
    #[error("transaction gas: {0}")]
    Exec(#[from] fractal_core::ExecError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
#[borsh(use_discriminant = true)]
pub enum RiscvTraceOpV1 {
    BeginBlock = 1,
    ApplyTx = 2,
    EndBlock = 3,
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RiscvTraceStepV1 {
    pub op: RiscvTraceOpV1,
    pub block_height: u64,
    pub tx_index: u32,
    pub gas: u64,
    pub accumulator: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct RiscvExecutionTraceV1 {
    pub magic: [u8; 8],
    pub chain_id: u64,
    pub start_block: u64,
    pub end_block: u64,
    pub first_parent_hash: [u8; 32],
    pub final_header_hash: [u8; 32],
    pub final_state_root: [u8; 32],
    pub aggregate_tx_root: [u8; 32],
    pub total_gas: u64,
    pub steps: Vec<RiscvTraceStepV1>,
}

impl RiscvExecutionTraceV1 {
    #[must_use]
    pub fn step_count(&self) -> u64 {
        self.steps.len() as u64
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, std::io::Error> {
        borsh::to_vec(self)
    }

    pub fn trace_root(&self) -> Result<[u8; 32], std::io::Error> {
        Ok(*blake3::hash(&self.to_bytes()?).as_bytes())
    }
}

/// Backwards-compatible export name for downstream callers while the docs migrate.
pub type RiscvGuestTraceStub = RiscvExecutionTraceV1;

fn tx_hash(tx: &fractal_core::Transaction) -> Result<[u8; 32], std::io::Error> {
    Ok(keccak256(&borsh::to_vec(tx)?))
}

fn step_accumulator(
    prior: &[u8; 32],
    op: RiscvTraceOpV1,
    height: u64,
    tx_index: u32,
    payload: &[u8],
) -> [u8; 32] {
    let mut buf = Vec::with_capacity(32 + 1 + 8 + 4 + payload.len());
    buf.extend_from_slice(prior);
    buf.push(op as u8);
    buf.extend_from_slice(&height.to_le_bytes());
    buf.extend_from_slice(&tx_index.to_le_bytes());
    buf.extend_from_slice(payload);
    *blake3::hash(&buf).as_bytes()
}

pub fn riscv_trace_from_blocks(
    chain_id: u64,
    blocks: &[Block],
) -> Result<RiscvExecutionTraceV1, RiscvTraceError> {
    let Some(first) = blocks.first() else {
        return Err(RiscvTraceError::EmptyRange);
    };
    let mut magic = [0u8; 8];
    magic.copy_from_slice(TRACE_MAGIC);
    let mut expected_height = first.header.height;
    let mut previous_header_hash: Option<[u8; 32]> = None;
    let mut total_gas = 0u64;
    let mut tx_roots = Vec::with_capacity(blocks.len() * 32);
    let mut steps = Vec::new();
    let mut acc = [0u8; 32];

    for block in blocks {
        if block.header.height != expected_height {
            return Err(RiscvTraceError::NonContiguousRange {
                got: block.header.height,
                expected: expected_height,
            });
        }
        if let Some(parent_hash) = previous_header_hash {
            if block.header.parent_hash != parent_hash {
                return Err(RiscvTraceError::ParentHashMismatch {
                    height: block.header.height,
                });
            }
        }
        let computed_tx_root = ordered_tx_root(&block.transactions)?;
        if computed_tx_root != block.header.tx_root {
            return Err(RiscvTraceError::TxRootMismatch {
                height: block.header.height,
            });
        }

        let header_hash = header_hash(&block.header)?;
        acc = step_accumulator(
            &acc,
            RiscvTraceOpV1::BeginBlock,
            block.header.height,
            0,
            &header_hash,
        );
        steps.push(RiscvTraceStepV1 {
            op: RiscvTraceOpV1::BeginBlock,
            block_height: block.header.height,
            tx_index: 0,
            gas: 0,
            accumulator: acc,
        });

        let mut replayed_gas = 0u64;
        for (tx_index, tx) in block.transactions.iter().enumerate() {
            let gas = fractal_core::tx_gas_limit(tx)?;
            replayed_gas = replayed_gas.saturating_add(gas);
            let tx_hash = tx_hash(tx)?;
            acc = step_accumulator(
                &acc,
                RiscvTraceOpV1::ApplyTx,
                block.header.height,
                tx_index as u32,
                &tx_hash,
            );
            steps.push(RiscvTraceStepV1 {
                op: RiscvTraceOpV1::ApplyTx,
                block_height: block.header.height,
                tx_index: tx_index as u32,
                gas,
                accumulator: acc,
            });
        }

        acc = step_accumulator(
            &acc,
            RiscvTraceOpV1::EndBlock,
            block.header.height,
            block.transactions.len() as u32,
            &block.header.state_root,
        );
        steps.push(RiscvTraceStepV1 {
            op: RiscvTraceOpV1::EndBlock,
            block_height: block.header.height,
            tx_index: block.transactions.len() as u32,
            gas: replayed_gas,
            accumulator: acc,
        });

        total_gas = total_gas.saturating_add(block.header.gas_used);
        tx_roots.extend_from_slice(&block.header.tx_root);
        previous_header_hash = Some(header_hash);
        expected_height = expected_height.saturating_add(1);
    }

    let last = blocks.last().expect("nonempty checked");
    Ok(RiscvExecutionTraceV1 {
        magic,
        chain_id,
        start_block: first.header.height,
        end_block: last.header.height,
        first_parent_hash: first.header.parent_hash,
        final_header_hash: previous_header_hash.expect("set in loop"),
        final_state_root: last.header.state_root,
        aggregate_tx_root: *blake3::hash(&tx_roots).as_bytes(),
        total_gas,
        steps,
    })
}

pub fn trace_from_checkpoint_job(job: &CheckpointJob) -> RiscvExecutionTraceV1 {
    let mut magic = [0u8; 8];
    magic.copy_from_slice(TRACE_MAGIC);
    let seed = borsh::to_vec(job).unwrap_or_default();
    let acc = step_accumulator(&[0u8; 32], RiscvTraceOpV1::EndBlock, job.height, 0, &seed);
    RiscvExecutionTraceV1 {
        magic,
        chain_id: job.chain_id,
        start_block: job.start_block,
        end_block: job.end_block,
        first_parent_hash: job.parent_hash,
        final_header_hash: job.header_hash,
        final_state_root: job.state_root,
        aggregate_tx_root: job.tx_root,
        total_gas: job.gas_used,
        steps: vec![RiscvTraceStepV1 {
            op: RiscvTraceOpV1::EndBlock,
            block_height: job.height,
            tx_index: 0,
            gas: job.gas_used,
            accumulator: acc,
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fractal_consensus::{BlockHeader, genesis_parent_qc, ordered_tx_root};
    use fractal_core::{HARDHAT_DEFAULT_SIGNER_0, NativeCall, Transaction, TxBody, VmKind};

    fn tx(nonce: u64) -> Transaction {
        Transaction {
            signer: HARDHAT_DEFAULT_SIGNER_0,
            nonce,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        }
    }

    fn block_with_txs(height: u64, transactions: Vec<Transaction>) -> Block {
        let tx_root = ordered_tx_root(&transactions).expect("tx root");
        let tx_count = transactions.len();
        Block {
            header: BlockHeader {
                version: 1,
                chain_id: 41,
                height,
                view: 0,
                parent_hash: [1u8; 32],
                parent_qc_hash: [2u8; 32],
                proposer: [3u8; 32],
                timestamp_ms: 0,
                parent_state_root: [0u8; 32],
                state_root: [4u8; 32],
                tx_root,
                gas_used: 100,
                gas_limit: 1_000_000,
                shard_id: 0,
                extra: [5u8; 32],
            },
            transactions,
            eth_signed_raw: vec![None; tx_count],
            parent_qc: genesis_parent_qc(),
            parent_qc_signer_indices: vec![],
        }
    }

    #[test]
    fn replay_trace_has_begin_tx_end_rows_and_root_changes() {
        let b1 = block_with_txs(1, vec![tx(0)]);
        let mut b2 = block_with_txs(1, vec![tx(1)]);
        b2.header.gas_used = b1.header.gas_used;
        let t1 = riscv_trace_from_blocks(41, &[b1]).expect("trace");
        let t2 = riscv_trace_from_blocks(41, &[b2]).expect("trace");

        assert_eq!(t1.steps.len(), 3);
        assert_eq!(t1.steps[0].op, RiscvTraceOpV1::BeginBlock);
        assert_eq!(t1.steps[1].op, RiscvTraceOpV1::ApplyTx);
        assert_eq!(t1.steps[2].op, RiscvTraceOpV1::EndBlock);
        assert_ne!(t1.trace_root().unwrap(), t2.trace_root().unwrap());
    }

    #[test]
    fn replay_trace_rejects_bad_tx_root() {
        let mut b = block_with_txs(1, vec![tx(0)]);
        b.header.tx_root = [9u8; 32];
        assert!(matches!(
            riscv_trace_from_blocks(41, &[b]),
            Err(RiscvTraceError::TxRootMismatch { height: 1 })
        ));
    }
}
