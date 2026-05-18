//! `fractal-wallet-cli` — reference operator CLI (W6 slice).
//!
//! Anchored on `docs/wallet.md` §15.2 (built-in policy templates) and §4.2
//! (capability mint / verify). **`chain submit-mint-cap`** posts §14.1 `WalletMintCapabilityV1` via RPC or `--apply-local`.
//! **`chain emergency-stop`** posts governance `WalletEmergencyStopV1` (`docs/wallet.md` §29).
//! **§14.5 Task module:** **`chain task-post`** through **`chain task-finalize`** submit native `Wallet*Task*` opcodes (`0x14`–`0x19`).
//! **W6-b:** `tools/wallet-web/` static stub + `policy dump-builtins` + `policy tool-classes` (§8.1 catalog).
//! The argv shape stays tiny and dependency-free so integration tests can call `run_argv` directly.

use std::collections::BTreeMap;

use borsh::BorshDeserialize;
use ed25519_dalek::SigningKey;
use fractal_wallet::{
    capability::{derive_cap_id, CapabilitySignBody, CapabilityToken},
    caveat::Caveat,
    delegate_sub_agent_production, run_verifier_tool_session_e2e, sessions_are_distinct,
    encode_tee_quote_v1, verify_production_tool_receipt, BudgetAccount, ProductionVerifyContext,
    SubAgentRole, TeeQuoteV1, ToolMarket, ToolReceipt, ToolIntent,
    policy_builtins as builtins,
    types::{Scope, TeeType, ToolClass, VerificationTier},
    verify_capability_with_revocation, RevocationEntry, RevocationSet,
    PolicyRegistry, ResolvedPolicy, TemplateId,
};
use rand::rngs::OsRng;
use serde_json::{json, Value};

mod chain;
mod reputation;

/// Top-level entry: returns the JSON output (serialized) on success or an
/// error message on failure. The binary in `main.rs` prints the JSON to stdout
/// and exits 0/1 accordingly.
pub fn run_argv(argv: &[String]) -> Result<String, String> {
    let v = run_argv_value(argv)?;
    serde_json::to_string_pretty(&v).map_err(|e| format!("json: {e}"))
}

/// Same as `run_argv` but returns the JSON `Value` for tests.
pub fn run_argv_value(argv: &[String]) -> Result<Value, String> {
    if argv.len() < 2 {
        return Err(usage());
    }
    match (argv[1].as_str(), argv.get(2).map(String::as_str)) {
        ("policy", Some("list")) => Ok(cmd_policy_list()),
        ("policy", Some("show")) => cmd_policy_show(&argv[3..]),
        ("policy", Some("dump-builtins")) => Ok(cmd_policy_dump_builtins()),
        ("policy", Some("tool-classes")) => Ok(cmd_policy_tool_classes()),
        ("cap", Some("mint")) => cmd_cap_mint(&argv[3..]),
        ("cap", Some("show")) => cmd_cap_show(&argv[3..]),
        ("cap", Some("attenuate")) => cmd_cap_attenuate(&argv[3..]),
        ("cap", Some("delegate")) => cmd_cap_delegate(&argv[3..]),
        ("cap", Some("build-revocation-proof")) => cmd_cap_build_revocation_proof(&argv[3..]),
        ("cap", Some("verify-revocation")) => cmd_cap_verify_revocation(&argv[3..]),
        ("session", Some("e2e-verifier-delegation")) => cmd_session_e2e_verifier_delegation(&argv[3..]),
        ("chain", Some("submit-mint-cap")) => chain::cmd_chain_submit_mint_cap(&argv[3..]),
        ("chain", Some("submit-create-budget")) => chain::cmd_chain_submit_create_budget(&argv[3..]),
        ("chain", Some("submit-fund-budget")) => chain::cmd_chain_submit_fund_budget(&argv[3..]),
        ("chain", Some("submit-revoke-cap")) => chain::cmd_chain_submit_revoke_cap(&argv[3..]),
        ("chain", Some("submit-reputation-snapshot")) => {
            chain::cmd_chain_submit_reputation_snapshot(&argv[3..])
        },
        ("chain", Some("task-post")) => chain::cmd_chain_task_post(&argv[3..]),
        ("chain", Some("task-checkout")) => chain::cmd_chain_task_checkout(&argv[3..]),
        ("chain", Some("task-renew-checkout")) => chain::cmd_chain_task_renew_checkout(&argv[3..]),
        ("chain", Some("task-submit")) => chain::cmd_chain_task_submit(&argv[3..]),
        ("chain", Some("task-verify")) => chain::cmd_chain_task_verify(&argv[3..]),
        ("chain", Some("task-finalize")) => chain::cmd_chain_task_finalize(&argv[3..]),
        ("chain", Some("emergency-stop")) => chain::cmd_chain_emergency_stop(&argv[3..]),
        ("chain", Some("submit-wallet-batch-settle")) => {
            chain::cmd_chain_submit_wallet_batch_settle(&argv[3..])
        }
        ("reputation", Some("preview")) => reputation::cmd_reputation_preview(&argv[3..]),
        ("reputation", Some("build-summary")) => reputation::cmd_reputation_build_summary(&argv[3..]),
        ("reputation", Some("show-store")) => reputation::cmd_reputation_show_store(&argv[3..]),
        ("reputation", Some("show-chain")) => reputation::cmd_reputation_show_chain(&argv[3..]),
        ("production", Some("verify-receipt")) => cmd_production_verify_receipt(&argv[3..]),
        ("help" | "--help" | "-h", _) => Ok(json!({ "usage": usage() })),
        _ => Err(usage()),
    }
}

