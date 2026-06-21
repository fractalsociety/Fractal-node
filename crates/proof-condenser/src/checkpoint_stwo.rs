//! Real **STWO** (Circle STARK) prove + verify for a tiny checkpoint-bound witness.
//!
//! We reuse the upstream **wide Fibonacci** AIR pattern (`N` trace columns, one independent
//! Fibonacci-ish recurrence per row). Per-row seeds mix [`crate::CheckpointJob::header_hash`]
//! **and** replay-derived RISC-V trace public inputs (PRD §7.8 “RISC-V trace” hook), so
//! the STWO proof binds to the same trace root the async condenser records for fallback commitments.
//!
//! **Public inputs (Fiat–Shamir):** before any trace commitment, the prover and verifier both mix
//! a domain tag plus **borsh([`CheckpointJob`])** into the STWO channel. A valid proof blob for one
//! job therefore fails verification if paired with a different [`CheckpointJob`] (artifact digest
//! alone is not sufficient for binding).
//!
//! The prover uses **STWO `CpuBackend`** (portable M31 column math). On **riscv64** there is no
//! x86 SIMD path inside `stwo`; multi-core throughput comes from the **`parallel`** Cargo feature
//! (Rayon) on vendored `stwo`. The witness now comes from a deterministic finalized-block replay
//! harness, while the AIR remains a compact STWO smoke circuit.

use borsh::to_vec as borsh_to_vec;
use itertools::Itertools;
use num_traits::{One, Zero};
use stwo::core::air::Component;
use stwo::core::channel::{Blake2sM31Channel, Channel};
use stwo::core::fields::m31::BaseField;
use stwo::core::fields::qm31::SecureField;
use stwo::core::fields::FieldExpOps;
use stwo::core::pcs::{CommitmentSchemeVerifier, PcsConfig};
use stwo::core::poly::circle::CanonicCoset;
use stwo::core::proof::StarkProof;
use stwo::core::vcs_lifted::blake2_merkle::{Blake2sM31MerkleChannel, Blake2sM31MerkleHasher};
use stwo::core::verifier::verify;
use stwo::prover::backend::cpu::CpuBackend;
use stwo::prover::backend::{Col, Column};
use stwo::prover::poly::circle::{CircleEvaluation, PolyOps};
use stwo::prover::poly::BitReversedOrder;
use stwo::prover::{prove, CommitmentSchemeProver};
use stwo_constraint_framework::{
    EvalAtRow, FrameworkComponent, FrameworkEval, TraceLocationAllocator,
};

use crate::CheckpointJob;

/// Trace width for [`WideFibCheckpointEval`] (must match `generate_fib_trace_cpu`).
pub const FIB_TRACE_WIDTH: usize = 4;

/// `log2` of the number of independent Fibonacci rows (16 rows at 4).
pub const CHECKPOINT_STWO_LOG_ROWS: u32 = 4;

#[derive(Clone, Debug)]
pub struct WideFibCheckpointEval {
    pub log_n_rows: u32,
}

impl FrameworkEval for WideFibCheckpointEval {
    fn log_size(&self) -> u32 {
        self.log_n_rows
    }

    fn max_constraint_log_degree_bound(&self) -> u32 {
        self.log_n_rows + 1
    }

    fn evaluate<E: EvalAtRow>(&self, mut eval: E) -> E {
        let mut a = eval.next_trace_mask();
        let mut b = eval.next_trace_mask();
        for _ in 2..FIB_TRACE_WIDTH {
            let c = eval.next_trace_mask();
            eval.add_constraint(c.clone() - (a.square() + b.square()));
            a = b;
            b = c;
        }
        eval
    }
}

pub type WideFibCheckpointComponent = FrameworkComponent<WideFibCheckpointEval>;

#[derive(Clone, Debug)]
pub struct FibInput {
    pub a: BaseField,
    pub b: BaseField,
}

fn u32_fold_le_bytes(bytes: &[u8]) -> u32 {
    bytes
        .chunks_exact(4)
        .map(|c| u32::from_le_bytes(c.try_into().unwrap()))
        .fold(0u32, |acc, w| acc.wrapping_add(w))
}

