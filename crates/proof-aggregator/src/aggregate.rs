//! `globalZkRoot` via Plonky2 recursive SNARK (Poseidon statement hash).

use fractal_crypto::hash::Hash256;
use fractal_shard::ProofSubmissionV1;
use thiserror::Error;

use crate::plonky2_agg::{
    hash_out_to_global_zk_root, poseidon_statement_digest, poseidon_verified_statement_digest,
    prove_global_aggregation, prove_global_aggregation_verified,
};
use crate::statement::{MAX_AGG_PROOFS, VerifiedStwoStatementV1};

/// Plonky2 aggregation wire version (Poseidon + SNARK verify).
pub const PLONKY2_AGGREGATOR_VERSION: u8 = 2;

/// Public inputs committed in a masterchain block (§7.10.3).
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct GlobalZkStatementV1 {
    pub global_state_root: Hash256,
    pub global_zk_root: Hash256,
    pub validity_proofs: Vec<ProofSubmissionV1>,
    pub verified_stwo_statements: Vec<VerifiedStwoStatementV1>,
}

/// Tier-2 aggregation output (on-chain root + off-chain SNARK bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregatedZkProofV1 {
    pub global_zk_root: Hash256,
    pub snark_bytes: Vec<u8>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum AggregatorError {
    #[error("unsupported aggregator version {0}")]
    UnsupportedVersion(u8),
    #[error("global_zk_root mismatch")]
    RootMismatch,
    #[error("no proofs to aggregate")]
    EmptyProofs,
    #[error("too many proofs (max {MAX_AGG_PROOFS})")]
    TooManyProofs,
    #[error("plonky2 prove failed: {0}")]
    ProveFailed(String),
    #[error("plonky2 verify failed: {0}")]
    VerifyFailed(String),
    #[error("verified STWO statement mismatch")]
    VerifiedStatementMismatch,
    #[error("invalid STWO artifact: {0}")]
    StwoArtifactInvalid(String),
}

/// Prove tier-2 aggregation and return SNARK bytes + `global_zk_root`.
pub fn prove_and_aggregate(
    masterchain_height: u64,
    global_state_root: &Hash256,
    proofs: &[ProofSubmissionV1],
) -> Result<AggregatedZkProofV1, AggregatorError> {
    if proofs.is_empty() {
        return Err(AggregatorError::EmptyProofs);
    }
    if proofs.len() > MAX_AGG_PROOFS {
        return Err(AggregatorError::TooManyProofs);
    }
    let (global_zk_root, snark_bytes) =
        prove_global_aggregation(masterchain_height, global_state_root, proofs)
            .map_err(|e| AggregatorError::ProveFailed(e.to_string()))?;
    Ok(AggregatedZkProofV1 {
        global_zk_root,
        snark_bytes,
    })
}

/// Prove tier-2 aggregation over STWO statements that were verified before circuit assignment.
pub fn prove_and_aggregate_verified(
    masterchain_height: u64,
    global_state_root: &Hash256,
    proofs: &[ProofSubmissionV1],
    verified: &[VerifiedStwoStatementV1],
) -> Result<AggregatedZkProofV1, AggregatorError> {
    if proofs.is_empty() {
        return Err(AggregatorError::EmptyProofs);
    }
    if proofs.len() > MAX_AGG_PROOFS {
        return Err(AggregatorError::TooManyProofs);
    }
    ensure_verified_statements_match(proofs, verified)?;
    let (global_zk_root, snark_bytes) =
        prove_global_aggregation_verified(masterchain_height, global_state_root, proofs, verified)
            .map_err(|e| AggregatorError::ProveFailed(e.to_string()))?;
    Ok(AggregatedZkProofV1 {
        global_zk_root,
        snark_bytes,
    })
}

/// Bind tier-1 digests into a single masterchain `global_zk_root` (runs Plonky2 prover).
pub fn aggregate_global_zk_root(
    masterchain_height: u64,
    global_state_root: &Hash256,
    proofs: &[ProofSubmissionV1],
) -> Result<Hash256, AggregatorError> {
    Ok(prove_and_aggregate(masterchain_height, global_state_root, proofs)?.global_zk_root)
}

/// Light-client check: recompute Poseidon digest and optional SNARK verify.
pub fn verify_global_zk_root(
    masterchain_height: u64,
    global_state_root: &Hash256,
    proofs: &[ProofSubmissionV1],
    expected: &Hash256,
    snark_bytes: Option<&[u8]>,
) -> Result<(), AggregatorError> {
    if proofs.is_empty() {
        if expected == &[0u8; 32] {
            return Ok(());
        }
        return Err(AggregatorError::RootMismatch);
    }
    if let Some(bytes) = snark_bytes {
        let root = crate::plonky2_agg::verify_global_aggregation_snark(
            bytes,
            masterchain_height,
            global_state_root,
            proofs,
        )
        .map_err(|e| AggregatorError::VerifyFailed(e.to_string()))?;
        if &root != expected {
            return Err(AggregatorError::RootMismatch);
        }
        return Ok(());
    }
    let digest = poseidon_statement_digest(masterchain_height, global_state_root, proofs);
    let got = hash_out_to_global_zk_root(&digest);
    if &got != expected {
        return Err(AggregatorError::RootMismatch);
    }
    Ok(())
}

/// Light-client check for the verified-STWO Plonky2 circuit.
pub fn verify_global_zk_root_verified(
    masterchain_height: u64,
    global_state_root: &Hash256,
    proofs: &[ProofSubmissionV1],
    verified: &[VerifiedStwoStatementV1],
    expected: &Hash256,
    snark_bytes: Option<&[u8]>,
) -> Result<(), AggregatorError> {
    if proofs.is_empty() {
        if expected == &[0u8; 32] {
            return Ok(());
        }
        return Err(AggregatorError::RootMismatch);
    }
    ensure_verified_statements_match(proofs, verified)?;
    if let Some(bytes) = snark_bytes {
        let root = crate::plonky2_agg::verify_global_aggregation_verified_snark(
            bytes,
            masterchain_height,
            global_state_root,
            proofs,
            verified,
        )
        .map_err(|e| AggregatorError::VerifyFailed(e.to_string()))?;
        if &root != expected {
            return Err(AggregatorError::RootMismatch);
        }
        return Ok(());
    }
    let digest =
        poseidon_verified_statement_digest(masterchain_height, global_state_root, proofs, verified);
    let got = hash_out_to_global_zk_root(&digest);
    if &got != expected {
        return Err(AggregatorError::RootMismatch);
    }
    Ok(())
}

pub(crate) fn ensure_verified_statements_match(
    proofs: &[ProofSubmissionV1],
    verified: &[VerifiedStwoStatementV1],
) -> Result<(), AggregatorError> {
    if proofs.len() != verified.len()
        || proofs
            .iter()
            .any(|p| !verified.iter().any(|s| s.matches_submission(p)))
    {
        return Err(AggregatorError::VerifiedStatementMismatch);
    }
    Ok(())
}
