//! Production sub-agent delegation flows (`docs/wallet.md` §12, §19.2).

use ed25519_dalek::SigningKey;

use crate::budget::{BudgetAccount, BudgetAccountId};
use crate::capability::{CapabilityId, CapabilitySignBody, CapabilityToken};
use crate::caveat::Caveat;
use crate::delegation::{
    allocate_budget_and_build_delegated_child_body, parent_allows_sub_agent_delegation,
    tightest_max_total_spend, DelegatedChildParams, DelegationError,
};
use crate::market::{
    provider_id_from_public_key, provider_verify_intent_capability, AgentCapabilityPresentation,
    MatchError, PostIntentError, ProviderCapabilityVerifyError, ToolIntent, ToolIntentBody,
    ToolMarket,
};
use crate::revocation::RevocationSet;
use crate::policy::builtins::FRAC;
use crate::types::{Amount, IntentId, Scope, TaskId, TimestampMs, ToolClass, VerificationTier};

/// Preset child profiles for common sub-agent roles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SubAgentRole {
    /// Verifier: `TEST_RUNNER` only, `NoRecursion`, shorter lifetime (§12.2 / §19.2).
    Verifier {
        max_total_spend: Amount,
        lifetime_ms: TimestampMs,
    },
}

impl SubAgentRole {
    /// Default verifier slice: 2 FRAC cap, 30-minute window.
    #[must_use]
    pub fn verifier_default() -> Self {
        Self::Verifier {
            max_total_spend: 2 * FRAC,
            lifetime_ms: 30 * 60 * 1000,
        }
    }
}

