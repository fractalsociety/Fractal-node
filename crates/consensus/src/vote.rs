//! HotStuff-2 votes (`docs/prd.md` §7.3 / §18 M7-d-3).
//!
//! Each validator publishes a [`Vote`] for the block it just applied. Nodes tally
//! votes per `(view, header_hash)` into a vote pool (M7-d-4); when the pool meets
//! [`ValidatorSet::quorum_threshold`] the constituent `Vote::signature`s are
//! aggregated into the QC's `aggregate_sig` (M7-d-6).
//!
//! The signed payload is intentionally narrow — just `(view, height, header_hash,
//! validator_index)` — so the canonical bytes are short, easy to verify, and
//! impossible to bind to a different block at a different height/view.
//!
//! Wire encoding is borsh and stable; gossipsub propagates `borsh::to_vec(&vote)`.

use borsh::{BorshDeserialize, BorshSerialize};
use fractal_crypto::hash::Hash256;
use fractal_crypto::{BlsError, BlsPublicKey, BlsSecretKey, BlsSignature};
use thiserror::Error;

use crate::validators::ValidatorSet;

/// Canonical body a validator signs.
///
/// `validator_index` is part of the signed body so a vote cannot be replayed
/// against the same `(view, header_hash)` claiming to be from a different validator.
#[derive(BorshSerialize, BorshDeserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct VoteSignBody {
    pub view: u64,
    pub height: u64,
    pub header_hash: Hash256,
    pub validator_index: u32,
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
    #[error(transparent)]
    Bls(#[from] BlsError),
}

impl Vote {
    /// Sign `body` with `sk` and assemble a [`Vote`]. Caller must ensure
    /// `body.validator_index` matches the index of `sk.public_key()` in the
    /// active [`ValidatorSet`] — this constructor does NOT cross-check (cheap,
    /// no allocation); see `verify` / `verify_against_validator_set` for that.
    pub fn sign(body: VoteSignBody, sk: &BlsSecretKey) -> Self {
        let signature = sk.sign(&body.sign_bytes());
        Self {
            view: body.view,
            height: body.height,
            header_hash: body.header_hash,
            validator_index: body.validator_index,
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
            validator_index: self.validator_index,
        }
    }