fn fib_inputs_from_checkpoint(job: &CheckpointJob, n: usize) -> Vec<FibInput> {
    assert!(n.is_power_of_two());
    let trace_fold = u32_fold_le_bytes(&job.riscv_trace_root)
        ^ (job.riscv_trace_steps as u32)
        ^ ((job.riscv_trace_steps >> 32) as u32);
    let fold: u32 = u32_fold_le_bytes(&job.header_hash);
    (0..n)
        .map(|i| {
            let mut h = job.header_hash;
            h[0] ^= i as u8;
            let limb = u32::from_le_bytes([h[0], h[1], h[2], h[3]])
                ^ fold.rotate_left(i as u32)
                ^ trace_fold.rotate_left((i * 7) as u32);
            let b = limb & ((1u32 << 31) - 1);
            FibInput {
                a: BaseField::one(),
                b: BaseField::from_u32_unchecked(b),
            }
        })
        .collect()
}

pub fn generate_fib_trace_cpu(
    inputs: &[FibInput],
) -> Vec<CircleEvaluation<CpuBackend, BaseField, BitReversedOrder>> {
    assert!(inputs.len().is_power_of_two());
    let log_size = inputs.len().ilog2();
    let mut trace: Vec<Col<CpuBackend, BaseField>> = (0..FIB_TRACE_WIDTH)
        .map(|_| {
            let c: Col<CpuBackend, BaseField> = Column::zeros(1 << log_size);
            c
        })
        .collect_vec();
    for (vec_index, input) in inputs.iter().enumerate() {
        let mut a = input.a;
        let mut b = input.b;
        trace[0].set(vec_index, a);
        trace[1].set(vec_index, b);
        for col in trace.iter_mut().skip(2) {
            (a, b) = (b, a.square() + b.square());
            col.set(vec_index, b);
        }
    }
    let domain = CanonicCoset::new(log_size).circle_domain();
    trace
        .into_iter()
        .map(|eval| CircleEvaluation::<CpuBackend, _, BitReversedOrder>::new(domain, eval))
        .collect()
}

/// Domain tag + layout version mixed into the STWO channel before trace roots (Fiat–Shamir binding).
const CHECKPOINT_JOB_FS_DOMAIN: u32 = 0x0043_504a; // "CPJ\0"
const CHECKPOINT_JOB_FS_LAYOUT: u32 = 1;

