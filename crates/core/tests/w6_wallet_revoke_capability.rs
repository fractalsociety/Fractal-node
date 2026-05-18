//! §14.1 `RevokeCapability` on-chain (`docs/wallet.md` §12.3 cascade).

use ed25519_dalek::{Signer, SigningKey};
use fractal_core::{
    apply_block, is_capability_revoked, mint_capability, mint_revocation_proof_bytes,
    revoke_capability, Account, ExecError, NativeCall, State, Transaction, TxBody, VmKind,
    WalletRevokeCapabilitySignBody, HARDHAT_DEFAULT_SIGNER_0, WEI_PER_FRAC,
};
use fractal_wallet::{
    build_delegated_child_body,
    capability::{CapabilitySignBody, CapabilityToken},
    caveat::Caveat,
    child_params_for_role,
    policy::builtins::FRAC,
    types::{Scope, ToolClass},
    SubAgentRole,
};
use rand::rngs::OsRng;

fn funded_state() -> State {
    let mut state = State::default();
    state.execution_timestamp_ms = 500_000;
    state.accounts.insert(
        HARDHAT_DEFAULT_SIGNER_0,
        Account {
            nonce: 0,
            balance: 100 * WEI_PER_FRAC,
        },
    );
    state
}

fn native_tx(nonce: u64, call: NativeCall) -> Transaction {
    Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce,
        vm: VmKind::Native,
        body: TxBody::Native(call),
    }
}

fn sign_revoke(
    issuer: &SigningKey,
    cap_id: [u8; 32],
    reason_code: u8,
    cascade: bool,
) -> [u8; 64] {
    let body = WalletRevokeCapabilitySignBody {
        cap_id,
        reason_code,
        cascade,
        chain_id: 41,
    };
    let msg = borsh::to_vec(&body).unwrap();
    issuer.sign(&msg).to_bytes()
}

fn coding_root(issuer: &SigningKey, subject: &SigningKey, cap_id: [u8; 32]) -> CapabilityToken {
    let body = CapabilitySignBody {
        version: 1,
        cap_id,
        chain_id: 41,
        issuer: issuer.verifying_key().to_bytes(),
        subject: subject.verifying_key().to_bytes(),
        parent_cap_id: None,
        scope: Scope {
            workspace_id: Some(1),
            project_id: None,
            task_id: Some(1),
            tool_class_mask: ToolClass::all_phase1_mask(),
            providers: None,
        },
        caveats: vec![Caveat::MaxTotalSpend(10 * FRAC)],
        budget_account: 0,
        not_before: 0,
        not_after: 2_000_000,
        nonce: 1,
    };
    CapabilityToken::sign(body, issuer).unwrap()
}

#[test]
fn revoke_capability_rejects_duplicate() {
    let mut state = funded_state();
    let issuer = SigningKey::generate(&mut OsRng);
    let subject = SigningKey::generate(&mut OsRng);
    let cap_id = [0xaa; 32];
    let token = coding_root(&issuer, &subject, cap_id);
    let borsh = borsh::to_vec(&token).unwrap();

    let proof = mint_revocation_proof_bytes(&state, &token).unwrap();
    mint_capability(
        &mut state,
        HARDHAT_DEFAULT_SIGNER_0,
        None,
        borsh,
        None,
        proof,
    )
    .unwrap();
    let sig = sign_revoke(&issuer, cap_id, 1, false);
    revoke_capability(&mut state, cap_id, 1, false, sig).unwrap();
    let sig2 = sign_revoke(&issuer, cap_id, 2, false);
    assert_eq!(
        revoke_capability(&mut state, cap_id, 2, false, sig2),
        Err(ExecError::WalletCapabilityAlreadyRevoked)
    );
}

#[test]
fn cascade_revoke_blocks_child_mint() {
    let mut state = funded_state();
    let issuer = SigningKey::generate(&mut OsRng);
    let coding = SigningKey::generate(&mut OsRng);
    let verifier = SigningKey::generate(&mut OsRng);
    let parent_id = [0x11; 32];
    let parent = coding_root(&issuer, &coding, parent_id);
    let parent_borsh = borsh::to_vec(&parent).unwrap();

    let parent_proof = mint_revocation_proof_bytes(&state, &parent).unwrap();
    mint_capability(
        &mut state,
        HARDHAT_DEFAULT_SIGNER_0,
        None,
        parent_borsh,
        None,
        parent_proof,
    )
    .unwrap();

    let sig = sign_revoke(&issuer, parent_id, 0, true);
    revoke_capability(&mut state, parent_id, 0, true, sig).unwrap();
    assert!(is_capability_revoked(&state, &parent_id));

    let child_params = child_params_for_role(
        &parent.body,
        &SubAgentRole::verifier_default(),
        [0x22; 32],
        verifier.verifying_key().to_bytes(),
        0,
        2,
    )
    .unwrap();
    let child_body = build_delegated_child_body(&parent.body, child_params).unwrap();
    let child = CapabilityToken::sign(child_body, &issuer).unwrap();
    assert_eq!(
        mint_revocation_proof_bytes(&state, &child),
        Err(ExecError::WalletRevocationProofInvalid)
    );
}

#[test]
fn revoke_via_native_call_round_trip() {
    let mut state = funded_state();
    let issuer = SigningKey::generate(&mut OsRng);
    let subject = SigningKey::generate(&mut OsRng);
    let cap_id = [0xbb; 32];
    let token = coding_root(&issuer, &subject, cap_id);
    let proof = mint_revocation_proof_bytes(&state, &token).unwrap();
    apply_block(
        &mut state,
        &[native_tx(
            0,
            NativeCall::WalletMintCapabilityV1 {
                parent_cap_id: None,
                child_token_borsh: borsh::to_vec(&token).unwrap(),
                budget_seed: None,
                revocation_proof_borsh: proof,
            },
        )],
    )
    .unwrap();

    let sig = sign_revoke(&issuer, cap_id, 3, false);
    apply_block(
        &mut state,
        &[native_tx(
            1,
            NativeCall::WalletRevokeCapabilityV1 {
                cap_id,
                reason_code: 3,
                cascade: false,
                issuer_sig: sig,
            },
        )],
    )
    .unwrap();

    assert!(state.wallet_revocation_entries.contains_key(&cap_id));
    assert!(is_capability_revoked(&state, &cap_id));
    assert_ne!(state.wallet_revocation_merkle_root, [0u8; 32]);
}

#[test]
fn revocation_merkle_root_matches_wallet_set() {
    use fractal_wallet::{RevocationEntry, RevocationSet};

    let mut state = funded_state();
    let issuer = SigningKey::generate(&mut OsRng);
    let subject = SigningKey::generate(&mut OsRng);
    let cap_id = [0xcc; 32];
    let token = coding_root(&issuer, &subject, cap_id);
    let proof = mint_revocation_proof_bytes(&state, &token).unwrap();
    mint_capability(
        &mut state,
        HARDHAT_DEFAULT_SIGNER_0,
        None,
        borsh::to_vec(&token).unwrap(),
        None,
        proof,
    )
    .unwrap();
    let sig = sign_revoke(&issuer, cap_id, 0, false);
    revoke_capability(&mut state, cap_id, 0, false, sig).unwrap();

    let set = RevocationSet::from_entries(state.wallet_revocation_entries.iter().map(
        |(id, e)| {
            (
                *id,
                RevocationEntry {
                    revoked_at_ms: e.revoked_at_ms,
                    reason_code: e.reason_code,
                    cascade: e.cascade,
                },
            )
        },
    ));
    assert_eq!(state.wallet_revocation_merkle_root, set.root());
}