    /// Verify the signature against an explicit public key.
    ///
    /// Use this when you've already looked up the validator's BLS pubkey
    /// (e.g. during aggregate verification, where you have a `&[BlsPublicKey]`
    /// in hand). For one-off checks against a [`ValidatorSet`], prefer
    /// [`verify_against_validator_set`](Self::verify_against_validator_set).
    pub fn verify(&self, pubkey: &BlsPublicKey) -> Result<(), VoteError> {
        self.signature.verify(&self.sign_body().sign_bytes(), pubkey)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    fn body(view: u64, height: u64, hh_byte: u8, idx: u32) -> VoteSignBody {
        VoteSignBody {
            view,
            height,
            header_hash: [hh_byte; 32],
            validator_index: idx,
        }
    }

    #[test]
    fn sign_then_verify_against_pubkey_round_trip() {
        let set = ValidatorSet::phase2_bft7_fixture();
        let idx: u32 = 3;
        let sk = set.dev_bls_secret(idx as usize).unwrap();
        let b = body(7, 4, 0xaa, idx);
        let v = Vote::sign(b, &sk);
        v.verify(set.bls_pubkey(idx as usize).unwrap())
            .expect("verify ok");
    }

    #[test]
    fn verify_against_validator_set_succeeds() {
        let set = ValidatorSet::phase2_bft7_fixture();
        for idx in 0u32..7 {
            let sk = set.dev_bls_secret(idx as usize).unwrap();
            let v = Vote::sign(body(11, 5, 0xbb, idx), &sk);
            v.verify_against_validator_set(&set)
                .unwrap_or_else(|e| panic!("idx={idx} verify failed: {e:?}"));
        }
    }

    #[test]
    fn verify_fails_when_view_is_tampered_after_sign() {
        let set = ValidatorSet::phase1_singleton();
        let sk = set.dev_bls_secret(0).unwrap();
        let mut v = Vote::sign(body(2, 1, 0x11, 0), &sk);
        v.view = 3; // tamper
        assert!(v.verify_against_validator_set(&set).is_err());
    }

    #[test]
    fn verify_fails_when_height_is_tampered_after_sign() {
        let set = ValidatorSet::phase1_singleton();
        let sk = set.dev_bls_secret(0).unwrap();
        let mut v = Vote::sign(body(2, 1, 0x11, 0), &sk);
        v.height = 2; // tamper
        assert!(v.verify_against_validator_set(&set).is_err());
    }

    #[test]
    fn verify_fails_when_header_hash_is_tampered_after_sign() {
        let set = ValidatorSet::phase1_singleton();
        let sk = set.dev_bls_secret(0).unwrap();
        let mut v = Vote::sign(body(2, 1, 0x11, 0), &sk);
        v.header_hash[0] ^= 0xff;
        assert!(v.verify_against_validator_set(&set).is_err());
    }

    #[test]
    fn verify_fails_when_validator_index_is_tampered_after_sign() {
        let set = ValidatorSet::phase2_bft7_fixture();
        let sk = set.dev_bls_secret(0).unwrap();
        let mut v = Vote::sign(body(2, 1, 0x11, 0), &sk);
        v.validator_index = 1; // would now try to verify with validator 1's pubkey
        assert!(v.verify_against_validator_set(&set).is_err());
    }

    #[test]
    fn verify_fails_for_wrong_explicit_pubkey() {
        let set = ValidatorSet::phase2_bft7_fixture();
        let sk = set.dev_bls_secret(0).unwrap();
        let v = Vote::sign(body(2, 1, 0x11, 0), &sk);
        // Verifying with a different validator's pubkey must fail.
        let other = set.bls_pubkey(2).unwrap();
        assert!(v.verify(other).is_err());
    }

    #[test]
    fn verify_against_validator_set_rejects_out_of_range_index() {
        let set = ValidatorSet::phase2_bft7_fixture();
        let sk = set.dev_bls_secret(0).unwrap();
        // Construct a vote claiming to be from validator 99.
        let v = Vote {
            view: 1,
            height: 1,
            header_hash: [0u8; 32],
            validator_index: 99,
            signature: sk.sign(b"anything"), // sig doesn't matter, index check is first
        };
        match v.verify_against_validator_set(&set) {
            Err(VoteError::ValidatorIndexOutOfRange { idx: 99, n: 7 }) => {}
            other => panic!("expected OOR, got {other:?}"),
        }
    }

    #[test]
    fn borsh_round_trip_preserves_signature_bytes() {
        let set = ValidatorSet::phase1_singleton();
        let sk = set.dev_bls_secret(0).unwrap();
        let v = Vote::sign(body(2, 1, 0x33, 0), &sk);
        let bytes = borsh::to_vec(&v).expect("encode");
        let decoded: Vote = borsh::from_slice(&bytes).expect("decode");
        assert_eq!(v, decoded);
        decoded
            .verify_against_validator_set(&set)
            .expect("verify after borsh round-trip");
    }

    #[test]
    fn sign_bytes_are_deterministic_for_same_body() {
        let b = body(42, 7, 0x55, 1);
        assert_eq!(b.sign_bytes(), b.sign_bytes());
    }

    #[test]
    fn votes_for_different_blocks_have_distinct_signatures() {
        let set = ValidatorSet::phase1_singleton();
        let sk = set.dev_bls_secret(0).unwrap();
        let v1 = Vote::sign(body(2, 1, 0xaa, 0), &sk);
        let v2 = Vote::sign(body(2, 1, 0xbb, 0), &sk);
        assert_ne!(
            v1.signature.0, v2.signature.0,
            "different header_hash must yield different signature"
        );
    }
}
