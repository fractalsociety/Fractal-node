//! §14.1–14.2 on-chain `MintCapability` and budget native calls (`docs/wallet.md`).

use ed25519_dalek::SigningKey;
use fractal_core::{
    apply_block, mint_revocation_proof_bytes, Account, ExecError, NativeCall, State, Transaction,
    TxBody, VmKind, WalletBudgetSeed, HARDHAT_DEFAULT_SIGNER_0, WEI_PER_FRAC,
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

fn mint_call(
    parent_cap_id: Option<[u8; 32]>,
    child_token_borsh: Vec<u8>,
    budget_seed: Option<WalletBudgetSeed>,
    revocation_proof_borsh: Vec<u8>,
) -> NativeCall {
    NativeCall::WalletMintCapabilityV1 {
        parent_cap_id,
        child_token_borsh,
        budget_seed,
        revocation_proof_borsh,
    }
}

fn proof_for_token(state: &State, token_bytes: &[u8]) -> Vec<u8> {
    let child: CapabilityToken = borsh::from_slice(token_bytes).unwrap();
    mint_revocation_proof_bytes(state, &child).unwrap()
}

fn native_tx(nonce: u64, call: NativeCall) -> Transaction {
    Transaction {
        signer: HARDHAT_DEFAULT_SIGNER_0,
        nonce,
        vm: VmKind::Native,
        body: TxBody::Native(call),
    }
}

fn coding_root_cap(issuer: &SigningKey, subject: &SigningKey, budget_id: u64) -> CapabilityToken {
    let body = CapabilitySignBody {
        version: 1,
        cap_id: [0xaa; 32],
        chain_id: 41,
        issuer: issuer.verifying_key().to_bytes(),
        subject: subject.verifying_key().to_bytes(),
        parent_cap_id: None,
        scope: Scope {
            workspace_id: Some(1),
            project_id: None,
            task_id: Some(7),
            tool_class_mask: ToolClass::all_phase1_mask(),
            providers: None,
        },
        caveats: vec![Caveat::MaxTotalSpend(10 * FRAC)],
        budget_account: budget_id,
        not_before: 0,
        not_after: 1_000_000,
        nonce: 1,
    };
    CapabilityToken::sign(body, issuer).unwrap()
}

#[test]
fn wallet_create_fund_and_close_budget() {
    let mut state = funded_state();
    apply_block(
        &mut state,
        &[native_tx(
            0,
            NativeCall::WalletCreateBudgetAccountV1 {
                parent: None,
                initial_deposit: 5 * WEI_PER_FRAC,
            },
        )],
    )
    .unwrap();
    let b = state.wallet_budgets.get(&1).unwrap();
    assert_eq!(b.total_deposited, 5 * WEI_PER_FRAC);
    assert_eq!(b.owner, HARDHAT_DEFAULT_SIGNER_0);

    apply_block(
        &mut state,
        &[native_tx(
            1,
            NativeCall::WalletFundBudgetAccountV1 {
                budget: 1,
                amount: WEI_PER_FRAC,
                source_budget: None,
            },
        )],
    )
    .unwrap();
    assert_eq!(
        state.wallet_budgets.get(&1).unwrap().total_deposited,
        6 * WEI_PER_FRAC
    );

    apply_block(
        &mut state,
        &[native_tx(
            2,
            NativeCall::WalletCloseBudgetAccountV1 { budget: 1 },
        )],
    )
    .unwrap();
    assert!(!state.wallet_budgets.contains_key(&1));
    assert_eq!(
        state.accounts.get(&HARDHAT_DEFAULT_SIGNER_0).unwrap().balance,
        100 * WEI_PER_FRAC
    );
}

#[test]
fn wallet_mint_root_capability_registers_holder() {
    let mut state = funded_state();
    apply_block(
        &mut state,
        &[native_tx(
            0,
            NativeCall::WalletCreateBudgetAccountV1 {
                parent: None,
                initial_deposit: 10 * WEI_PER_FRAC,
            },
        )],
    )
    .unwrap();

    let mut rng = OsRng;
    let issuer = SigningKey::generate(&mut rng);
    let subject = SigningKey::generate(&mut rng);
    let cap = coding_root_cap(&issuer, &subject, 1);
    let borsh = borsh::to_vec(&cap).unwrap();

    let proof = proof_for_token(&state, &borsh);
    apply_block(
        &mut state,
        &[native_tx(1, mint_call(None, borsh.clone(), None, proof))],
    )
    .unwrap();

    assert_eq!(state.wallet_capabilities.get(&[0xaa; 32]), Some(&borsh));
    assert_eq!(
        state.wallet_cap_holders.get(&[0xaa; 32]),
        Some(&HARDHAT_DEFAULT_SIGNER_0)
    );
}

#[test]
fn wallet_mint_child_with_budget_seed_splits_linked_budget() {
    let mut state = funded_state();
    apply_block(
        &mut state,
        &[
            native_tx(
                0,
                NativeCall::WalletCreateBudgetAccountV1 {
                    parent: None,
                    initial_deposit: 10 * WEI_PER_FRAC,
                },
            ),
            native_tx(
                1,
                NativeCall::WalletCreateBudgetAccountV1 {
                    parent: Some(1),
                    initial_deposit: 0,
                },
            ),
        ],
    )
    .unwrap();

    let mut rng = OsRng;
    let issuer = SigningKey::generate(&mut rng);
    let coding = SigningKey::generate(&mut rng);
    let verifier = SigningKey::generate(&mut rng);
    let parent_cap = coding_root_cap(&issuer, &coding, 1);
    let parent_borsh = borsh::to_vec(&parent_cap).unwrap();

    let parent_proof = proof_for_token(&state, &parent_borsh);
    apply_block(
        &mut state,
        &[native_tx(2, mint_call(None, parent_borsh, None, parent_proof))],
    )
    .unwrap();

    let child_params = child_params_for_role(
        &parent_cap.body,
        &SubAgentRole::verifier_default(),
        [0xbb; 32],
        verifier.verifying_key().to_bytes(),
        2,
        2,
    )
    .unwrap();
    let child_body = build_delegated_child_body(&parent_cap.body, child_params).unwrap();
    let child = CapabilityToken::sign(child_body, &issuer).unwrap();
    let child_borsh = borsh::to_vec(&child).unwrap();
    let child_proof = proof_for_token(&state, &child_borsh);

    apply_block(
        &mut state,
        &[native_tx(
            3,
            mint_call(
                Some([0xaa; 32]),
                child_borsh,
                Some(WalletBudgetSeed {
                    from_budget: 1,
                    amount: 2 * WEI_PER_FRAC,
                }),
                child_proof,
            ),
        )],
    )
    .unwrap();

    assert_eq!(
        state.wallet_budgets.get(&1).unwrap().total_deposited,
        8 * WEI_PER_FRAC
    );
    assert_eq!(
        state.wallet_budgets.get(&2).unwrap().total_deposited,
        2 * WEI_PER_FRAC
    );
    assert!(state.wallet_capabilities.contains_key(&[0xbb; 32]));
}

#[test]
fn wallet_mint_duplicate_cap_rejected() {
    let mut state = funded_state();
    apply_block(
        &mut state,
        &[native_tx(
            0,
            NativeCall::WalletCreateBudgetAccountV1 {
                parent: None,
                initial_deposit: WEI_PER_FRAC,
            },
        )],
    )
    .unwrap();

    let issuer = SigningKey::generate(&mut OsRng);
    let subject = SigningKey::generate(&mut OsRng);
    let cap = coding_root_cap(&issuer, &subject, 1);
    let borsh = borsh::to_vec(&cap).unwrap();

    let proof1 = proof_for_token(&state, &borsh);
    apply_block(
        &mut state,
        &[native_tx(1, mint_call(None, borsh.clone(), None, proof1))],
    )
    .unwrap();

    let proof2 = proof_for_token(&state, &borsh);
    let err = apply_block(
        &mut state,
        &[native_tx(2, mint_call(None, borsh, None, proof2))],
    )
    .unwrap_err();
    assert_eq!(err, ExecError::DuplicateWalletCapability);
}

#[test]
fn wallet_mint_rejects_missing_revocation_proof() {
    let mut state = funded_state();
    apply_block(
        &mut state,
        &[native_tx(
            0,
            NativeCall::WalletCreateBudgetAccountV1 {
                parent: None,
                initial_deposit: WEI_PER_FRAC,
            },
        )],
    )
    .unwrap();

    let issuer = SigningKey::generate(&mut OsRng);
    let subject = SigningKey::generate(&mut OsRng);
    let cap = coding_root_cap(&issuer, &subject, 1);
    let borsh = borsh::to_vec(&cap).unwrap();

    let err = apply_block(
        &mut state,
        &[native_tx(
            1,
            NativeCall::WalletMintCapabilityV1 {
                parent_cap_id: None,
                child_token_borsh: borsh,
                budget_seed: None,
                revocation_proof_borsh: vec![],
            },
        )],
    )
    .unwrap_err();
    assert_eq!(err, ExecError::WalletRevocationProofRequired);
}

#[test]
fn wallet_mint_rejects_revocation_proof_root_mismatch() {
    let mut state = funded_state();
    let issuer = SigningKey::generate(&mut OsRng);
    let subject = SigningKey::generate(&mut OsRng);
    let cap = coding_root_cap(&issuer, &subject, 1);
    let borsh = borsh::to_vec(&cap).unwrap();
    let proof_bytes = mint_revocation_proof_bytes(&state, &cap).unwrap();
    state.wallet_revocation_merkle_root = [0x99; 32];

    let err = apply_block(
        &mut state,
        &[native_tx(
            0,
            NativeCall::WalletMintCapabilityV1 {
                parent_cap_id: None,
                child_token_borsh: borsh,
                budget_seed: None,
                revocation_proof_borsh: proof_bytes,
            },
        )],
    )
    .unwrap_err();
    assert_eq!(err, ExecError::WalletRevocationProofInvalid);
}
