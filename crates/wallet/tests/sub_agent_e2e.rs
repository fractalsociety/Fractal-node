//! Production sub-agent delegation E2E (`docs/wallet.md` §12.2, §19.2).

use ed25519_dalek::SigningKey;
use fractal_wallet::{
    capability::{CapabilitySignBody, CapabilityToken},
    caveat::Caveat,
    delegate_sub_agent_production, policy::builtins::FRAC, run_verifier_tool_session_e2e,
    types::{Scope, ToolClass},
    BudgetAccount, SubAgentRole, ToolMarket,
};
use rand::rngs::OsRng;

#[test]
fn sub_agent_production_path_matches_session_module() {
    let mut rng = OsRng;
    let issuer = SigningKey::generate(&mut rng);
    let coding = SigningKey::generate(&mut rng);
    let verifier = SigningKey::generate(&mut rng);
    let provider = SigningKey::generate(&mut rng);

    let parent_body = CapabilitySignBody {
        version: 1,
        cap_id: [0x11; 32],
        chain_id: 41,
        issuer: issuer.verifying_key().to_bytes(),
        subject: coding.verifying_key().to_bytes(),
        parent_cap_id: None,
        scope: Scope {
            workspace_id: None,
            project_id: None,
            task_id: Some(99),
            tool_class_mask: ToolClass::all_phase1_mask(),
            providers: None,
        },
        caveats: vec![Caveat::MaxTotalSpend(10 * FRAC)],
        budget_account: 1,
        not_before: 0,
        not_after: 10_000_000,
        nonce: 1,
    };
    let parent = CapabilityToken::sign(parent_body, &issuer).unwrap();
    let mut parent_budget = BudgetAccount::new(1, None, 10 * FRAC);

    let bundle = delegate_sub_agent_production(
        &parent,
        &issuer,
        &mut parent_budget,
        2,
        2 * FRAC,
        SubAgentRole::verifier_default(),
        &verifier,
        [0x22; 32],
    )
    .unwrap();

    assert_ne!(
        bundle.parent_token.body.subject,
        bundle.child_token.body.subject
    );

    let mut market = ToolMarket::default();
    let out = run_verifier_tool_session_e2e(&bundle, 99, FRAC, 50_000, &mut market, &provider).unwrap();
    assert!(out.settled);
}
