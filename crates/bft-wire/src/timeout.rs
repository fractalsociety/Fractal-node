//! Local timeout messages + timeout pool for view liveness (`docs/prd.md` §7.4, §18 M7-f).
//!
//! Each [`Timeout`] signs a [`TimeoutSignBody`] with the timed-out `view` plus the replica's
//! highest prepare-phase [`QuorumCertificate`] (`high_qc`). Slots are keyed by
//! `(view, hash_qc(high_qc))`. [`TimeoutPool::try_form_best_timeout_cert_for_view`] selects the
//! strongest quorum among all slots for a `view` (highest [`high_qc_rank`]) so nodes can advance
//! after a partition without sharing the same local `high_prepare_qc` hash.

use std::collections::{BTreeMap, BTreeSet};

use borsh::{BorshDeserialize, BorshSerialize};
use fractal_crypto::hash::Hash256;
use fractal_crypto::{AggregateSignature, BlsError, BlsPublicKey, BlsSecretKey, BlsSignature};
use thiserror::Error;

use crate::qc::{hash_qc, QuorumCertificate};
use crate::validators::ValidatorSet;

/// Lexicographic rank of a prepare QC for comparing timeout branches (`block_height`, then `view`).
#[must_use]
pub fn high_qc_rank(qc: &QuorumCertificate) -> (u64, u64) {
    (qc.block_height, qc.view)
}

/// Body signed for a view-timeout: stuck `view` plus the signer's highest seen prepare QC.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct TimeoutSignBody {
    pub view: u64,
    pub high_qc: QuorumCertificate,
}

impl TimeoutSignBody {
    pub fn sign_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("TimeoutSignBody borsh serialization")
    }
}

/// A validator signals it has timed out waiting for progress in `view`, binding `high_qc`.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Timeout {
    pub view: u64,
    pub high_qc: QuorumCertificate,
    pub validator_index: u32,
    pub signature: BlsSignature,
}

#[derive(Debug, Error)]
pub enum TimeoutError {
    #[error("validator_index {idx} out of range (validator set size {n})")]
    ValidatorIndexOutOfRange { idx: u32, n: usize },
    #[error("validator_index {0} has no BLS public key in this set")]
    NoPublicKey(u32),
    #[error("timeout cert has {got} signers but quorum threshold is {need}")]
    InsufficientSigners { got: usize, need: usize },
    #[error("duplicate signer index {0} in timeout certificate")]
    DuplicateSignerIndex(u32),
    #[error(transparent)]
    Bls(#[from] BlsError),
}

impl Timeout {
    pub fn sign(body: TimeoutSignBody, validator_index: u32, sk: &BlsSecretKey) -> Self {
        let signature = sk.sign(&body.sign_bytes());
        Self {
            view: body.view,
            high_qc: body.high_qc,
            validator_index,
            signature,
        }
    }

    #[must_use]
    pub fn sign_body(&self) -> TimeoutSignBody {
        TimeoutSignBody {
            view: self.view,
            high_qc: self.high_qc.clone(),
        }
    }

    pub fn verify(&self, pubkey: &BlsPublicKey) -> Result<(), TimeoutError> {
        self.signature
            .verify(&self.sign_body().sign_bytes(), pubkey)
            .map_err(TimeoutError::from)?;
        Ok(())
    }

    pub fn verify_against_validator_set(&self, set: &ValidatorSet) -> Result<(), TimeoutError> {
        let n = set.len();
        let idx_usize = self.validator_index as usize;
        if idx_usize >= n {
            return Err(TimeoutError::ValidatorIndexOutOfRange {
                idx: self.validator_index,
                n,
            });
        }
        let pk = set
            .bls_pubkey(idx_usize)
            .ok_or(TimeoutError::NoPublicKey(self.validator_index))?;
        self.verify(pk)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordTimeoutOutcome {
    Accepted,
    ReachedQuorum,
    DuplicateValidator,
    OutOfRange,
    BadSignature,
}

/// Quorum BLS aggregate over [`TimeoutSignBody`] for one `(timeout_view, high_qc_hash)` slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormedTimeoutCert {
    pub view: u64,
    pub high_qc: QuorumCertificate,
    pub aggregate_sig: AggregateSignature,
    pub signer_indices: Vec<u32>,
}

type TimeoutSlotKey = (u64, Hash256);

#[derive(Debug, Clone, Default)]
pub struct TimeoutPool {
    entries: BTreeMap<TimeoutSlotKey, BTreeMap<u32, Timeout>>,
}

impl TimeoutPool {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&mut self, t: Timeout, validators: &ValidatorSet) -> RecordTimeoutOutcome {
        let n = validators.len();
        let idx_usize = t.validator_index as usize;
        if idx_usize >= n {
            return RecordTimeoutOutcome::OutOfRange;
        }
        let Some(pk) = validators.bls_pubkey(idx_usize) else {
            return RecordTimeoutOutcome::OutOfRange;
        };
        if t.verify(pk).is_err() {
            return RecordTimeoutOutcome::BadSignature;
        }
        let Ok(hqh) = hash_qc(&t.high_qc) else {
            return RecordTimeoutOutcome::BadSignature;
        };
        let key = (t.view, hqh);
        let slot = self.entries.entry(key).or_default();
        if slot.contains_key(&t.validator_index) {
            return RecordTimeoutOutcome::DuplicateValidator;
        }
        let threshold = validators.quorum_threshold();
        let prev_count = slot.len();
        slot.insert(t.validator_index, t);
        let new_count = slot.len();
        if prev_count < threshold && new_count >= threshold {
            RecordTimeoutOutcome::ReachedQuorum
        } else {
            RecordTimeoutOutcome::Accepted
        }
    }

