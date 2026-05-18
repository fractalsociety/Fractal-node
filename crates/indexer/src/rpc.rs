//! JSON-RPC client for chain sync.

use borsh::BorshDeserialize;
use fractal_core::{Transaction, VmKind};
use serde_json::Value;

pub fn rpc_post(url: &str, method: &str, params: Value) -> Result<Value, String> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1u64,
        "method": method,
        "params": params,
    });
    let resp = ureq::post(url)
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|e| format!("http: {e}"))?;
    let v: Value = resp.into_json().map_err(|e| format!("json: {e}"))?;
    if let Some(err) = v.get("error") {
        return Err(format!("rpc error: {err}"));
    }
    Ok(v)
}

pub fn rpc_result_str(url: &str, method: &str, params: Value) -> Result<String, String> {
    let v = rpc_post(url, method, params)?;
    v.get("result")
        .and_then(|x| x.as_str())
        .map(String::from)
        .ok_or_else(|| "missing result string".into())
}

pub fn head_number(url: &str) -> Result<u64, String> {
    let hex = rpc_result_str(url, "eth_blockNumber", serde_json::json!([]))?;
    u64::from_str_radix(hex.trim_start_matches("0x"), 16).map_err(|e| format!("blockNumber: {e}"))
}

pub fn decode_tx_from_rpc(txv: &Value) -> Option<Transaction> {
    let fb = txv.pointer("/result/fractalTxBorsh")?.as_str()?;
    let raw = hex::decode(fb.trim_start_matches("0x")).ok()?;
    let tx = Transaction::try_from_slice(&raw).ok()?;
    Some(tx)
}

pub fn block_timestamp_ms(bv: &Value) -> u64 {
    let hex = bv
        .pointer("/result/timestamp")
        .and_then(|x| x.as_str())
        .unwrap_or("0x0");
    let sec = u64::from_str_radix(hex.trim_start_matches("0x"), 16).unwrap_or(0);
    sec.saturating_mul(1000)
}

pub fn is_native_tx(tx: &Transaction) -> bool {
    matches!(tx.vm, VmKind::Native)
}
