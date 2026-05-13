//! W6-a integration coverage for `fractal-wallet-cli`.
//!
//! These tests exercise the public `run_argv_value` surface (no spawned process)
//! so they are stable regardless of cargo's binary discovery on the host.

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