fn cmd_production_verify_receipt(args: &[String]) -> Result<Value, String> {
    let mut template_id: TemplateId = builtins::CODING_AGENT_V2_PRODUCTION_ID;
    let mut tool_class = ToolClass::GithubWrite;
    let mut tier = VerificationTier::Attested;
    let mut measurement = [0x01u8; 32];
    let mut payload = [0xcdu8; 32];
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--template" => {
                i += 1;
                template_id = args[i].parse().map_err(|e| format!("--template: {e}"))?;
            }
            "--tool-class" => {
                i += 1;
                let d: u8 = args[i].parse().map_err(|e| format!("--tool-class: {e}"))?;
                tool_class = ToolClass::from_discriminant(d)
                    .ok_or_else(|| format!("unknown tool class {d}"))?;
            }
            "--tier" => {
                i += 1;
                tier = match args[i].as_str() {
                    "trusted" => VerificationTier::Trusted,
                    "optimistic" => VerificationTier::Optimistic,
                    "attested" => VerificationTier::Attested,
                    "replicated" => VerificationTier::Replicated,
                    "proven" => VerificationTier::Proven,
                    other => return Err(format!("unknown --tier {other}")),
                };
            }
            "--measurement-hex" => {
                i += 1;
                measurement = parse_hex32(&args[i], "measurement")?;
            }
            "--payload-commitment-hex" => {
                i += 1;
                payload = parse_hex32(&args[i], "payload commitment")?;
            }
            flag => return Err(format!("unknown flag {flag}")),
        }
        i += 1;
    }
    let mut reg = PolicyRegistry::default();
    builtins::register_builtins(&mut reg).expect("builtins");
    let policy = reg
        .resolve(template_id)
        .map_err(|e| format!("policy resolve: {e:?}"))?;
    let mut rng = OsRng;
    let agent = SigningKey::generate(&mut rng);
    let provider = SigningKey::generate(&mut rng);
    let intent_id = [0xabu8; 32];
    let intent = ToolIntent::sign(
        fractal_wallet::ToolIntentBody {
            intent_id,
            agent_session: agent.verifying_key().to_bytes(),
            task_id: 42,
            tool_class,
            payload_commitment: payload,
            max_price: 50,
            verification_tier: tier,
            deadline_ms: 9_999_999,
            nonce: 1,
        },
        &agent,
    )
    .map_err(|e| format!("intent sign: {e}"))?;
    let q = TeeQuoteV1 {
        tee_type: TeeType::IntelTdx,
        measurement,
        report_data: payload,
        enclave_pubkey: [0u8; 32],
    };
    let att = fractal_wallet::TeeAttestation {
        tee_type: TeeType::IntelTdx,
        quote: encode_tee_quote_v1(&q).map_err(|e| format!("tee quote: {e}"))?,
    };
    let body = fractal_wallet::ToolReceiptBody {
        intent_id,
        task_id: 42,
        agent_session: agent.verifying_key().to_bytes(),
        provider_id: fractal_wallet::provider_id_from_public_key(&provider.verifying_key().to_bytes()),
        tool_class,
        payload_commitment: payload,
        output_commitment: [0xee; 32],
        output_pointer: "da://demo".into(),
        metering: fractal_wallet::MeteringRecord {
            input_tokens: 0,
            output_tokens: 0,
            wall_duration_ms: 100,
            bytes_metered: 0,
        },
        cost: 10,
        started_at: 1,
        completed_at: 2,
        attestation: Some(att),
    };
    let receipt = ToolReceipt::sign_new(body, &provider).map_err(|e| format!("receipt: {e}"))?;
    let provider_pk = provider.verifying_key().to_bytes();
    let ctx = ProductionVerifyContext {
        intent: &intent,
        receipt: &receipt,
        provider_pk: &provider_pk,
        caveats: &policy.caveats,
        required_attestations: &policy.required_attestations,
        expected_measurement: Some(measurement),
        now_ms: 1000,
    };
    let report = verify_production_tool_receipt(&ctx).map_err(|e| format!("verify: {e:?}"))?;
    Ok(json!({
        "ok": report.all_ok(),
        "method": format!("{:?}", report.method),
        "providerSignatureOk": report.provider_signature_ok,
        "intentBindingOk": report.intent_binding_ok,
        "tierOk": report.tier_ok,
        "meteringOk": report.metering_ok,
        "attestationOk": report.attestation_ok,
        "caveatsOk": report.caveats_ok,
        "templateId": template_id,
        "toolClass": format!("{tool_class:?}"),
    }))
}

fn usage() -> String {
    "usage: fractal-wallet-cli <command> [args]\n  policy list\n  policy show <template_id>\n  policy dump-builtins\n  policy tool-classes\n  cap mint --template <id> --chain-id <n> --not-after-ms <t> [--workspace <id>] [--cap-id <hex32>] [--nonce <n>]\n  cap show <token_hex>\n  cap attenuate --parent-hex <hex> --issuer-secret <hex32> [--not-after-ms <t>] [--workspace <id>] [--max-total-spend <amount>] [--tool-mask <hex>] [--cap-id <hex32>] [--nonce <n>]\n  cap delegate --parent-hex <hex> --issuer-secret <hex32> --parent-budget-deposited <amount> --delegate-amount <amount> --child-budget-id <n> [--role verifier] [--run-e2e] [--task-id <n>]\n  session e2e-verifier-delegation --chain-id <n> [--parent-budget <amount>] [--delegate-amount <amount>] [--workspace <id>]\n  chain submit-mint-cap --token-hex <hex> [--parent-cap-id <hex32>] [--proof-hex <hex>] [--from-rpc] [--from-budget <n> --seed-amount <amount>] [--signer <addr>] [--nonce <n>] [--rpc-url <url>] [--apply-local]\n  chain submit-create-budget --initial-deposit <amount> [--budget-parent <id>] [--signer <addr>] [--nonce <n>] [--rpc-url <url>] [--apply-local]\n  chain submit-fund-budget --budget <id> --amount <amount> [--source-budget <id>] [--signer <addr>] [--nonce <n>] [--rpc-url <url>] [--apply-local]\n  chain submit-revoke-cap --cap-id <hex32> [--reason-code <n>] [--cascade] --issuer-secret <hex32> [--chain-id <n>] [--signer <addr>] [--nonce <n>] [--rpc-url <url>] [--apply-local]\n  chain task-post [--metadata-uri <s>] --bounty-budget <n> [--tool-budget <n>] [--verifier-budget <n>] [--signer <addr>] [--nonce <n>] [--rpc-url <url>] [--apply-local]\n  chain task-checkout --task-id <id> --agent-session-hex <hex32> --expiry-ms <t> [--signer <addr>] [--nonce <n>] [--rpc-url <url>] [--apply-local]\n  chain task-renew-checkout --task-id <id> [--evidence-uri <s>] --new-expiry-ms <t> [...]\n  chain task-submit --task-id <id> --artifact-pointer <s> --tool-receipt-root <hex32> [...]\n  chain task-verify --task-id <id> --verifier-sig <hex64> [--verify-score <n>] [...]\n  chain task-finalize --task-id <id> [...]\n  chain emergency-stop (--engage|--disengage) [--signer <addr>] [--nonce <n>] [--rpc-url <url>] [--apply-local]\n  chain submit-wallet-batch-settle --batch-id <hex32> --provider-secret <hex32> --tool-class <u8> --payout-to <addr> --receipt-borsh-hex <hex> [...] [--total-cost <amount>] [--submitted-at-ms <t>] [--signer <addr>] [--nonce <n>] [--rpc-url <url>] [--apply-local]\n  cap build-revocation-proof --cap-id <hex32> [--parent-cap-id <hex32>] [--revoked-cap-id <hex32>] [--revoked-cascade] [--from-rpc] [--rpc-url <url>]\n  cap verify-revocation --token-hex <hex> --proof-hex <hex> --revocation-root-hex <hex32> [--not-after-ms <t>]"
        .into()
}

