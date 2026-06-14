use crate::types::Amount;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WalletFinalityStatus {
    Soft,
    Proof,
}

impl WalletFinalityStatus {
    pub fn is_proof_final(self) -> bool {
        matches!(self, Self::Proof)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HighValueFinalityPolicy {
    pub high_value_threshold: Amount,
    pub require_proof_for_high_value: bool,
}

impl HighValueFinalityPolicy {
    pub const fn new(high_value_threshold: Amount) -> Self {
        Self {
            high_value_threshold,
            require_proof_for_high_value: true,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WalletFinalityWarning {
    pub amount: Amount,
    pub threshold: Amount,
    pub finality_status: WalletFinalityStatus,
    pub code: &'static str,
    pub message: &'static str,
}

pub fn warn_if_high_value_soft_final(
    amount: Amount,
    finality_status: WalletFinalityStatus,
    policy: HighValueFinalityPolicy,
) -> Option<WalletFinalityWarning> {
    if !policy.require_proof_for_high_value {
        return None;
    }
    if amount < policy.high_value_threshold || finality_status.is_proof_final() {
        return None;
    }
    Some(WalletFinalityWarning {
        amount,
        threshold: policy.high_value_threshold,
        finality_status,
        code: "high_value_soft_final",
        message: "High-value action is only soft-final; wait for proof finality before relying on settlement.",
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::policy::builtins::FRAC;

    #[test]
    fn warns_for_high_value_soft_final_action() {
        let warning = warn_if_high_value_soft_final(
            10 * FRAC,
            WalletFinalityStatus::Soft,
            HighValueFinalityPolicy::new(5 * FRAC),
        )
        .expect("warning");

        assert_eq!(warning.code, "high_value_soft_final");
        assert_eq!(warning.amount, 10 * FRAC);
        assert_eq!(warning.threshold, 5 * FRAC);
    }

    #[test]
    fn does_not_warn_for_proof_final_high_value_action() {
        assert_eq!(
            warn_if_high_value_soft_final(
                10 * FRAC,
                WalletFinalityStatus::Proof,
                HighValueFinalityPolicy::new(5 * FRAC),
            ),
            None
        );
    }

    #[test]
    fn does_not_warn_for_low_value_soft_final_action() {
        assert_eq!(
            warn_if_high_value_soft_final(
                FRAC,
                WalletFinalityStatus::Soft,
                HighValueFinalityPolicy::new(5 * FRAC),
            ),
            None
        );
    }
}
