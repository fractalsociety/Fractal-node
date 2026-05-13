//! Validator identities and view-based leader selection (`docs/prd.md` §7.2–7.4, §18 M7-b).
//!
//! `ValidatorId` is a 32-byte public fingerprint (BLS or Ed25519-derived in production). Phase 1
//! uses a single deterministic id; the BFT-7 fixture rotates `expected_proposer(view)` with
//! `view % 7` so one dev binary can simulate round-robin headers before vote gossip lands.

use fractal_crypto::hash::keccak256;

pub type ValidatorId = [u8; 32];

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValidatorSet {
    ids: Vec<ValidatorId>,
}

impl ValidatorSet {
    /// Phase 1 testnet: one logical validator (`n = 1`, `f = 0`).
    pub fn phase1_singleton() -> Self {
        Self {
            ids: vec![keccak256(b"FC_PHASE1_SINGLETON_V0")],
        }
    }

    /// Phase 2 dev fixture: seven deterministic ids (`n = 7`, `f = 2` when fully wired).
    pub fn phase2_bft7_fixture() -> Self {
        let ids = (0u8..7u8)
            .map(|i| {
                let mut seed = *b"FRACTALCHAIN_BFT7_V0____________";
                seed[31] = i;
                keccak256(&seed)
            })
            .collect();
        Self { ids }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.ids.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
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
        self.ids[self.leader_index(view)]
    }

    /// Whether the validator at `my_index` should be the round leader for `view`.
    ///
    /// Used by `producer_loop` to gate block production on the local node's
    /// `FRACTAL_VALIDATOR_INDEX` (`docs/prd.md` §7 M7-c) — only the round
    /// leader proposes; followers skip the produce tick. For singleton sets
    /// this is always `my_index == 0`.
    #[must_use]
    pub fn is_proposer_for_view(&self, view: u64, my_index: usize) -> bool {
        self.leader_index(view) == my_index
    }

    /// Ordered list for RPC / debugging (clone is cheap: ≤7 ids).
    #[must_use]
    pub fn ids(&self) -> &[ValidatorId] {
        &self.ids
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
            seen.insert(v.ids[i as usize]);
        }
        assert_eq!(seen.len(), 7, "fixture ids must be distinct");
    }

    #[test]
    fn is_proposer_for_view_singleton_always_index_zero() {
        let v = ValidatorSet::phase1_singleton();
        for view in 0u64..20 {
            assert!(v.is_proposer_for_view(view, 0));
            assert!(!v.is_proposer_for_view(view, 1)); // out-of-range never matches
        }
    }

    #[test]
    fn is_proposer_for_view_bft7_each_index_owns_exactly_one_mod() {
        let v = ValidatorSet::phase2_bft7_fixture();
        for idx in 0usize..7 {
            for view in 0u64..21 {
                let want = (view as usize) % 7 == idx;
                assert_eq!(
                    v.is_proposer_for_view(view, idx),
                    want,
                    "idx={idx} view={view} expected {want}"
                );
            }
        }
    }
}
