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
const NATIVE_STATE_TRANSITION_AIR_V1_TAG: &[u8] = b"FRACTAL_NATIVE_STATE_TRANSITION_AIR_V1";
const NATIVE_STATE_TRANSITION_FS_V1_TAG: &[u8] = b"FRACTAL_NATIVE_STATE_TRANSITION_FS_V1";
const NATIVE_STATE_TRANSITION_FIXTURE_V1_TAG: &[u8] = b"FRACTAL_NATIVE_STATE_TRANSITION_FIXTURE_V1";
const NATIVE_RECURSIVE_WRAP_FIXTURE_V1_TAG: &[u8] = b"FRACTAL_NATIVE_RECURSIVE_WRAP_FIXTURE_V1";
const NATIVE_COMPRESSED_PLONKY2_FIXTURE_V1_TAG: &[u8] =
    b"FRACTAL_NATIVE_COMPRESSED_PLONKY2_FIXTURE_V1";
const EVM_ZKVM_TRANSITION_STATEMENT_V1_TAG: &[u8] = b"FRACTAL_EVM_ZKVM_TRANSITION_STATEMENT_V1";
const EVM_ZKVM_FIXTURE_V1_TAG: &[u8] = b"FRACTAL_EVM_ZKVM_FIXTURE_V1";
const NATIVE_MIXED_COMPONENT_STATEMENT_V1_TAG: &[u8] =
    b"FRACTAL_NATIVE_MIXED_COMPONENT_STATEMENT_V1";
const MIXED_INTRABLOCK_AGGREGATE_FIXTURE_V1_TAG: &[u8] =
    b"FRACTAL_MIXED_INTRABLOCK_AGGREGATE_FIXTURE_V1";
