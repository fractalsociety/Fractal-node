//! Validator identities, BLS keys, and view-based leader selection
//! (`docs/prd.md` §7.2–7.4, §18 M7-b / M7-d).
//!
//! Each `ValidatorEntry` carries a 32-byte fingerprint (the historical `ValidatorId`,
//! kept for backwards compatibility with `BlockHeader.proposer`) **and** a 48-byte
//! BLS12-381 G1 public key for HotStuff-2 vote aggregation. The Phase-1 singleton
//! and Phase-2 BFT-7 / M8 BFT-21 fixtures both derive their BLS keypairs deterministically from
//! the same `keccak256(seed)` bytes used for fingerprints so existing tests stay
//! reproducible and `bls_pubkey(idx)` can be cross-checked against
//! `dev_bls_secret(idx).public_key()`.
//!
//! **Dev keys are NOT for mainnet:** real deployments must register operator-supplied
//! BLS keys via config and never call `dev_bls_secret`.

use borsh::{BorshDeserialize, BorshSerialize};
use fractal_crypto::hash::keccak256;
use fractal_crypto::{BlsPublicKey, BlsSecretKey};

pub type ValidatorId = [u8; 32];

/// One validator's stable identity for HotStuff-2 voting.
#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ValidatorEntry {
    /// 32-byte stable fingerprint placed in `BlockHeader.proposer` (`docs/prd.md` §7.2).
    pub fingerprint: ValidatorId,
    /// Compressed BLS12-381 G1 public key (48 bytes). For dev fixtures this is
    /// deterministically derived from the fingerprint via `BlsSecretKey::from_ikm`.
    pub bls_pubkey: BlsPublicKey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatorSet {
    entries: Vec<ValidatorEntry>,
}

/// IKM for the dev BLS key of the singleton validator.
const SINGLETON_SEED: &[u8] = b"FC_PHASE1_SINGLETON_V0";

/// Constant 32-byte prefix for BFT-7 dev fixture IKM; last byte = validator index (`0..7`).
const BFT7_SEED_PREFIX: [u8; 32] = *b"FRACTALCHAIN_BFT7_V0____________";

/// Constant 32-byte prefix for BFT-21 (PRD M8) dev fixture IKM; last byte = validator index (`0..21`).
const BFT21_SEED_PREFIX: [u8; 32] = *b"FRACTALCHAIN_BFT21_V0___________";

fn bft7_seed(idx: u8) -> [u8; 32] {
    let mut s = BFT7_SEED_PREFIX;
    s[31] = idx;
    s
}

fn bft21_seed(idx: u8) -> [u8; 32] {
    let mut s = BFT21_SEED_PREFIX;
    s[31] = idx;
    s
}

fn dev_secret_from_seed(seed: &[u8]) -> BlsSecretKey {
    // `from_ikm` requires ≥ 32 bytes; keccak256 widens shorter seeds while keeping
    // them deterministic.
    let ikm = keccak256(seed);
    BlsSecretKey::from_ikm(&ikm).expect("dev BLS ikm valid")
}

fn dev_entry_from_seed(seed: &[u8]) -> ValidatorEntry {
    let fingerprint = keccak256(seed);
    let bls_pubkey = dev_secret_from_seed(seed).public_key();
    ValidatorEntry {
        fingerprint,
        bls_pubkey,
    }
}

impl ValidatorSet {
    /// Phase 1 testnet: one logical validator (`n = 1`, `f = 0`).
    pub fn phase1_singleton() -> Self {
        Self {
            entries: vec![dev_entry_from_seed(SINGLETON_SEED)],
        }
    }

    /// Phase 2 dev fixture: seven deterministic validators (`n = 7`, `f = 2` when fully wired).
    pub fn phase2_bft7_fixture() -> Self {
        let entries = (0u8..7u8)
            .map(|i| dev_entry_from_seed(&bft7_seed(i)))
            .collect();
        Self { entries }
    }

    /// Phase 3 / M8 dev fixture: 21 deterministic validators (`n = 21`, `f = 6`, quorum `13`).
    pub fn phase3_bft21_fixture() -> Self {
        let entries = (0u8..21u8)
            .map(|i| dev_entry_from_seed(&bft21_seed(i)))
            .collect();
        Self { entries }
    }

