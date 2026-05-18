//! Consensus misbehavior evidence (`docs/prd.md` §12.2) verified before permissionless slash.
//!
//! [`ConsensusMisbehaviorEvidenceV1`] bundles cryptographically checkable faults:
//! double-vote, conflicting QC, or timeout/QC equivocation. On-chain slashing consumes
//! `keccak256(borsh(evidence))` once verification succeeds.

use borsh::{BorshDeserialize, BorshSerialize};
use fractal_crypto::hash::{keccak256, Hash256};
use thiserror::Error;

use crate::timeout::Timeout;
use crate::validators::{ValidatorEntry, ValidatorId, ValidatorSet};
use crate::vote::{verify_formed_qc, FormedQc, Vote};

/// Kind tag for indexers / explorers.
#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum MisbehaviorKind {
    DoubleVote,
    ConflictingQc,
    TimeoutEquivocation,
}

/// Wire evidence for permissionless [`NativeCall::SlashConsensusStakeVerified`].
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum ConsensusMisbehaviorEvidenceV1 {
    /// Two distinct votes from the same validator at the same `(view, height)` but different `header_hash`.
    DoubleVote {
        offender_fingerprint: ValidatorId,
        vote_a: Vote,
        vote_b: Vote,
    },
    /// Two distinct formed QCs at the same `(view, block_height)` certifying different headers; offender must appear in both signer sets.
    ConflictingQc {
        offender_fingerprint: ValidatorId,
        qc_a: FormedQc,
        qc_b: FormedQc,
    },
    /// Two timeouts for the same `view` with different `high_qc` from the same validator.
    TimeoutEquivocation {
        offender_fingerprint: ValidatorId,
        timeout_a: Timeout,
        timeout_b: Timeout,
    },
}

#[derive(Debug, Error)]
pub enum MisbehaviorError {
    #[error("validator fingerprint does not match evidence offender")]
    FingerprintMismatch,
    #[error("misbehavior evidence is malformed or inconsistent")]
    InvalidEvidence,
    #[error("validator index {0} is out of range for the active set")]
    ValidatorIndexOutOfRange(u32),
}

impl ConsensusMisbehaviorEvidenceV1 {
    #[must_use]
    pub fn kind(&self) -> MisbehaviorKind {
        match self {
            Self::DoubleVote { .. } => MisbehaviorKind::DoubleVote,
            Self::ConflictingQc { .. } => MisbehaviorKind::ConflictingQc,
            Self::TimeoutEquivocation { .. } => MisbehaviorKind::TimeoutEquivocation,
        }
    }

    /// `keccak256(borsh(self))` — replay key for on-chain consumption.
    pub fn evidence_hash(&self) -> Result<Hash256, std::io::Error> {
        Ok(keccak256(&borsh::to_vec(self)?))
    }

    #[must_use]
    pub fn declared_offender_fingerprint(&self) -> ValidatorId {
        match self {
            Self::DoubleVote {
                offender_fingerprint,
                ..
            }
            | Self::ConflictingQc {
                offender_fingerprint,
                ..
            }
            | Self::TimeoutEquivocation {
                offender_fingerprint,
                ..
            } => *offender_fingerprint,
        }
    }

    fn offender_index(&self, validators: &ValidatorSet) -> Result<u32, MisbehaviorError> {
        let fp = self.declared_offender_fingerprint();
        validators
            .entries()
            .iter()
            .position(|e| e.fingerprint == fp)
            .map(|i| i as u32)
            .ok_or(MisbehaviorError::InvalidEvidence)
    }
}

/// Verify evidence and return its canonical hash for replay protection.
pub fn verify_consensus_misbehavior_evidence(
    evidence: &ConsensusMisbehaviorEvidenceV1,
    validators: &ValidatorSet,
    stake_weights: Option<&[u128]>,
    expected_fingerprint: &ValidatorId,
) -> Result<Hash256, MisbehaviorError> {
    if evidence.declared_offender_fingerprint() != *expected_fingerprint {
        return Err(MisbehaviorError::FingerprintMismatch);
    }
    let offender_idx = evidence.offender_index(validators)?;
    match evidence {
        ConsensusMisbehaviorEvidenceV1::DoubleVote { vote_a, vote_b, .. } => {
            if vote_a.validator_index != offender_idx || vote_b.validator_index != offender_idx {
                return Err(MisbehaviorError::InvalidEvidence);
            }
            if vote_a.validator_index != vote_b.validator_index {
                return Err(MisbehaviorError::InvalidEvidence);
            }
            if vote_a.view != vote_b.view || vote_a.height != vote_b.height {
                return Err(MisbehaviorError::InvalidEvidence);
            }
            if vote_a.header_hash == vote_b.header_hash {
                return Err(MisbehaviorError::InvalidEvidence);
            }
            vote_a
                .verify_against_validator_set(validators)
                .map_err(|_| MisbehaviorError::InvalidEvidence)?;
            vote_b
                .verify_against_validator_set(validators)
                .map_err(|_| MisbehaviorError::InvalidEvidence)?;
        }
        ConsensusMisbehaviorEvidenceV1::ConflictingQc { qc_a, qc_b, .. } => {
            if qc_a.qc.view != qc_b.qc.view || qc_a.qc.block_height != qc_b.qc.block_height {
                return Err(MisbehaviorError::InvalidEvidence);
            }
            if qc_a.qc.block_header_hash == qc_b.qc.block_header_hash {
                return Err(MisbehaviorError::InvalidEvidence);
            }
            if !qc_a.signer_indices.contains(&offender_idx)
                || !qc_b.signer_indices.contains(&offender_idx)
            {
                return Err(MisbehaviorError::InvalidEvidence);
            }
            verify_formed_qc(qc_a, validators, stake_weights)
                .map_err(|_| MisbehaviorError::InvalidEvidence)?;
            verify_formed_qc(qc_b, validators, stake_weights)
                .map_err(|_| MisbehaviorError::InvalidEvidence)?;
        }
        ConsensusMisbehaviorEvidenceV1::TimeoutEquivocation {
            timeout_a,
            timeout_b,
            ..
        } => {
            if timeout_a.validator_index != offender_idx || timeout_b.validator_index != offender_idx {
                return Err(MisbehaviorError::InvalidEvidence);
            }
            if timeout_a.validator_index != timeout_b.validator_index {
                return Err(MisbehaviorError::InvalidEvidence);
            }
            if timeout_a.view != timeout_b.view {
                return Err(MisbehaviorError::InvalidEvidence);
            }
            if crate::qc::hash_qc(&timeout_a.high_qc).ok()
                == crate::qc::hash_qc(&timeout_b.high_qc).ok()
            {
                return Err(MisbehaviorError::InvalidEvidence);
            }
            timeout_a
                .verify_against_validator_set(validators)
                .map_err(|_| MisbehaviorError::InvalidEvidence)?;
            timeout_b
                .verify_against_validator_set(validators)
                .map_err(|_| MisbehaviorError::InvalidEvidence)?;
        }
    }
    evidence
        .evidence_hash()
        .map_err(|_| MisbehaviorError::InvalidEvidence)
}