pub const NATIVE_TRACE_COLUMN_COUNT_V1: usize = 16;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum NativeStateTransitionAirError {
    #[error("native state transition AIR only accepts witness version 1")]
    UnsupportedWitnessVersion,
    #[error("native state transition AIR requires circuit_version native_state_transition_v1")]
    CircuitVersion,
    #[error("native state transition AIR coverage manifest mismatch")]
    CoverageManifest,
    #[error("native state transition AIR cannot prove EVM execution")]
    EvmExecutionPresent,
    #[error("native state transition AIR cannot prove EVM-to-native dispatch rows")]
    PrecompileDispatchPresent,
    #[error("native state transition AIR feature set exceeds native coverage")]
    Coverage,
    #[error("native state transition AIR gas sum mismatch")]
    Gas,
    #[error("native state transition AIR receipt root mismatch")]
    ReceiptRoot,
    #[error("native state transition AIR native event root mismatch")]
    NativeEventRoot,
    #[error("native state transition AIR fixed gas mismatch at tx index {0}")]
    FixedGas(u32),
    #[error("native state transition AIR trace row/transaction mismatch at tx index {0}")]
    TraceTx(u32),
    #[error("native state transition AIR native subtrie witness is missing authenticated paths")]
    NativeSubtrieWitness,
    #[error("native state transition AIR fixture proof digest mismatch")]
    FixtureDigest,
    #[error("native state transition AIR borsh encoding failed")]
    Encode,
}

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
    #[error("native recursive fixture does not bind to the submitted block proof")]
    NativeRecursiveFixture,
    #[error("native recursive fixture requires circuit_version native_state_transition_v1")]
    NativeCircuitVersion,
    #[error("native inter-block chain link is invalid")]
    NativeInterBlockChain,
    #[error("EVM zkVM fixture does not bind to the submitted mixed block proof")]
    EvmZkVmFixture,
    #[error("EVM zkVM fixture requires circuit_version mixed_state_transition_v1")]
    MixedCircuitVersion,
    #[error("mixed intra-block aggregate fixture does not bind native and EVM component proofs")]
    MixedAggregateFixture,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct StwoExecutionAirAdapterV1 {
    pub version: u16,
    pub air_id: Hash256,
    pub chain_id: u64,
    pub height: u64,
    pub block_hash: Hash256,
    pub timestamp_ms: u64,
    pub parent_state_root: Hash256,
    pub state_root: Hash256,
    pub tx_root: Hash256,
    pub receipt_root: Hash256,
    pub native_event_root: Hash256,
    pub evm_log_root: Hash256,
    pub gas_used: u64,
    pub zone_namespace: [u8; 8],
    pub da_root: Hash256,
    pub circuit_version: crate::CircuitVersion,
    pub coverage_manifest_digest: Hash256,
    pub feature_set: crate::ExecutionFeatureSetV1,
    pub public_input_digest: Hash256,
    pub public_input_limbs: [u64; 8],
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct NativeStateTransitionTraceColumnRowV1 {
    pub tx_index: u32,
    pub columns: [u64; NATIVE_TRACE_COLUMN_COUNT_V1],
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct NativeStateTransitionAirV1 {
    pub version: u16,
    pub air_id: Hash256,
    pub trace_column_count: u16,
    pub state_commitment_scheme: crate::StateCommitmentScheme,
    pub circuit_version: crate::CircuitVersion,
    pub coverage_manifest_digest: Hash256,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct NativeStateTransitionStatementV1 {
    pub version: u16,
    pub air_id: Hash256,
    pub public_input_digest: Hash256,
    pub witness_digest: Hash256,
    pub fiat_shamir_transcript_digest: Hash256,
    pub trace_root: Hash256,
    pub native_subtrie_access_digest: Hash256,
    pub constraint_digest: Hash256,
    pub gas_used: u64,
    pub native_event_root: Hash256,
    pub receipt_root: Hash256,
    pub feature_set: crate::ExecutionFeatureSetV1,
    pub trace_rows: u32,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct NativeStateTransitionProofFixtureV1 {
    pub version: u16,
    pub air: NativeStateTransitionAirV1,
    pub statement: NativeStateTransitionStatementV1,
    pub proof_digest: Hash256,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct NativeRecursiveWrapFixtureV1 {
    pub version: u16,
    pub fixture_id: Hash256,
    pub native_statement_digest: Hash256,
    pub public_input_digest: Hash256,
    pub public_input_limbs: [u64; 8],
    pub circuit_version: crate::CircuitVersion,
    pub coverage_manifest_digest: Hash256,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct CompressedPlonky2NativeProofFixtureV1 {
    pub version: u16,
    pub verifier_id: Hash256,
    pub recursive_fixture_digest: Hash256,
    pub public_input_digest: Hash256,
    pub public_input_limbs: [u64; 8],
    pub proof_digest: Hash256,
    pub compressed: bool,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct EvmZkVmTransitionStatementV1 {
    pub version: u16,
    pub zkvm_choice: crate::ZkVmChoiceV1,
    pub zkvm_target_id: Hash256,
    pub revm_subset_digest: Hash256,
    pub public_input_digest: Hash256,
    pub witness_digest: Hash256,
    pub evm_trace_root: Hash256,
    pub evm_state_access_digest: Hash256,
    pub pre_evm_root: Hash256,
    pub post_evm_root: Hash256,
    pub unified_post_state_root: Hash256,
    pub evm_log_root: Hash256,
    pub gas_used: u64,
    pub feature_set: crate::ExecutionFeatureSetV1,
    pub covered_features: crate::ExecutionFeatureSetV1,
    pub evm_trace_rows: u32,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct EvmZkVmProofFixtureV1 {
    pub version: u16,
    pub statement: EvmZkVmTransitionStatementV1,
    pub coverage_manifest_digest: Hash256,
    pub proof_digest: Hash256,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct NativeMixedComponentStatementV1 {
    pub version: u16,
    pub component_id: Hash256,
    pub public_input_digest: Hash256,
    pub witness_digest: Hash256,
    pub native_trace_root: Hash256,
    pub precompile_dispatch_root: Hash256,
    pub native_state_access_digest: Hash256,
    pub native_event_root: Hash256,
    pub receipt_root: Hash256,
    pub gas_used: u64,
    pub feature_set: crate::ExecutionFeatureSetV1,
    pub native_trace_rows: u32,
    pub precompile_dispatch_rows: u32,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct MixedIntraBlockAggregateFixtureV1 {
    pub version: u16,
    pub public_input_digest: Hash256,
    pub witness_digest: Hash256,
    pub circuit_version: crate::CircuitVersion,
    pub coverage_manifest_digest: Hash256,
    pub native_component_statement: NativeMixedComponentStatementV1,
    pub evm_zkvm_fixture: EvmZkVmProofFixtureV1,
    pub native_component_digest: Hash256,
    pub evm_component_digest: Hash256,
    pub aggregate_proof_digest: Hash256,
    pub compressed: bool,
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
    NativeRecursiveFixtureV1 {
        statement: NativeStateTransitionStatementV1,
        recursive_fixture: NativeRecursiveWrapFixtureV1,
        compressed_plonky2_fixture: CompressedPlonky2NativeProofFixtureV1,
    },
    EvmZkVmFixtureV1 {
        fixture: EvmZkVmProofFixtureV1,
    },
    MixedIntraBlockAggregateFixtureV1 {
        fixture: MixedIntraBlockAggregateFixtureV1,
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
        timestamp_ms: proof.timestamp_ms,
        parent_state_root: proof.parent_state_root,
        state_root: proof.state_root,
        tx_root: proof.tx_root,
        receipt_root: proof.receipt_root,
        native_event_root: proof.native_event_root,
        evm_log_root: proof.evm_log_root,
        gas_used: proof.gas_used,
        zone_namespace: proof.zone_namespace,
        da_root: proof.da_root,
        circuit_version: proof.circuit_version,
        coverage_manifest_digest: proof.coverage_manifest_digest,
        feature_set: proof.feature_set,
        public_input_digest,
        public_input_limbs: stwo_plonky2_public_input_limbs(&public_input_digest),
    })
}

pub fn stwo_execution_air_adapter_digest(
    adapter: &StwoExecutionAirAdapterV1,
) -> Result<Hash256, std::io::Error> {
    Ok(keccak256(&borsh::to_vec(adapter)?))
}

pub fn native_state_transition_air_id() -> Hash256 {
    keccak256(NATIVE_STATE_TRANSITION_AIR_V1_TAG)
}

pub fn native_state_transition_air_v1() -> NativeStateTransitionAirV1 {
    let manifest = crate::coverage_manifest_for_circuit_version(
        crate::CircuitVersion::NativeStateTransitionV1,
    );
    NativeStateTransitionAirV1 {
        version: 1,
        air_id: native_state_transition_air_id(),
        trace_column_count: NATIVE_TRACE_COLUMN_COUNT_V1 as u16,
        state_commitment_scheme: crate::STATE_COMMITMENT_SCHEME_V1,
        circuit_version: crate::CircuitVersion::NativeStateTransitionV1,
        coverage_manifest_digest: crate::coverage_manifest_digest(&manifest)
            .expect("coverage manifest borsh"),
    }
}

fn native_call_opcode(call: &fractal_core::NativeCall) -> u8 {
    match call {
        fractal_core::NativeCall::RegisterAgent { .. } => 0x01,
        fractal_core::NativeCall::UpdateAgent { .. } => 0x02,
        fractal_core::NativeCall::SuspendAgent { .. } => 0x03,
        fractal_core::NativeCall::SettleReceipt(_) => 0x04,
        fractal_core::NativeCall::SettleBatch(_) => 0x05,
        fractal_core::NativeCall::ClaimPayout { .. } => 0x06,
        fractal_core::NativeCall::FileDispute { .. } => 0x07,
        fractal_core::NativeCall::ResolveDispute { .. } => 0x08,
        fractal_core::NativeCall::Stake { .. } => 0x09,
        fractal_core::NativeCall::Unstake { .. } => 0x0a,
        fractal_core::NativeCall::Slash { .. } => 0x0b,
        fractal_core::NativeCall::Delegate { .. } => 0x0c,
        fractal_core::NativeCall::WithdrawRewards { .. } => 0x0d,
        fractal_core::NativeCall::WalletTaskReceiptAnchorV1 { .. } => 0x0e,
        fractal_core::NativeCall::NoOp => 0x00,
        fractal_core::NativeCall::SetChainEconomics { .. } => 0x0f,
    }
}

fn low_u64(bytes: &Hash256, offset: usize) -> u64 {
    u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("hash limb"))
}

pub fn native_state_transition_trace_columns_v1(
    witness: &crate::MixedExecutionWitnessV1,
) -> Result<Vec<NativeStateTransitionTraceColumnRowV1>, NativeStateTransitionAirError> {
    witness
        .native_trace_rows
        .iter()
        .map(|row| {
            let tx = witness
                .transactions
                .get(row.tx_index as usize)
                .ok_or(NativeStateTransitionAirError::TraceTx(row.tx_index))?;
            if row.tx_hash
                != crate::tx_hash(tx).map_err(|_| NativeStateTransitionAirError::Encode)?
            {
                return Err(NativeStateTransitionAirError::TraceTx(row.tx_index));
            }
            let payload_hash = keccak256(
                &borsh::to_vec(&row.call).map_err(|_| NativeStateTransitionAirError::Encode)?,
            );
            let signer_hash = keccak256(&row.signer);
            let mut columns = [0u64; NATIVE_TRACE_COLUMN_COUNT_V1];
            columns[0] = u64::from(row.tx_index);
            columns[1] = u64::from(native_call_opcode(&row.call));
            columns[2] = row.nonce;
            columns[3] = row.gas_used;
            columns[4] = row.signer_pre_nonce;
            columns[5] = row.signer_post_nonce;
            columns[6] = row.signer_pre_balance as u64;
            columns[7] = row.signer_post_balance as u64;
            columns[8] = row.state_access_indices.len() as u64;
            columns[9] = low_u64(&row.tx_hash, 0);
            columns[10] = low_u64(&row.tx_hash, 8);
            columns[11] = low_u64(&payload_hash, 0);
            columns[12] = low_u64(&payload_hash, 8);
            columns[13] = low_u64(&signer_hash, 0);
            columns[14] = row.native_event_start as u64;
            columns[15] = row.native_event_count as u64;
            Ok(NativeStateTransitionTraceColumnRowV1 {
                tx_index: row.tx_index,
                columns,
            })
        })
        .collect()
}

fn digest_borsh<T: BorshSerialize>(value: &T) -> Result<Hash256, NativeStateTransitionAirError> {
    Ok(keccak256(
        &borsh::to_vec(value).map_err(|_| NativeStateTransitionAirError::Encode)?,
    ))
}

fn digest_borsh_io<T: BorshSerialize>(value: &T) -> Result<Hash256, std::io::Error> {
    Ok(keccak256(&borsh::to_vec(value)?))
}

fn native_subtrie_access_digest(
    witness: &crate::MixedExecutionWitnessV1,
) -> Result<Hash256, NativeStateTransitionAirError> {
    let native_rows: Vec<_> = witness
        .state_accesses
        .iter()
        .filter(|row| {
            matches!(
                row.namespace,
                crate::StateCommitmentNamespace::Native | crate::StateCommitmentNamespace::Accounts
            )
        })
        .collect();
    if native_rows.iter().any(|row| {
        let needs_pre = matches!(
            row.kind,
            crate::StateAccessKindV1::Read | crate::StateAccessKindV1::ReadWrite
        );
        let needs_post = matches!(
            row.kind,
            crate::StateAccessKindV1::Write | crate::StateAccessKindV1::ReadWrite
        );
        (needs_pre && row.pre_value_hash.is_some() && row.pre_state_path.is_empty())
            || (needs_post && row.post_value_hash.is_some() && row.post_state_path.is_empty())
    }) {
        return Err(NativeStateTransitionAirError::NativeSubtrieWitness);
    }
    digest_borsh(&native_rows)
}

pub fn native_state_transition_statement_v1(
    witness: &crate::MixedExecutionWitnessV1,
) -> Result<NativeStateTransitionStatementV1, NativeStateTransitionAirError> {
    if witness.version != crate::MIXED_EXECUTION_WITNESS_V1 {
        return Err(NativeStateTransitionAirError::UnsupportedWitnessVersion);
    }
    if witness.public_inputs.circuit_version != crate::CircuitVersion::NativeStateTransitionV1 {
        return Err(NativeStateTransitionAirError::CircuitVersion);
    }
    let air = native_state_transition_air_v1();
    if witness.public_inputs.coverage_manifest_digest != air.coverage_manifest_digest {
        return Err(NativeStateTransitionAirError::CoverageManifest);
    }
    if !witness.evm_trace_rows.is_empty() {
        return Err(NativeStateTransitionAirError::EvmExecutionPresent);
    }
    if !witness.precompile_dispatch_rows.is_empty() {
        return Err(NativeStateTransitionAirError::PrecompileDispatchPresent);
    }
    let coverage = crate::coverage_manifest_for_circuit_version(air.circuit_version);
    if !witness
        .public_inputs
        .feature_set
        .contains_only(coverage.covered_features)
    {
        return Err(NativeStateTransitionAirError::Coverage);
    }
    if witness.gas_sum != witness.public_inputs.gas_used {
        return Err(NativeStateTransitionAirError::Gas);
    }
    let receipt_root = crate::tx_receipt_root(&witness.tx_receipts)
        .map_err(|_| NativeStateTransitionAirError::Encode)?;
    if receipt_root != witness.public_inputs.receipt_root {
        return Err(NativeStateTransitionAirError::ReceiptRoot);
    }
    let tx_gas_sum = witness.tx_receipts.iter().try_fold(0u64, |sum, receipt| {
        sum.checked_add(receipt.gas_used)
            .ok_or(NativeStateTransitionAirError::Gas)
    })?;
    if tx_gas_sum != witness.public_inputs.gas_used {
        return Err(NativeStateTransitionAirError::Gas);
    }
    for row in &witness.native_trace_rows {
        let tx = witness
            .transactions
            .get(row.tx_index as usize)
            .ok_or(NativeStateTransitionAirError::TraceTx(row.tx_index))?;
        if row.native_event_root != witness.public_inputs.native_event_root {
            return Err(NativeStateTransitionAirError::NativeEventRoot);
        }
        let fixed_gas = fractal_core::intrinsic_gas(tx)
            .map_err(|_| NativeStateTransitionAirError::FixedGas(row.tx_index))?;
        if row.gas_used != fixed_gas {
            return Err(NativeStateTransitionAirError::FixedGas(row.tx_index));
        }
    }
    let trace_columns = native_state_transition_trace_columns_v1(witness)?;
    let public_input_digest = digest_borsh(&witness.public_inputs)?;
    let witness_digest = crate::mixed_execution_witness_digest(witness)
        .map_err(|_| NativeStateTransitionAirError::Encode)?;
    let trace_root = digest_borsh(&trace_columns)?;
    let native_subtrie_access_digest = native_subtrie_access_digest(witness)?;
    let constraint_digest = digest_borsh(&(
        &trace_columns,
        witness.public_inputs.gas_used,
        witness.public_inputs.receipt_root,
        witness.public_inputs.native_event_root,
        witness.public_inputs.feature_set,
        native_subtrie_access_digest,
    ))?;
    let fiat_shamir_transcript_digest = digest_borsh(&(
        NATIVE_STATE_TRANSITION_FS_V1_TAG,
        public_input_digest,
        witness_digest,
        trace_root,
        native_subtrie_access_digest,
        constraint_digest,
    ))?;
    Ok(NativeStateTransitionStatementV1 {
        version: 1,
        air_id: air.air_id,
        public_input_digest,
        witness_digest,
        fiat_shamir_transcript_digest,
        trace_root,
        native_subtrie_access_digest,
        constraint_digest,
        gas_used: witness.public_inputs.gas_used,
        native_event_root: witness.public_inputs.native_event_root,
        receipt_root: witness.public_inputs.receipt_root,
        feature_set: witness.public_inputs.feature_set,
        trace_rows: trace_columns.len() as u32,
    })
}

pub fn prove_native_state_transition_fixture_v1(
    witness: &crate::MixedExecutionWitnessV1,
) -> Result<NativeStateTransitionProofFixtureV1, NativeStateTransitionAirError> {
    let air = native_state_transition_air_v1();
    let statement = native_state_transition_statement_v1(witness)?;
    let proof_digest = digest_borsh(&(NATIVE_STATE_TRANSITION_FIXTURE_V1_TAG, &air, &statement))?;
    Ok(NativeStateTransitionProofFixtureV1 {
        version: 1,
        air,
        statement,
        proof_digest,
    })
}

pub fn verify_native_state_transition_fixture_v1(
    fixture: &NativeStateTransitionProofFixtureV1,
    witness: &crate::MixedExecutionWitnessV1,
) -> Result<(), NativeStateTransitionAirError> {
    if fixture.version != 1 || fixture.air != native_state_transition_air_v1() {
        return Err(NativeStateTransitionAirError::CircuitVersion);
    }
    let expected_statement = native_state_transition_statement_v1(witness)?;
    if fixture.statement != expected_statement {
        return Err(NativeStateTransitionAirError::FixtureDigest);
    }
    let expected_digest = digest_borsh(&(
        NATIVE_STATE_TRANSITION_FIXTURE_V1_TAG,
        &fixture.air,
        &fixture.statement,
    ))?;
    if fixture.proof_digest != expected_digest {
        return Err(NativeStateTransitionAirError::FixtureDigest);
    }
    Ok(())
}

pub fn native_state_transition_statement_digest_v1(
    statement: &NativeStateTransitionStatementV1,
) -> Result<Hash256, std::io::Error> {
    Ok(keccak256(&borsh::to_vec(statement)?))
}

pub fn evm_zkvm_transition_statement_v1(
    witness: &crate::MixedExecutionWitnessV1,
) -> Result<EvmZkVmTransitionStatementV1, std::io::Error> {
    if witness.public_inputs.circuit_version != crate::CircuitVersion::MixedStateTransitionV1 {
        return Err(std::io::Error::other(
            "EVM zkVM statement requires mixed_state_transition_v1",
        ));
    }
    if witness.evm_zkvm_surface != crate::evm_zkvm_surface_v1() {
        return Err(std::io::Error::other("EVM zkVM surface mismatch"));
    }
    crate::verify_transactions_eligible_for_circuit(
        &witness.transactions,
        crate::CircuitVersion::MixedStateTransitionV1,
    )
    .map_err(std::io::Error::other)?;

    let manifest =
        crate::coverage_manifest_for_circuit_version(crate::CircuitVersion::MixedStateTransitionV1);
    if witness.public_inputs.coverage_manifest_digest != crate::coverage_manifest_digest(&manifest)?
        || !witness
            .public_inputs
            .feature_set
            .contains_only(manifest.covered_features)
    {
        return Err(std::io::Error::other("mixed coverage manifest mismatch"));
    }

    let evm_trace_gas = witness.evm_trace_rows.iter().try_fold(0u64, |sum, row| {
        sum.checked_add(row.gas_used)
            .ok_or_else(|| std::io::Error::other("EVM trace gas overflow"))
    })?;
    let evm_receipt_gas = witness
        .tx_receipts
        .iter()
        .zip(&witness.transactions)
        .try_fold(0u64, |sum, (receipt, tx)| {
            if tx.vm == fractal_core::VmKind::Evm {
                sum.checked_add(receipt.gas_used)
                    .ok_or_else(|| std::io::Error::other("EVM receipt gas overflow"))
            } else {
                Ok(sum)
            }
        })?;
    if evm_trace_gas != evm_receipt_gas {
        return Err(std::io::Error::other("EVM trace gas mismatch"));
    }

    let pre_evm_root = witness
        .evm_trace_rows
        .first()
        .map(|row| row.pre_evm_root)
        .unwrap_or(witness.pre_state_commitment.evm_root);
    let post_evm_root = witness
        .evm_trace_rows
        .last()
        .map(|row| row.post_evm_root)
        .unwrap_or(witness.post_state_commitment.evm_root);
    if pre_evm_root != witness.pre_state_commitment.evm_root
        || post_evm_root != witness.post_state_commitment.evm_root
    {
        return Err(std::io::Error::other("EVM state commitment mismatch"));
    }

    let evm_state_rows: Vec<_> = witness
        .state_accesses
        .iter()
        .filter(|row| {
            matches!(
                row.namespace,
                crate::StateCommitmentNamespace::Accounts | crate::StateCommitmentNamespace::Evm
            )
        })
        .collect();
    let public_input_digest = digest_borsh_io(&witness.public_inputs)?;
    let witness_digest = crate::mixed_execution_witness_digest(witness)?;
    let evm_trace_root = digest_borsh_io(&witness.evm_trace_rows)?;
    let evm_state_access_digest = digest_borsh_io(&evm_state_rows)?;
    let revm_subset_digest = digest_borsh_io(&witness.evm_zkvm_surface)?;
    let zkvm_target_id = keccak256(witness.evm_zkvm_surface.zkvm_target.as_bytes());

    Ok(EvmZkVmTransitionStatementV1 {
        version: 1,
        zkvm_choice: witness.evm_zkvm_surface.zkvm_choice,
        zkvm_target_id,
        revm_subset_digest,
        public_input_digest,
        witness_digest,
        evm_trace_root,
        evm_state_access_digest,
        pre_evm_root,
        post_evm_root,
        unified_post_state_root: witness.public_inputs.post_state_root,
        evm_log_root: witness.public_inputs.evm_log_root,
        gas_used: evm_trace_gas,
        feature_set: witness.public_inputs.feature_set,
        covered_features: witness.evm_zkvm_surface.covered_features,
        evm_trace_rows: witness.evm_trace_rows.len() as u32,
    })
}

pub fn native_mixed_component_statement_v1(
    witness: &crate::MixedExecutionWitnessV1,
) -> Result<NativeMixedComponentStatementV1, std::io::Error> {
    if witness.public_inputs.circuit_version != crate::CircuitVersion::MixedStateTransitionV1 {
        return Err(std::io::Error::other(
            "native mixed component requires mixed_state_transition_v1",
        ));
    }
    let manifest =
        crate::coverage_manifest_for_circuit_version(crate::CircuitVersion::MixedStateTransitionV1);
    if witness.public_inputs.coverage_manifest_digest != crate::coverage_manifest_digest(&manifest)?
        || !witness
            .public_inputs
            .feature_set
            .contains_only(manifest.covered_features)
    {
        return Err(std::io::Error::other("mixed coverage manifest mismatch"));
    }
    let trace_columns = native_state_transition_trace_columns_v1(witness)
        .map_err(|err| std::io::Error::other(err.to_string()))?;
    for row in &witness.native_trace_rows {
        let tx = witness
            .transactions
            .get(row.tx_index as usize)
            .ok_or_else(|| std::io::Error::other("native trace tx index out of bounds"))?;
        let fixed_gas = fractal_core::intrinsic_gas(tx).map_err(std::io::Error::other)?;
        if row.gas_used != fixed_gas {
            return Err(std::io::Error::other("native trace fixed gas mismatch"));
        }
        if row.native_event_root != witness.public_inputs.native_event_root {
            return Err(std::io::Error::other("native event root mismatch"));
        }
    }
    for row in &witness.precompile_dispatch_rows {
        let tx = witness
            .transactions
            .get(row.tx_index as usize)
            .ok_or_else(|| std::io::Error::other("precompile trace tx index out of bounds"))?;
        let fractal_core::TxBody::EvmCall { to, calldata, .. } = &tx.body else {
            return Err(std::io::Error::other("precompile row is not an EVM call"));
        };
        if !fractal_core::is_native_precompile_address(to)
            || *to != row.precompile_address
            || keccak256(calldata) != row.calldata_hash
        {
            return Err(std::io::Error::other("precompile dispatch row mismatch"));
        }
    }
    let receipt_root = crate::tx_receipt_root(&witness.tx_receipts)?;
    if receipt_root != witness.public_inputs.receipt_root {
        return Err(std::io::Error::other("receipt root mismatch"));
    }
    let gas_used = witness
        .native_trace_rows
        .iter()
        .try_fold(0u64, |sum, row| {
            sum.checked_add(row.gas_used)
                .ok_or_else(|| std::io::Error::other("native gas overflow"))
        })?;
    let native_state_rows: Vec<_> = witness
        .state_accesses
        .iter()
        .filter(|row| {
            matches!(
                row.namespace,
                crate::StateCommitmentNamespace::Accounts | crate::StateCommitmentNamespace::Native
            )
        })
        .collect();
    let public_input_digest = digest_borsh_io(&witness.public_inputs)?;
    let witness_digest = crate::mixed_execution_witness_digest(witness)?;
    let native_trace_root = digest_borsh_io(&trace_columns)?;
    let precompile_dispatch_root = digest_borsh_io(&witness.precompile_dispatch_rows)?;
    let native_state_access_digest = digest_borsh_io(&native_state_rows)?;
    Ok(NativeMixedComponentStatementV1 {
        version: 1,
        component_id: keccak256(NATIVE_MIXED_COMPONENT_STATEMENT_V1_TAG),
        public_input_digest,
        witness_digest,
        native_trace_root,
        precompile_dispatch_root,
        native_state_access_digest,
        native_event_root: witness.public_inputs.native_event_root,
        receipt_root: witness.public_inputs.receipt_root,
        gas_used,
        feature_set: witness.public_inputs.feature_set,
        native_trace_rows: witness.native_trace_rows.len() as u32,
        precompile_dispatch_rows: witness.precompile_dispatch_rows.len() as u32,
    })
}

pub fn evm_zkvm_proof_fixture_v1(
    witness: &crate::MixedExecutionWitnessV1,
) -> Result<EvmZkVmProofFixtureV1, std::io::Error> {
    let statement = evm_zkvm_transition_statement_v1(witness)?;
    let manifest =
        crate::coverage_manifest_for_circuit_version(crate::CircuitVersion::MixedStateTransitionV1);
    let coverage_manifest_digest = crate::coverage_manifest_digest(&manifest)?;
    let proof_digest = digest_borsh_io(&(
        EVM_ZKVM_FIXTURE_V1_TAG,
        EVM_ZKVM_TRANSITION_STATEMENT_V1_TAG,
        &statement,
        coverage_manifest_digest,
    ))?;
    Ok(EvmZkVmProofFixtureV1 {
        version: 1,
        statement,
        coverage_manifest_digest,
        proof_digest,
    })
}

pub fn evm_zkvm_proof_envelope_v1(
    witness: &crate::MixedExecutionWitnessV1,
) -> Result<StwoPlonky2ProofEnvelope, std::io::Error> {
    Ok(StwoPlonky2ProofEnvelope::EvmZkVmFixtureV1 {
        fixture: evm_zkvm_proof_fixture_v1(witness)?,
    })
}

pub fn mixed_intrablock_aggregate_fixture_v1(
    witness: &crate::MixedExecutionWitnessV1,
) -> Result<MixedIntraBlockAggregateFixtureV1, std::io::Error> {
    let native_component_statement = native_mixed_component_statement_v1(witness)?;
    let evm_zkvm_fixture = evm_zkvm_proof_fixture_v1(witness)?;
    let manifest =
        crate::coverage_manifest_for_circuit_version(crate::CircuitVersion::MixedStateTransitionV1);
    let coverage_manifest_digest = crate::coverage_manifest_digest(&manifest)?;
    let public_input_digest = digest_borsh_io(&witness.public_inputs)?;
    let witness_digest = crate::mixed_execution_witness_digest(witness)?;
    let native_component_digest = digest_borsh_io(&native_component_statement)?;
    let evm_component_digest = digest_borsh_io(&evm_zkvm_fixture)?;
    let aggregate_proof_digest = digest_borsh_io(&(
        MIXED_INTRABLOCK_AGGREGATE_FIXTURE_V1_TAG,
        public_input_digest,
        witness_digest,
        coverage_manifest_digest,
        native_component_digest,
        evm_component_digest,
        true,
    ))?;
    Ok(MixedIntraBlockAggregateFixtureV1 {
        version: 1,
        public_input_digest,
        witness_digest,
        circuit_version: crate::CircuitVersion::MixedStateTransitionV1,
        coverage_manifest_digest,
        native_component_statement,
        evm_zkvm_fixture,
        native_component_digest,
        evm_component_digest,
        aggregate_proof_digest,
        compressed: true,
    })
}

pub fn mixed_intrablock_aggregate_proof_envelope_v1(
    witness: &crate::MixedExecutionWitnessV1,
) -> Result<StwoPlonky2ProofEnvelope, std::io::Error> {
    Ok(
        StwoPlonky2ProofEnvelope::MixedIntraBlockAggregateFixtureV1 {
            fixture: mixed_intrablock_aggregate_fixture_v1(witness)?,
        },
    )
}

fn native_recursive_wrap_fixture_digest_v1(
    fixture: &NativeRecursiveWrapFixtureV1,
) -> Result<Hash256, std::io::Error> {
    Ok(keccak256(&borsh::to_vec(fixture)?))
}

pub fn native_recursive_wrap_fixture_v1(
    statement: &NativeStateTransitionStatementV1,
    proof: &crate::BlockValidityProof,
) -> Result<NativeRecursiveWrapFixtureV1, std::io::Error> {
    let public_input_digest = crate::validity_proof_public_input_digest(proof)?;
    let native_statement_digest = native_state_transition_statement_digest_v1(statement)?;
    let body = (
        NATIVE_RECURSIVE_WRAP_FIXTURE_V1_TAG,
        native_statement_digest,
        public_input_digest,
        stwo_plonky2_public_input_limbs(&public_input_digest),
        proof.circuit_version,
        proof.coverage_manifest_digest,
    );
    Ok(NativeRecursiveWrapFixtureV1 {
        version: 1,
        fixture_id: keccak256(&borsh::to_vec(&body)?),
        native_statement_digest,
        public_input_digest,
        public_input_limbs: stwo_plonky2_public_input_limbs(&public_input_digest),
        circuit_version: proof.circuit_version,
        coverage_manifest_digest: proof.coverage_manifest_digest,
    })
}

pub fn compressed_plonky2_native_proof_fixture_v1(
    recursive_fixture: &NativeRecursiveWrapFixtureV1,
    verifier_id: Hash256,
) -> Result<CompressedPlonky2NativeProofFixtureV1, std::io::Error> {
    let recursive_fixture_digest = native_recursive_wrap_fixture_digest_v1(recursive_fixture)?;
    let body = (
        NATIVE_COMPRESSED_PLONKY2_FIXTURE_V1_TAG,
        verifier_id,
        recursive_fixture_digest,
        recursive_fixture.public_input_digest,
        recursive_fixture.public_input_limbs,
        true,
    );
    Ok(CompressedPlonky2NativeProofFixtureV1 {
        version: 1,
        verifier_id,
        recursive_fixture_digest,
        public_input_digest: recursive_fixture.public_input_digest,
        public_input_limbs: recursive_fixture.public_input_limbs,
        proof_digest: keccak256(&borsh::to_vec(&body)?),
        compressed: true,
    })
}

pub fn native_recursive_proof_envelope_v1(
    statement: NativeStateTransitionStatementV1,
    proof: &crate::BlockValidityProof,
    verifier_id: Hash256,
) -> Result<StwoPlonky2ProofEnvelope, std::io::Error> {
    let recursive_fixture = native_recursive_wrap_fixture_v1(&statement, proof)?;
    let compressed_plonky2_fixture =
        compressed_plonky2_native_proof_fixture_v1(&recursive_fixture, verifier_id)?;
    Ok(StwoPlonky2ProofEnvelope::NativeRecursiveFixtureV1 {
        statement,
        recursive_fixture,
        compressed_plonky2_fixture,
    })
}

pub fn verify_native_inter_block_chain_v1(
    previous: &crate::BlockValidityProof,
    next: &crate::BlockValidityProof,
) -> Result<(), ProductionProofVerifyError> {
    if previous.circuit_version != crate::CircuitVersion::NativeStateTransitionV1
        || next.circuit_version != crate::CircuitVersion::NativeStateTransitionV1
    {
        return Err(ProductionProofVerifyError::NativeCircuitVersion);
    }
    if previous.chain_id != next.chain_id
        || previous.height.saturating_add(1) != next.height
        || previous.state_root != next.parent_state_root
    {
        return Err(ProductionProofVerifyError::NativeInterBlockChain);
    }
    Ok(())
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
        StwoPlonky2ProofEnvelope::NativeRecursiveFixtureV1 {
            statement,
            recursive_fixture,
            compressed_plonky2_fixture,
        } => verify_native_recursive_fixture(
            statement,
            recursive_fixture,
            compressed_plonky2_fixture,
            public_input_digest,
        ),
        StwoPlonky2ProofEnvelope::EvmZkVmFixtureV1 { fixture } => {
            verify_evm_zkvm_fixture(fixture, public_input_digest)
        }
        StwoPlonky2ProofEnvelope::MixedIntraBlockAggregateFixtureV1 { fixture } => {
            verify_mixed_intrablock_aggregate_fixture(fixture, public_input_digest)
        }
    }
}

fn verify_mixed_intrablock_aggregate_fixture(
    fixture: MixedIntraBlockAggregateFixtureV1,
    public_input_digest: Hash256,
) -> Result<(), ProductionProofVerifyError> {
    let manifest =
        crate::coverage_manifest_for_circuit_version(crate::CircuitVersion::MixedStateTransitionV1);
    let mixed_coverage_digest = crate::coverage_manifest_digest(&manifest)
        .map_err(|_| ProductionProofVerifyError::MixedAggregateFixture)?;
    if fixture.version != 1
        || !fixture.compressed
        || fixture.circuit_version != crate::CircuitVersion::MixedStateTransitionV1
        || fixture.coverage_manifest_digest != mixed_coverage_digest
        || fixture.public_input_digest != public_input_digest
        || fixture.native_component_statement.public_input_digest != public_input_digest
        || fixture.evm_zkvm_fixture.statement.public_input_digest != public_input_digest
        || fixture.native_component_statement.witness_digest != fixture.witness_digest
        || fixture.evm_zkvm_fixture.statement.witness_digest != fixture.witness_digest
        || !fixture
            .native_component_statement
            .feature_set
            .contains_only(manifest.covered_features)
        || !fixture
            .evm_zkvm_fixture
            .statement
            .feature_set
            .contains_only(manifest.covered_features)
    {
        return Err(ProductionProofVerifyError::MixedAggregateFixture);
    }
    if fixture.native_component_digest
        != digest_borsh_io(&fixture.native_component_statement)
            .map_err(|_| ProductionProofVerifyError::MixedAggregateFixture)?
        || fixture.evm_component_digest
            != digest_borsh_io(&fixture.evm_zkvm_fixture)
                .map_err(|_| ProductionProofVerifyError::MixedAggregateFixture)?
    {
        return Err(ProductionProofVerifyError::MixedAggregateFixture);
    }
    verify_evm_zkvm_fixture(fixture.evm_zkvm_fixture.clone(), public_input_digest)
        .map_err(|_| ProductionProofVerifyError::MixedAggregateFixture)?;
    let expected_aggregate_digest = digest_borsh_io(&(
        MIXED_INTRABLOCK_AGGREGATE_FIXTURE_V1_TAG,
        fixture.public_input_digest,
        fixture.witness_digest,
        fixture.coverage_manifest_digest,
        fixture.native_component_digest,
        fixture.evm_component_digest,
        true,
    ))
    .map_err(|_| ProductionProofVerifyError::MixedAggregateFixture)?;
    if fixture.aggregate_proof_digest != expected_aggregate_digest {
        return Err(ProductionProofVerifyError::MixedAggregateFixture);
    }
    Ok(())
}

fn verify_evm_zkvm_fixture(
    fixture: EvmZkVmProofFixtureV1,
    public_input_digest: Hash256,
) -> Result<(), ProductionProofVerifyError> {
    let manifest =
        crate::coverage_manifest_for_circuit_version(crate::CircuitVersion::MixedStateTransitionV1);
    let mixed_coverage_digest = crate::coverage_manifest_digest(&manifest)
        .map_err(|_| ProductionProofVerifyError::EvmZkVmFixture)?;
    let surface = crate::evm_zkvm_surface_v1();
    if fixture.version != 1
        || fixture.coverage_manifest_digest != mixed_coverage_digest
        || fixture.statement.version != 1
        || fixture.statement.zkvm_choice != surface.zkvm_choice
        || fixture.statement.zkvm_target_id != keccak256(surface.zkvm_target.as_bytes())
        || fixture.statement.revm_subset_digest
            != digest_borsh_io(&surface).map_err(|_| ProductionProofVerifyError::EvmZkVmFixture)?
        || fixture.statement.public_input_digest != public_input_digest
    {
        return Err(ProductionProofVerifyError::EvmZkVmFixture);
    }
    if fixture.statement.covered_features != surface.covered_features {
        return Err(ProductionProofVerifyError::EvmZkVmFixture);
    }
    let expected_proof_digest = digest_borsh_io(&(
        EVM_ZKVM_FIXTURE_V1_TAG,
        EVM_ZKVM_TRANSITION_STATEMENT_V1_TAG,
        &fixture.statement,
        fixture.coverage_manifest_digest,
    ))
    .map_err(|_| ProductionProofVerifyError::EvmZkVmFixture)?;
    if fixture.proof_digest != expected_proof_digest {
        return Err(ProductionProofVerifyError::EvmZkVmFixture);
    }
    Ok(())
}

fn verify_native_recursive_fixture(
    statement: NativeStateTransitionStatementV1,
    recursive_fixture: NativeRecursiveWrapFixtureV1,
    compressed_plonky2_fixture: CompressedPlonky2NativeProofFixtureV1,
    public_input_digest: Hash256,
) -> Result<(), ProductionProofVerifyError> {
    let manifest = crate::coverage_manifest_for_circuit_version(
        crate::CircuitVersion::NativeStateTransitionV1,
    );
    let native_coverage_digest = crate::coverage_manifest_digest(&manifest)
        .map_err(|_| ProductionProofVerifyError::NativeRecursiveFixture)?;
    if statement.version != 1
        || statement.air_id != native_state_transition_air_id()
        || statement.public_input_digest != public_input_digest
    {
        return Err(ProductionProofVerifyError::NativeRecursiveFixture);
    }
    if recursive_fixture.version != 1
        || recursive_fixture.circuit_version != crate::CircuitVersion::NativeStateTransitionV1
        || recursive_fixture.coverage_manifest_digest != native_coverage_digest
        || recursive_fixture.public_input_digest != public_input_digest
        || recursive_fixture.public_input_limbs
            != stwo_plonky2_public_input_limbs(&public_input_digest)
        || recursive_fixture.native_statement_digest
            != native_state_transition_statement_digest_v1(&statement)
                .map_err(|_| ProductionProofVerifyError::NativeRecursiveFixture)?
    {
        return Err(ProductionProofVerifyError::NativeRecursiveFixture);
    }
    let body = (
        NATIVE_RECURSIVE_WRAP_FIXTURE_V1_TAG,
        recursive_fixture.native_statement_digest,
        recursive_fixture.public_input_digest,
        recursive_fixture.public_input_limbs,
        recursive_fixture.circuit_version,
        recursive_fixture.coverage_manifest_digest,
    );
    let expected_fixture_id = keccak256(
        &borsh::to_vec(&body).map_err(|_| ProductionProofVerifyError::NativeRecursiveFixture)?,
    );
    if recursive_fixture.fixture_id != expected_fixture_id {
        return Err(ProductionProofVerifyError::NativeRecursiveFixture);
    }
    let recursive_fixture_digest = native_recursive_wrap_fixture_digest_v1(&recursive_fixture)
        .map_err(|_| ProductionProofVerifyError::NativeRecursiveFixture)?;
    if compressed_plonky2_fixture.version != 1
        || !compressed_plonky2_fixture.compressed
        || compressed_plonky2_fixture.recursive_fixture_digest != recursive_fixture_digest
        || compressed_plonky2_fixture.public_input_digest != public_input_digest
        || compressed_plonky2_fixture.public_input_limbs
            != stwo_plonky2_public_input_limbs(&public_input_digest)
    {
        return Err(ProductionProofVerifyError::NativeRecursiveFixture);
    }
    let compressed_body = (
        NATIVE_COMPRESSED_PLONKY2_FIXTURE_V1_TAG,
        compressed_plonky2_fixture.verifier_id,
        compressed_plonky2_fixture.recursive_fixture_digest,
        compressed_plonky2_fixture.public_input_digest,
        compressed_plonky2_fixture.public_input_limbs,
        true,
    );
    let expected_proof_digest = keccak256(
        &borsh::to_vec(&compressed_body)
            .map_err(|_| ProductionProofVerifyError::NativeRecursiveFixture)?,
    );
    if compressed_plonky2_fixture.proof_digest != expected_proof_digest {
        return Err(ProductionProofVerifyError::NativeRecursiveFixture);
    }
    Ok(())
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
