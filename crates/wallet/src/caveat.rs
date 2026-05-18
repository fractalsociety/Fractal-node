//! Caveats — Phase 1 first six variants plus `NoRecursion` for §12 sub-agent delegation.

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
    /// When present on a parent capability, that token must not mint further child capabilities (`docs/wallet.md` §12.1).
    NoRecursion,
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
            (Self::NoRecursion, Self::NoRecursion) => true,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_recursion_borsh_round_trip() {
        let c = Caveat::NoRecursion;
        let v = borsh::to_vec(&c).unwrap();
        let d: Caveat = borsh::from_slice(&v).unwrap();
        assert_eq!(c, d);
    }

    #[test]
    fn child_may_add_no_recursion_when_parent_lacks_it() {
        let parent = vec![Caveat::MaxTotalSpend(100)];
        let child = vec![Caveat::MaxTotalSpend(50), Caveat::NoRecursion];
        assert!(caveats_attenuate_parent(&parent, &child));
    }

    #[test]
    fn parent_no_recursion_requires_child_no_recursion() {
        let parent = vec![Caveat::MaxTotalSpend(100), Caveat::NoRecursion];
        let child_ok = vec![Caveat::MaxTotalSpend(50), Caveat::NoRecursion];
        assert!(caveats_attenuate_parent(&parent, &child_ok));
        let child_bad = vec![Caveat::MaxTotalSpend(50)];
        assert!(!caveats_attenuate_parent(&parent, &child_bad));
    }
}