    #[must_use]
    pub fn count(&self, view: u64, high_qc_hash: Hash256) -> usize {
        self.entries
            .get(&(view, high_qc_hash))
            .map_or(0, BTreeMap::len)
    }

    pub fn try_form_timeout_cert(
        &self,
        view: u64,
        high_qc_hash: Hash256,
        validators: &ValidatorSet,
    ) -> Option<FormedTimeoutCert> {
        let slot = self.entries.get(&(view, high_qc_hash))?;
        if slot.len() < validators.quorum_threshold() {
            return None;
        }
        let mut it = slot.values();
        let first = it.next()?;
        for other in it {
            if other.high_qc != first.high_qc {
                return None;
            }
        }
        let signer_indices: Vec<u32> = slot.keys().copied().collect();
        let sigs: Vec<BlsSignature> = slot.values().map(|t| t.signature).collect();
        let aggregate_sig = AggregateSignature::from_signatures(&sigs).ok()?;
        Some(FormedTimeoutCert {
            view,
            high_qc: first.high_qc.clone(),
            aggregate_sig,
            signer_indices,
        })
    }

    /// Best quorum timeout certificate for `view`: maximal [`high_qc_rank`] among slots that
    /// already meet `quorum_threshold`; tie-break on larger `hash_qc(high_qc)`.
    pub fn try_form_best_timeout_cert_for_view(
        &self,
        view: u64,
        validators: &ValidatorSet,
    ) -> Option<FormedTimeoutCert> {
        let th = validators.quorum_threshold();
        let mut best: Option<(FormedTimeoutCert, Hash256)> = None;
        for ((v, hqh), slot) in &self.entries {
            if *v != view || slot.len() < th {
                continue;
            }
            let Some(formed) = self.try_form_timeout_cert(view, *hqh, validators) else {
                continue;
            };
            if verify_formed_timeout_cert(&formed, validators).is_err() {
                continue;
            }
            let rank = high_qc_rank(&formed.high_qc);
            let replace = match &best {
                None => true,
                Some((b, bh)) => {
                    let br = high_qc_rank(&b.high_qc);
                    rank > br || (rank == br && *hqh > *bh)
                }
            };
            if replace {
                best = Some((formed, *hqh));
            }
        }
        best.map(|(f, _)| f)
    }

    pub fn prune_view(&mut self, view: u64) {
        self.entries.retain(|(v, _), _| *v != view);
    }

