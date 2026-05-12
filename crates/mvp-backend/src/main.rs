//! PRD §18 **M5** — minimal “Core MVP backend”: push one `SETTLE_BATCH` then `CLAIM_PAYOUT` txs over `eth_sendRawTransaction` (borsh `Transaction`).
//!
//! Anchored on **`docs/prd.md` M5**, not `docs/wallet.md`.
//!
//! Usage:
//! ```text
//! FRACTAL_RPC_URL=http://127.0.0.1:8545 cargo run -p fractal-mvp-backend --bin fractal-mvp-bridge
//! MVP_RECEIPT_COUNT=100   # optional; default 100 for PRD exit scale
//! ```

use std::time::{SystemTime, UNIX_EPOCH};

use fractal_core::{Address, Transaction, HARDHAT_DEFAULT_SIGNER_0, HARDHAT_DEFAULT_SIGNER_1};

fn addr_hex(a: &Address) -> String {
    format!("0x{}", hex::encode(a))
}

fn rpc(url: &str, method: &str, params: serde_json::Value) -> Result<serde_json::Value, String> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1u64,
        "method": method,
        "params": params,
    });
    let resp: serde_json::Value = ureq::post(url)
        .set("Content-Type", "application/json; charset=utf-8")
        .send_json(body)
        .map_err(|e| format!("http: {e}"))?
        .into_json()
        .map_err(|e| format!("json: {e}"))?;
    if let Some(err) = resp.get("error") {
        return Err(format!("rpc error: {err}"));
    }
    resp.get("result")
        .cloned()
        .ok_or_else(|| "missing result".to_string())
}

fn get_nonce(rpc_url: &str, who: &Address) -> Result<u64, String> {
    let v = rpc(
        rpc_url,
        "eth_getTransactionCount",
        serde_json::json!([addr_hex(who), "latest"]),
    )?;
    let s = v.as_str().ok_or("nonce not string")?;
    let hex = s.strip_prefix("0x").ok_or("nonce hex")?;
    u64::from_str_radix(hex, 16).map_err(|e| format!("parse nonce: {e}"))
}

fn send_borsh_tx(rpc_url: &str, tx: &Transaction) -> Result<String, String> {
    let raw = borsh::to_vec(tx).map_err(|e| format!("borsh: {e}"))?;
    let hex = format!("0x{}", hex::encode(raw));
    let h = rpc(
        rpc_url,
        "eth_sendRawTransaction",
        serde_json::json!([hex]),
    )?;
    h.as_str()
        .map(std::string::ToString::to_string)
        .ok_or_else(|| "tx hash not string".to_string())
}

fn main() {
    if let Err(e) = run() {
        eprintln!(
            "{}",
            serde_json::json!({
                "step": "mvp_bridge_failed",
                "message": e,
            })
        );
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let rpc_url = std::env::var("FRACTAL_RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8545".into());
    let count: u32 = std::env::var("MVP_RECEIPT_COUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100);
    if count == 0 {
        return Err("MVP_RECEIPT_COUNT must be > 0".into());
    }

    let operator = HARDHAT_DEFAULT_SIGNER_0;
    let agent = HARDHAT_DEFAULT_SIGNER_1;

    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let mut batch_id = [0u8; 32];
    batch_id[0..8].copy_from_slice(&ts.to_be_bytes());

    let op_nonce = get_nonce(&rpc_url, &operator)?;
    let ag_nonce = get_nonce(&rpc_url, &agent)?;

    let (settle, claims) = fractal_sdk::m5::build_settle_then_claim_txs(
        operator,
        op_nonce,
        agent,
        ag_nonce,
        batch_id,
        count,
        1,
        ts,
    );

    eprintln!(
        "{}",
        serde_json::json!({
            "step": "mvp_bridge_start",
            "rpcUrl": rpc_url,
            "operator": addr_hex(&operator),
            "agent": addr_hex(&agent),
            "receiptCount": count,
            "batchId": format!("0x{}", hex::encode(batch_id)),
        })
    );

    let h0 = send_borsh_tx(&rpc_url, &settle)?;
    eprintln!(
        "{}",
        serde_json::json!({ "step": "settle_submitted", "txHash": h0 })
    );

    for (i, tx) in claims.iter().enumerate() {
        let h = send_borsh_tx(&rpc_url, tx)?;
        if (i + 1) % 25 == 0 || i + 1 == claims.len() {
            eprintln!(
                "{}",
                serde_json::json!({
                    "step": "claims_progress",
                    "done": i + 1,
                    "total": claims.len(),
                    "lastTxHash": h,
                })
            );
        }
    }

    eprintln!(
        "{}",
        serde_json::json!({
            "step": "mvp_bridge_done",
            "settleTxHash": h0,
            "claimCount": claims.len(),
        })
    );
    Ok(())
}