/// Fully wired delegation: signed child token + linked budgets + verification report.
#[derive(Clone, Debug)]
pub struct ProductionDelegationBundle {
    pub parent_token: CapabilityToken,
    pub child_token: CapabilityToken,
    pub child_subject: SigningKey,
    pub parent_budget: BudgetAccount,
    pub child_budget: BudgetAccount,
    pub report: DelegationVerificationReport,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DelegationVerificationReport {
    pub parent_signature_ok: bool,
    pub child_signature_ok: bool,
    pub attenuation_ok: bool,
    pub parent_allows_delegation: bool,
    pub budget_linked_ok: bool,
    pub child_has_no_recursion: bool,
    pub child_tool_mask_subset: bool,
}

impl DelegationVerificationReport {
    #[must_use]
    pub fn all_ok(&self) -> bool {
        self.parent_signature_ok
            && self.child_signature_ok
            && self.attenuation_ok
            && self.parent_allows_delegation
            && self.budget_linked_ok
            && self.child_has_no_recursion
            && self.child_tool_mask_subset
    }
}

/// Outcome of a verifier sub-agent exercising the tool market on the child budget.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifierSessionOutcome {
    pub intent_id: IntentId,
    pub tool_class: ToolClass,
    pub price: Amount,
    pub child_budget_spent: Amount,
    pub child_budget_available: Amount,
    pub settled: bool,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SessionE2eError {
    #[error("provider capability verification: {0}")]
    ProviderCapability(#[from] ProviderCapabilityVerifyError),
    #[error(transparent)]
    Delegation(#[from] DelegationError),
    #[error("capability does not authorize tool class {0:?}")]
    ToolNotAuthorized(ToolClass),
    #[error("intent post failed: {0:?}")]
    PostIntent(PostIntentError),
    #[error("match failed: {0:?}")]
    Match(MatchError),
    #[error("delegation verification failed")]
    VerificationFailed,
    #[error("market settle failed")]
    SettleFailed,
}

/// Build child params for `role` strictly narrower than `parent`.
pub fn child_params_for_role(
    parent: &CapabilitySignBody,
    role: &SubAgentRole,
    child_cap_id: CapabilityId,
    child_subject: [u8; 32],
    child_budget_account: BudgetAccountId,
    child_nonce: u64,
) -> Result<DelegatedChildParams, DelegationError> {
    if !parent_allows_sub_agent_delegation(parent) {
        return Err(DelegationError::ParentNoRecursion);
    }
    let (tool_class_mask, max_total_spend, lifetime_ms) = match role {
        SubAgentRole::Verifier {
            max_total_spend,
            lifetime_ms,
        } => (ToolClass::TestRunner.bit(), *max_total_spend, *lifetime_ms),
    };
    if tool_class_mask & parent.scope.tool_class_mask != tool_class_mask {
        return Err(DelegationError::NotAttenuated);
    }
    let child_not_after = parent
        .not_before
        .saturating_add(lifetime_ms)
        .min(parent.not_after);
    if child_not_after <= parent.not_before {
        return Err(DelegationError::NotAttenuated);
    }
    let mut child_caveats = parent.caveats.clone();
    apply_max_total_spend(&mut child_caveats, max_total_spend);
    if !child_caveats.iter().any(|c| matches!(c, Caveat::NoRecursion)) {
        child_caveats.push(Caveat::NoRecursion);
    }
    Ok(DelegatedChildParams {
        cap_id: child_cap_id,
        subject: child_subject,
        scope: Scope {
            workspace_id: parent.scope.workspace_id,
            project_id: parent.scope.project_id,
            task_id: parent.scope.task_id,
            tool_class_mask,
            providers: parent.scope.providers.clone(),
        },
        not_before: parent.not_before,
        not_after: child_not_after,
        caveats: child_caveats,
        budget_account: child_budget_account,
        nonce: child_nonce,
    })
}

fn apply_max_total_spend(caveats: &mut Vec<Caveat>, cap: Amount) {
    let mut applied = false;
    for c in caveats.iter_mut() {
        if let Caveat::MaxTotalSpend(v) = c {
            *v = (*v).min(cap);
            applied = true;
        }
    }
    if !applied {
        caveats.push(Caveat::MaxTotalSpend(cap));
    }
}

/// Mint a production child capability, split parent→child budget, and verify invariants.
pub fn delegate_sub_agent_production(
    parent_token: &CapabilityToken,
    issuer_sk: &SigningKey,
    parent_budget: &mut BudgetAccount,
    child_budget_id: BudgetAccountId,
    delegate_amount: Amount,
    role: SubAgentRole,
    child_subject_sk: &SigningKey,
    child_cap_id: CapabilityId,
) -> Result<ProductionDelegationBundle, DelegationError> {
    parent_token.verify().map_err(|_| DelegationError::NotAttenuated)?;
    let params = child_params_for_role(
        &parent_token.body,
        &role,
        child_cap_id,
        child_subject_sk.verifying_key().to_bytes(),
        child_budget_id,
        parent_token.body.nonce.saturating_add(1),
    )?;
    let mut child_budget = BudgetAccount::new(child_budget_id, Some(parent_budget.id), 0);
    let child_body = allocate_budget_and_build_delegated_child_body(
        &parent_token.body,
        parent_budget,
        &mut child_budget,
        delegate_amount,
        params,
    )?;
    let child_token = CapabilityToken::sign(child_body, issuer_sk).map_err(|_| {
        DelegationError::NotAttenuated
    })?;
    let report = verify_delegation_pair(parent_token, &child_token, parent_budget, &child_budget);
    if !report.all_ok() {
        return Err(DelegationError::NotAttenuated);
    }
    Ok(ProductionDelegationBundle {
        parent_token: parent_token.clone(),
        child_token,
        child_subject: child_subject_sk.clone(),
        parent_budget: parent_budget.clone(),
        child_budget,
        report,
    })
}

#[must_use]
pub fn verify_delegation_pair(
    parent: &CapabilityToken,
    child: &CapabilityToken,
    parent_budget: &BudgetAccount,
    child_budget: &BudgetAccount,
) -> DelegationVerificationReport {
    let parent_signature_ok = parent.verify().is_ok();
    let child_signature_ok = child.verify().is_ok();
    let attenuation_ok =
        CapabilityToken::verify_attenuation_from_parent(&child.body, &parent.body);
    let parent_allows_delegation = parent_allows_sub_agent_delegation(&parent.body);
    let budget_linked_ok =
        child_budget.parent == Some(parent_budget.id) && child_budget.id != parent_budget.id;
    let child_has_no_recursion = child
        .body
        .caveats
        .iter()
        .any(|c| matches!(c, Caveat::NoRecursion));
    let child_tool_mask_subset =
        (child.body.scope.tool_class_mask & parent.body.scope.tool_class_mask)
            == child.body.scope.tool_class_mask;
    DelegationVerificationReport {
        parent_signature_ok,
        child_signature_ok,
        attenuation_ok,
        parent_allows_delegation,
        budget_linked_ok,
        child_has_no_recursion,
        child_tool_mask_subset,
    }
}

#[must_use]
pub fn capability_allows_tool(cap: &CapabilityToken, class: ToolClass) -> bool {
    cap.body.scope.tool_class_mask & class.bit() != 0
}

/// Verifier sub-agent posts a `TEST_RUNNER` intent on the **child** budget and settles (trusted).
pub fn run_verifier_tool_session_e2e(
    bundle: &ProductionDelegationBundle,
    task_id: TaskId,
    max_price: Amount,
    now_ms: TimestampMs,
    market: &mut ToolMarket,
    provider_sk: &SigningKey,
) -> Result<VerifierSessionOutcome, SessionE2eError> {
    if !bundle.report.all_ok() {
        return Err(SessionE2eError::VerificationFailed);
    }
    let class = ToolClass::TestRunner;
    if !capability_allows_tool(&bundle.child_token, class) {
        return Err(SessionE2eError::ToolNotAuthorized(class));
    }
    if let Some(child_max) = tightest_max_total_spend(&bundle.child_token.body.caveats) {
        if max_price > child_max {
            return Err(SessionE2eError::Delegation(
                DelegationError::MaxSpendExceedsDelegation {
                    child_max,
                    delegated: max_price,
                },
            ));
        }
    }
    let mut child_budget = bundle.child_budget.clone();
    let intent_body = ToolIntentBody {
        intent_id: [0xee; 32],
        agent_session: bundle.child_token.body.subject,
        task_id,
        tool_class: class,
        payload_commitment: [0xab; 32],
        max_price,
        verification_tier: VerificationTier::Trusted,
        deadline_ms: bundle.child_token.body.not_after,
        nonce: 1,
    };
    let intent = ToolIntent::sign(intent_body, &bundle.child_subject)
        .map_err(|_| SessionE2eError::VerificationFailed)?;
    let mut ancestors = Vec::new();
    if let Some(parent) = bundle.child_token.body.parent_cap_id {
        ancestors.push(parent);
    }
    let revocation_set = RevocationSet::default();
    let revocation_proof = revocation_set
        .build_verify_proof(bundle.child_token.body.cap_id, &ancestors)
        .map_err(|_| SessionE2eError::VerificationFailed)?;
    let revocation_root = revocation_proof.revocation_root;
    let cap_pres = AgentCapabilityPresentation {
        token: &bundle.child_token,
        revocation_root: &revocation_root,
        ancestor_chain: &ancestors,
        revocation_proof: &revocation_proof,
    };
    provider_verify_intent_capability(&intent, &cap_pres, now_ms)?;
    market
        .post_intent(intent)
        .map_err(SessionE2eError::PostIntent)?;
    let provider_pk = provider_sk.verifying_key().to_bytes();
    let provider_id = provider_id_from_public_key(&provider_pk);
    let quote_body = crate::market::QuoteBody {
        quote_id: [0xcc; 32],
        intent_id: [0xee; 32],
        provider_id,
        price: max_price,
        expiry_ms: now_ms.saturating_add(60_000),
    };
    let quote = crate::market::Quote::sign(quote_body, provider_sk)
        .map_err(|_| SessionE2eError::VerificationFailed)?;
    let mut stake = crate::market::ProviderStake {
        amount: 10 * FRAC,
        locked: 0,
    };
    market
        .match_intent(
            [0xee; 32],
            &quote,
            &mut child_budget,
            &mut stake,
            &provider_pk,
            now_ms,
        )
        .map_err(SessionE2eError::Match)?;
    market
        .post_receipt([0xee; 32], [0x99; 32], now_ms)
        .map_err(|_| SessionE2eError::SettleFailed)?;
    market
        .settle_trusted([0xee; 32], &mut child_budget, &mut stake)
        .map_err(|_| SessionE2eError::SettleFailed)?;
    Ok(VerifierSessionOutcome {
        intent_id: [0xee; 32],
        tool_class: class,
        price: max_price,
        child_budget_spent: child_budget.spent,
        child_budget_available: child_budget.available(),
        settled: true,
    })
}

/// True when parent and child sessions are distinct keys (§19.2 separation of duties).
#[must_use]
pub fn sessions_are_distinct(bundle: &ProductionDelegationBundle) -> bool {
    bundle.parent_token.body.subject != bundle.child_token.body.subject
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilitySignBody;
    use ed25519_dalek::SigningKey;
    use rand::rngs::OsRng;

    fn coding_parent(issuer: &SigningKey, subject: &SigningKey) -> CapabilityToken {
        let body = CapabilitySignBody {
            version: 1,
            cap_id: [1u8; 32],
            chain_id: 41,
            issuer: issuer.verifying_key().to_bytes(),
            subject: subject.verifying_key().to_bytes(),
            parent_cap_id: None,
            scope: Scope {
                workspace_id: Some(1),
                project_id: None,
                task_id: Some(42),
                tool_class_mask: ToolClass::all_phase1_mask(),
                providers: None,
            },
            caveats: vec![Caveat::MaxTotalSpend(10 * FRAC)],
            budget_account: 1,
            not_before: 0,
            not_after: 3_600_000,
            nonce: 1,
        };
        CapabilityToken::sign(body, issuer).unwrap()
    }

    #[test]
    fn production_delegate_and_verifier_e2e() {
        let mut rng = OsRng;
        let issuer = SigningKey::generate(&mut rng);
        let coding = SigningKey::generate(&mut rng);
        let verifier = SigningKey::generate(&mut rng);
        let provider = SigningKey::generate(&mut rng);

        let parent = coding_parent(&issuer, &coding);
        let mut parent_budget = BudgetAccount::new(1, None, 10 * FRAC);

        let bundle = delegate_sub_agent_production(
            &parent,
            &issuer,
            &mut parent_budget,
            2,
            2 * FRAC,
            SubAgentRole::verifier_default(),
            &verifier,
            [2u8; 32],
        )
        .unwrap();

        assert!(bundle.report.all_ok());
        assert_eq!(parent_budget.total_deposited, 8 * FRAC);
        assert_eq!(bundle.child_budget.total_deposited, 2 * FRAC);
        assert!(capability_allows_tool(&bundle.child_token, ToolClass::TestRunner));
        assert!(sessions_are_distinct(&bundle));

        let mut market = ToolMarket::default();
        let outcome = run_verifier_tool_session_e2e(
            &bundle,
            42,
            FRAC,
            1000,
            &mut market,
            &provider,
        )
        .unwrap();
        assert!(outcome.settled);
        assert_eq!(outcome.child_budget_spent, FRAC);
    }
}