/// Build a [`ValidatorSet`] from on-chain registry rows (sorted by fingerprint).
pub fn validator_set_from_registry(rows: &[([u8; 32], [u8; 48])]) -> Result<ValidatorSet, MisbehaviorError> {
    let mut entries: Vec<ValidatorEntry> = Vec::with_capacity(rows.len());
    for (fp, pk_bytes) in rows {
        entries.push(ValidatorEntry {
            fingerprint: *fp,
            bls_pubkey: fractal_crypto::BlsPublicKey(*pk_bytes),
        });
    }
    Ok(ValidatorSet::from_entries(entries))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::qc::singleton_qc_certifying;
    use crate::vote::{VotePool, VoteSignBody};

    #[test]
    fn double_vote_evidence_verifies() {
        let set = ValidatorSet::phase2_bft7_fixture();
        let idx = 2u32;
        let sk = set.dev_bls_secret(idx as usize).unwrap();
        let body_a = VoteSignBody {
            view: 4,
            height: 10,
            header_hash: [0xAA; 32],
        };
        let body_b = VoteSignBody {
            view: 4,
            height: 10,
            header_hash: [0xBB; 32],
        };
        let vote_a = Vote::sign(body_a, idx, &sk);
        let vote_b = Vote::sign(body_b, idx, &sk);
        let fp = set.entry(idx as usize).unwrap().fingerprint;
        let ev = ConsensusMisbehaviorEvidenceV1::DoubleVote {
            offender_fingerprint: fp,
            vote_a,
            vote_b,
        };
        verify_consensus_misbehavior_evidence(&ev, &set, None, &fp).unwrap();
    }

    #[test]
    fn conflicting_qc_evidence_verifies() {
        let set = ValidatorSet::phase2_bft7_fixture();
        let idx = 1u32;
        let mut pool = VotePool::new();
        for (hh, h) in [([0x11; 32], 9u64), ([0x22; 32], 9u64)] {
            for i in 0..set.quorum_threshold() {
                let ski = set.dev_bls_secret(i).unwrap();
                let vote = Vote::sign(
                    VoteSignBody {
                        view: 3,
                        height: h,
                        header_hash: hh,
                    },
                    i as u32,
                    &ski,
                );
                let _ = pool.record(vote, &set, None);
            }
        }
        let qc_a = pool
            .try_form_qc(3, 9, [0x11; 32], &set, None)
            .expect("qc a");
        let qc_b = pool
            .try_form_qc(3, 9, [0x22; 32], &set, None)
            .expect("qc b");
        let fp = set.entry(idx as usize).unwrap().fingerprint;
        let ev = ConsensusMisbehaviorEvidenceV1::ConflictingQc {
            offender_fingerprint: fp,
            qc_a,
            qc_b,
        };
        verify_consensus_misbehavior_evidence(&ev, &set, None, &fp).unwrap();
    }

    #[test]
    fn timeout_equivocation_evidence_verifies() {
        let set = ValidatorSet::phase2_bft7_fixture();
        let idx = 0u32;
        let sk = set.dev_bls_secret(0).unwrap();
        let qc_lo = singleton_qc_certifying([0x01; 32], 5, 2);
        let qc_hi = singleton_qc_certifying([0x02; 32], 8, 2);
        let t_a = Timeout::sign(
            crate::timeout::TimeoutSignBody {
                view: 7,
                high_qc: qc_lo,
            },
            idx,
            &sk,
        );
        let t_b = Timeout::sign(
            crate::timeout::TimeoutSignBody {
                view: 7,
                high_qc: qc_hi,
            },
            idx,
            &sk,
        );
        let fp = set.entry(0).unwrap().fingerprint;
        let ev = ConsensusMisbehaviorEvidenceV1::TimeoutEquivocation {
            offender_fingerprint: fp,
            timeout_a: t_a,
            timeout_b: t_b,
        };
        verify_consensus_misbehavior_evidence(&ev, &set, None, &fp).unwrap();
    }
}
