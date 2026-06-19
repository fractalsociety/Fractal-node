//! HotStuff-2 votes + vote pool (`docs/prd.md` §7.3 / §18 M7-d-3 / M7-d-4).
//!
//! Each validator publishes a [`Vote`] for the block it just applied. Nodes tally
//! votes per `(view, header_hash)` in [`VotePool`] (M7-d-4); when the pool meets
//! [`ValidatorSet::quorum_threshold`] the constituent `Vote::signature`s are
//! aggregated into a [`FormedQc`] via [`VotePool::try_form_qc`] for the producer to
//! thread into the next block's `parent_qc_hash` (M7-d-6).
//!
//! The signed payload is intentionally narrow — just `(view, height, header_hash,
//! validator_index)` — so the canonical bytes are short, easy to verify, and
//! impossible to bind to a different block at a different height/view.
//!
//! Wire encoding is borsh and stable; gossipsub propagates `borsh::to_vec(&vote)`.

use std::collections::{BTreeMap, BTreeSet};

use borsh::{BorshDeserialize, BorshSerialize};
use fractal_crypto::hash::Hash256;
use fractal_crypto::{AggregateSignature, BlsError, BlsPublicKey, BlsSecretKey, BlsSignature};
use thiserror::Error;

use crate::qc::QuorumCertificate;
use crate::validators::ValidatorSet;

/// Canonical body a validator signs.
///
/// `validator_index` is intentionally **not** signed — the BLS public key
/// already binds a signature to a specific signer. If an attacker captures
/// Alice's vote and relabels `validator_index` as Bob's, the receiver will
/// look up Bob's pubkey and BLS verification will fail (Alice's signature
/// cannot be verified against Bob's pubkey). Keeping the sign body identical
/// for every signer lets aggregate verification use BLS's cheap
/// `fast_aggregate_verify` (M7-d-6).
#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoteSignBody {
    pub view: u64,
    pub height: u64,
    pub header_hash: Hash256,
}

impl VoteSignBody {
    /// Canonical bytes passed to `BlsSecretKey::sign` / `BlsSignature::verify`.
    pub fn sign_bytes(&self) -> Vec<u8> {
        borsh::to_vec(self).expect("VoteSignBody borsh never fails (fixed-size fields)")
    }
}

/// Validator's vote for a specific committed block.
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct Vote {
    pub view: u64,
    pub height: u64,
    pub header_hash: Hash256,
    pub validator_index: u32,
    pub signature: BlsSignature,
}

#[derive(Debug, Error)]
pub enum VoteError {
    #[error("validator_index {idx} out of range (validator set size {n})")]
    ValidatorIndexOutOfRange { idx: u32, n: usize },
    #[error("validator_index {0} has no BLS public key in this set")]
    NoPublicKey(u32),
    #[error("QC has {got} signers but quorum threshold is {need}")]
    InsufficientSigners { got: usize, need: usize },
    #[error("duplicate signer index {0} in QC signer_indices")]
    DuplicateSignerIndex(u32),
    #[error(transparent)]
    Bls(#[from] BlsError),
}

impl Vote {
    /// Sign `body` with `sk` for validator `validator_index` and assemble a [`Vote`].
    ///
    /// The caller must ensure `sk.public_key()` matches
    /// `validators.bls_pubkey(validator_index)` in the active [`ValidatorSet`] —
    /// this constructor does NOT cross-check (cheap, no allocation); see
    /// `verify_against_validator_set` for that.
    pub fn sign(body: VoteSignBody, validator_index: u32, sk: &BlsSecretKey) -> Self {
        let signature = sk.sign(&body.sign_bytes());
        Self {
            view: body.view,
            height: body.height,
            header_hash: body.header_hash,
            validator_index,
            signature,
        }
    }

    /// Body that was (or should be) signed for this vote.
    #[must_use]
    pub fn sign_body(&self) -> VoteSignBody {
        VoteSignBody {
            view: self.view,
            height: self.height,
            header_hash: self.header_hash,
        }
    }

