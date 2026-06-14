//! Sub-agent delegation — attenuated child capabilities + linked budget split (`docs/wallet.md` §12).

pub mod session;

use thiserror::Error;

use crate::budget::{BudgetAccount, BudgetError};
use crate::capability::{CapabilityId, CapabilitySignBody, CapabilityToken};
use crate::caveat::Caveat;
use crate::types::{Amount, PublicKey, Scope, TimestampMs};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DelegatedChildParams {
    pub cap_id: CapabilityId,
    pub subject: PublicKey,
    pub scope: Scope,
    pub not_before: TimestampMs,
    pub not_after: TimestampMs,
    pub caveats: Vec<Caveat>,
    pub budget_account: u64,
    pub nonce: u64,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum DelegationError {
    #[error("parent capability carries NoRecursion; sub-agent mint disallowed")]
    ParentNoRecursion,
    #[error("delegated capability body is not a strict attenuation of the parent")]
    NotAttenuated,
    #[error(transparent)]
    Budget(#[from] BudgetError),
    #[error("child MaxTotalSpend ({child_max}) exceeds delegated budget amount ({delegated})")]
    MaxSpendExceedsDelegation {
        child_max: Amount,
        delegated: Amount,
    },
}

/// True if `parent` may mint a strictly attenuated child capability (§12.1).
pub fn parent_allows_sub_agent_delegation(parent: &CapabilitySignBody) -> bool {
    !parent
        .caveats
        .iter()
        .any(|c| matches!(c, Caveat::NoRecursion))
}

/// Tightest `MaxTotalSpend` bound implied by `caveats` (minimum of all such caveats), if any.
pub fn tightest_max_total_spend(caveats: &[Caveat]) -> Option<Amount> {
    caveats
        .iter()
        .filter_map(|c| match c {
            Caveat::MaxTotalSpend(a) => Some(*a),
            _ => None,
        })
        .min()
}

/// Build a child [`CapabilitySignBody`] linked to `parent` and passing [`CapabilityToken::verify_attenuation_from_parent`].
pub fn build_delegated_child_body(
    parent: &CapabilitySignBody,
    params: DelegatedChildParams,
) -> Result<CapabilitySignBody, DelegationError> {
    if !parent_allows_sub_agent_delegation(parent) {
        return Err(DelegationError::ParentNoRecursion);
    }
    let body = CapabilitySignBody {
        version: parent.version,
        cap_id: params.cap_id,
        chain_id: parent.chain_id,
        issuer: parent.issuer,
        subject: params.subject,
        parent_cap_id: Some(parent.cap_id),
        scope: params.scope,
        caveats: params.caveats,
        budget_account: params.budget_account,
        not_before: params.not_before,
        not_after: params.not_after,
        nonce: params.nonce,
    };
    if !CapabilityToken::verify_attenuation_from_parent(&body, parent) {
        return Err(DelegationError::NotAttenuated);
    }
    Ok(body)
}

/// Validates delegation, then moves `delegate_amount` along the linked parent→child budget accounts (§12.1).
///
/// Child `MaxTotalSpend` caveats, if present, must not exceed `delegate_amount`.
pub fn allocate_budget_and_build_delegated_child_body(
    parent_cap: &CapabilitySignBody,
    parent_budget: &mut BudgetAccount,
    child_budget: &mut BudgetAccount,
    delegate_amount: Amount,
    params: DelegatedChildParams,
) -> Result<CapabilitySignBody, DelegationError> {
    if let Some(child_max) = tightest_max_total_spend(&params.caveats) {
        if child_max > delegate_amount {
            return Err(DelegationError::MaxSpendExceedsDelegation {
                child_max,
                delegated: delegate_amount,
            });
        }
    }
    let body = build_delegated_child_body(parent_cap, params)?;
    BudgetAccount::allocate_to_linked_child(parent_budget, child_budget, delegate_amount)?;
    Ok(body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::ToolClass;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn scope_phase1() -> Scope {
        Scope {
            workspace_id: None,
            project_id: None,
            task_id: None,
            tool_class_mask: ToolClass::all_phase1_mask(),
            providers: None,
        }
    }

    #[test]
    fn delegate_verifier_style_body_and_budget() {
        let mut rng = OsRng;
        let issuer = SigningKey::generate(&mut rng);
        let coding_agent = SigningKey::generate(&mut rng);
        let verifier = SigningKey::generate(&mut rng);

        let parent_body = CapabilitySignBody {
            version: 1,
            cap_id: [7u8; 32],
            chain_id: 41,
            issuer: issuer.verifying_key().to_bytes(),
            subject: coding_agent.verifying_key().to_bytes(),
            parent_cap_id: None,
            scope: scope_phase1(),
            caveats: vec![Caveat::MaxTotalSpend(100)],
            budget_account: 1,
            not_before: 0,
            not_after: 1_000_000,
            nonce: 1,
        };
        let parent = CapabilityToken::sign(parent_body, &issuer).unwrap();

        let mut parent_budget = BudgetAccount::new(1, None, 1000);
        let mut child_budget = BudgetAccount::new(2, Some(1), 0);

        let child_scope = Scope {
            workspace_id: None,
            project_id: None,
            task_id: None,
            tool_class_mask: ToolClass::Browser.bit(),
            providers: None,
        };

        let params = DelegatedChildParams {
            cap_id: [8u8; 32],
            subject: verifier.verifying_key().to_bytes(),
            scope: child_scope,
            not_before: 10,
            not_after: 500_000,
            caveats: vec![Caveat::MaxTotalSpend(40)],
            budget_account: 2,
            nonce: 2,
        };

        let child_body = allocate_budget_and_build_delegated_child_body(
            &parent.body,
            &mut parent_budget,
            &mut child_budget,
            50,
            params,
        )
        .unwrap();

        assert_eq!(parent_budget.total_deposited, 950);
        assert_eq!(child_budget.total_deposited, 50);

        let child = CapabilityToken::sign(child_body, &issuer).unwrap();
        child.verify().unwrap();
    }

    #[test]
    fn max_spend_over_delegation_amount_rejected_before_budget_move() {
        let mut rng = OsRng;
        let issuer = SigningKey::generate(&mut rng);
        let s1 = SigningKey::generate(&mut rng);
        let s2 = SigningKey::generate(&mut rng);

        let parent_body = CapabilitySignBody {
            version: 1,
            cap_id: [1u8; 32],
            chain_id: 41,
            issuer: issuer.verifying_key().to_bytes(),
            subject: s1.verifying_key().to_bytes(),
            parent_cap_id: None,
            scope: scope_phase1(),
            caveats: vec![Caveat::MaxTotalSpend(100)],
            budget_account: 1,
            not_before: 0,
            not_after: 1_000_000,
            nonce: 1,
        };
        let parent = CapabilityToken::sign(parent_body, &issuer).unwrap();

        let mut parent_budget = BudgetAccount::new(1, None, 1000);
        let mut child_budget = BudgetAccount::new(2, Some(1), 0);

        let params = DelegatedChildParams {
            cap_id: [2u8; 32],
            subject: s2.verifying_key().to_bytes(),
            scope: Scope {
                tool_class_mask: ToolClass::Browser.bit(),
                ..scope_phase1()
            },
            not_before: 0,
            not_after: 500_000,
            caveats: vec![Caveat::MaxTotalSpend(80)],
            budget_account: 2,
            nonce: 2,
        };

        let err = allocate_budget_and_build_delegated_child_body(
            &parent.body,
            &mut parent_budget,
            &mut child_budget,
            50,
            params,
        )
        .unwrap_err();
        assert_eq!(
            err,
            DelegationError::MaxSpendExceedsDelegation {
                child_max: 80,
                delegated: 50
            }
        );
        assert_eq!(parent_budget.total_deposited, 1000);
        assert_eq!(child_budget.total_deposited, 0);
    }

    #[test]
    fn parent_no_recursion_errors_before_attenuation() {
        let mut rng = OsRng;
        let issuer = SigningKey::generate(&mut rng);
        let s1 = SigningKey::generate(&mut rng);
        let s2 = SigningKey::generate(&mut rng);

        let parent_body = CapabilitySignBody {
            version: 1,
            cap_id: [3u8; 32],
            chain_id: 41,
            issuer: issuer.verifying_key().to_bytes(),
            subject: s1.verifying_key().to_bytes(),
            parent_cap_id: None,
            scope: scope_phase1(),
            caveats: vec![Caveat::MaxTotalSpend(100), Caveat::NoRecursion],
            budget_account: 1,
            not_before: 0,
            not_after: 1_000_000,
            nonce: 1,
        };
        let parent = CapabilityToken::sign(parent_body, &issuer).unwrap();

        let params = DelegatedChildParams {
            cap_id: [4u8; 32],
            subject: s2.verifying_key().to_bytes(),
            scope: Scope {
                tool_class_mask: ToolClass::Browser.bit(),
                ..scope_phase1()
            },
            not_before: 0,
            not_after: 500_000,
            caveats: vec![Caveat::MaxTotalSpend(50), Caveat::NoRecursion],
            budget_account: 1,
            nonce: 2,
        };

        let err = build_delegated_child_body(&parent.body, params).unwrap_err();
        assert_eq!(err, DelegationError::ParentNoRecursion);
    }
}
