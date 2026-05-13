//! W6-a / W6-b integration coverage for `fractal-wallet-cli` and `tools/wallet-web`.
//!
//! These tests exercise the public `run_argv_value` surface (no spawned process)
//! so they are stable regardless of cargo's binary discovery on the host.

use std::path::PathBuf;

use serde_json::Value;

fn argv(parts: &[&str]) -> Vec<String> {
    let mut v = vec!["fractal-wallet-cli".to_string()];
    v.extend(parts.iter().map(std::string::ToString::to_string));
    v
}

#[test]
fn policy_list_shows_three_builtins() {
    let v = fractal_cli::run_argv_value(&argv(&["policy", "list"])).unwrap();
    let templates = v.get("templates").and_then(Value::as_array).unwrap();
    assert_eq!(templates.len(), 3, "exactly 3 §15.2 built-ins");
    let names: Vec<&str> = templates
        .iter()
        .filter_map(|t| t.get("name").and_then(Value::as_str))
        .collect();
    assert!(names.contains(&"tpl:research-agent-v1"));
    assert!(names.contains(&"tpl:coding-agent-v1"));
    assert!(names.contains(&"tpl:verifier-agent-v1"));
}

#[test]
fn policy_show_research_renders_known_caps() {
    let v = fractal_cli::run_argv_value(&argv(&["policy", "show", "1"])).unwrap();
    assert_eq!(v.get("templateId").and_then(Value::as_u64), Some(1));
    let cap = v.get("totalCap").and_then(Value::as_str).unwrap();
    // 3 FRAC = 3 * 10^18 base units
    assert_eq!(cap, "3000000000000000000");
    let caveats = v.get("caveats").and_then(Value::as_array).unwrap();
    assert!(caveats.iter().any(|c| c.as_str().unwrap_or("").contains("MaxTotalSpend(3000000000000000000)")));
}

#[test]
fn cap_mint_then_show_round_trip() {
    let mint = fractal_cli::run_argv_value(&argv(&[
        "cap",
        "mint",
        "--template",
        "2", // coding-agent
        "--chain-id",
        "41",
        "--not-after-ms",
        "1000000",
        "--workspace",
        "7",
        "--nonce",
        "9",
    ]))
    .expect("mint succeeds");
    let token_hex = mint.get("tokenHex").and_then(Value::as_str).unwrap();
    assert!(token_hex.starts_with("0x"));
    assert_eq!(
        mint.get("templateName").and_then(Value::as_str),
        Some("tpl:coding-agent-v1")
    );

    let show = fractal_cli::run_argv_value(&argv(&["cap", "show", token_hex])).expect("show ok");
    assert_eq!(show.get("chainId").and_then(Value::as_u64), Some(41));
    assert_eq!(show.get("nonce").and_then(Value::as_u64), Some(9));
    assert_eq!(show.get("signatureOk").and_then(Value::as_bool), Some(true));
    assert_eq!(
        show.get("autonomousToolMaskOk").and_then(Value::as_bool),
        Some(true)
    );
    let scope = show.get("scope").unwrap();
    assert_eq!(scope.get("workspaceId").and_then(Value::as_u64), Some(7));
    let classes = scope.get("toolClasses").and_then(Value::as_array).unwrap();
    assert!(classes.len() >= 2, "coding template exposes ≥2 tool classes");
}

#[test]
fn cap_mint_rejects_bad_window() {
    let err = fractal_cli::run_argv_value(&argv(&[
        "cap",
        "mint",
        "--template",
        "1",
        "--chain-id",
        "41",
        "--not-after-ms",
        "0",
    ]))
    .unwrap_err();
    assert!(err.contains("must be >"), "error msg = {err}");
}

#[test]
fn unknown_command_returns_usage_error() {
    let err = fractal_cli::run_argv_value(&argv(&["zzz"])).unwrap_err();
    assert!(err.starts_with("usage:"));
}

fn mint_research_root() -> Value {
    fractal_cli::run_argv_value(&argv(&[
        "cap",
        "mint",
        "--template",
        "1", // research-agent
        "--chain-id",
        "41",
        "--not-after-ms",
        "1000000",
        "--workspace",
        "7",
        "--nonce",
        "1",
    ]))
    .expect("root mint succeeds")
}

