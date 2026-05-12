//! Caveats — Phase 1 first six variants only (`docs/wallet.md` §4.4, §25.1).

use borsh::{BorshDeserialize, BorshSerialize};

use crate::types::{Amount, TeeType, ToolClass};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub enum Caveat {
    MaxTotalSpend(Amount),
    MaxPerCallSpend { class: ToolClass, max: Amount },
    RateLimit {
        class: ToolClass,
        count: u32,
        window_seconds: u32,
    },
    RequireApprovalAbove(Amount),
    OutputCommitmentRequired(ToolClass),
    TeeAttestationRequired { class: ToolClass, tee: TeeType },
}

impl Caveat {
    /// Whether `self` is **stricter or equal** than `other` for the same variant key (used for attenuation).
    pub fn is_stricter_or_equal(&self, other: &Caveat) -> bool {
        match (self, other) {
            (Self::MaxTotalSpend(a), Self::MaxTotalSpend(b)) => *a <= *b,
            (
                Self::MaxPerCallSpend { class: c1, max: m1 },
                Self::MaxPerCallSpend { class: c2, max: m2 },
            ) => c1 == c2 && *m1 <= *m2,
            (
                Self::RateLimit {
                    class: c1,
                    count: n1,
                    window_seconds: w1,
                },
                Self::RateLimit {
                    class: c2,
                    count: n2,
                    window_seconds: w2,
                },
            ) => c1 == c2 && *n1 <= *n2 && *w1 == *w2,
            (Self::RequireApprovalAbove(a), Self::RequireApprovalAbove(b)) => *a <= *b,
            (Self::OutputCommitmentRequired(c1), Self::OutputCommitmentRequired(c2)) => c1 == c2,
            (
                Self::TeeAttestationRequired { class: c1, tee: t1 },
                Self::TeeAttestationRequired { class: c2, tee: t2 },
            ) => c1 == c2 && t1 == t2,
            _ => false,
        }
    }
}

/// Every parent caveat must be matched by a child caveat that is stricter or equal on the same constraint key.
pub fn caveats_attenuate_parent(parent: &[Caveat], child: &[Caveat]) -> bool {
    for p in parent {
        if !child.iter().any(|c| c.is_stricter_or_equal(p)) {
            return false;
        }
    }
    true
}