fn cmd_policy_list() -> Value {
    let mut reg = PolicyRegistry::default();
    builtins::register_builtins(&mut reg).expect("builtins register");
    let mut entries = Vec::new();
    for (id, name) in builtins::all_ids() {
        let r = reg.resolve(id).expect("builtin resolves");
        entries.push(json!({
            "templateId": id,
            "name": name,
            "totalCap": r.default_budget.total_cap.to_string(),
            "caveatCount": r.caveats.len(),
            "rateLimitClasses": r.rate_limits.len(),
            "suggestedToolMask": format!("0x{:016x}", builtins::suggested_tool_class_mask(id).unwrap_or(0)),
        }));
    }
    json!({ "templates": entries })
}

fn cmd_policy_show(args: &[String]) -> Result<Value, String> {
    let id_str = args.first().ok_or_else(|| "policy show: missing <template_id>".to_string())?;
    let id: TemplateId = id_str.parse().map_err(|e| format!("template_id: {e}"))?;
    let mut reg = PolicyRegistry::default();
    builtins::register_builtins(&mut reg).expect("builtins register");
    let resolved = reg.resolve(id).map_err(|e| format!("resolve: {e}"))?;
    Ok(render_policy(id, &resolved))
}

/// JSON for `tools/wallet-web/builtins.json` (reference web client). Regenerate with
/// `cargo run -p fractal-cli -- policy dump-builtins > tools/wallet-web/builtins.json`.
fn cmd_policy_dump_builtins() -> Value {
    let mut reg = PolicyRegistry::default();
    builtins::register_builtins(&mut reg).expect("builtins register");
    let mut templates = Vec::new();
    for (id, _name) in builtins::all_ids() {
        let resolved = reg.resolve(id).expect("builtin resolves");
        let mut v = render_policy(id, &resolved);
        if let Value::Object(ref mut m) = v {
            if let Some((n, d)) = builtins::meta(id) {
                m.insert("name".into(), json!(n));
                m.insert("description".into(), json!(d));
            }
        }
        templates.push(v);
    }
    let tool_classes: Vec<Value> = ToolClass::VARIANTS
        .into_iter()
        .map(|c| {
            json!({
                "specName": c.spec_name(),
                "discriminant": c as u8,
                "bitHex": format!("0x{:016x}", c.bit()),
            })
        })
        .collect();
    json!({
        "schemaVersion": 2,
        "phase1MaskHex": format!("0x{:016x}", ToolClass::all_phase1_mask()),
        "toolClasses": tool_classes,
        "templates": templates,
    })
}

/// JSON catalog for `docs/wallet.md` §8.1 (borsh discriminants + default verification tier).
fn cmd_policy_tool_classes() -> Value {
    let rows: Vec<Value> = ToolClass::VARIANTS
        .into_iter()
        .map(|c| {
            json!({
                "specName": c.spec_name(),
                "discriminant": c as u8,
                "bitHex": format!("0x{:016x}", c.bit()),
                "pricingNotes": c.spec_pricing_notes(),
                "defaultVerificationTier": format!("{:?}", c.default_verification_tier()),
            })
        })
        .collect();
    json!({
        "schemaVersion": 1,
        "source": "docs/wallet.md §8.1",
        "phase1MaskHex": format!("0x{:016x}", ToolClass::all_phase1_mask()),
        "phase2SliceMaskHex": format!("0x{:016x}", ToolClass::phase2_tool_class_mask()),
        "fullCatalogMaskHex": format!("0x{:016x}", ToolClass::all_v2_catalog_mask()),
        "classes": rows,
    })
}

fn render_policy(id: TemplateId, r: &ResolvedPolicy) -> Value {
    let per_tool: BTreeMap<String, String> = r
        .default_budget
        .per_tool
        .iter()
        .map(|(k, v)| (format!("{k:?}"), v.to_string()))
        .collect();
    let rate: BTreeMap<String, Value> = r
        .rate_limits
        .iter()
        .map(|(k, v)| {
            (
                format!("{k:?}"),
                json!({ "count": v.count, "windowSeconds": v.window_seconds }),
            )
        })
        .collect();
    let caveats: Vec<String> = r.caveats.iter().map(|c| format!("{c:?}")).collect();
    let caveats_borsh = borsh::to_vec(&r.caveats).expect("caveats borsh");
    json!({
        "templateId": id,
        "totalCap": r.default_budget.total_cap.to_string(),
        "perToolCap": per_tool,
        "rateLimits": rate,
        "caveats": caveats,
        "mintCaveatsBorshHex": format!("0x{}", hex::encode(caveats_borsh)),
        "suggestedToolMask": format!("0x{:016x}", builtins::suggested_tool_class_mask(id).unwrap_or(0)),
    })
}

#[derive(Debug, Default)]
pub(crate) struct CapFlags {
    template: Option<TemplateId>,
    chain_id: Option<u32>,
    not_after_ms: Option<u64>,
    not_before_ms: Option<u64>,
    workspace: Option<u64>,
    nonce: Option<u64>,
    cap_id_hex: Option<String>,
    // attenuate-only:
    parent_hex: Option<String>,
    issuer_secret_hex: Option<String>,
    max_total_spend: Option<u128>,
    tool_mask_hex: Option<String>,
    parent_budget_deposited: Option<u128>,
    delegate_amount: Option<u128>,
    child_budget_id: Option<u64>,
    role: Option<String>,
    run_e2e: bool,
    task_id: Option<u64>,
    token_hex: Option<String>,
    parent_cap_id_hex: Option<String>,
    from_budget: Option<u64>,
    seed_amount: Option<u128>,
    signer_hex: Option<String>,
    rpc_url: Option<String>,
    apply_local: bool,
    budget_id: Option<u64>,
    budget_parent: Option<u64>,
    initial_deposit: Option<u128>,
    fund_amount: Option<u128>,
    source_budget: Option<u64>,
    reason_code: Option<u8>,
    revoke_cascade: bool,
    issuer_sig_hex: Option<String>,
    revoked_entry_cap_hex: Option<String>,
    proof_hex: Option<String>,
    revocation_root_hex: Option<String>,
    from_rpc: bool,
    provider_id_hex: Option<String>,
    tool_class: Option<u8>,
    summary_json_path: Option<String>,
    summary_borsh_hex: Option<String>,
    metadata_uri: Option<String>,
    bounty_budget: Option<u128>,
    tool_budget: Option<u128>,
    verifier_budget: Option<u128>,
    agent_session_hex: Option<String>,
    expiry_ms: Option<u64>,
    evidence_uri: Option<String>,
    new_expiry_ms: Option<u64>,
    artifact_pointer: Option<String>,
    tool_receipt_root_hex: Option<String>,
    verifier_sig_hex: Option<String>,
    verify_score: Option<u8>,
    /// `chain emergency-stop`: Some(true) = --engage, Some(false) = --disengage.
    emergency_engage: Option<bool>,
    batch_id_hex: Option<String>,
    payout_to_hex: Option<String>,
    provider_secret_hex: Option<String>,
    submitted_at_ms: Option<u64>,
    total_cost: Option<u128>,
    receipt_borsh_hex: Vec<String>,
}

