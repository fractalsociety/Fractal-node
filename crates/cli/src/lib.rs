//! `fractal-wallet-cli` — reference operator CLI (W6 slice).
//!
//! Anchored on `docs/wallet.md` §15.2 (built-in policy templates) and §4.2
//! (capability mint / verify). All subcommands are **offline** in this slice; no chain RPC.
//! **W6-b:** `tools/wallet-web/` static stub + `policy dump-builtins`. **W6-c (next):** provider SDK surfaces.
//! The argv shape stays tiny and dependency-free so integration tests can call `run_argv` directly.

use std::collections::BTreeMap;

use borsh::BorshDeserialize;
use ed25519_dalek::SigningKey;
use fractal_wallet::{
    capability::{derive_cap_id, CapabilitySignBody, CapabilityToken},
    caveat::Caveat,
    policy_builtins as builtins,
    types::{Scope, ToolClass},
    PolicyRegistry, ResolvedPolicy, TemplateId,
};
use rand::rngs::OsRng;
use serde_json::{json, Value};

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
        ("cap", Some("mint")) => cmd_cap_mint(&argv[3..]),
        ("cap", Some("show")) => cmd_cap_show(&argv[3..]),
        ("help" | "--help" | "-h", _) => Ok(json!({ "usage": usage() })),
        _ => Err(usage()),
    }
}

fn usage() -> String {
    "usage: fractal-wallet-cli <command> [args]\n  policy list\n  policy show <template_id>\n  policy dump-builtins\n  cap mint --template <id> --chain-id <n> --not-after-ms <t> [--workspace <id>] [--cap-id <hex32>] [--nonce <n>]\n  cap show <token_hex>"
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
    json!({
        "schemaVersion": 1,
        "templates": templates,
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
    json!({
        "templateId": id,
        "totalCap": r.default_budget.total_cap.to_string(),
        "perToolCap": per_tool,
        "rateLimits": rate,
        "caveats": caveats,
        "suggestedToolMask": format!("0x{:016x}", builtins::suggested_tool_class_mask(id).unwrap_or(0)),
    })
}

#[derive(Debug, Default)]
struct CapMintArgs {
    template: Option<TemplateId>,
    chain_id: Option<u32>,
    not_after_ms: Option<u64>,
    not_before_ms: Option<u64>,
    workspace: Option<u64>,
    nonce: Option<u64>,
    cap_id_hex: Option<String>,
}

fn parse_flags(args: &[String]) -> Result<CapMintArgs, String> {
    let mut out = CapMintArgs::default();
    let mut i = 0;
    while i < args.len() {
        let k = &args[i];
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

fn parse_cap_id(s: &str) -> Result<[u8; 32], String> {
    let h = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(h).map_err(|e| format!("--cap-id hex: {e}"))?;
    if bytes.len() != 32 {
        return Err("--cap-id must be 32 bytes".into());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
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
