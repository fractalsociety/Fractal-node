use borsh::{BorshDeserialize, BorshSerialize};
use fractal_crypto::hash::{keccak256, Hash256};
use qp_plonky2_verifier::field::types::PrimeField64;
use qp_plonky2_verifier::util::serialization::DefaultGateSerializer;
use qp_plonky2_verifier::{
    CompressedProofWithPublicInputs, ProofWithPublicInputs, VerifierCircuitData,
    C as Plonky2Config, D as PLONKY2_D, F as Plonky2Field,
};
use thiserror::Error;

const FRACTAL_PUBLIC_INPUT_DIGEST_LIMBS: usize = 8;
const STWO_EXECUTION_AIR_ADAPTER_V1_TAG: &[u8] = b"FRACTAL_STWO_EXECUTION_AIR_ADAPTER_V1";
const STWO_RECURSIVE_FIXTURE_V1_TAG: &[u8] = b"FRACTAL_STWO_RECURSIVE_FIXTURE_V1";

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProductionProofVerifyError {
    #[error("production proof envelope is malformed")]
    MalformedEnvelope,
    #[error("production proof envelope version is unsupported")]
    UnsupportedVersion,
    #[error("production STWO adapter is not linked")]
    StwoAdapterUnavailable,
    #[error("production STWO execution AIR adapter does not match public inputs")]
    StwoAirAdapter,
    #[error("canonical recursive proof fixture does not match STWO AIR adapter")]
    RecursiveFixture,
    #[error("production proof public inputs do not bind to block public inputs")]
    PublicInputDigest,
    #[error("production Plonky2 verifier data is invalid")]
    Plonky2VerifierData,
    #[error("production Plonky2 proof is invalid")]
    Plonky2Proof,
    #[error("production Plonky2 proof was rejected by verifier")]
    Plonky2Rejected,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct StwoExecutionAirAdapterV1 {
    pub version: u16,
    pub air_id: Hash256,
    pub chain_id: u64,
    pub height: u64,
    pub block_hash: Hash256,
    pub state_root: Hash256,
    pub tx_root: Hash256,
    pub zone_namespace: [u8; 8],
    pub da_root: Hash256,
    pub public_input_digest: Hash256,
    pub public_input_limbs: [u64; 8],
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct CanonicalRecursiveProofFixtureV1 {
    pub version: u16,
    pub fixture_id: Hash256,
    pub stwo_air_adapter_digest: Hash256,
    pub plonky2_verifier_id: Hash256,
    pub public_input_digest: Hash256,
    pub public_input_limbs: [u64; 8],
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum StwoPlonky2ProofEnvelope {
    Plonky2PoseidonGoldilocksV1 {
        verifier_circuit_data: Vec<u8>,
        proof_with_public_inputs: Vec<u8>,
        compressed: bool,
    },
    StwoV1 {
        air_adapter: StwoExecutionAirAdapterV1,
        recursive_fixture: CanonicalRecursiveProofFixtureV1,
        proof_bytes: Vec<u8>,
    },
}

pub fn stwo_plonky2_public_input_limbs(public_input_digest: &Hash256) -> [u64; 8] {
    let mut limbs = [0u64; FRACTAL_PUBLIC_INPUT_DIGEST_LIMBS];
    for (idx, chunk) in public_input_digest.chunks_exact(4).enumerate() {
        limbs[idx] = u32::from_le_bytes(chunk.try_into().expect("chunk is four bytes")) as u64;
    }
    limbs
}

pub fn stwo_execution_air_id() -> Hash256 {
    keccak256(STWO_EXECUTION_AIR_ADAPTER_V1_TAG)
}

pub fn stwo_execution_air_adapter_v1(
    proof: &crate::BlockValidityProof,
) -> Result<StwoExecutionAirAdapterV1, std::io::Error> {
    let public_input_digest = crate::validity_proof_public_input_digest(proof)?;
    Ok(StwoExecutionAirAdapterV1 {
        version: 1,
        air_id: stwo_execution_air_id(),
        chain_id: proof.chain_id,
        height: proof.height,
        block_hash: proof.block_hash,
        state_root: proof.state_root,
        tx_root: proof.tx_root,
        zone_namespace: proof.zone_namespace,
        da_root: proof.da_root,
        public_input_digest,
        public_input_limbs: stwo_plonky2_public_input_limbs(&public_input_digest),
    })
}

pub fn stwo_execution_air_adapter_digest(
    adapter: &StwoExecutionAirAdapterV1,
) -> Result<Hash256, std::io::Error> {
    Ok(keccak256(&borsh::to_vec(adapter)?))
}

pub fn canonical_recursive_proof_fixture_v1(
    adapter: &StwoExecutionAirAdapterV1,
    plonky2_verifier_id: Hash256,
) -> Result<CanonicalRecursiveProofFixtureV1, std::io::Error> {
    let stwo_air_adapter_digest = stwo_execution_air_adapter_digest(adapter)?;
    let body = (
        STWO_RECURSIVE_FIXTURE_V1_TAG,
        stwo_air_adapter_digest,
        plonky2_verifier_id,
        adapter.public_input_digest,
        adapter.public_input_limbs,
    );
    Ok(CanonicalRecursiveProofFixtureV1 {
        version: 1,
        fixture_id: keccak256(&borsh::to_vec(&body)?),
        stwo_air_adapter_digest,
        plonky2_verifier_id,
        public_input_digest: adapter.public_input_digest,
        public_input_limbs: adapter.public_input_limbs,
    })
}

pub fn verify_stwo_plonky2_proof(
    proof_bytes: &[u8],
    public_input_digest: Hash256,
) -> Result<(), ProductionProofVerifyError> {
    let envelope = StwoPlonky2ProofEnvelope::try_from_slice(proof_bytes)
        .map_err(|_| ProductionProofVerifyError::MalformedEnvelope)?;
    match envelope {
        StwoPlonky2ProofEnvelope::Plonky2PoseidonGoldilocksV1 {
            verifier_circuit_data,
            proof_with_public_inputs,
            compressed,
        } => verify_plonky2_poseidon_goldilocks(
            verifier_circuit_data,
            proof_with_public_inputs,
            compressed,
            public_input_digest,
        ),
        StwoPlonky2ProofEnvelope::StwoV1 {
            air_adapter,
            recursive_fixture,
            proof_bytes,
        } => verify_stwo_execution_fixture(
            air_adapter,
            recursive_fixture,
            proof_bytes,
            public_input_digest,
        ),
    }
}

fn verify_stwo_execution_fixture(
    air_adapter: StwoExecutionAirAdapterV1,
    recursive_fixture: CanonicalRecursiveProofFixtureV1,
    proof_bytes: Vec<u8>,
    public_input_digest: Hash256,
) -> Result<(), ProductionProofVerifyError> {
    if air_adapter.version != 1 || air_adapter.air_id != stwo_execution_air_id() {
        return Err(ProductionProofVerifyError::StwoAirAdapter);
    }
    if air_adapter.public_input_digest != public_input_digest
        || air_adapter.public_input_limbs != stwo_plonky2_public_input_limbs(&public_input_digest)
    {
        return Err(ProductionProofVerifyError::PublicInputDigest);
    }
    let adapter_digest = stwo_execution_air_adapter_digest(&air_adapter)
        .map_err(|_| ProductionProofVerifyError::StwoAirAdapter)?;
    if recursive_fixture.version != 1
        || recursive_fixture.stwo_air_adapter_digest != adapter_digest
        || recursive_fixture.public_input_digest != public_input_digest
        || recursive_fixture.public_input_limbs != air_adapter.public_input_limbs
    {
        return Err(ProductionProofVerifyError::RecursiveFixture);
    }
    let expected_fixture =
        canonical_recursive_proof_fixture_v1(&air_adapter, recursive_fixture.plonky2_verifier_id)
            .map_err(|_| ProductionProofVerifyError::RecursiveFixture)?;
    if expected_fixture.fixture_id != recursive_fixture.fixture_id {
        return Err(ProductionProofVerifyError::RecursiveFixture);
    }
    if proof_bytes.is_empty() {
        return Err(ProductionProofVerifyError::Plonky2Proof);
    }
    Err(ProductionProofVerifyError::StwoAdapterUnavailable)
}

fn verify_plonky2_poseidon_goldilocks(
    verifier_circuit_data: Vec<u8>,
    proof_with_public_inputs: Vec<u8>,
    compressed: bool,
    public_input_digest: Hash256,
) -> Result<(), ProductionProofVerifyError> {
    let verifier_data = VerifierCircuitData::<Plonky2Field, Plonky2Config, PLONKY2_D>::from_bytes(
        verifier_circuit_data,
        &DefaultGateSerializer,
    )
    .map_err(|_| ProductionProofVerifyError::Plonky2VerifierData)?;

    if compressed {
        let proof =
            CompressedProofWithPublicInputs::<Plonky2Field, Plonky2Config, PLONKY2_D>::from_bytes(
                proof_with_public_inputs,
                &verifier_data.common,
            )
            .map_err(|_| ProductionProofVerifyError::Plonky2Proof)?;
        verify_public_input_digest(&proof.public_inputs, &public_input_digest)?;
        verifier_data
            .verify_compressed(proof)
            .map_err(|_| ProductionProofVerifyError::Plonky2Rejected)
    } else {
        let proof = ProofWithPublicInputs::<Plonky2Field, Plonky2Config, PLONKY2_D>::from_bytes(
            proof_with_public_inputs,
            &verifier_data.common,
        )
        .map_err(|_| ProductionProofVerifyError::Plonky2Proof)?;
        verify_public_input_digest(&proof.public_inputs, &public_input_digest)?;
        verifier_data
            .verify(proof)
            .map_err(|_| ProductionProofVerifyError::Plonky2Rejected)
    }
}

fn verify_public_input_digest(
    public_inputs: &[Plonky2Field],
    expected_digest: &Hash256,
) -> Result<(), ProductionProofVerifyError> {
    if public_inputs.len() < FRACTAL_PUBLIC_INPUT_DIGEST_LIMBS {
        return Err(ProductionProofVerifyError::PublicInputDigest);
    }
    let expected = stwo_plonky2_public_input_limbs(expected_digest);
    for (got, want) in public_inputs
        .iter()
        .take(FRACTAL_PUBLIC_INPUT_DIGEST_LIMBS)
        .zip(expected)
    {
        if got.to_canonical_u64() != want {
            return Err(ProductionProofVerifyError::PublicInputDigest);
        }
    }
    Ok(())
}

pub fn stwo_plonky2_verifier_id(verifier_circuit_data: &[u8]) -> Hash256 {
    keccak256(verifier_circuit_data)
}
