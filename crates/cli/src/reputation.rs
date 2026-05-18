//! Governance reputation snapshot helpers (`docs/wallet.md` §10.4, §17).

use std::fs;

use borsh::BorshDeserialize;
use fractal_wallet::{
    compute_reputation_score_milli, ReputationLedgerSummary, ReputationParams, SettlementEvent,
    ToolClass,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::chain::{resolve_rpc_url, rpc_post};
use crate::{parse_cap_id, parse_flags, CapFlags};

#[derive(Deserialize)]
struct SettlementEventJson {
    settled_at_ms: u64,
    #[serde(default = "default_weight")]
    weight: u128,
}

fn default_weight() -> u128 {
    1
}

#[derive(Deserialize)]
struct ReputationSummaryJson {
    tool_class: u8,
    #[serde(default)]
    successful: Vec<SettlementEventJson>,
    #[serde(default)]
    failed_settlements: u64,
    #[serde(default)]
    slashing_events: u64,
    first_seen_ms: u64,
    now_ms: u64,
    #[serde(default)]
    available_stake: u128,
    #[serde(default)]
    distinct_client_count: u32,
}

pub fn summary_from_json_value(v: &Value) -> Result<ReputationLedgerSummary, String> {
    let j: ReputationSummaryJson =
        serde_json::from_value(v.clone()).map_err(|e| format!("summary json: {e}"))?;
    summary_from_parsed(j)
}

fn summary_from_parsed(j: ReputationSummaryJson) -> Result<ReputationLedgerSummary, String> {
    let tc = ToolClass::from_discriminant(j.tool_class)
        .ok_or_else(|| format!("invalid tool_class {}", j.tool_class))?;
    Ok(ReputationLedgerSummary {
        tool_class: tc,
        successful: j
            .successful
            .into_iter()
            .map(|e| SettlementEvent {
                settled_at_ms: e.settled_at_ms,
                weight: e.weight,
            })
            .collect(),
        failed_settlements: j.failed_settlements,
        slashing_events: j.slashing_events,
        first_seen_ms: j.first_seen_ms,
        now_ms: j.now_ms,
        available_stake: j.available_stake,
        distinct_client_count: j.distinct_client_count,
    })
}

pub fn load_summary_from_path(path: &str) -> Result<ReputationLedgerSummary, String> {
    let raw = fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
    let v: Value = serde_json::from_str(&raw).map_err(|e| format!("parse {path}: {e}"))?;
    summary_from_json_value(&v)
}

pub fn cmd_reputation_preview(args: &[String]) -> Result<Value, String> {
    let parsed = parse_flags(args)?;
    let summary = resolve_summary(&parsed)?;
    let score = compute_reputation_score_milli(&summary, &ReputationParams::default());
    let borsh = borsh::to_vec(&summary).map_err(|e| format!("borsh: {e}"))?;
    Ok(json!({
        "toolClass": summary.tool_class as u8,
        "scoreMilli": score.to_string(),
        "summaryBorshHex": format!("0x{}", hex::encode(&borsh)),
        "successfulCount": summary.successful.len(),
        "failedSettlements": summary.failed_settlements,
        "slashingEvents": summary.slashing_events,
    }))
}

pub fn cmd_reputation_build_summary(args: &[String]) -> Result<Value, String> {
    let out = cmd_reputation_preview(args)?;
    let parsed = parse_flags(args)?;
    let provider = parsed
        .provider_id_hex
        .as_ref()
        .ok_or("--provider-id required")?;
    let provider_id = parse_cap_id(provider)?;
    let tool_class = parsed
        .tool_class
        .ok_or("--tool-class required (must match summary.tool_class)")?;
    let summary = resolve_summary(&parsed)?;
    if summary.tool_class as u8 != tool_class {
        return Err(format!(
            "--tool-class {tool_class} != summary.tool_class {}",
            summary.tool_class as u8
        ));
    }
    let borsh = borsh::to_vec(&summary).map_err(|e| format!("borsh: {e}"))?;
    Ok(json!({
        "providerId": format!("0x{}", hex::encode(provider_id)),
        "toolClass": tool_class,
        "scoreMilli": out.get("scoreMilli").cloned().unwrap_or(Value::Null),
        "summaryBorshHex": format!("0x{}", hex::encode(&borsh)),
        "hint": "chain submit-reputation-snapshot --provider-id … --tool-class … --summary-borsh-hex …",
    }))
}

pub fn cmd_reputation_show_store(args: &[String]) -> Result<Value, String> {
    let path = if let Some(p) = args.first() {
        p.clone()
    } else {
        std::env::var("INDEXER_REPUTATION_STORE_PATH").map_err(|_| {
            "usage: reputation show-store <path> (or set INDEXER_REPUTATION_STORE_PATH)".to_string()
        })?
    };
    let raw = fs::read_to_string(&path).map_err(|e| format!("read {path}: {e}"))?;
    let v: Value = serde_json::from_str(&raw).map_err(|e| format!("parse: {e}"))?;
    Ok(v)
}

pub fn cmd_reputation_show_chain(args: &[String]) -> Result<Value, String> {
    let parsed = parse_flags(args)?;
    let rpc = resolve_rpc_url(&parsed)
        .ok_or("--rpc-url or FRACTAL_RPC_URL required")?;
    let v = rpc_post(&rpc, "fractal_getWalletReputation", json!([]))?;
    if let Some(pid) = &parsed.provider_id_hex {
        let want = parse_cap_id(pid)?;
        if let Some(tc) = parsed.tool_class {
            let score = v
                .get("scores")
                .and_then(Value::as_array)
                .and_then(|rows| {
                    rows.iter().find(|r| {
                        let pid_ok = r
                            .get("providerId")
                            .and_then(|x| x.as_str())
                            .and_then(|h| parse_cap_id(h).ok())
                            == Some(want);
                        pid_ok
                            && r.get("toolClass").and_then(|x| x.as_u64())
                                == Some(u64::from(tc))
                    })
                });
            return Ok(json!({ "rpcUrl": rpc, "match": score.cloned().unwrap_or(Value::Null) }));
        }
    }
    Ok(json!({ "rpcUrl": rpc, "reputation": v }))
}

fn resolve_summary(parsed: &CapFlags) -> Result<ReputationLedgerSummary, String> {
    if let Some(hx) = &parsed.summary_borsh_hex {
        let bytes = crate::parse_hex(hx, "--summary-borsh-hex")?;
        return ReputationLedgerSummary::try_from_slice(&bytes)
            .map_err(|e| format!("summary borsh: {e}"));
    }
    if let Some(p) = &parsed.summary_json_path {
        return load_summary_from_path(p);
    }
    Err("provide --summary-json <path> or --summary-borsh-hex <hex>".into())
}

