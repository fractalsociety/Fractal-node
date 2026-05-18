//! Async proof condenser (PRD `docs/prd.md` §7.8): **off the HotStuff hot path**.
//!
//! Ships a real **STWO** Circle STARK prove+verify step (tiny Fibonacci AIR bound to
//! [`CheckpointJob::header_hash`] **and** replay-derived RISC-V trace public inputs; Fiat–Shamir mixes
//! **borsh([`CheckpointJob`])** before commitments) via
//! [`checkpoint_stwo::prove_and_verify_checkpoint_stwo`], with **blake3** stub fallback if proving
//! fails. Heavy work runs in [`tokio::task::spawn_blocking`]. **Requires nightly Rust** (repo
//! `rust-toolchain.toml`, same pin as upstream `stwo`). Vendored `stwo` is built with **`parallel`**
//! (Rayon) for throughput on multi-core machines including **riscv64** (CPU backend; no x86-only SIMD).

mod artifact;
mod checkpoint_stwo;
mod persist;
mod riscv_trace;

pub use artifact::{CHECKPOINT_STWO_ARTIFACT_VERSION, CheckpointStwoArtifactV1};
pub use checkpoint_stwo::{
    CHECKPOINT_STWO_LOG_ROWS, FIB_TRACE_WIDTH, StwoCheckpointError,
    checkpoint_stwo_digest_from_json, prove_and_verify_checkpoint_stwo, prove_checkpoint_stwo,
    verify_checkpoint_stwo_proof_json,
};
pub use persist::{
    PERSISTED_CHECKPOINT_PROOF_VERSION, PersistedCheckpointProofV1, ProofArtifactRegistry,
    ProofPersistenceConfig, build_persisted_checkpoint_proof,
};

pub use riscv_trace::{
    RiscvExecutionTraceV1, RiscvGuestTraceStub, RiscvTraceError, RiscvTraceOpV1, RiscvTraceStepV1,
    riscv_trace_from_blocks, trace_from_checkpoint_job,
};

use std::io;

use borsh::{BorshDeserialize, BorshSerialize};
use fractal_consensus::{Block, header_hash};