    /// Verify the signature against an explicit public key.
    ///
    /// Use this when you've already looked up the validator's BLS pubkey
    /// (e.g. during aggregate verification, where you have a `&[BlsPublicKey]`
    /// in hand). For one-off checks against a [`ValidatorSet`], prefer
    /// [`verify_against_validator_set`](Self::verify_against_validator_set).
    pub fn verify(&self, pubkey: &BlsPublicKey) -> Result<(), VoteError> {
        self.signature
            .verify(&self.sign_body().sign_bytes(), pubkey)?;
        Ok(())
    }

    /// Verify against the active validator set: look up the pubkey by
    /// `validator_index` and run [`Self::verify`].
    pub fn verify_against_validator_set(&self, set: &ValidatorSet) -> Result<(), VoteError> {
        let n = set.len();
        let idx_usize = self.validator_index as usize;
        if idx_usize >= n {
            return Err(VoteError::ValidatorIndexOutOfRange {
                idx: self.validator_index,
                n,
            });
        }
        let pk = set
            .bls_pubkey(idx_usize)
            .ok_or(VoteError::NoPublicKey(self.validator_index))?;
        self.verify(pk)
    }
}

/// Outcome of inserting a vote into a [`VotePool`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecordVoteOutcome {
    /// Vote was new and the pool is still below quorum after this insert.
    Accepted,
    /// Vote was new and crossed the `quorum_threshold` boundary on this insert.
    ReachedQuorum,
    /// Pool already had a vote from this `validator_index` at `(view, header_hash)`.
    /// (Either the same signature or a different one — we keep the first observed.)
    DuplicateValidator,
    /// `validator_index` falls outside `[0, validator_set.len())`.
    OutOfRange,
    /// BLS signature failed to verify against `validator_set.bls_pubkey(idx)`.
    BadSignature,
}

/// QC + the indices that signed it. `QuorumCertificate.aggregate_sig` is opaque
/// 96 bytes (see `crates/consensus/src/qc.rs`); knowing **which** validators are
/// covered by the aggregate is required to verify it (`fast_aggregate_verify`
/// needs the same pubkey set). M7-d-6 will thread `signer_indices` into the
/// on-wire QC envelope (e.g. as a bitmap) — until then the producer/follower
/// carry it alongside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormedQc {
    pub qc: QuorumCertificate,
    pub signer_indices: Vec<u32>,
}

/// In-memory tally of [`Vote`]s, keyed by the block they cover.
///
/// Internal map: `(view, header_hash) -> { validator_index -> Vote }`.
/// Using a `BTreeMap` on validator_index makes `try_form_qc` deterministic —
/// the same pool always produces the same `signer_indices` and the same
/// aggregate bytes (BLS aggregation is deterministic on input order).
#[derive(Debug, Clone, Default)]
pub struct VotePool {
    entries: BTreeMap<(u64, Hash256), BTreeMap<u32, Vote>>,
}

impl VotePool {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert `vote` after verifying its signature against `validators`.
    ///
    /// Reports the result via [`RecordVoteOutcome`]:
    /// - First valid vote that crosses the quorum line → `ReachedQuorum`
    ///   (caller can immediately call [`Self::try_form_qc`]).
    /// - Subsequent valid votes still under quorum → `Accepted`.
    /// - Repeat from same validator at same `(view, header_hash)` → `DuplicateValidator`.
    /// - Malformed `validator_index` → `OutOfRange`.
    /// - BLS verify fail → `BadSignature`.
    pub fn record(&mut self, vote: Vote, validators: &ValidatorSet) -> RecordVoteOutcome {
        let n = validators.len();
        let idx_usize = vote.validator_index as usize;
        if idx_usize >= n {
            return RecordVoteOutcome::OutOfRange;
        }
        let Some(pk) = validators.bls_pubkey(idx_usize) else {
            return RecordVoteOutcome::OutOfRange;
        };
        if vote.verify(pk).is_err() {
            return RecordVoteOutcome::BadSignature;
        }
        let key = (vote.view, vote.header_hash);
        let slot = self.entries.entry(key).or_default();
        if slot.contains_key(&vote.validator_index) {
            return RecordVoteOutcome::DuplicateValidator;
        }
        let threshold = validators.quorum_threshold();
        let prev_count = slot.len();
        slot.insert(vote.validator_index, vote);
        let new_count = slot.len();
        if prev_count < threshold && new_count >= threshold {
            RecordVoteOutcome::ReachedQuorum
        } else {
            RecordVoteOutcome::Accepted
        }
    }

