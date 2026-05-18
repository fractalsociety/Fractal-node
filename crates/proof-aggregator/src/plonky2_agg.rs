//! Production Plonky2 prove / verify for global masterchain aggregation.

use std::sync::OnceLock;

use anyhow::{Context, anyhow};
use fractal_crypto::hash::Hash256;
use fractal_shard::ProofSubmissionV1;
use plonky2::field::goldilocks_field::GoldilocksField;
use plonky2::field::types::{Field, PrimeField64};
use plonky2::hash::hash_types::HashOut;
use plonky2::iop::target::Target;
use plonky2::iop::witness::{PartialWitness, WitnessWrite};
use plonky2::plonk::circuit_builder::CircuitBuilder;
use plonky2::plonk::circuit_data::{CircuitConfig, CircuitData};
use plonky2::plonk::config::{GenericConfig, Hasher, PoseidonGoldilocksConfig};
use plonky2::plonk::proof::ProofWithPublicInputs;

use crate::aggregate::PLONKY2_AGGREGATOR_VERSION;
use crate::statement::{
    MAX_AGG_PROOFS, VerifiedStwoStatementV1, encode_statement_u64, encode_verified_statement_u64,
    statement_field_len, verified_statement_field_len,
};

const D: usize = 2;
type C = PoseidonGoldilocksConfig;
type F = GoldilocksField;

struct AggCircuit {
    data: CircuitData<F, C, D>,
    witness_targets: Vec<Target>,
}

static CIRCUIT: OnceLock<AggCircuit> = OnceLock::new();
static VERIFIED_CIRCUIT: OnceLock<AggCircuit> = OnceLock::new();

fn agg_circuit() -> &'static AggCircuit {
    CIRCUIT.get_or_init(build_circuit)
}

fn build_circuit() -> AggCircuit {
    build_circuit_with_len(statement_field_len())
}

fn verified_agg_circuit() -> &'static AggCircuit {
    VERIFIED_CIRCUIT.get_or_init(build_verified_circuit)
}

fn build_verified_circuit() -> AggCircuit {
    build_circuit_with_len(verified_statement_field_len())
}

fn build_circuit_with_len(n: usize) -> AggCircuit {
    let config = CircuitConfig::standard_recursion_config();
    let mut builder = CircuitBuilder::<F, D>::new(config);
    let witness_targets: Vec<_> = (0..n).map(|_| builder.add_virtual_target()).collect();
    let hash = builder
        .hash_n_to_hash_no_pad::<<C as GenericConfig<D>>::InnerHasher>(witness_targets.clone());
    builder.register_public_inputs(&hash.elements);
    let data = builder.build::<C>();
    AggCircuit {
        data,
        witness_targets,
    }
}

fn u64s_to_fields(values: &[u64]) -> Vec<F> {
    values.iter().map(|&x| F::from_canonical_u64(x)).collect()
}

/// Poseidon statement digest (off-circuit, matches in-circuit hash).
#[must_use]
pub fn poseidon_statement_digest(
    masterchain_height: u64,
    global_state_root: &Hash256,
    proofs: &[ProofSubmissionV1],
) -> HashOut<F> {
    let values = encode_statement_u64(
        PLONKY2_AGGREGATOR_VERSION,
        masterchain_height,
        global_state_root,
        proofs,
    );
    let fields = u64s_to_fields(&values);
    <C as GenericConfig<D>>::InnerHasher::hash_no_pad(&fields)
}

/// Poseidon digest for verified-STWO statements (off-circuit, matches in-circuit hash).
#[must_use]
pub fn poseidon_verified_statement_digest(
    masterchain_height: u64,
    global_state_root: &Hash256,
    proofs: &[ProofSubmissionV1],
    verified: &[VerifiedStwoStatementV1],
) -> HashOut<F> {
    let values = encode_verified_statement_u64(
        PLONKY2_AGGREGATOR_VERSION,
        masterchain_height,
        global_state_root,
        proofs,
        verified,
    );
    let fields = u64s_to_fields(&values);
    <C as GenericConfig<D>>::InnerHasher::hash_no_pad(&fields)
}