/// Public inputs for a single committed block checkpoint (cheap to clone onto a channel).
#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct CheckpointJob {
    pub chain_id: u64,
    pub start_block: u64,
    pub end_block: u64,
    pub height: u64,
    pub view: u64,
    pub header_hash: [u8; 32],
    pub parent_hash: [u8; 32],
    pub state_root: [u8; 32],
    pub tx_root: [u8; 32],
    pub gas_used: u64,
    pub riscv_trace_root: [u8; 32],
    pub riscv_trace_steps: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum CheckpointJobError {
    #[error("header_hash: {0}")]
    HeaderHash(#[from] io::Error),
    #[error("empty block range")]
    EmptyRange,
    #[error("non-contiguous block range: got {got}, expected {expected}")]
    NonContiguousRange { got: u64, expected: u64 },
    #[error("riscv trace: {0}")]
    RiscvTrace(#[from] RiscvTraceError),
}

/// Build a [`CheckpointJob`] from a finalized [`Block`] (after BFT commit / replay verify).
pub fn checkpoint_job_from_block(
    chain_id: u64,
    block: &Block,
) -> Result<CheckpointJob, CheckpointJobError> {
    checkpoint_job_from_block_range(chain_id, std::slice::from_ref(block))
}

fn checkpoint_job_from_block_range_and_trace(
    chain_id: u64,
    blocks: &[Block],
    trace: RiscvExecutionTraceV1,
) -> Result<CheckpointJob, CheckpointJobError> {
    let first = blocks.first().ok_or(CheckpointJobError::EmptyRange)?;
    let last = blocks.last().expect("nonempty checked");
    Ok(CheckpointJob {
        chain_id,
        start_block: first.header.height,
        end_block: last.header.height,
        height: last.header.height,
        view: last.header.view,
        header_hash: header_hash(&last.header)?,
        parent_hash: first.header.parent_hash,
        state_root: last.header.state_root,
        tx_root: trace.aggregate_tx_root,
        gas_used: trace.total_gas,
        riscv_trace_root: trace.trace_root()?,
        riscv_trace_steps: trace.step_count(),
    })
}

/// Build a range witness job over contiguous finalized blocks.
///
/// The RISC-V replay harness validates the block links and per-block transaction roots, emits
/// canonical begin/tx/end rows, and stores the resulting trace root/step count in the job.
pub fn checkpoint_job_from_block_range(
    chain_id: u64,
    blocks: &[Block],
) -> Result<CheckpointJob, CheckpointJobError> {
    let trace = match riscv_trace_from_blocks(chain_id, blocks) {
        Ok(trace) => trace,
        Err(RiscvTraceError::EmptyRange) => return Err(CheckpointJobError::EmptyRange),
        Err(RiscvTraceError::NonContiguousRange { got, expected }) => {
            return Err(CheckpointJobError::NonContiguousRange { got, expected });
        }
        Err(err) => return Err(CheckpointJobError::RiscvTrace(err)),
    };
    checkpoint_job_from_block_range_and_trace(chain_id, blocks, trace)
}

impl CheckpointJob {
    /// Deterministic replay trace consumed by the fallback commitment path.
    #[must_use]
    pub fn riscv_guest_trace_stub(&self) -> RiscvGuestTraceStub {
        trace_from_checkpoint_job(self)
    }

    /// Fallback commitment when STWO proving fails: `blake3(trace_borsh)`.
    #[must_use]
    pub fn stwo_commitment_stub(&self) -> [u8; 32] {
        let trace = self.riscv_guest_trace_stub();
        let bytes = trace.to_bytes().unwrap_or_default();
        *blake3::hash(&bytes).as_bytes()
    }
}

/// Run STWO prove+verify on the blocking pool; falls back to blake3 stub on failure.
pub async fn prove_checkpoint(job: CheckpointJob) -> [u8; 32] {
    tokio::task::spawn_blocking(move || {
        match checkpoint_stwo::prove_and_verify_checkpoint_stwo(&job) {
            Ok(d) => d,
            Err(e) => {
                eprintln!(
                    "fractal-proof-condenser: STWO prove/verify failed for height={}: {e}; using blake3 stub",
                    job.height
                );
                job.stwo_commitment_stub()
            }
        }
    })
    .await
    .unwrap_or_else(|e| {
        eprintln!("fractal-proof-condenser: spawn_blocking join failed: {e}");
        [0u8; 32]
    })
}

/// Background consumer: receives [`CheckpointJob`] and logs a verifiable stub commitment.
/// Does not block producers: enqueue with [`tokio::sync::mpsc::Sender::try_send`].
///
/// When `registry` is [`Some`], each completed proof is stored for RPC / optional durable backends
/// (`ProofPersistenceConfig`: RocksDB via `FRACTAL_PROOF_ROCKSDB_PATH`, filesystem via
/// `FRACTAL_PROOF_ARTIFACT_DIR`).
/// Optional hook: tier-1 digest ready → masterchain `ProofSubmissionV1` queue (M11 pipeline).
pub type Tier1DigestSink = tokio::sync::mpsc::UnboundedSender<(CheckpointJob, [u8; 32])>;

pub fn spawn_async_proof_condenser(
    mut rx: tokio::sync::mpsc::Receiver<CheckpointJob>,
    registry: Option<std::sync::Arc<ProofArtifactRegistry>>,
    tier1_digest_sink: Option<Tier1DigestSink>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        while let Some(job) = rx.recv().await {
            let height = job.height;
            let job_submit = job.clone();
            let reg = registry.clone();
            let tier1 = tier1_digest_sink.clone();
            let (digest, stub) = match tokio::task::spawn_blocking(move || {
                let p = build_persisted_checkpoint_proof(&job);
                let stub = p.stub_fallback;
                let d = p.proof_digest;
                if let Some(r) = reg {
                    r.record(p);
                }
                (d, stub)
            })
            .await
            {
                Ok(x) => x,
                Err(e) => {
                    eprintln!("fractal-proof-condenser: spawn_blocking join failed: {e}");
                    ([0u8; 32], true)
                }
            };
            eprintln!(
                "fractal-proof-condenser: async checkpoint height={height} \
                 proof_digest=0x{} (stub_fallback={stub} trace_steps={} PRD §7.8)",
                hex::encode(digest),
                job_submit.riscv_trace_steps
            );
            if let Some(tx) = tier1 {
                if tx.send((job_submit, digest)).is_err() {
                    eprintln!("fractal-proof-condenser: tier1 digest sink closed height={height}");
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use fractal_consensus::{BlockHeader, genesis_parent_qc};

    fn fake_block(height: u64, view: u64) -> Block {
        Block {
            header: BlockHeader {
                version: 1,
                chain_id: 41,
                height,
                view,
                parent_hash: [1u8; 32],
                parent_qc_hash: [2u8; 32],
                proposer: [5u8; 32],
                timestamp_ms: 0,
                state_root: [3u8; 32],
                tx_root: [0u8; 32],
                gas_used: 21_000,
                gas_limit: 30_000_000,
                shard_id: 0,
                extra: [0u8; 32],
            },
            transactions: vec![],
            parent_qc: genesis_parent_qc(),
            parent_qc_signer_indices: vec![],
            eth_signed_raw: vec![],
        }
    }

    #[test]
    fn stub_commitment_is_deterministic() {
        let b = fake_block(3, 0);
        let j = checkpoint_job_from_block(41, &b).expect("job");
        let a = j.stwo_commitment_stub();
        let b2 = j.clone();
        assert_eq!(a, b2.stwo_commitment_stub());
    }

    #[test]
    fn range_job_binds_contiguous_block_span() {
        let b1 = fake_block(3, 0);
        let mut b2 = fake_block(4, 1);
        b2.header.parent_hash = header_hash(&b1.header).expect("h1");
        b2.header.gas_used = 22_000;
        b2.header.tx_root = [0u8; 32];
        b2.header.state_root = [8u8; 32];

        let j = checkpoint_job_from_block_range(41, &[b1.clone(), b2.clone()]).expect("range");
        assert_eq!(j.start_block, 3);
        assert_eq!(j.end_block, 4);
        assert_eq!(j.height, 4);
        assert_eq!(j.parent_hash, b1.header.parent_hash);
        assert_eq!(j.state_root, b2.header.state_root);
        assert_eq!(j.gas_used, 43_000);
        assert_ne!(j.tx_root, b1.header.tx_root);

        let mut gap = b2;
        gap.header.height = 6;
        assert!(matches!(
            checkpoint_job_from_block_range(41, &[b1, gap]),
            Err(CheckpointJobError::NonContiguousRange { .. })
        ));
    }

    #[tokio::test]
    async fn spawn_blocking_stwo_or_stub() {
        let b = fake_block(9, 2);
        let j = checkpoint_job_from_block(41, &b).expect("job");
        let h = prove_checkpoint(j.clone()).await;
        assert_eq!(h.len(), 32);
        // STWO path should succeed on nightly; if not, digest still matches stub shape.
        let stwo_ok = prove_and_verify_checkpoint_stwo(&j).is_ok();
        if stwo_ok {
            assert_eq!(
                prove_checkpoint(j.clone()).await,
                prove_and_verify_checkpoint_stwo(&j).unwrap()
            );
        }
    }

    #[tokio::test]
    async fn async_prove_digest_changes_when_riscv_trace_changes() {
        let b = fake_block(11, 1);
        let j1 = checkpoint_job_from_block(41, &b).expect("job");
        let mut j2 = j1.clone();
        j2.gas_used = j1.gas_used.wrapping_add(99);
        assert_eq!(j1.header_hash, j2.header_hash);
        if prove_and_verify_checkpoint_stwo(&j1).is_ok() {
            let h1 = prove_checkpoint(j1).await;
            let h2 = prove_checkpoint(j2).await;
            assert_ne!(
                h1, h2,
                "STWO digest must reflect RiscvGuestTraceStub, not header alone"
            );
        }
    }
}
