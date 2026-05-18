//! On-chain consensus misbehavior slashing (`docs/prd.md` §12.2).

use fractal_bft_wire::{
    verify_consensus_misbehavior_evidence, validator_set_from_registry,
    ConsensusMisbehaviorEvidenceV1, MisbehaviorError,
};
use fractal_crypto::Hash256;

use crate::consensus_stake::permissionless_validator_entries;
use crate::error::ExecError;
use crate::state::State;
use crate::validator_stake_weights;

/// Active validator set for evidence verification (permissionless registry + min stake).
pub fn validator_set_for_slashing(state: &State) -> Result<fractal_bft_wire::ValidatorSet, ExecError> {
    let rows = permissionless_validator_entries(state);
    if rows.is_empty() {
        return Err(ExecError::ValidatorNotRegistered);
    }
    validator_set_from_registry(&rows).map_err(|_| ExecError::InvalidMisbehaviorEvidence)
}

/// Deserialize evidence and return its canonical replay hash.
pub fn misbehavior_evidence_hash(evidence_borsh: &[u8]) -> Result<Hash256, ExecError> {
    let evidence: ConsensusMisbehaviorEvidenceV1 =
        borsh::from_slice(evidence_borsh).map_err(|_| ExecError::InvalidMisbehaviorEvidence)?;
    evidence
        .evidence_hash()
        .map_err(|_| ExecError::InvalidMisbehaviorEvidence)
}

/// Verify `evidence_borsh` against the active permissionless validator set.
pub fn verify_slashing_evidence_borsh(
    state: &State,
    validator_fingerprint: &[u8; 32],
    evidence_borsh: &[u8],
) -> Result<(), ExecError> {
    let evidence: ConsensusMisbehaviorEvidenceV1 =
        borsh::from_slice(evidence_borsh).map_err(|_| ExecError::InvalidMisbehaviorEvidence)?;
    let validators = validator_set_for_slashing(state)?;
    let fps: Vec<[u8; 32]> = validators.ids();
    let weights = validator_stake_weights(state, &fps);
    let stake_slice: Option<Vec<u128>> = if weights.iter().all(|w| *w == 0) {
        None
    } else {
        Some(weights)
    };
    verify_consensus_misbehavior_evidence(
        &evidence,
        &validators,
        stake_slice.as_deref(),
        validator_fingerprint,
    )
    .map_err(map_misbehavior_err)?;
    Ok(())
}

fn map_misbehavior_err(e: MisbehaviorError) -> ExecError {
    match e {
        MisbehaviorError::FingerprintMismatch => ExecError::MisbehaviorFingerprintMismatch,
        _ => ExecError::InvalidMisbehaviorEvidence,
    }
}