#[test]
fn cap_attenuate_round_trip_holds_invariants() {
    let parent = mint_research_root();
    let parent_hex = parent.get("tokenHex").and_then(Value::as_str).unwrap();
    let issuer_secret = parent.get("issuerSecret").and_then(Value::as_str).unwrap();

    // Strictly narrower: half the time, half the spend, same workspace.
    let child = fractal_cli::run_argv_value(&argv(&[
        "cap",
        "attenuate",
        "--parent-hex",
        parent_hex,
        "--issuer-secret",
        issuer_secret,
        "--not-after-ms",
        "500000",
        "--max-total-spend",
        "1000000000000000000", // 1 FRAC (parent has 3 FRAC)
        "--nonce",
        "2",
    ]))
    .expect("attenuate succeeds");

    assert_eq!(child.get("attenuationOk").and_then(Value::as_bool), Some(true));
    let child_token = child.get("tokenHex").and_then(Value::as_str).unwrap();

    let show = fractal_cli::run_argv_value(&argv(&["cap", "show", child_token])).unwrap();
    assert_eq!(show.get("signatureOk").and_then(Value::as_bool), Some(true));
    assert_eq!(show.get("notAfterMs").and_then(Value::as_u64), Some(500_000));
    let parent_cap_id = parent.get("capId").and_then(Value::as_str).unwrap();
    assert_eq!(
        show.get("parentCapId").and_then(Value::as_str),
        Some(parent_cap_id)
    );
    let caveats = show.get("caveats").and_then(Value::as_array).unwrap();
    assert!(caveats
        .iter()
        .any(|c| c.as_str().unwrap_or("").contains("MaxTotalSpend(1000000000000000000)")));
}

#[test]
fn cap_attenuate_rejects_widening_time() {
    let parent = mint_research_root();
    let parent_hex = parent.get("tokenHex").and_then(Value::as_str).unwrap();
    let issuer_secret = parent.get("issuerSecret").and_then(Value::as_str).unwrap();
    let err = fractal_cli::run_argv_value(&argv(&[
        "cap",
        "attenuate",
        "--parent-hex",
        parent_hex,
        "--issuer-secret",
        issuer_secret,
        "--not-after-ms",
        "9999999999", // wider than parent's 1_000_000
    ]))
    .unwrap_err();
    assert!(err.contains("not-after-ms"), "err = {err}");
}

#[test]
fn cap_attenuate_rejects_widening_spend() {
    let parent = mint_research_root();
    let parent_hex = parent.get("tokenHex").and_then(Value::as_str).unwrap();
    let issuer_secret = parent.get("issuerSecret").and_then(Value::as_str).unwrap();
    // parent has MaxTotalSpend(3 FRAC) = 3e18; ask for 5 FRAC = 5e18.
    let err = fractal_cli::run_argv_value(&argv(&[
        "cap",
        "attenuate",
        "--parent-hex",
        parent_hex,
        "--issuer-secret",
        issuer_secret,
        "--max-total-spend",
        "5000000000000000000",
    ]))
    .unwrap_err();
    assert!(err.contains("MaxTotalSpend"), "err = {err}");
}

#[test]
fn cap_attenuate_rejects_wrong_secret() {
    let parent = mint_research_root();
    let parent_hex = parent.get("tokenHex").and_then(Value::as_str).unwrap();
    // Use a *different* mint's issuer secret — should fail the issuer-match check.
    let other = mint_research_root();
    let other_secret = other.get("issuerSecret").and_then(Value::as_str).unwrap();
    let err = fractal_cli::run_argv_value(&argv(&[
        "cap",
        "attenuate",
        "--parent-hex",
        parent_hex,
        "--issuer-secret",
        other_secret,
    ]))
    .unwrap_err();
    assert!(err.contains("does not match parent.issuer"), "err = {err}");
}

/// `tools/wallet-web/builtins.json` must match `policy dump-builtins` (regenerate after policy edits).
#[test]
fn wallet_web_builtins_json_matches_cli_dump() {
    let dump = fractal_cli::run_argv_value(&argv(&["policy", "dump-builtins"])).expect("dump-builtins");
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/wallet-web/builtins.json");
    let file = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let from_file: Value = serde_json::from_str(&file).expect("parse builtins.json");
    assert_eq!(
        dump, from_file,
        "tools/wallet-web/builtins.json is stale; run:\n  cargo run -p fractal-cli -- policy dump-builtins > tools/wallet-web/builtins.json"
    );
}
