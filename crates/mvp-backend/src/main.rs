//! PRD §18 **M5** — Core MVP **bridge** binary: one `SETTLE_BATCH` then `CLAIM_PAYOUT` per receipt over
//! `eth_sendRawTransaction` (borsh [`fractal_core::Transaction`]).
//!
//! Anchored on **`docs/prd.md` M5**, not `docs/wallet.md`.
//!
//! ```text
//! FRACTAL_RPC_URL=http://127.0.0.1:8545 cargo run -p fractal-mvp-backend --bin fractal-mvp-bridge
//! MVP_RECEIPT_COUNT=100   # optional; default 100 for PRD exit scale (synthetic receipts)
//! MVP_RECEIPTS_JSON=/path/to/export.json   # optional; real off-chain receipts (see testdata/mvp_receipts_sample.json)
//! cargo run -p fractal-mvp-backend --bin fractal-mvp-bridge -- --help
//! ```

use fractal_mvp_backend::receipt_json;

use std::time::{SystemTime, UNIX_EPOCH};

use fractal_core::{Address, Transaction, HARDHAT_DEFAULT_SIGNER_0, HARDHAT_DEFAULT_SIGNER_1};

fn addr_hex(a: &Address) -> String {
    format!("0x{}", hex::encode(a))
}

fn usage() -> &'static str {
    "\
fractal-mvp-bridge — PRD M5 Core MVP bridge

Submits one SETTLE_BATCH then one CLAIM_PAYOUT per receipt via JSON-RPC
(eth_sendRawTransaction with borsh-encoded native Transaction).

Environment:
  FRACTAL_RPC_URL       JSON-RPC HTTP endpoint (default: http://127.0.0.1:8545)
  MVP_RECEIPT_COUNT     Synthetic receipt count (default: 100). Ignored when MVP_RECEIPTS_JSON is set.
  MVP_RECEIPTS_JSON     Path to operator receipt export JSON (see crates/mvp-backend/testdata/mvp_receipts_sample.json)

Examples:
  FRACTAL_RPC_URL=http://127.0.0.1:8545 cargo run -p fractal-mvp-backend --bin fractal-mvp-bridge
  MVP_RECEIPT_COUNT=3 cargo run -p fractal-mvp-backend --bin fractal-mvp-bridge
  MVP_RECEIPTS_JSON=./crates/mvp-backend/testdata/mvp_receipts_sample.json \\
    cargo run -p fractal-mvp-backend --bin fractal-mvp-bridge
"
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

fn eth_get_balance(rpc_url: &str, who: &Address) -> Result<String, String> {
    let v = rpc(
        rpc_url,
        "eth_getBalance",
        serde_json::json!([addr_hex(who), "latest"]),
    )?;
    v.as_str()
        .map(std::string::ToString::to_string)
        .ok_or_else(|| "eth_getBalance: result not string".to_string())
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

fn preflight_rpc(rpc_url: &str) -> Result<serde_json::Value, String> {
    let chain_id = rpc(rpc_url, "eth_chainId", serde_json::json!([]))?;
    let block = rpc(rpc_url, "eth_blockNumber", serde_json::json!([]))?;
    Ok(serde_json::json!({
        "eth_chainId": chain_id,
        "eth_blockNumber": block,
    }))
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
    if std::env::args().any(|a| a == "--help" || a == "-h") {
        eprint!("{}", usage());
        return Ok(());
    }

    let rpc_url = std::env::var("FRACTAL_RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8545".into());
    let preflight = preflight_rpc(&rpc_url)?;

    let (settle, claims, operator, claim_agent, total_payout, receipt_count, batch_hex, mode) =
        if let Ok(path) = std::env::var("MVP_RECEIPTS_JSON") {
            let (payload, claim_agent) = receipt_json::load_settle_payload_from_json(&path)?;
            let operator = payload.operator;
            let total: u128 = payload.payout_entries.iter().map(|e| e.amount).sum();
            let n = payload.receipts.len() as u32;
            let batch_hex = format!("0x{}", hex::encode(payload.batch_id));
            let op_nonce = get_nonce(&rpc_url, &operator)?;
            let ag_nonce = get_nonce(&rpc_url, &claim_agent)?;
            let (settle, claims) = fractal_sdk::m5::build_settle_then_claim_txs_from_payload(
                payload,
                op_nonce,
                claim_agent,
                ag_nonce,
            )
            .map_err(|e| format!("{e:?}"))?;
            (
                settle,
                claims,
                operator,
                claim_agent,
                total,
                n,
                batch_hex,
                "json_receipts",
            )
        } else {
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
            let total = count as u128;
            let batch_hex = format!("0x{}", hex::encode(batch_id));
            (
                settle,
                claims,
                operator,
                agent,
                total,
                count,
                batch_hex,
                "synthetic",
            )
        };

    let balance_before = eth_get_balance(&rpc_url, &claim_agent).ok();

    eprintln!(
        "{}",
        serde_json::json!({
            "step": "mvp_bridge_start",
            "rpcUrl": rpc_url,
            "preflight": preflight,
            "mode": mode,
            "operator": addr_hex(&operator),
            "claimAgent": addr_hex(&claim_agent),
            "receiptCount": receipt_count,
            "batchId": batch_hex,
            "totalPayoutWei": total_payout.to_string(),
            "claimAgentBalanceBefore": balance_before,
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

    let balance_after = eth_get_balance(&rpc_url, &claim_agent)?;

    eprintln!(
        "{}",
        serde_json::json!({
            "step": "mvp_bridge_done",
            "settleTxHash": h0,
            "claimCount": claims.len(),
            "totalPayoutWei": total_payout.to_string(),
            "claimAgentBalanceAfter": balance_after,
            "eth_getBalanceNote": "native tFRAC claims credit the same 20-byte account key as Ethereum balance RPC",
        })
    );
    Ok(())
}