    /// Permissionless mainnet: build a set from on-chain registry rows (caller must sort for determinism).
    pub fn from_entries(entries: Vec<ValidatorEntry>) -> Self {
        Self { entries }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// HotStuff-style leader index for `view` (stable for a given validator set).
    #[must_use]
    pub fn leader_index(&self, view: u64) -> usize {
        let n = self.len().max(1);
        (view as usize) % n
    }

    /// `BlockHeader.proposer` must equal this for the block's `view`.
    #[must_use]
    pub fn expected_proposer(&self, view: u64) -> ValidatorId {
        self.entries[self.leader_index(view)].fingerprint
    }

    /// Whether the validator at `my_index` should be the round leader for `view`
    /// (`docs/prd.md` §7 M7-c).
    #[must_use]
    pub fn is_proposer_for_view(&self, view: u64, my_index: usize) -> bool {
        self.leader_index(view) == my_index
    }

    /// HotStuff-2 / PBFT quorum threshold (`docs/prd.md` §7.3 / M7-d).
    ///
    /// Standard BFT bound: tolerate `f = floor((n - 1) / 3)` Byzantine validators and
    /// require `2f + 1` votes for a quorum certificate. Clamped to ≥ 1 so a singleton
    /// chain still requires its own vote.
    ///
    /// Sizes: n=1→1, n=2→1, n=3→1, n=4→3, n=5→3, n=6→3, n=7→5, n=21→13.
    #[must_use]
    pub fn quorum_threshold(&self) -> usize {
        let n = self.len();
        let f = n.saturating_sub(1) / 3;
        (2 * f + 1).max(1)
    }

    /// BLS public key for `idx` (used to verify a `Vote` from validator `idx`).
    #[must_use]
    pub fn bls_pubkey(&self, idx: usize) -> Option<&BlsPublicKey> {
        self.entries.get(idx).map(|e| &e.bls_pubkey)
    }

    /// Validator entry by index.
    #[must_use]
    pub fn entry(&self, idx: usize) -> Option<&ValidatorEntry> {
        self.entries.get(idx)
    }

    /// Ordered list (clone is cheap: ≤21 entries for in-repo fixtures).
    #[must_use]
    pub fn entries(&self) -> &[ValidatorEntry] {
        &self.entries
    }

    /// Ordered fingerprints (back-compat helper).
    #[must_use]
    pub fn ids(&self) -> Vec<ValidatorId> {
        self.entries.iter().map(|e| e.fingerprint).collect()
    }

    /// **Dev only.** Recover the deterministic BLS secret key for `idx` so test code
    /// and the M7-d devnet can sign votes without an external key file. Real
    /// deployments must provide keys via config (`FRACTAL_VALIDATOR_SECRET_HEX` on
    /// the node binary) and treat this as unavailable.
    ///
    /// Returns `Some` for the built-in `phase1_singleton` / `phase2_bft7_fixture` /
    /// `phase3_bft21_fixture` fixtures; future operator-provisioned sets will return `None`
    /// once construction learns to accept pre-baked pubkeys without the dev secret.
    pub fn dev_bls_secret(&self, idx: usize) -> Option<BlsSecretKey> {
        let n = self.len();
        if idx >= n {
            return None;
        }
        if n == 1 {
            return Some(dev_secret_from_seed(SINGLETON_SEED));
        }
        if n == 7 {
            return Some(dev_secret_from_seed(&bft7_seed(idx as u8)));
        }
        if n == 21 {
            return Some(dev_secret_from_seed(&bft21_seed(idx as u8)));
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn singleton_leader_always_zero() {
        let v = ValidatorSet::phase1_singleton();
        assert_eq!(v.len(), 1);
        for view in 0u64..20 {
            assert_eq!(v.leader_index(view), 0);
        }
    }

    #[test]
    fn bft7_fixture_rotates_mod_7() {
        let v = ValidatorSet::phase2_bft7_fixture();
        assert_eq!(v.len(), 7);
        assert_eq!(v.leader_index(0), 0);
        assert_eq!(v.leader_index(6), 6);
        assert_eq!(v.leader_index(7), 0);
        let mut seen = std::collections::BTreeSet::new();
        for i in 0u8..7u8 {
            seen.insert(v.entries[i as usize].fingerprint);
        }
        assert_eq!(seen.len(), 7, "fixture fingerprints must be distinct");
    }

    #[test]
    fn is_proposer_for_view_singleton_always_index_zero() {
        let v = ValidatorSet::phase1_singleton();
        for view in 0u64..20 {
            assert!(v.is_proposer_for_view(view, 0));
            assert!(!v.is_proposer_for_view(view, 1));
        }
    }

    #[test]
    fn is_proposer_for_view_bft7_each_index_owns_exactly_one_mod() {
        let v = ValidatorSet::phase2_bft7_fixture();
        for idx in 0usize..7 {
            for view in 0u64..21 {
                let want = (view as usize) % 7 == idx;
                assert_eq!(v.is_proposer_for_view(view, idx), want);
            }
        }
    }

    #[test]
    fn quorum_threshold_matches_prd_table() {
        // Sizes the PRD calls out explicitly + the small-set fallbacks.
        // Hand-built one-off sets so we exercise every n from 1..=7.
        for (n, expected) in [(1usize, 1usize), (2, 1), (3, 1), (4, 3), (5, 3), (6, 3), (7, 5)] {
            let entries = (0..n)
                .map(|i| {
                    let mut seed = [0u8; 32];
                    seed[31] = i as u8;
                    seed[0] = b'Q';
                    ValidatorEntry {
                        fingerprint: keccak256(&seed),
                        bls_pubkey: BlsSecretKey::from_ikm(&seed).unwrap().public_key(),
                    }
                })
                .collect();
            let v = ValidatorSet { entries };
            assert_eq!(
                v.quorum_threshold(),
                expected,
                "n={n} → expected {expected}"
            );
        }
    }

    #[test]
    fn bls_pubkey_matches_dev_secret_for_singleton_and_bft7() {
        let v = ValidatorSet::phase1_singleton();
        let sk = v.dev_bls_secret(0).expect("singleton has dev secret");
        assert_eq!(&sk.public_key(), v.bls_pubkey(0).unwrap());
        assert!(v.dev_bls_secret(1).is_none(), "out-of-range");
        assert!(v.bls_pubkey(1).is_none());

        let v = ValidatorSet::phase2_bft7_fixture();
        for idx in 0..7 {
            let sk = v.dev_bls_secret(idx).expect("bft7 has dev secret");
            assert_eq!(&sk.public_key(), v.bls_pubkey(idx).unwrap());
        }
        assert!(v.dev_bls_secret(7).is_none());
        assert!(v.bls_pubkey(7).is_none());
    }

    #[test]
    fn dev_bls_keys_are_distinct_across_bft7_validators() {
        let v = ValidatorSet::phase2_bft7_fixture();
        let mut seen = std::collections::BTreeSet::new();
        for idx in 0..7 {
            let pk = v.bls_pubkey(idx).unwrap().0;
            assert!(seen.insert(pk), "duplicate BLS pubkey at idx={idx}");
        }
    }

    #[test]
    fn five_of_seven_aggregate_signature_verifies_via_validator_set() {
        // End-to-end sanity that ValidatorSet keys interoperate with
        // fractal_crypto::AggregateSignature for the quorum case (5-of-7).
        use fractal_crypto::AggregateSignature;
        let v = ValidatorSet::phase2_bft7_fixture();
        let msg = b"M7-d quorum payload";
        let signers: Vec<usize> = vec![0, 1, 2, 4, 6]; // exactly threshold 5
        assert_eq!(signers.len(), v.quorum_threshold());
        let sigs: Vec<_> = signers
            .iter()
            .map(|&idx| v.dev_bls_secret(idx).unwrap().sign(msg))
            .collect();
        let pks: Vec<_> = signers.iter().map(|&idx| *v.bls_pubkey(idx).unwrap()).collect();
        let agg = AggregateSignature::from_signatures(&sigs).expect("aggregate");
        agg.verify(msg, &pks).expect("aggregate verify");
    }

    #[test]
    fn aggregate_verify_fails_when_missing_a_signer_pubkey() {
        use fractal_crypto::AggregateSignature;
        let v = ValidatorSet::phase2_bft7_fixture();
        let msg = b"quorum payload";
        let sigs: Vec<_> = (0..5).map(|i| v.dev_bls_secret(i).unwrap().sign(msg)).collect();
        let agg = AggregateSignature::from_signatures(&sigs).unwrap();
        let pks: Vec<_> = (0..4).map(|i| *v.bls_pubkey(i).unwrap()).collect(); // 4 of 5
        assert!(agg.verify(msg, &pks).is_err());
    }

    #[test]
    fn bft21_fixture_rotates_mod_21() {
        let v = ValidatorSet::phase3_bft21_fixture();
        assert_eq!(v.len(), 21);
        assert_eq!(v.quorum_threshold(), 13);
        assert_eq!(v.leader_index(0), 0);
        assert_eq!(v.leader_index(20), 20);
        assert_eq!(v.leader_index(21), 0);
        let mut seen = std::collections::BTreeSet::new();
        for i in 0u8..21u8 {
            seen.insert(v.entries[i as usize].fingerprint);
        }
        assert_eq!(seen.len(), 21);
    }

    #[test]
    fn bls_pubkey_matches_dev_secret_for_bft21() {
        let v = ValidatorSet::phase3_bft21_fixture();
        for idx in 0..21 {
            let sk = v.dev_bls_secret(idx).expect("bft21 dev secret");
            assert_eq!(&sk.public_key(), v.bls_pubkey(idx).unwrap());
        }
        assert!(v.dev_bls_secret(21).is_none());
    }

    #[test]
    fn thirteen_of_twenty_one_aggregate_verifies() {
        use fractal_crypto::AggregateSignature;
        let v = ValidatorSet::phase3_bft21_fixture();
        let msg = b"M8 BFT-21 quorum";
        assert_eq!(v.quorum_threshold(), 13);
        let signers: Vec<usize> = (0..13).collect();
        let sigs: Vec<_> = signers
            .iter()
            .map(|&idx| v.dev_bls_secret(idx).unwrap().sign(msg))
            .collect();
        let pks: Vec<_> = signers.iter().map(|&idx| *v.bls_pubkey(idx).unwrap()).collect();
        let agg = AggregateSignature::from_signatures(&sigs).expect("aggregate");
        agg.verify(msg, &pks).expect("aggregate verify");
    }
}