/// Map Plonky2 hash output to `globalZkRoot` bytes (four LE u64 limbs).
#[must_use]
pub fn hash_out_to_global_zk_root(h: &HashOut<F>) -> Hash256 {
    let mut out = [0u8; 32];
    for (i, elem) in h.elements.iter().enumerate() {
        let start = i * 8;
        out[start..start + 8].copy_from_slice(&elem.to_canonical_u64().to_le_bytes());
    }
    out
}

/// Prove tier-2 aggregation; returns `(global_zk_root, snark_bytes)`.
pub fn prove_global_aggregation(
    masterchain_height: u64,
    global_state_root: &Hash256,
    proofs: &[ProofSubmissionV1],
) -> anyhow::Result<(Hash256, Vec<u8>)> {
    if proofs.is_empty() {
        return Err(anyhow!("cannot prove empty proof set"));
    }
    if proofs.len() > MAX_AGG_PROOFS {
        return Err(anyhow!(
            "too many proofs: {} > {MAX_AGG_PROOFS}",
            proofs.len()
        ));
    }
    let circuit = agg_circuit();
    let values = encode_statement_u64(
        PLONKY2_AGGREGATOR_VERSION,
        masterchain_height,
        global_state_root,
        proofs,
    );
    let fields = u64s_to_fields(&values);
    let expected = <C as GenericConfig<D>>::InnerHasher::hash_no_pad(&fields);
    let mut pw = PartialWitness::new();
    for (target, field) in circuit.witness_targets.iter().zip(fields.iter()) {
        pw.set_target(*target, *field)?;
    }
    let proof = circuit.data.prove(pw)?;
    circuit.data.verify(proof.clone())?;
    let public_hash = HashOut {
        elements: [
            proof.public_inputs[0],
            proof.public_inputs[1],
            proof.public_inputs[2],
            proof.public_inputs[3],
        ],
    };
    if public_hash.elements != expected.elements {
        return Err(anyhow!("public hash mismatch after prove"));
    }
    let global_zk_root = hash_out_to_global_zk_root(&expected);
    Ok((global_zk_root, proof.to_bytes()))
}

/// Prove tier-2 aggregation over verified STWO public statements.
pub fn prove_global_aggregation_verified(
    masterchain_height: u64,
    global_state_root: &Hash256,
    proofs: &[ProofSubmissionV1],
    verified: &[VerifiedStwoStatementV1],
) -> anyhow::Result<(Hash256, Vec<u8>)> {
    if proofs.is_empty() {
        return Err(anyhow!("cannot prove empty proof set"));
    }
    if proofs.len() > MAX_AGG_PROOFS {
        return Err(anyhow!(
            "too many proofs: {} > {MAX_AGG_PROOFS}",
            proofs.len()
        ));
    }
    if proofs.len() != verified.len()
        || proofs
            .iter()
            .any(|p| !verified.iter().any(|s| s.matches_submission(p)))
    {
        return Err(anyhow!(
            "verified STWO statements do not match proof submissions"
        ));
    }
    let circuit = verified_agg_circuit();
    let values = encode_verified_statement_u64(
        PLONKY2_AGGREGATOR_VERSION,
        masterchain_height,
        global_state_root,
        proofs,
        verified,
    );
    let fields = u64s_to_fields(&values);
    let expected = <C as GenericConfig<D>>::InnerHasher::hash_no_pad(&fields);
    let mut pw = PartialWitness::new();
    for (target, field) in circuit.witness_targets.iter().zip(fields.iter()) {
        pw.set_target(*target, *field)?;
    }
    let proof = circuit.data.prove(pw)?;
    circuit.data.verify(proof.clone())?;
    let public_hash = HashOut {
        elements: [
            proof.public_inputs[0],
            proof.public_inputs[1],
            proof.public_inputs[2],
            proof.public_inputs[3],
        ],
    };
    if public_hash.elements != expected.elements {
        return Err(anyhow!("public hash mismatch after prove"));
    }
    Ok((hash_out_to_global_zk_root(&expected), proof.to_bytes()))
}