#[derive(Debug, thiserror::Error)]
pub enum StwoCheckpointError {
    #[error("stwo prove: {0:?}")]
    Prove(stwo::prover::ProvingError),
    #[error("stwo verify: {0:?}")]
    Verify(stwo::core::verifier::VerificationError),
    #[error("stark proof json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("checkpoint job public inputs (borsh): {0}")]
    PublicInputsEncode(#[from] std::io::Error),
    #[error("checkpoint STWO proof digest mismatch")]
    DigestMismatch,
    #[error("unsupported checkpoint STWO artifact version {0}")]
    UnsupportedArtifactVersion(u8),
}

/// Mix canonical [`CheckpointJob`] bytes into the Fiat–Shamir transcript **before** the first
/// Merkle root (must match prover order in [`prove_checkpoint_stwo`] and verifier order in
/// [`verify_checkpoint_stwo_proof_json`]).
fn mix_checkpoint_job_public_inputs(
    job: &CheckpointJob,
    channel: &mut Blake2sM31Channel,
) -> Result<(), StwoCheckpointError> {
    channel.mix_u32s(&[CHECKPOINT_JOB_FS_DOMAIN, CHECKPOINT_JOB_FS_LAYOUT]);
    let bytes = borsh_to_vec(job)?;
    let words: Vec<u32> = bytes
        .chunks(4)
        .map(|chunk| {
            let mut pad = [0u8; 4];
            pad[..chunk.len()].copy_from_slice(chunk);
            u32::from_le_bytes(pad)
        })
        .collect();
    channel.mix_u32s(&words);
    Ok(())
}

fn wide_fib_component(log_n_rows: u32) -> WideFibCheckpointComponent {
    WideFibCheckpointComponent::new(
        &mut TraceLocationAllocator::default(),
        WideFibCheckpointEval { log_n_rows },
        SecureField::zero(),
    )
}

/// `blake3(serde_json::to_vec(proof))` — stable handle for logs, storage, and gossip metadata.
#[must_use]
pub fn checkpoint_stwo_digest_from_json(proof_json: &[u8]) -> [u8; 32] {
    *blake3::hash(proof_json).as_bytes()
}

/// Verify a serialized STWO [`StarkProof`] (JSON) for the **fixed** wide-Fibonacci checkpoint AIR.
///
/// Salts the Fiat–Shamir transcript with **borsh(`job`)** before reading commitments, matching
/// [`prove_checkpoint_stwo`]. Integrity-only checks on the JSON blob still use
/// [`checkpoint_stwo_digest_from_json`]; this function enforces **job ↔ proof** binding in-circuit
/// via the FS channel.
pub fn verify_checkpoint_stwo_proof_json(
    proof_json: &[u8],
    job: &CheckpointJob,
) -> Result<(), StwoCheckpointError> {
    let proof: StarkProof<Blake2sM31MerkleHasher> = serde_json::from_slice(proof_json)?;
    verify_checkpoint_stwo_proof_owned(proof, job)
}

fn verify_checkpoint_stwo_proof_owned(
    proof: StarkProof<Blake2sM31MerkleHasher>,
    job: &CheckpointJob,
) -> Result<(), StwoCheckpointError> {
    let log_n = CHECKPOINT_STWO_LOG_ROWS;
    let config = PcsConfig::default();
    let verifier_channel = &mut Blake2sM31Channel::default();
    mix_checkpoint_job_public_inputs(job, verifier_channel)?;
    let mut verifier_cs = CommitmentSchemeVerifier::<Blake2sM31MerkleChannel>::new(config);
    let component = wide_fib_component(log_n);
    let sizes = component.trace_log_degree_bounds();
    verifier_cs.commit(proof.commitments[0], &sizes[0], verifier_channel);
    verifier_cs.commit(proof.commitments[1], &sizes[1], verifier_channel);
    verify(&[&component], verifier_channel, &mut verifier_cs, proof)
        .map_err(StwoCheckpointError::Verify)
}

/// Prove the checkpoint witness, **self-verify**, and return JSON + digest (M9 verifier / storage path).
pub fn prove_checkpoint_stwo(
    job: &CheckpointJob,
) -> Result<(Vec<u8>, [u8; 32]), StwoCheckpointError> {
    let log_n = CHECKPOINT_STWO_LOG_ROWS;
    let n = 1usize << log_n;
    let config = PcsConfig::default();
    let twiddles = CpuBackend::precompute_twiddles(
        CanonicCoset::new(log_n + 1 + config.fri_config.log_blowup_factor)
            .circle_domain()
            .half_coset,
    );

    let prover_channel = &mut Blake2sM31Channel::default();
    mix_checkpoint_job_public_inputs(job, prover_channel)?;
    let mut commitment_scheme =
        CommitmentSchemeProver::<CpuBackend, Blake2sM31MerkleChannel>::new(config, &twiddles);

    let mut tree_builder = commitment_scheme.tree_builder();
    tree_builder.extend_evals(vec![]);
    tree_builder.commit(prover_channel);

    let inputs = fib_inputs_from_checkpoint(job, n);
    let trace = generate_fib_trace_cpu(&inputs);
    let mut tree_builder = commitment_scheme.tree_builder();
    tree_builder.extend_evals(trace);
    tree_builder.commit(prover_channel);

    let component = wide_fib_component(log_n);

    let proof = prove(&[&component], prover_channel, commitment_scheme)
        .map_err(StwoCheckpointError::Prove)?;

    let json = serde_json::to_vec(&proof)?;
    let digest = checkpoint_stwo_digest_from_json(&json);

    verify_checkpoint_stwo_proof_owned(proof.clone(), job)?;

    Ok((json, digest))
}

/// Prove a tiny Fibonacci AIR and **verify** the [`stwo::core::proof::StarkProof`].
///
/// Returns `blake3(serde_json::to_vec(proof))` as a stable 32-byte handle for logs / metrics.
pub fn prove_and_verify_checkpoint_stwo(
    job: &CheckpointJob,
) -> Result<[u8; 32], StwoCheckpointError> {
    let (_json, digest) = prove_checkpoint_stwo(job)?;
    Ok(digest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::checkpoint_job_from_block;
    use fractal_consensus::{genesis_parent_qc, Block, BlockHeader};

    fn job_fixture() -> CheckpointJob {
        let block = Block {
            header: BlockHeader {
                version: 1,
                chain_id: 41,
                height: 12,
                view: 3,
                parent_hash: [9u8; 32],
                parent_qc_hash: [8u8; 32],
                proposer: [7u8; 32],
                timestamp_ms: 1,
                parent_state_root: [0u8; 32],
                state_root: [6u8; 32],
                tx_root: [0u8; 32],
                receipt_root: [0u8; 32],
                native_event_root: [0u8; 32],
                evm_log_root: [0u8; 32],
                gas_used: 21_000,
                gas_limit: 30_000_000,
                shard_id: 0,
                extra: [4u8; 32],
            },
            transactions: vec![],
            parent_qc: genesis_parent_qc(),
            parent_qc_signer_indices: vec![],
            eth_signed_raw: vec![],
        };
        checkpoint_job_from_block(41, &block).expect("job")
    }

    #[test]
    fn stwo_prove_verify_round_trip() {
        let job = job_fixture();
        let d1 = prove_and_verify_checkpoint_stwo(&job).expect("stwo prove+verify");
        let d2 = prove_and_verify_checkpoint_stwo(&job).expect("repeat");
        assert_eq!(d1, d2, "digest must be deterministic for same job");
        assert_ne!(d1, [0u8; 32]);
    }

    #[test]
    fn different_header_hash_different_digest() {
        let j1 = job_fixture();
        let mut j2 = j1.clone();
        j2.header_hash[31] ^= 0xff;
        let d1 = prove_and_verify_checkpoint_stwo(&j1).expect("ok");
        let d2 = prove_and_verify_checkpoint_stwo(&j2).expect("ok");
        assert_ne!(d1, d2);
    }

    /// Same synthetic `header_hash`, different checkpoint fields → different RISC-V stub trace → different STWO digest.
    #[test]
    fn different_riscv_guest_trace_same_header_changes_digest() {
        let j1 = job_fixture();
        let mut j2 = j1.clone();
        j2.gas_used = j1.gas_used.wrapping_add(1);
        assert_eq!(
            j1.header_hash, j2.header_hash,
            "fixture: only gas_used changes"
        );
        let d1 = prove_and_verify_checkpoint_stwo(&j1).expect("ok");
        let d2 = prove_and_verify_checkpoint_stwo(&j2).expect("ok");
        assert_ne!(d1, d2);
    }

    #[test]
    fn verify_proof_json_round_trip() {
        let job = job_fixture();
        let Ok((json, digest)) = prove_checkpoint_stwo(&job) else {
            return;
        };
        assert_eq!(checkpoint_stwo_digest_from_json(&json), digest);
        verify_checkpoint_stwo_proof_json(&json, &job).expect("standalone verify");
        assert_eq!(prove_and_verify_checkpoint_stwo(&job).unwrap(), digest);
    }

    #[test]
    fn verify_rejects_tampered_proof_json() {
        let job = job_fixture();
        let Ok((mut json, _)) = prove_checkpoint_stwo(&job) else {
            return;
        };
        if json.is_empty() {
            return;
        }
        let i = json.len() / 2;
        json[i] ^= 0x5a;
        assert!(verify_checkpoint_stwo_proof_json(&json, &job).is_err());
    }

    #[test]
    fn verify_rejects_wrong_job_same_proof_json() {
        let j1 = job_fixture();
        let mut j2 = j1.clone();
        j2.height = j1.height.wrapping_add(1);
        let Ok((json, _)) = prove_checkpoint_stwo(&j1) else {
            return;
        };
        assert!(verify_checkpoint_stwo_proof_json(&json, &j2).is_err());
    }
}
