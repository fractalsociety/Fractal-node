//! Tier-1 [`ProofSubmissionV1`] acceptance rules (masterchain verifier sketch).

use fractal_shard::{ProofSubmissionV1, ShardAnchor};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum SubmissionError {
    #[error("empty proof digest")]
    EmptyDigest,
    #[error("invalid block range")]
    InvalidRange,
    #[error("unknown shard {0}")]
    UnknownShard(u32),
    #[error("end_block {end} exceeds anchor height {anchor} for shard {shard}")]
    RangeExceedsAnchor { shard: u32, end: u64, anchor: u64 },
    #[error("duplicate submission for shard {shard} range [{start},{end}]")]
    Duplicate { shard: u32, start: u64, end: u64 },
}

/// Build a tier-1 submission from a checkpoint / STWO digest (§6.2.1).
#[must_use]
pub fn proof_submission_from_checkpoint_digest(
    shard_id: u32,
    start_block: u64,
    end_block: u64,
    prover: [u8; 20],
    proof_digest: [u8; 32],
    lag_seconds: u32,
) -> ProofSubmissionV1 {
    ProofSubmissionV1 {
        shard_id,
        start_block,
        end_block,
        prover,
        lag_seconds,
        proof_digest,
    }
}

/// Validate a proof submission against the latest shard anchors on this masterchain tick.
pub fn validate_proof_submission(
    sub: &ProofSubmissionV1,
    anchors: &[ShardAnchor],
) -> Result<(), SubmissionError> {
    if sub.proof_digest == [0u8; 32] {
        return Err(SubmissionError::EmptyDigest);
    }
    if sub.start_block > sub.end_block {
        return Err(SubmissionError::InvalidRange);
    }
    let anchor = anchors
        .iter()
        .find(|a| a.shard_id == sub.shard_id)
        .ok_or(SubmissionError::UnknownShard(sub.shard_id))?;
    if sub.end_block > anchor.block_height {
        return Err(SubmissionError::RangeExceedsAnchor {
            shard: sub.shard_id,
            end: sub.end_block,
            anchor: anchor.block_height,
        });
    }
    Ok(())
}

/// Reject duplicates in a batch (same shard + range).
pub fn dedupe_submissions(
    subs: &[ProofSubmissionV1],
) -> Result<Vec<ProofSubmissionV1>, SubmissionError> {
    let mut out = Vec::with_capacity(subs.len());
    for s in subs {
        if out.iter().any(|p: &ProofSubmissionV1| {
            p.shard_id == s.shard_id && p.start_block == s.start_block && p.end_block == s.end_block
        }) {
            return Err(SubmissionError::Duplicate {
                shard: s.shard_id,
                start: s.start_block,
                end: s.end_block,
            });
        }
        out.push(s.clone());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor(shard: u32, h: u64) -> ShardAnchor {
        ShardAnchor {
            shard_id: shard,
            block_height: h,
            state_root: [0u8; 32],
            witness_commitment: [0u8; 32],
        }
    }

    #[test]
    fn rejects_range_past_anchor() {
        let sub = proof_submission_from_checkpoint_digest(0, 1, 200, [0xaa; 20], [1u8; 32], 0);
        let err = validate_proof_submission(&sub, &[anchor(0, 100)]).unwrap_err();
        assert!(matches!(err, SubmissionError::RangeExceedsAnchor { .. }));
    }
}