/// Verify a tier-2 SNARK against the masterchain statement.
pub fn verify_global_aggregation_snark(
    snark_bytes: &[u8],
    masterchain_height: u64,
    global_state_root: &Hash256,
    proofs: &[ProofSubmissionV1],
) -> anyhow::Result<Hash256> {
    if proofs.is_empty() {
        return Err(anyhow!("empty proof set"));
    }
    let circuit = agg_circuit();
    let expected_digest = poseidon_statement_digest(masterchain_height, global_state_root, proofs);
    let proof =
        ProofWithPublicInputs::<F, C, D>::from_bytes(snark_bytes.to_vec(), &circuit.data.common)
            .context("decode plonky2 proof")?;
    circuit.data.verify(proof.clone())?;
    let public_hash = HashOut {
        elements: [
            proof.public_inputs[0],
            proof.public_inputs[1],
            proof.public_inputs[2],
            proof.public_inputs[3],
        ],
    };
    if public_hash.elements != expected_digest.elements {
        return Err(anyhow!("statement digest mismatch"));
    }
    Ok(hash_out_to_global_zk_root(&expected_digest))
}

/// Verify a tier-2 SNARK against verified STWO public statements.
pub fn verify_global_aggregation_verified_snark(
    snark_bytes: &[u8],
    masterchain_height: u64,
    global_state_root: &Hash256,
    proofs: &[ProofSubmissionV1],
    verified: &[VerifiedStwoStatementV1],
) -> anyhow::Result<Hash256> {
    if proofs.is_empty() {
        return Err(anyhow!("empty proof set"));
    }
    if proofs.len() != verified.len()
        || proofs
            .iter()
            .any(|p| !verified.iter().any(|s| s.matches_submission(p)))
    {
        return Err(anyhow!(
            "verified STWO statements do not match proof submissions"
        ));
    }
    let circuit = verified_agg_circuit();
    let expected_digest =
        poseidon_verified_statement_digest(masterchain_height, global_state_root, proofs, verified);
    let proof =
        ProofWithPublicInputs::<F, C, D>::from_bytes(snark_bytes.to_vec(), &circuit.data.common)
            .context("decode plonky2 proof")?;
    circuit.data.verify(proof.clone())?;
    let public_hash = HashOut {
        elements: [
            proof.public_inputs[0],
            proof.public_inputs[1],
            proof.public_inputs[2],
            proof.public_inputs[3],
        ],
    };
    if public_hash.elements != expected_digest.elements {
        return Err(anyhow!("statement digest mismatch"));
    }
    Ok(hash_out_to_global_zk_root(&expected_digest))
}

#[cfg(test)]
mod tests {
    use super::*;
    use fractal_shard::ProofSubmissionV1;

    fn sample_proofs() -> Vec<ProofSubmissionV1> {
        vec![
            ProofSubmissionV1 {
                shard_id: 0,
                start_block: 1,
                end_block: 100,
                prover: [0xaa; 20],
                lag_seconds: 1,
                proof_digest: [1u8; 32],
            },
            ProofSubmissionV1 {
                shard_id: 1,
                start_block: 1,
                end_block: 200,
                prover: [0xbb; 20],
                lag_seconds: 2,
                proof_digest: [2u8; 32],
            },
        ]
    }

    #[test]
    fn plonky2_prove_verify_round_trip() {
        let gsr = [3u8; 32];
        let proofs = sample_proofs();
        let (root, bytes) = prove_global_aggregation(7, &gsr, &proofs).expect("prove");
        assert_ne!(root, [0u8; 32]);
        let verified = verify_global_aggregation_snark(&bytes, 7, &gsr, &proofs).expect("verify");
        assert_eq!(verified, root);
    }

    #[test]
    fn plonky2_verified_statement_round_trip() {
        let gsr = [3u8; 32];
        let proofs = sample_proofs();
        let verified: Vec<_> = proofs
            .iter()
            .map(|p| VerifiedStwoStatementV1 {
                shard_id: p.shard_id,
                chain_id: 41,
                start_block: p.start_block,
                end_block: p.end_block,
                height: p.end_block,
                header_hash: [11u8; 32],
                parent_hash: [12u8; 32],
                state_root: [13u8; 32],
                tx_root: [14u8; 32],
                gas_used: 21_000,
                proof_digest: p.proof_digest,
            })
            .collect();
        let (root, bytes) =
            prove_global_aggregation_verified(7, &gsr, &proofs, &verified).expect("prove");
        assert_ne!(root, [0u8; 32]);
        let verified_root =
            verify_global_aggregation_verified_snark(&bytes, 7, &gsr, &proofs, &verified)
                .expect("verify");
        assert_eq!(verified_root, root);
    }
}
