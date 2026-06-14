//! Poll JSON-RPC for new heads and minimal block metadata (`docs/wallet.md` W6-d / W6-e).
//!
//! ```text
//! INDEXER_RPC_URL=http://127.0.0.1:8545 INDEXER_POLL_MS=3000 cargo run -p fractal-indexer-stub
//! INDEXER_JSON_LOG=1 …   # newline-delimited JSON events on stderr
//! ```

use std::time::Duration;

use serde_json::Value;

fn rpc_value(url: &str, method: &str, params: serde_json::Value) -> Result<Value, String> {
    let body = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1u64,
        "method": method,
        "params": params,
    });
    let resp = ureq::post(url)
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(|e| e.to_string())?;
    let v: Value = resp.into_json().map_err(|e| e.to_string())?;
    if let Some(err) = v.get("error") {
        return Err(err.to_string());
    }
    Ok(v)
}

fn rpc_str(url: &str, method: &str, params: serde_json::Value) -> Result<String, String> {
    let v = rpc_value(url, method, params)?;
    v.get("result")
        .and_then(|x| x.as_str())
        .map(String::from)
        .ok_or_else(|| "missing result".into())
}

fn log_json(evt: &str, extra: Value) {
    let mut m = serde_json::Map::new();
    m.insert(
        "evt".to_string(),
        serde_json::Value::String(evt.to_string()),
    );
    if let Value::Object(o) = extra {
        for (k, v) in o {
            m.insert(k, v);
        }
    }
    eprintln!("{}", serde_json::Value::Object(m));
}

fn main() {
    let url = std::env::var("INDEXER_RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8545".into());
    let poll_ms: u64 = std::env::var("INDEXER_POLL_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5_000);
    let json_log = std::env::var("INDEXER_JSON_LOG")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    eprintln!("fractal-indexer-stub: rpc={url} poll_ms={poll_ms} json_log={json_log}");

    let mut last: Option<u64> = None;
    loop {
        match rpc_str(&url, "eth_blockNumber", serde_json::json!([])) {
            Ok(hex) => {
                let h = u64::from_str_radix(hex.trim_start_matches("0x"), 16).unwrap_or(0);
                if last != Some(h) {
                    if json_log {
                        log_json("head", serde_json::json!({ "number": h, "hex": hex }));
                    } else {
                        eprintln!("fractal-indexer-stub: head {hex} ({h})");
                    }
                    last = Some(h);
                    let tag = format!("0x{:x}", h);
                    match rpc_value(
                        &url,
                        "eth_getBlockByNumber",
                        serde_json::json!([tag, false]),
                    ) {
                        Ok(bv) => {
                            let res = bv.get("result").cloned().unwrap_or(Value::Null);
                            let n_tx = res
                                .get("transactions")
                                .and_then(|t| t.as_array())
                                .map(|a| a.len())
                                .unwrap_or(0);
                            let bh = res.get("hash").and_then(|x| x.as_str()).unwrap_or("");
                            if json_log {
                                log_json(
                                    "block",
                                    serde_json::json!({
                                        "number": h,
                                        "hash": bh,
                                        "transactionCount": n_tx,
                                    }),
                                );
                            } else {
                                eprintln!("fractal-indexer-stub: block {tag} hash={bh} txs={n_tx}");
                            }
                        }
                        Err(e) => eprintln!("fractal-indexer-stub: eth_getBlockByNumber: {e}"),
                    }
                }
            }
            Err(e) => eprintln!("fractal-indexer-stub: rpc error: {e}"),
        }
        std::thread::sleep(Duration::from_millis(poll_ms));
    }
}