pub(crate) fn parse_flags(args: &[String]) -> Result<CapFlags, String> {
    let mut out = CapFlags::default();
    let mut i = 0;
    while i < args.len() {
        let k = &args[i];
        if k == "--run-e2e" {
            out.run_e2e = true;
            i += 1;
            continue;
        }
        if k == "--apply-local" {
            out.apply_local = true;
            i += 1;
            continue;
        }
        if k == "--cascade" {
            out.revoke_cascade = true;
            i += 1;
            continue;
        }
        if k == "--from-rpc" {
            out.from_rpc = true;
            i += 1;
            continue;
        }
        if k == "--engage" {
            out.emergency_engage = Some(true);
            i += 1;
            continue;
        }
        if k == "--disengage" {
            out.emergency_engage = Some(false);
            i += 1;
            continue;
        }
        let v = args
            .get(i + 1)
            .ok_or_else(|| format!("flag {k} requires value"))?;
        match k.as_str() {
            "--template" => out.template = Some(v.parse().map_err(|e| format!("--template: {e}"))?),
            "--chain-id" => out.chain_id = Some(v.parse().map_err(|e| format!("--chain-id: {e}"))?),
            "--not-after-ms" => {
                out.not_after_ms = Some(v.parse().map_err(|e| format!("--not-after-ms: {e}"))?)
            }
            "--not-before-ms" => {
                out.not_before_ms = Some(v.parse().map_err(|e| format!("--not-before-ms: {e}"))?)
            }
            "--workspace" => out.workspace = Some(v.parse().map_err(|e| format!("--workspace: {e}"))?),
            "--nonce" => out.nonce = Some(v.parse().map_err(|e| format!("--nonce: {e}"))?),
            "--cap-id" => out.cap_id_hex = Some(v.clone()),
            "--parent-hex" => out.parent_hex = Some(v.clone()),
            "--issuer-secret" => out.issuer_secret_hex = Some(v.clone()),
            "--max-total-spend" => {
                out.max_total_spend = Some(v.parse().map_err(|e| format!("--max-total-spend: {e}"))?)
            }
            "--tool-mask" => out.tool_mask_hex = Some(v.clone()),
            "--parent-budget-deposited" => {
                out.parent_budget_deposited =
                    Some(v.parse().map_err(|e| format!("--parent-budget-deposited: {e}"))?)
            }
            "--delegate-amount" => {
                out.delegate_amount =
                    Some(v.parse().map_err(|e| format!("--delegate-amount: {e}"))?)
            }
            "--child-budget-id" => {
                out.child_budget_id =
                    Some(v.parse().map_err(|e| format!("--child-budget-id: {e}"))?)
            }
            "--role" => out.role = Some(v.clone()),
            "--task-id" => {
                out.task_id = Some(v.parse().map_err(|e| format!("--task-id: {e}"))?)
            }
            "--parent-budget" => {
                out.parent_budget_deposited =
                    Some(v.parse().map_err(|e| format!("--parent-budget: {e}"))?)
            }
            "--token-hex" => out.token_hex = Some(v.clone()),
            "--parent-cap-id" => out.parent_cap_id_hex = Some(v.clone()),
            "--from-budget" => {
                out.from_budget = Some(v.parse().map_err(|e| format!("--from-budget: {e}"))?)
            }
            "--seed-amount" => {
                out.seed_amount = Some(v.parse().map_err(|e| format!("--seed-amount: {e}"))?)
            }
            "--signer" => out.signer_hex = Some(v.clone()),
            "--rpc-url" => out.rpc_url = Some(v.clone()),
            "--budget" => {
                out.budget_id = Some(v.parse().map_err(|e| format!("--budget: {e}"))?)
            }
            "--budget-parent" => {
                out.budget_parent = Some(v.parse().map_err(|e| format!("--budget-parent: {e}"))?)
            }
            "--initial-deposit" => {
                out.initial_deposit =
                    Some(v.parse().map_err(|e| format!("--initial-deposit: {e}"))?)
            }
            "--amount" => {
                out.fund_amount = Some(v.parse().map_err(|e| format!("--amount: {e}"))?)
            }
            "--source-budget" => {
                out.source_budget =
                    Some(v.parse().map_err(|e| format!("--source-budget: {e}"))?)
            }
            "--reason-code" => {
                out.reason_code =
                    Some(v.parse().map_err(|e| format!("--reason-code: {e}"))?)
            }
            "--issuer-sig" => out.issuer_sig_hex = Some(v.clone()),
            "--revoked-cap-id" => out.revoked_entry_cap_hex = Some(v.clone()),
            "--proof-hex" => out.proof_hex = Some(v.clone()),
            "--revocation-root-hex" => out.revocation_root_hex = Some(v.clone()),
            "--provider-id" => out.provider_id_hex = Some(v.clone()),
            "--tool-class" => {
                out.tool_class = Some(v.parse().map_err(|e| format!("--tool-class: {e}"))?)
            }
            "--summary-json" => out.summary_json_path = Some(v.clone()),
            "--summary-borsh-hex" => out.summary_borsh_hex = Some(v.clone()),
            "--metadata-uri" => out.metadata_uri = Some(v.clone()),
            "--bounty-budget" => {
                out.bounty_budget = Some(v.parse().map_err(|e| format!("--bounty-budget: {e}"))?)
            }
            "--tool-budget" => {
                out.tool_budget = Some(v.parse().map_err(|e| format!("--tool-budget: {e}"))?)
            }
            "--verifier-budget" => {
                out.verifier_budget =
                    Some(v.parse().map_err(|e| format!("--verifier-budget: {e}"))?)
            }
            "--agent-session-hex" => out.agent_session_hex = Some(v.clone()),
            "--expiry-ms" => {
                out.expiry_ms = Some(v.parse().map_err(|e| format!("--expiry-ms: {e}"))?)
            }
            "--evidence-uri" => out.evidence_uri = Some(v.clone()),
            "--new-expiry-ms" => {
                out.new_expiry_ms = Some(v.parse().map_err(|e| format!("--new-expiry-ms: {e}"))?)
            }
            "--artifact-pointer" => out.artifact_pointer = Some(v.clone()),
            "--tool-receipt-root" => out.tool_receipt_root_hex = Some(v.clone()),
            "--verifier-sig" => out.verifier_sig_hex = Some(v.clone()),
            "--verify-score" => {
                out.verify_score = Some(v.parse().map_err(|e| format!("--verify-score: {e}"))?)
            }
            "--batch-id" => out.batch_id_hex = Some(v.clone()),
            "--payout-to" => out.payout_to_hex = Some(v.clone()),
            "--provider-secret" => out.provider_secret_hex = Some(v.clone()),
            "--submitted-at-ms" => {
                out.submitted_at_ms =
                    Some(v.parse().map_err(|e| format!("--submitted-at-ms: {e}"))?)
            }
            "--total-cost" => {
                out.total_cost = Some(v.parse().map_err(|e| format!("--total-cost: {e}"))?)
            }
            "--receipt-borsh-hex" => out.receipt_borsh_hex.push(v.clone()),
            other => return Err(format!("unknown flag: {other}")),
        }
        i += 2;
    }
    Ok(out)
}