    /// Number of votes currently held for `(view, header_hash)`.
    #[must_use]
    pub fn count(&self, view: u64, header_hash: Hash256) -> usize {
        self.entries
            .get(&(view, header_hash))
            .map_or(0, BTreeMap::len)
    }

    /// All votes currently held for `(view, header_hash)`, in ascending validator-index order.
    #[must_use]
    pub fn votes(&self, view: u64, header_hash: Hash256) -> Vec<&Vote> {
        self.entries
            .get(&(view, header_hash))
            .map(|m| m.values().collect())
            .unwrap_or_default()
    }

    /// Validator indices that signed `(view, header_hash)`, ascending.
    #[must_use]
    pub fn signer_indices(&self, view: u64, header_hash: Hash256) -> Vec<u32> {
        self.entries
            .get(&(view, header_hash))
            .map(|m| m.keys().copied().collect())
            .unwrap_or_default()
    }

    /// Best vote slot observed for `height`, returning
    /// `(view, header_hash, vote_count, signer_indices)` for diagnostics.
    ///
    /// The pool is keyed by `(view, header_hash)`, so height-level diagnostics
    /// must inspect the contained votes. Ties are resolved deterministically by
    /// the `BTreeMap` iteration order.
    #[must_use]
    pub fn best_slot_for_height(&self, height: u64) -> Option<(u64, Hash256, usize, Vec<u32>)> {
        self.entries
            .iter()
            .filter_map(|((view, header_hash), slot)| {
                let first = slot.values().next()?;
                (first.height == height).then(|| {
                    (
                        *view,
                        *header_hash,
                        slot.len(),
                        slot.keys().copied().collect::<Vec<_>>(),
                    )
                })
            })
            .max_by_key(|(view, header_hash, count, _)| (*count, *view, *header_hash))
    }

    /// Aggregate the pool's signatures into a [`FormedQc`] if `(view, header_hash)`
    /// has reached `validators.quorum_threshold()`. Returns `None` otherwise.
    ///
    /// The aggregate covers **all** valid signatures collected so far (≥ threshold),
    /// not just the minimum needed — extras are still verifiable and tighten the
    /// trust set. Iteration order is ascending validator_index, so the result is
    /// deterministic.
    pub fn try_form_qc(
        &self,
        view: u64,
        block_height: u64,
        header_hash: Hash256,
        validators: &ValidatorSet,
    ) -> Option<FormedQc> {
        let slot = self.entries.get(&(view, header_hash))?;
        if slot.len() < validators.quorum_threshold() {
            return None;
        }
        let signer_indices: Vec<u32> = slot.keys().copied().collect();
        let sigs: Vec<BlsSignature> = slot.values().map(|v| v.signature).collect();
        let aggregate_sig = AggregateSignature::from_signatures(&sigs).ok()?;
        Some(FormedQc {
            qc: QuorumCertificate {
                view,
                block_height,
                block_header_hash: header_hash,
                aggregate_sig,
            },
            signer_indices,
        })
    }

    /// Drop entries strictly below `min_height` (call after the chain advances).
    ///
    /// Old votes can never form a useful QC once their height has been buried, so
    /// the pool should be pruned to bound memory. Conservative default: keep
    /// votes at exactly `min_height` (current tip) so a late-arriving vote can
    /// still complete a QC for the just-committed block.
    pub fn prune_below_height(&mut self, min_height: u64) {
        let stale: BTreeSet<(u64, Hash256)> = self
            .entries
            .iter()
            .filter_map(|(k, slot)| {
                let lowest = slot.values().next().map_or(u64::MAX, |v| v.height);
                if lowest < min_height {
                    Some(*k)
                } else {
                    None
                }
            })
            .collect();
        for k in stale {
            self.entries.remove(&k);
        }
    }