    pub fn prune_views_before(&mut self, min_keep_view: u64) {
        self.entries.retain(|(v, _), _| *v >= min_keep_view);
    }
}

pub fn verify_formed_timeout_cert(
    formed: &FormedTimeoutCert,
    validators: &ValidatorSet,
) -> Result<(), TimeoutError> {
    let threshold = validators.quorum_threshold();
    if formed.signer_indices.len() < threshold {
        return Err(TimeoutError::InsufficientSigners {
            got: formed.signer_indices.len(),
            need: threshold,
        });
    }
    let mut seen = BTreeSet::new();
    let mut pubkeys: Vec<BlsPublicKey> = Vec::with_capacity(formed.signer_indices.len());
    for idx in &formed.signer_indices {
        if !seen.insert(*idx) {
            return Err(TimeoutError::DuplicateSignerIndex(*idx));
        }
        let i = *idx as usize;
        if i >= validators.len() {
            return Err(TimeoutError::ValidatorIndexOutOfRange {
                idx: *idx,
                n: validators.len(),
            });
        }
        let pk = validators
            .bls_pubkey(i)
            .ok_or(TimeoutError::NoPublicKey(*idx))?;
        pubkeys.push(*pk);
    }
    let body = TimeoutSignBody {
        view: formed.view,
        high_qc: formed.high_qc.clone(),
    };
    formed.aggregate_sig.verify(&body.sign_bytes(), &pubkeys)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genesis_parent_qc;
    use crate::singleton_qc_certifying;
    use crate::ValidatorSet;

    #[test]
    fn timeout_pool_reaches_quorum_and_verifies() {
        let vset = ValidatorSet::phase2_bft7_fixture();
        let mut pool = TimeoutPool::new();
        let view = 3u64;
        let hq = genesis_parent_qc();
        for i in 0u32..5 {
            let sk = vset.dev_bls_secret(i as usize).expect("bft7 dev key");
            let t = Timeout::sign(
                TimeoutSignBody {
                    view,
                    high_qc: hq.clone(),
                },
                i,
                &sk,
            );
            let out = pool.record(t, &vset);
            if i < 4 {
                assert!(
                    matches!(out, RecordTimeoutOutcome::Accepted),
                    "i={i} out={out:?}"
                );
            } else {
                assert_eq!(out, RecordTimeoutOutcome::ReachedQuorum);
            }
        }
        let hqh = hash_qc(&hq).unwrap();
        let formed = pool
            .try_form_timeout_cert(view, hqh, &vset)
            .expect("formed cert");
        verify_formed_timeout_cert(&formed, &vset).expect("verify");
    }

    #[test]
    fn duplicate_validator_rejected() {
        let vset = ValidatorSet::phase2_bft7_fixture();
        let mut pool = TimeoutPool::new();
        let view = 0u64;
        let hq = genesis_parent_qc();
        let sk = vset.dev_bls_secret(0).unwrap();
        let t = Timeout::sign(
            TimeoutSignBody {
                view,
                high_qc: hq.clone(),
            },
            0,
            &sk,
        );
        assert_eq!(
            pool.record(t.clone(), &vset),
            RecordTimeoutOutcome::Accepted
        );
        assert_eq!(
            pool.record(t, &vset),
            RecordTimeoutOutcome::DuplicateValidator
        );
    }

    #[test]
    fn different_high_qc_splits_slots() {
        let vset = ValidatorSet::phase2_bft7_fixture();
        let mut pool = TimeoutPool::new();
        let view = 1u64;
        let hq_a = genesis_parent_qc();
        let mut hq_b = genesis_parent_qc();
        hq_b.view = 99;
        let sk0 = vset.dev_bls_secret(0).unwrap();
        let sk1 = vset.dev_bls_secret(1).unwrap();
        pool.record(
            Timeout::sign(
                TimeoutSignBody {
                    view,
                    high_qc: hq_a.clone(),
                },
                0,
                &sk0,
            ),
            &vset,
        );
        pool.record(
            Timeout::sign(
                TimeoutSignBody {
                    view,
                    high_qc: hq_b.clone(),
                },
                1,
                &sk1,
            ),
            &vset,
        );
        assert_eq!(pool.count(view, hash_qc(&hq_a).unwrap()), 1);
        assert_eq!(pool.count(view, hash_qc(&hq_b).unwrap()), 1);
    }

    #[test]
    fn best_timeout_cert_picks_highest_high_qc() {
        let vset = ValidatorSet::phase2_bft7_fixture();
        let mut pool = TimeoutPool::new();
        let view = 4u64;
        let hq_low = genesis_parent_qc();
        let hq_high = singleton_qc_certifying([7u8; 32], 5, 0);
        assert!(high_qc_rank(&hq_high) > high_qc_rank(&hq_low));
        for i in 0u32..5 {
            let sk = vset.dev_bls_secret(i as usize).unwrap();
            pool.record(
                Timeout::sign(
                    TimeoutSignBody {
                        view,
                        high_qc: hq_low.clone(),
                    },
                    i,
                    &sk,
                ),
                &vset,
            );
        }
        for i in 0u32..5 {
            let sk = vset.dev_bls_secret(i as usize).unwrap();
            pool.record(
                Timeout::sign(
                    TimeoutSignBody {
                        view,
                        high_qc: hq_high.clone(),
                    },
                    i,
                    &sk,
                ),
                &vset,
            );
        }
        let best = pool
            .try_form_best_timeout_cert_for_view(view, &vset)
            .expect("best cert");
        assert_eq!(best.high_qc, hq_high);
        verify_formed_timeout_cert(&best, &vset).unwrap();
    }
}