fn cmd_cap_mint(args: &[String]) -> Result<Value, String> {
    let parsed = parse_flags(args)?;
    let template_id = parsed.template.ok_or("--template required")?;
    let chain_id = parsed.chain_id.ok_or("--chain-id required")?;
    let not_after = parsed.not_after_ms.ok_or("--not-after-ms required")?;
    let not_before = parsed.not_before_ms.unwrap_or(0);
    if not_after <= not_before {
        return Err("--not-after-ms must be > --not-before-ms".into());
    }

    let mut reg = PolicyRegistry::default();
    builtins::register_builtins(&mut reg).expect("builtins register");
    let resolved = reg.resolve(template_id).map_err(|e| format!("resolve: {e}"))?;
    let tool_mask = builtins::suggested_tool_class_mask(template_id)
        .ok_or_else(|| format!("no suggested tool mask for template {template_id}"))?;

    let cap_id = match &parsed.cap_id_hex {
        Some(h) => parse_cap_id(h)?,
        None => {
            // No on-chain serial yet (W6-a is offline); derive a random secret and
            // hash it with the template id via the wallet's `derive_cap_id` helper
            // so the format matches future on-chain mints (`docs/wallet.md` §4.2).
            use rand::RngCore;
            let mut secret = [0u8; 32];
            OsRng.fill_bytes(&mut secret);
            derive_cap_id(&secret, template_id)
        }
    };

    let mut rng = OsRng;
    let issuer_sk = SigningKey::generate(&mut rng);
    let subject_sk = SigningKey::generate(&mut rng);

    let body = CapabilitySignBody {
        version: 1,
        cap_id,
        chain_id,
        issuer: issuer_sk.verifying_key().to_bytes(),
        subject: subject_sk.verifying_key().to_bytes(),
        parent_cap_id: None,
        scope: Scope {
            workspace_id: parsed.workspace,
            project_id: None,
            task_id: None,
            tool_class_mask: tool_mask,
            providers: None,
        },
        caveats: resolved.caveats.clone(),
        budget_account: 0,
        not_before,
        not_after,
        nonce: parsed.nonce.unwrap_or(1),
    };

    let token = CapabilityToken::sign(body, &issuer_sk).map_err(|e| format!("sign: {e}"))?;
    token
        .verify()
        .map_err(|e| format!("self-verify after sign: {e:?}"))?;
    token
        .verify_autonomous_tool_mask()
        .map_err(|e| format!("autonomous tool mask: {e:?}"))?;
    let token_bytes = borsh::to_vec(&token).map_err(|e| format!("borsh: {e}"))?;

    Ok(json!({
        "templateId": template_id,
        "templateName": builtins::all_ids().iter().find(|(i,_)| *i==template_id).map(|(_,n)| *n).unwrap_or(""),
        "capId": format!("0x{}", hex::encode(cap_id)),
        "chainId": chain_id,
        "notBeforeMs": not_before,
        "notAfterMs": not_after,
        "toolClassMask": format!("0x{:016x}", tool_mask),
        "caveatCount": resolved.caveats.len(),
        "issuerPub": format!("0x{}", hex::encode(issuer_sk.verifying_key().to_bytes())),
        "issuerSecret": format!("0x{}", hex::encode(issuer_sk.to_bytes())),
        "subjectPub": format!("0x{}", hex::encode(subject_sk.verifying_key().to_bytes())),
        "subjectSecret": format!("0x{}", hex::encode(subject_sk.to_bytes())),
        "tokenHex": format!("0x{}", hex::encode(token_bytes)),
    }))
}

pub(crate) fn parse_cap_id(s: &str) -> Result<[u8; 32], String> {
    parse_hex32(s, "--cap-id")
}