    /// Total `(view, header_hash)` slots tracked.
    #[must_use]
    pub fn slots_len(&self) -> usize {
        self.entries.len()
    }
}

/// Verify a [`FormedQc`] end-to-end: check threshold, look up every signer's
/// pubkey, then run BLS `fast_aggregate_verify` over the QC's
/// `(view, block_height, block_header_hash)`.
///
/// Used by followers in M7-d-6 to verify a QC threaded through `parent_qc_hash`.
/// Returns `Err` on insufficient signers, missing pubkey, or aggregate verify
/// failure.
pub fn verify_formed_qc(formed: &FormedQc, validators: &ValidatorSet) -> Result<(), VoteError> {
    let threshold = validators.quorum_threshold();
    if formed.signer_indices.len() < threshold {
        return Err(VoteError::InsufficientSigners {
            got: formed.signer_indices.len(),
            need: threshold,
        });
    }
    // Reject duplicate indices: the producer must never pad the QC.
    let mut seen = BTreeSet::new();
    let mut pubkeys: Vec<BlsPublicKey> = Vec::with_capacity(formed.signer_indices.len());
    for idx in &formed.signer_indices {
        if !seen.insert(*idx) {
            return Err(VoteError::DuplicateSignerIndex(*idx));
        }
        let i = *idx as usize;
        if i >= validators.len() {
            return Err(VoteError::ValidatorIndexOutOfRange {
                idx: *idx,
                n: validators.len(),
            });
        }
        let pk = validators
            .bls_pubkey(i)
            .ok_or(VoteError::NoPublicKey(*idx))?;
        pubkeys.push(*pk);
    }
    let msg = VoteSignBody {
        view: formed.qc.view,
        height: formed.qc.block_height,
        header_hash: formed.qc.block_header_hash,
    }
    .sign_bytes();
    formed.qc.aggregate_sig.verify(&msg, &pubkeys)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(view: u64, height: u64, hh_byte: u8) -> VoteSignBody {
        VoteSignBody {
            view,
            height,
            header_hash: [hh_byte; 32],
        }
    }

    fn sign_for(set: &ValidatorSet, idx: u32, b: VoteSignBody) -> Vote {
        let sk = set.dev_bls_secret(idx as usize).unwrap();
        Vote::sign(b, idx, &sk)
    }

    #[test]
    fn sign_then_verify_against_pubkey_round_trip() {
        let set = ValidatorSet::phase2_bft7_fixture();
        let idx: u32 = 3;
        let v = sign_for(&set, idx, body(7, 4, 0xaa));
        v.verify(set.bls_pubkey(idx as usize).unwrap())
            .expect("verify ok");
    }

    #[test]
    fn verify_against_validator_set_succeeds() {
        let set = ValidatorSet::phase2_bft7_fixture();
        for idx in 0u32..7 {
            let v = sign_for(&set, idx, body(11, 5, 0xbb));
            v.verify_against_validator_set(&set)
                .unwrap_or_else(|e| panic!("idx={idx} verify failed: {e:?}"));
        }
    }

    #[test]
    fn verify_fails_when_view_is_tampered_after_sign() {
        let set = ValidatorSet::phase1_singleton();
        let mut v = sign_for(&set, 0, body(2, 1, 0x11));
        v.view = 3;
        assert!(v.verify_against_validator_set(&set).is_err());
    }

    #[test]
    fn verify_fails_when_height_is_tampered_after_sign() {
        let set = ValidatorSet::phase1_singleton();
        let mut v = sign_for(&set, 0, body(2, 1, 0x11));
        v.height = 2;
        assert!(v.verify_against_validator_set(&set).is_err());
    }

    #[test]
    fn verify_fails_when_header_hash_is_tampered_after_sign() {
        let set = ValidatorSet::phase1_singleton();
        let mut v = sign_for(&set, 0, body(2, 1, 0x11));
        v.header_hash[0] ^= 0xff;
        assert!(v.verify_against_validator_set(&set).is_err());
    }

    #[test]
    fn relabeling_validator_index_breaks_verification() {
        // Anti-replay: BLS already binds sig to a specific signer's public key.
        // An attacker that captures Alice's vote (idx=0) and relabels it as
        // coming from Bob (idx=1) sees verification fail because Alice's sig
        // can't be verified against Bob's pubkey.
        let set = ValidatorSet::phase2_bft7_fixture();
        let mut v = sign_for(&set, 0, body(2, 1, 0x11));
        v.validator_index = 1;
        assert!(v.verify_against_validator_set(&set).is_err());
    }

    #[test]
    fn verify_fails_for_wrong_explicit_pubkey() {
        let set = ValidatorSet::phase2_bft7_fixture();
        let v = sign_for(&set, 0, body(2, 1, 0x11));
        let other = set.bls_pubkey(2).unwrap();
        assert!(v.verify(other).is_err());
    }

    #[test]
    fn verify_against_validator_set_rejects_out_of_range_index() {
        let set = ValidatorSet::phase2_bft7_fixture();
        let sk = set.dev_bls_secret(0).unwrap();
        let v = Vote {
            view: 1,
            height: 1,
            header_hash: [0u8; 32],
            validator_index: 99,
            signature: sk.sign(b"anything"),
        };
        match v.verify_against_validator_set(&set) {
            Err(VoteError::ValidatorIndexOutOfRange { idx: 99, n: 7 }) => {}
            other => panic!("expected OOR, got {other:?}"),
        }
    }

    #[test]
    fn borsh_round_trip_preserves_signature_bytes() {
        let set = ValidatorSet::phase1_singleton();
        let v = sign_for(&set, 0, body(2, 1, 0x33));
        let bytes = borsh::to_vec(&v).expect("encode");
        let decoded: Vote = borsh::from_slice(&bytes).expect("decode");
        assert_eq!(v, decoded);
        decoded
            .verify_against_validator_set(&set)
            .expect("verify after borsh round-trip");
    }

    #[test]
    fn sign_bytes_are_deterministic_for_same_body() {
        let b = body(42, 7, 0x55);
        assert_eq!(b.sign_bytes(), b.sign_bytes());
    }

    #[test]
    fn votes_for_different_blocks_have_distinct_signatures() {
        let set = ValidatorSet::phase1_singleton();
        let v1 = sign_for(&set, 0, body(2, 1, 0xaa));
        let v2 = sign_for(&set, 0, body(2, 1, 0xbb));
        assert_ne!(
            v1.signature.0, v2.signature.0,
            "different header_hash must yield different signature"
        );
    }

    // ------- VotePool / FormedQc / verify_formed_qc tests (M7-d-4) -------

    #[test]
    fn pool_starts_empty_and_records_one_vote_below_quorum() {
        let set = ValidatorSet::phase2_bft7_fixture();
        let mut pool = VotePool::new();
        assert_eq!(pool.slots_len(), 0);
        let v = sign_for(&set, 0, body(1, 1, 0xa1));
        let out = pool.record(v.clone(), &set);
        assert_eq!(out, RecordVoteOutcome::Accepted);
        assert_eq!(pool.count(1, [0xa1; 32]), 1);
        assert_eq!(pool.signer_indices(1, [0xa1; 32]), vec![0u32]);
        // Below quorum (need 5 of 7) → no QC yet.
        assert!(pool.try_form_qc(1, 1, [0xa1; 32], &set).is_none());
    }

    #[test]
    fn pool_reports_reached_quorum_on_fifth_distinct_signer() {
        let set = ValidatorSet::phase2_bft7_fixture();
        let mut pool = VotePool::new();
        let hh = [0xb2; 32];
        // First 4: Accepted.
        for idx in 0u32..4 {
            let out = pool.record(sign_for(&set, idx, body(2, 1, 0xb2)), &set);
            assert_eq!(out, RecordVoteOutcome::Accepted, "idx={idx}");
        }
        // 5th: ReachedQuorum.
        let out = pool.record(sign_for(&set, 4, body(2, 1, 0xb2)), &set);
        assert_eq!(out, RecordVoteOutcome::ReachedQuorum);
        // 6th and 7th: Accepted (already at quorum, but still new and valid).
        for idx in 5u32..7 {
            let out = pool.record(sign_for(&set, idx, body(2, 1, 0xb2)), &set);
            assert_eq!(out, RecordVoteOutcome::Accepted, "idx={idx}");
        }
        assert_eq!(pool.count(2, hh), 7);
    }

    #[test]
    fn best_slot_for_height_reports_largest_height_slot() {
        let set = ValidatorSet::phase2_bft7_fixture();
        let mut pool = VotePool::new();
        for idx in 0u32..3 {
            let _ = pool.record(sign_for(&set, idx, body(3, 2, 0x44)), &set);
        }
        for idx in 0u32..5 {
            let _ = pool.record(sign_for(&set, idx, body(4, 2, 0x55)), &set);
        }

        let (view, header_hash, count, signers) =
            pool.best_slot_for_height(2).expect("height 2 slot");
        assert_eq!(view, 4);
        assert_eq!(header_hash, [0x55; 32]);
        assert_eq!(count, 5);
        assert_eq!(signers, vec![0, 1, 2, 3, 4]);
        assert!(pool.best_slot_for_height(99).is_none());
    }

    #[test]
    fn pool_rejects_duplicate_validator_index() {
        let set = ValidatorSet::phase2_bft7_fixture();
        let mut pool = VotePool::new();
        let v = sign_for(&set, 3, body(1, 1, 0xcc));
        assert_eq!(pool.record(v.clone(), &set), RecordVoteOutcome::Accepted);
        assert_eq!(pool.record(v, &set), RecordVoteOutcome::DuplicateValidator);
    }

    #[test]
    fn pool_rejects_out_of_range_validator_index() {
        let set = ValidatorSet::phase2_bft7_fixture();
        let mut pool = VotePool::new();
        let sk = set.dev_bls_secret(0).unwrap();
        let mut v = Vote::sign(body(1, 1, 0xdd), 0, &sk);
        v.validator_index = 99;
        assert_eq!(pool.record(v, &set), RecordVoteOutcome::OutOfRange);
    }

    #[test]
    fn pool_rejects_bad_signature() {
        let set = ValidatorSet::phase2_bft7_fixture();
        let mut pool = VotePool::new();
        let mut v = sign_for(&set, 1, body(1, 1, 0xee));
        v.signature.0[0] ^= 0xff;
        assert_eq!(pool.record(v, &set), RecordVoteOutcome::BadSignature);
    }

    #[test]
    fn singleton_pool_reaches_quorum_immediately() {
        let set = ValidatorSet::phase1_singleton();
        let mut pool = VotePool::new();
        let v = sign_for(&set, 0, body(0, 1, 0x42));
        assert_eq!(pool.record(v, &set), RecordVoteOutcome::ReachedQuorum);
        let formed = pool
            .try_form_qc(0, 1, [0x42; 32], &set)
            .expect("singleton forms QC");
        assert_eq!(formed.signer_indices, vec![0u32]);
        assert_eq!(formed.qc.view, 0);
        assert_eq!(formed.qc.block_height, 1);
        assert_eq!(formed.qc.block_header_hash, [0x42; 32]);
        verify_formed_qc(&formed, &set).expect("singleton QC verifies");
    }

    #[test]
    fn try_form_qc_bft7_five_of_seven_round_trip() {
        let set = ValidatorSet::phase2_bft7_fixture();
        let mut pool = VotePool::new();
        let hh = [0x99; 32];
        for idx in 0u32..5 {
            let _ = pool.record(sign_for(&set, idx, body(3, 9, 0x99)), &set);
        }
        let formed = pool
            .try_form_qc(3, 9, hh, &set)
            .expect("five signers form QC");
        assert_eq!(formed.signer_indices, vec![0u32, 1, 2, 3, 4]);
        verify_formed_qc(&formed, &set).expect("aggregate verifies");
    }

    #[test]
    fn try_form_qc_returns_none_when_below_threshold() {
        let set = ValidatorSet::phase2_bft7_fixture();
        let mut pool = VotePool::new();
        for idx in 0u32..3 {
            let _ = pool.record(sign_for(&set, idx, body(1, 1, 0xfe)), &set);
        }
        assert!(pool.try_form_qc(1, 1, [0xfe; 32], &set).is_none());
    }

    #[test]
    fn try_form_qc_aggregates_all_collected_signers_not_just_threshold() {
        let set = ValidatorSet::phase2_bft7_fixture();
        let mut pool = VotePool::new();
        for idx in 0u32..7 {
            let _ = pool.record(sign_for(&set, idx, body(4, 4, 0xab)), &set);
        }
        let formed = pool.try_form_qc(4, 4, [0xab; 32], &set).unwrap();
        assert_eq!(
            formed.signer_indices.len(),
            7,
            "include all collected signers"
        );
        verify_formed_qc(&formed, &set).expect("7-of-7 verifies");
    }

    #[test]
    fn verify_formed_qc_rejects_insufficient_signers() {
        let set = ValidatorSet::phase2_bft7_fixture();
        // Build a FormedQc by hand with only 4 signers (threshold is 5).
        let body = body(1, 1, 0x77);
        let sigs: Vec<BlsSignature> = (0..4)
            .map(|i| set.dev_bls_secret(i).unwrap().sign(&body.sign_bytes()))
            .collect();
        let agg = AggregateSignature::from_signatures(&sigs).unwrap();
        let formed = FormedQc {
            qc: QuorumCertificate {
                view: 1,
                block_height: 1,
                block_header_hash: [0x77; 32],
                aggregate_sig: agg,
            },
            signer_indices: vec![0, 1, 2, 3],
        };
        match verify_formed_qc(&formed, &set) {
            Err(VoteError::InsufficientSigners { got: 4, need: 5 }) => {}
            other => panic!("expected InsufficientSigners, got {other:?}"),
        }
    }

    #[test]
    fn verify_formed_qc_rejects_duplicate_signer_indices() {
        let set = ValidatorSet::phase2_bft7_fixture();
        let body = body(2, 2, 0x88);
        let sigs: Vec<BlsSignature> = (0..5)
            .map(|i| set.dev_bls_secret(i).unwrap().sign(&body.sign_bytes()))
            .collect();
        let agg = AggregateSignature::from_signatures(&sigs).unwrap();
        let formed = FormedQc {
            qc: QuorumCertificate {
                view: 2,
                block_height: 2,
                block_header_hash: [0x88; 32],
                aggregate_sig: agg,
            },
            signer_indices: vec![0, 1, 2, 3, 3], // duplicate 3
        };
        match verify_formed_qc(&formed, &set) {
            Err(VoteError::DuplicateSignerIndex(3)) => {}
            other => panic!("expected DuplicateSignerIndex(3), got {other:?}"),
        }
    }

    #[test]
    fn verify_formed_qc_rejects_wrong_signer_set_for_aggregate() {
        // QC claims signers {0,1,2,3,4} but the aggregate was over {0,1,2,3,5}.
        // BLS aggregate verify must fail.
        let set = ValidatorSet::phase2_bft7_fixture();
        let body = body(5, 5, 0x33);
        let signing_indices = [0usize, 1, 2, 3, 5];
        let sigs: Vec<BlsSignature> = signing_indices
            .iter()
            .map(|&i| set.dev_bls_secret(i).unwrap().sign(&body.sign_bytes()))
            .collect();
        let agg = AggregateSignature::from_signatures(&sigs).unwrap();
        let formed = FormedQc {
            qc: QuorumCertificate {
                view: 5,
                block_height: 5,
                block_header_hash: [0x33; 32],
                aggregate_sig: agg,
            },
            signer_indices: vec![0, 1, 2, 3, 4],
        };
        assert!(verify_formed_qc(&formed, &set).is_err());
    }

    #[test]
    fn prune_below_height_drops_old_entries() {
        let set = ValidatorSet::phase2_bft7_fixture();
        let mut pool = VotePool::new();
        // Three slots at heights 1, 2, 3 (one vote each from validator 0).
        for h in 1u64..=3 {
            let _ = pool.record(sign_for(&set, 0, body(h, h, h as u8)), &set);
        }
        assert_eq!(pool.slots_len(), 3);
        pool.prune_below_height(3);
        assert_eq!(pool.slots_len(), 1, "only height >= 3 should remain");
    }
}
