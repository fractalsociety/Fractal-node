//! Revocation SMT-style proofs at verify time (`docs/wallet.md` §4.6).

use ed25519_dalek::SigningKey;
use fractal_wallet::{
    capability::{CapabilitySignBody, CapabilityToken},
    caveat::Caveat,
    policy::builtins::FRAC,
    types::{Scope, ToolClass},
    verify_capability_with_revocation, RevocationEntry, RevocationSet,
};
use rand::rngs::OsRng;

#[test]
fn provider_verify_path_under_5ms_budget_smoke() {
    let mut rng = OsRng;
    let issuer = SigningKey::generate(&mut rng);
    let subject = SigningKey::generate(&mut rng);
    let parent_id = [1u8; 32];
    let child_id = [2u8; 32];
    let body = CapabilitySignBody {
        version: 1,
        cap_id: child_id,
        chain_id: 41,
        issuer: issuer.verifying_key().to_bytes(),
        subject: subject.verifying_key().to_bytes(),
        parent_cap_id: Some(parent_id),
        scope: Scope {
            workspace_id: None,
            project_id: None,
            task_id: None,
            tool_class_mask: ToolClass::TestRunner.bit(),
            providers: None,
        },
        caveats: vec![Caveat::MaxTotalSpend(FRAC)],
        budget_account: 0,
        not_before: 0,
        not_after: 9_999_999,
        nonce: 1,
    };
    let token = CapabilityToken::sign(body, &issuer).unwrap();

    let mut set = RevocationSet::default();
    set.revoke(
        parent_id,
        RevocationEntry {
            revoked_at_ms: 1,
            reason_code: 0,
            cascade: false,
        },
    )
    .unwrap();
    let root = set.root();
    let proof = set.build_verify_proof(child_id, &[parent_id]).unwrap();
    verify_capability_with_revocation(&token, 1000, &root, &[parent_id], &proof).unwrap();
}
