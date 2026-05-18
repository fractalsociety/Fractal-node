//! Verified STWO artifact adapters for tier-2 Plonky2 aggregation.

use fractal_proof_condenser::CheckpointStwoArtifactV1;
use fractal_shard::ProofSubmissionV1;

use crate::aggregate::AggregatorError;
use crate::statement::VerifiedStwoStatementV1;

/// Decode and verify a tier-1 STWO artifact, then expose its public statement for Plonky2.
pub fn verify_stwo_artifact_submission(
    sub: &ProofSubmissionV1,
    artifact_v1_borsh: &[u8],
) -> Result<VerifiedStwoStatementV1, AggregatorError> {
    let artifact = CheckpointStwoArtifactV1::from_bytes(artifact_v1_borsh)
        .map_err(|e| AggregatorError::StwoArtifactInvalid(e.to_string()))?;
    if artifact.job.start_block != sub.start_block || artifact.job.end_block != sub.end_block {
        return Err(AggregatorError::VerifiedStatementMismatch);
    }
    artifact
        .verify(&sub.proof_digest)
        .map_err(|e| AggregatorError::StwoArtifactInvalid(e.to_string()))?;
    Ok(VerifiedStwoStatementV1::from_checkpoint_job(
        sub.shard_id,
        &artifact.job,
        sub.proof_digest,
    ))
}
