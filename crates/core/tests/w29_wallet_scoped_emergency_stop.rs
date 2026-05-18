//! §14.1 scoped `EmergencyStop { scope, master_sig }` gates matching capability mints.

use ed25519_dalek::{Signer, SigningKey};
use fractal_core::{
    Account, ExecError, HARDHAT_DEFAULT_SIGNER_0, NativeCall, State, Transaction, TxBody, VmKind,
    WEI_PER_FRAC, WalletEmergencyScopeV1, WalletScopedEmergencyStopSignBodyV1, apply_block,
    mint_revocation_proof_bytes,
};
use fractal_wallet::{
    capability::{CapabilitySignBody, CapabilityToken},
    caveat::Caveat,
    policy::builtins::FRAC,
    types::{Scope, ToolClass},
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

fn sign_stop(
    issuer: &SigningKey,
    state: &State,
    engage: bool,
    scope: WalletEmergencyScopeV1,
) -> [u8; 64] {
    let body = WalletScopedEmergencyStopSignBodyV1 {
        chain_id: state.wallet_chain_id,
        engage,
        scope,
    };
    let msg = borsh::to_vec(&body).unwrap();
    issuer.sign(&msg).to_bytes()
}

fn capability(
    issuer: &SigningKey,
    subject: &SigningKey,
    cap_id: [u8; 32],
    workspace_id: u64,
    tool_class_mask: u64,
) -> CapabilityToken {
    let body = CapabilitySignBody {
        version: 1,
        cap_id,
        chain_id: 41,
        issuer: issuer.verifying_key().to_bytes(),
        subject: subject.verifying_key().to_bytes(),
        parent_cap_id: None,
        scope: Scope {
            workspace_id: Some(workspace_id),
            project_id: None,
            task_id: None,
            tool_class_mask,
            providers: None,
        },
        caveats: vec![Caveat::MaxTotalSpend(10 * FRAC)],
        budget_account: 0,
        not_before: 0,
        not_after: 2_000_000,
        nonce: workspace_id,
    };
    CapabilityToken::sign(body, issuer).unwrap()
}

fn mint_call(token: &CapabilityToken, state: &State) -> NativeCall {
    NativeCall::WalletMintCapabilityV1 {
        parent_cap_id: None,
        child_token_borsh: borsh::to_vec(token).unwrap(),
        budget_seed: None,
        revocation_proof_borsh: mint_revocation_proof_bytes(state, token).unwrap(),
    }
}

#[test]
fn scoped_workspace_stop_blocks_only_matching_master_and_scope() {
    let mut state = funded_state();
    let master = SigningKey::generate(&mut OsRng);
    let other_master = SigningKey::generate(&mut OsRng);
    let subject = SigningKey::generate(&mut OsRng);
    let scope = WalletEmergencyScopeV1 {
        workspace_id: Some(7),
        project_id: None,
        task_id: None,
        tool_class_mask: 0,
        provider_id: None,
    };
    let engage_sig = sign_stop(&master, &state, true, scope.clone());

    apply_block(
        &mut state,
        &[native_tx(
            0,
            NativeCall::WalletScopedEmergencyStopV1 {
                engage: true,
                scope: scope.clone(),
                master_public_key: master.verifying_key().to_bytes(),
                master_sig: engage_sig,
            },
        )],
    )
    .unwrap();

    let stopped = capability(&master, &subject, [0x11; 32], 7, ToolClass::Browser.bit());
    let stopped_call = mint_call(&stopped, &state);
    let err = apply_block(&mut state, &[native_tx(1, stopped_call)]).unwrap_err();
    assert_eq!(err, ExecError::WalletEmergencyStopActive);

    let other_workspace = capability(&master, &subject, [0x22; 32], 8, ToolClass::Browser.bit());
    let other_workspace_call = mint_call(&other_workspace, &state);
    apply_block(&mut state, &[native_tx(1, other_workspace_call)]).unwrap();

    let other_master_cap = capability(
        &other_master,
        &subject,
        [0x33; 32],
        7,
        ToolClass::Browser.bit(),
    );
    let other_master_call = mint_call(&other_master_cap, &state);
    apply_block(&mut state, &[native_tx(2, other_master_call)]).unwrap();
}

#[test]
fn scoped_stop_disengage_reopens_matching_scope() {
    let mut state = funded_state();
    let master = SigningKey::generate(&mut OsRng);
    let subject = SigningKey::generate(&mut OsRng);
    let scope = WalletEmergencyScopeV1 {
        workspace_id: Some(7),
        project_id: None,
        task_id: None,
        tool_class_mask: ToolClass::Browser.bit(),
        provider_id: None,
    };

    let engage_sig = sign_stop(&master, &state, true, scope.clone());
    apply_block(
        &mut state,
        &[native_tx(
            0,
            NativeCall::WalletScopedEmergencyStopV1 {
                engage: true,
                scope: scope.clone(),
                master_public_key: master.verifying_key().to_bytes(),
                master_sig: engage_sig,
            },
        )],
    )
    .unwrap();
    let disengage_sig = sign_stop(&master, &state, false, scope.clone());
    apply_block(
        &mut state,
        &[native_tx(
            1,
            NativeCall::WalletScopedEmergencyStopV1 {
                engage: false,
                scope: scope.clone(),
                master_public_key: master.verifying_key().to_bytes(),
                master_sig: disengage_sig,
            },
        )],
    )
    .unwrap();

    let reopened = capability(&master, &subject, [0x44; 32], 7, ToolClass::Browser.bit());
    let reopened_call = mint_call(&reopened, &state);
    apply_block(&mut state, &[native_tx(2, reopened_call)]).unwrap();
    assert!(state.wallet_capabilities.contains_key(&[0x44; 32]));
}