pub(crate) fn parse_hex32(s: &str, label: &str) -> Result<[u8; 32], String> {
    let h = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(h).map_err(|e| format!("{label} hex: {e}"))?;
    if bytes.len() != 32 {
        return Err(format!("{label} must be 32 bytes"));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

pub(crate) fn parse_hex(s: &str, label: &str) -> Result<Vec<u8>, String> {
    let h = s.strip_prefix("0x").unwrap_or(s);
    hex::decode(h).map_err(|e| format!("{label} hex: {e}"))
}

fn parse_u64_hex_or_dec(s: &str, label: &str) -> Result<u64, String> {
    if let Some(h) = s.strip_prefix("0x") {
        u64::from_str_radix(h, 16).map_err(|e| format!("{label} hex: {e}"))
    } else {
        s.parse::<u64>().map_err(|e| format!("{label}: {e}"))
    }
}

fn cmd_cap_attenuate(args: &[String]) -> Result<Value, String> {
    let parsed = parse_flags(args)?;
    let parent_hex = parsed
        .parent_hex
        .as_ref()
        .ok_or("--parent-hex required")?;
    let issuer_secret_hex = parsed
        .issuer_secret_hex
        .as_ref()
        .ok_or("--issuer-secret required")?;

    let parent_bytes = parse_hex(parent_hex, "--parent-hex")?;
    let parent =
        CapabilityToken::try_from_slice(&parent_bytes).map_err(|e| format!("parent decode: {e}"))?;
    parent
        .verify()
        .map_err(|e| format!("parent signature invalid: {e:?}"))?;

    if parent
        .body
        .caveats
        .iter()
        .any(|c| matches!(c, Caveat::NoRecursion))
    {
        return Err("parent carries NoRecursion; cannot mint delegated child capability".into());
    }

    let issuer_secret = parse_hex32(issuer_secret_hex, "--issuer-secret")?;
    let issuer_sk = SigningKey::from_bytes(&issuer_secret);
    if issuer_sk.verifying_key().to_bytes() != parent.body.issuer {
        return Err("--issuer-secret does not match parent.issuer".into());
    }

    // Child scope: clone parent, optionally narrow workspace + tool mask.
    let child_workspace = match parsed.workspace {
        Some(w) => Some(w),
        None => parent.body.scope.workspace_id,
    };
    let child_mask = match &parsed.tool_mask_hex {
        Some(s) => parse_u64_hex_or_dec(s, "--tool-mask")?,
        None => parent.body.scope.tool_class_mask,
    };
    if child_mask & parent.body.scope.tool_class_mask != child_mask {
        return Err("--tool-mask must be a subset of parent's tool_class_mask".into());
    }

    // Child time window: clone parent, optionally narrow `not_after`.
    let child_not_after = parsed.not_after_ms.unwrap_or(parent.body.not_after);
    if child_not_after > parent.body.not_after {
        return Err("--not-after-ms must be ≤ parent.not_after".into());
    }
    let child_not_before = match parsed.not_before_ms {
        Some(t) => t,
        None => parent.body.not_before,
    };
    if child_not_before < parent.body.not_before {
        return Err("--not-before-ms must be ≥ parent.not_before".into());
    }

    // Child caveats: every parent caveat must be matched by a child caveat that is stricter or equal
    // (`caveat::caveats_attenuate_parent`). Start by cloning parent's, then optionally lower the
    // `MaxTotalSpend` cap if `--max-total-spend` is provided.
    let mut child_caveats: Vec<Caveat> = parent.body.caveats.clone();
    if let Some(new_cap) = parsed.max_total_spend {
        let mut applied = false;
        let mut parent_cap_min: Option<u128> = None;
        for c in parent.body.caveats.iter() {
            if let Caveat::MaxTotalSpend(p) = c {
                parent_cap_min = Some(parent_cap_min.map_or(*p, |m| m.min(*p)));
            }
        }
        if let Some(p_cap) = parent_cap_min {
            if new_cap > p_cap {
                return Err(format!(
                    "--max-total-spend ({new_cap}) > parent MaxTotalSpend ({p_cap})"
                ));
            }
        }
        for c in child_caveats.iter_mut() {
            if let Caveat::MaxTotalSpend(v) = c {
                *v = new_cap;
                applied = true;
            }
        }
        if !applied {
            child_caveats.push(Caveat::MaxTotalSpend(new_cap));
        }
    }

    let cap_id = match &parsed.cap_id_hex {
        Some(h) => parse_cap_id(h)?,
        None => {
            use rand::RngCore;
            let mut secret = [0u8; 32];
            OsRng.fill_bytes(&mut secret);
            derive_cap_id(&secret, parsed.nonce.unwrap_or(0))
        }
    };

    // Generate a fresh subject key for the child session.
    let mut rng = OsRng;
    let subject_sk = SigningKey::generate(&mut rng);

    let child_body = CapabilitySignBody {
        version: parent.body.version,
        cap_id,
        chain_id: parent.body.chain_id,
        issuer: parent.body.issuer,
        subject: subject_sk.verifying_key().to_bytes(),
        parent_cap_id: Some(parent.body.cap_id),
        scope: Scope {
            workspace_id: child_workspace,
            project_id: parent.body.scope.project_id,
            task_id: parent.body.scope.task_id,
            tool_class_mask: child_mask,
            providers: parent.body.scope.providers.clone(),
        },
        caveats: child_caveats,
        budget_account: parent.body.budget_account,
        not_before: child_not_before,
        not_after: child_not_after,
        nonce: parsed.nonce.unwrap_or(parent.body.nonce.saturating_add(1)),
    };

    if !CapabilityToken::verify_attenuation_from_parent(&child_body, &parent.body) {
        return Err(
            "child fails verify_attenuation_from_parent (scope/time/caveat envelope not strictly narrower)"
                .into(),
        );
    }

    let token = CapabilityToken::sign(child_body, &issuer_sk).map_err(|e| format!("sign: {e}"))?;
    token
        .verify()
        .map_err(|e| format!("self-verify after sign: {e:?}"))?;
    let token_bytes = borsh::to_vec(&token).map_err(|e| format!("borsh: {e}"))?;

    Ok(json!({
        "parentCapId": format!("0x{}", hex::encode(parent.body.cap_id)),
        "childCapId": format!("0x{}", hex::encode(cap_id)),
        "issuerPub": format!("0x{}", hex::encode(parent.body.issuer)),
        "subjectPub": format!("0x{}", hex::encode(subject_sk.verifying_key().to_bytes())),
        "subjectSecret": format!("0x{}", hex::encode(subject_sk.to_bytes())),
        "chainId": parent.body.chain_id,
        "notBeforeMs": child_not_before,
        "notAfterMs": child_not_after,
        "toolClassMask": format!("0x{:016x}", child_mask),
        "caveatCount": token.body.caveats.len(),
        "attenuationOk": true,
        "tokenHex": format!("0x{}", hex::encode(token_bytes)),
    }))
}

fn parse_sub_agent_role(role: Option<&str>) -> Result<SubAgentRole, String> {
    match role.unwrap_or("verifier") {
        "verifier" => Ok(SubAgentRole::verifier_default()),
        other => Err(format!("unknown --role {other}; supported: verifier")),
    }
}

fn load_parent_token(parent_hex: &str) -> Result<CapabilityToken, String> {
    let parent_bytes = parse_hex(parent_hex, "--parent-hex")?;
    let parent =
        CapabilityToken::try_from_slice(&parent_bytes).map_err(|e| format!("parent decode: {e}"))?;
    parent
        .verify()
        .map_err(|e| format!("parent signature invalid: {e:?}"))?;
    if parent
        .body
        .caveats
        .iter()
        .any(|c| matches!(c, Caveat::NoRecursion))
    {
        return Err("parent carries NoRecursion; cannot delegate sub-agent".into());
    }
    Ok(parent)
}

fn load_issuer_sk(
    issuer_secret_hex: &str,
    expected_issuer: &[u8; 32],
) -> Result<SigningKey, String> {
    let issuer_secret = parse_hex32(issuer_secret_hex, "--issuer-secret")?;
    let issuer_sk = SigningKey::from_bytes(&issuer_secret);
    if issuer_sk.verifying_key().to_bytes() != *expected_issuer {
        return Err("--issuer-secret does not match parent.issuer".into());
    }
    Ok(issuer_sk)
}

fn cmd_cap_delegate(args: &[String]) -> Result<Value, String> {
    let parsed = parse_flags(args)?;
    let parent_hex = parsed
        .parent_hex
        .as_ref()
        .ok_or("--parent-hex required")?;
    let issuer_secret_hex = parsed
        .issuer_secret_hex
        .as_ref()
        .ok_or("--issuer-secret required")?;
    let parent_deposited = parsed
        .parent_budget_deposited
        .ok_or("--parent-budget-deposited required")?;
    let delegate_amount = parsed
        .delegate_amount
        .ok_or("--delegate-amount required")?;
    let child_budget_id = parsed
        .child_budget_id
        .ok_or("--child-budget-id required")?;

    let parent = load_parent_token(parent_hex)?;
    let issuer_sk = load_issuer_sk(issuer_secret_hex, &parent.body.issuer)?;

    let parent_budget_id = if parent.body.budget_account != 0 {
        parent.body.budget_account
    } else {
        1
    };
    let mut parent_budget = BudgetAccount::new(parent_budget_id, None, parent_deposited);
    if delegate_amount > parent_budget.available() {
        return Err(format!(
            "--delegate-amount ({delegate_amount}) exceeds parent budget available ({})",
            parent_budget.available()
        ));
    }

    let role = parse_sub_agent_role(parsed.role.as_deref())?;
    let mut rng = OsRng;
    let child_subject_sk = SigningKey::generate(&mut rng);
    let child_cap_id = match &parsed.cap_id_hex {
        Some(h) => parse_cap_id(h)?,
        None => {
            use rand::RngCore;
            let mut secret = [0u8; 32];
            OsRng.fill_bytes(&mut secret);
            derive_cap_id(&secret, parsed.nonce.unwrap_or(0))
        }
    };

    let bundle = delegate_sub_agent_production(
        &parent,
        &issuer_sk,
        &mut parent_budget,
        child_budget_id,
        delegate_amount,
        role,
        &child_subject_sk,
        child_cap_id,
    )
    .map_err(|e| format!("delegate: {e}"))?;

    let child_bytes = borsh::to_vec(&bundle.child_token).map_err(|e| format!("borsh: {e}"))?;
    let mut out = json!({
        "parentCapId": format!("0x{}", hex::encode(parent.body.cap_id)),
        "childCapId": format!("0x{}", hex::encode(child_cap_id)),
        "parentBudgetId": parent_budget.id,
        "childBudgetId": bundle.child_budget.id,
        "parentBudgetDeposited": parent_budget.total_deposited.to_string(),
        "childBudgetDeposited": bundle.child_budget.total_deposited.to_string(),
        "parentSubjectPub": format!("0x{}", hex::encode(parent.body.subject)),
        "childSubjectPub": format!("0x{}", hex::encode(bundle.child_token.body.subject)),
        "childSubjectSecret": format!("0x{}", hex::encode(child_subject_sk.to_bytes())),
        "sessionsDistinct": sessions_are_distinct(&bundle),
        "verificationReport": {
            "parentSignatureOk": bundle.report.parent_signature_ok,
            "childSignatureOk": bundle.report.child_signature_ok,
            "attenuationOk": bundle.report.attenuation_ok,
            "parentAllowsDelegation": bundle.report.parent_allows_delegation,
            "budgetLinkedOk": bundle.report.budget_linked_ok,
            "childHasNoRecursion": bundle.report.child_has_no_recursion,
            "childToolMaskSubset": bundle.report.child_tool_mask_subset,
            "allOk": bundle.report.all_ok(),
        },
        "childTokenHex": format!("0x{}", hex::encode(child_bytes)),
    });

    if parsed.run_e2e {
        let provider_sk = SigningKey::generate(&mut rng);
        let task_id = parsed
            .task_id
            .or(parent.body.scope.task_id)
            .unwrap_or(1);
        let mut market = ToolMarket::default();
        let session = run_verifier_tool_session_e2e(
            &bundle,
            task_id,
            builtins::FRAC,
            1_000,
            &mut market,
            &provider_sk,
        )
        .map_err(|e| format!("verifier e2e: {e}"))?;
        out["e2e"] = json!({
            "settled": session.settled,
            "intentId": format!("0x{}", hex::encode(session.intent_id)),
            "childBudgetSpent": session.child_budget_spent.to_string(),
            "childBudgetAvailable": session.child_budget_available.to_string(),
        });
    }

    Ok(out)
}

fn cmd_session_e2e_verifier_delegation(args: &[String]) -> Result<Value, String> {
    let parsed = parse_flags(args)?;
    let chain_id = parsed.chain_id.unwrap_or(41);
    let not_after = parsed.not_after_ms.unwrap_or(3_600_000);
    let parent_deposited = parsed
        .parent_budget_deposited
        .unwrap_or(10 * builtins::FRAC);
    let delegate_amount = parsed.delegate_amount.unwrap_or(2 * builtins::FRAC);
    let child_budget_id = parsed.child_budget_id.unwrap_or(2);

    let mint_args = vec![
        "--template".to_string(),
        "2".to_string(),
        "--chain-id".to_string(),
        chain_id.to_string(),
        "--not-after-ms".to_string(),
        not_after.to_string(),
    ];
    let mut mint_argv = mint_args;
    if let Some(w) = parsed.workspace {
        mint_argv.push("--workspace".to_string());
        mint_argv.push(w.to_string());
    }
    let mint = cmd_cap_mint(&mint_argv)?;
    let parent_hex = mint
        .get("tokenHex")
        .and_then(Value::as_str)
        .ok_or("mint missing tokenHex")?;
    let issuer_secret = mint
        .get("issuerSecret")
        .and_then(Value::as_str)
        .ok_or("mint missing issuerSecret")?;

    let mut delegate_argv = vec![
        "--parent-hex".to_string(),
        parent_hex.to_string(),
        "--issuer-secret".to_string(),
        issuer_secret.to_string(),
        "--parent-budget-deposited".to_string(),
        parent_deposited.to_string(),
        "--delegate-amount".to_string(),
        delegate_amount.to_string(),
        "--child-budget-id".to_string(),
        child_budget_id.to_string(),
        "--role".to_string(),
        "verifier".to_string(),
        "--run-e2e".to_string(),
    ];
    if let Some(t) = parsed.task_id {
        delegate_argv.push("--task-id".to_string());
        delegate_argv.push(t.to_string());
    }
    let delegate = cmd_cap_delegate(&delegate_argv)?;

    Ok(json!({
        "mint": mint,
        "delegate": delegate,
        "chainId": chain_id,
        "parentBudgetDeposited": parent_deposited.to_string(),
        "delegateAmount": delegate_amount.to_string(),
    }))
}

fn cmd_cap_build_revocation_proof(args: &[String]) -> Result<Value, String> {
    let parsed = parse_flags(args)?;
    let cap_id = parse_cap_id(
        parsed
            .cap_id_hex
            .as_ref()
            .ok_or("--cap-id required")?,
    )?;
    let mut set = RevocationSet::default();
    let mut loaded_from_rpc = false;
    if parsed.from_rpc {
        let rpc = chain::resolve_rpc_url(&parsed)
            .ok_or("--from-rpc requires --rpc-url or FRACTAL_RPC_URL")?;
        let (loaded, _root) = chain::load_revocation_set_from_rpc(&rpc)?;
        set = loaded;
        loaded_from_rpc = true;
    }
    if let Some(revoked_hex) = &parsed.revoked_entry_cap_hex {
        let revoked_id = parse_cap_id(revoked_hex)?;
        set.revoke(
            revoked_id,
            RevocationEntry {
                revoked_at_ms: parsed.not_after_ms.unwrap_or(1),
                reason_code: parsed.reason_code.unwrap_or(0),
                cascade: parsed.revoke_cascade,
            },
        )
        .map_err(|e| format!("revoked entry: {e}"))?;
    }
    let mut ancestors = Vec::new();
    if let Some(parent_hex) = &parsed.parent_cap_id_hex {
        ancestors.push(parse_cap_id(parent_hex)?);
    }
    let proof = set
        .build_verify_proof(cap_id, &ancestors)
        .map_err(|e| format!("build proof: {e}"))?;
    let proof_bytes = borsh::to_vec(&proof).map_err(|e| format!("borsh proof: {e}"))?;
    Ok(json!({
        "capId": format!("0x{}", hex::encode(cap_id)),
        "revocationRoot": format!("0x{}", hex::encode(proof.revocation_root)),
        "revokedLeafCount": proof.revoked_leaf_count,
        "proofBytes": proof.encoded_len(),
        "proofHex": format!("0x{}", hex::encode(proof_bytes)),
        "ancestorWitnessCount": proof.ancestors.len(),
        "loadedFromRpc": loaded_from_rpc,
    }))
}

fn cmd_cap_verify_revocation(args: &[String]) -> Result<Value, String> {
    let parsed = parse_flags(args)?;
    let token_hex = parsed
        .token_hex
        .as_ref()
        .ok_or("--token-hex required")?;
    let proof_hex = parsed
        .proof_hex
        .as_ref()
        .ok_or("--proof-hex required")?;
    let root_hex = parsed
        .revocation_root_hex
        .as_ref()
        .ok_or("--revocation-root-hex required")?;
    let token_bytes = parse_hex(token_hex, "--token-hex")?;
    let proof_bytes = parse_hex(proof_hex, "--proof-hex")?;
    let root = parse_cap_id(root_hex)?;
    let token =
        CapabilityToken::try_from_slice(&token_bytes).map_err(|e| format!("token borsh: {e}"))?;
    let proof = fractal_wallet::RevocationVerifyProof::try_from_slice(&proof_bytes)
        .map_err(|e| format!("proof borsh: {e}"))?;
    let mut ancestors = Vec::new();
    if let Some(parent) = token.body.parent_cap_id {
        ancestors.push(parent);
    }
    let now_ms = parsed.not_after_ms.unwrap_or(0);
    verify_capability_with_revocation(&token, now_ms, &root, &ancestors, &proof)
        .map_err(|e| format!("verify: {e}"))?;
    Ok(json!({
        "capId": format!("0x{}", hex::encode(token.body.cap_id)),
        "revocationRoot": format!("0x{}", hex::encode(root)),
        "revocationOk": true,
        "ancestorCount": ancestors.len(),
    }))
}

fn cmd_cap_show(args: &[String]) -> Result<Value, String> {
    let token_hex = args
        .first()
        .ok_or_else(|| "cap show: missing <token_hex>".to_string())?;
    let h = token_hex.strip_prefix("0x").unwrap_or(token_hex);
    let bytes = hex::decode(h).map_err(|e| format!("token hex: {e}"))?;
    let token = CapabilityToken::try_from_slice(&bytes).map_err(|e| format!("borsh decode: {e}"))?;
    let sig_ok = token.verify().is_ok();
    let mask_ok = token.verify_autonomous_tool_mask().is_ok();

    let scope = &token.body.scope;
    let caveats: Vec<String> = token.body.caveats.iter().map(render_caveat).collect();

    Ok(json!({
        "version": token.body.version,
        "capId": format!("0x{}", hex::encode(token.body.cap_id)),
        "chainId": token.body.chain_id,
        "issuerPub": format!("0x{}", hex::encode(token.body.issuer)),
        "subjectPub": format!("0x{}", hex::encode(token.body.subject)),
        "parentCapId": token.body.parent_cap_id.map(|p| format!("0x{}", hex::encode(p))),
        "scope": {
            "workspaceId": scope.workspace_id,
            "projectId": scope.project_id,
            "taskId": scope.task_id,
            "toolClassMask": format!("0x{:016x}", scope.tool_class_mask),
            "toolClasses": tool_classes_in_mask(scope.tool_class_mask).iter().map(|t| format!("{t:?}")).collect::<Vec<_>>(),
            "providersCount": scope.providers.as_ref().map(std::collections::BTreeSet::len),
        },
        "caveats": caveats,
        "budgetAccount": token.body.budget_account,
        "notBeforeMs": token.body.not_before,
        "notAfterMs": token.body.not_after,
        "nonce": token.body.nonce,
        "signatureOk": sig_ok,
        "autonomousToolMaskOk": mask_ok,
    }))
}

fn render_caveat(c: &Caveat) -> String {
    match c {
        Caveat::MaxTotalSpend(a) => format!("MaxTotalSpend({a})"),
        Caveat::MaxPerCallSpend { class, max } => format!("MaxPerCallSpend({class:?}, {max})"),
        Caveat::RateLimit {
            class,
            count,
            window_seconds,
        } => format!("RateLimit({class:?}, count={count}, window_s={window_seconds})"),
        Caveat::RequireApprovalAbove(a) => format!("RequireApprovalAbove({a})"),
        Caveat::OutputCommitmentRequired(c) => format!("OutputCommitmentRequired({c:?})"),
        Caveat::TeeAttestationRequired { class, tee } => {
            format!("TeeAttestationRequired({class:?}, {tee:?})")
        }
        Caveat::NoRecursion => "NoRecursion".to_string(),
    }
}

fn tool_classes_in_mask(mask: u64) -> Vec<ToolClass> {
    let mut out = Vec::new();
    for bit in 0..ToolClass::COUNT as u32 {
        if mask & (1u64 << bit) != 0 {
            if let Some(c) = ToolClass::from_bit(bit) {
                out.push(c);
            }
        }
    }
    out
}
