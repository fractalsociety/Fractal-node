//! PRD §18 **M6** — devnet faucet (rate-limited). Sends `VmKind::Evm` / `Transfer` from [`fractal_core::DEVNET_FAUCET_TREASURY`].
//!
//! Not `docs/wallet.md`; this is JSON-RPC + native account balances only.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::extract::ConnectInfo;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Json};
use axum::routing::{get, post};
use axum::Router;
use fractal_core::{
    Address, Transaction, TxBody, VmKind, DEVNET_FAUCET_TREASURY, HARDHAT_DEFAULT_SIGNER_0,
};
use serde::Deserialize;
use tokio::sync::Mutex;

#[derive(Clone)]
struct AppState {
    rpc_url: String,
    drip: u128,
    cooldown: Duration,
    limits: Arc<Mutex<HashMap<String, Instant>>>,
}

#[derive(Deserialize)]
struct FundBody {
    address: String,
}

fn parse_addr(s: &str) -> Result<Address, String> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let b = hex::decode(s).map_err(|e| e.to_string())?;
    if b.len() != 20 {
        return Err("address must be 20 bytes".into());
    }
    let mut a = [0u8; 20];
    a.copy_from_slice(&b);
    Ok(a)
}

fn rpc_call(
    url: &str,
    method: &str,
    params: serde_json::Value,
) -> Result<serde_json::Value, String> {
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
        return Err(format!("rpc: {err}"));
    }
    resp.get("result")
        .cloned()
        .ok_or_else(|| "missing result".to_string())
}

fn addr_hex(a: &Address) -> String {
    format!("0x{}", hex::encode(a))
}

fn get_nonce(rpc_url: &str, who: &Address) -> Result<u64, String> {
    let v = rpc_call(
        rpc_url,
        "eth_getTransactionCount",
        serde_json::json!([addr_hex(who), "latest"]),
    )?;
    let s = v.as_str().ok_or("nonce not string")?;
    let hex = s.strip_prefix("0x").ok_or("nonce hex")?;
    u64::from_str_radix(hex, 16).map_err(|e| format!("nonce parse: {e}"))
}

fn send_transfer(
    rpc_url: &str,
    from: &Address,
    nonce: u64,
    to: Address,
    amount: u128,
) -> Result<String, String> {
    let tx = Transaction {
        signer: *from,
        nonce,
        vm: VmKind::Evm,
        body: TxBody::Transfer { to, amount },
    };
    let raw = borsh::to_vec(&tx).map_err(|e| format!("borsh: {e}"))?;
    let hex = format!("0x{}", hex::encode(raw));
    let h = rpc_call(rpc_url, "eth_sendRawTransaction", serde_json::json!([hex]))?;
    h.as_str()
        .map(std::string::ToString::to_string)
        .ok_or_else(|| "tx hash not string".to_string())
}

fn rate_key(ip: std::net::IpAddr, xfwd: Option<&str>) -> String {
    if let Some(s) = xfwd {
        let first = s.split(',').next().unwrap_or(s).trim();
        if !first.is_empty() {
            return format!("xff:{first}");
        }
    }
    format!("ip:{ip}")
}

async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "ok": true, "service": "fractal-faucet" }))
}

async fn index() -> Html<&'static str> {
    Html(include_str!("static/index.html"))
}

async fn fund(
    ConnectInfo(ci): ConnectInfo<SocketAddr>,
    axum::extract::State(st): axum::extract::State<AppState>,
    headers: axum::http::HeaderMap,
    Json(body): Json<FundBody>,
) -> impl IntoResponse {
    let xfwd = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());
    let key = rate_key(ci.ip(), xfwd);

    {
        let mut g = st.limits.lock().await;
        if let Some(t) = g.get(&key) {
            if t.elapsed() < st.cooldown {
                let wait = st.cooldown.saturating_sub(t.elapsed()).as_secs();
                return (
                    StatusCode::TOO_MANY_REQUESTS,
                    Json(serde_json::json!({
                        "error": "rate_limited",
                        "retry_after_secs": wait,
                    })),
                )
                    .into_response();
            }
        }
        g.insert(key.clone(), Instant::now());
    }

    let to = match parse_addr(&body.address) {
        Ok(a) => a,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "bad_address", "message": e })),
            )
                .into_response();
        }
    };

    let nonce = match get_nonce(&st.rpc_url, &DEVNET_FAUCET_TREASURY) {
        Ok(n) => n,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": "rpc_nonce", "message": e })),
            )
                .into_response();
        }
    };

    let tx_hash = match send_transfer(&st.rpc_url, &DEVNET_FAUCET_TREASURY, nonce, to, st.drip) {
        Ok(h) => h,
        Err(e) => {
            return (
                StatusCode::BAD_GATEWAY,
                Json(serde_json::json!({ "error": "rpc_send", "message": e })),
            )
                .into_response();
        }
    };

    Json(serde_json::json!({
        "ok": true,
        "txHash": tx_hash,
        "to": body.address,
        "amount": st.drip.to_string(),
        "treasury": addr_hex(&DEVNET_FAUCET_TREASURY),
    }))
    .into_response()
}

#[tokio::main]
async fn main() {
    if let Err(e) = run().await {
        eprintln!("fractal-faucet: {e}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let rpc_url =
        std::env::var("FRACTAL_RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8545".into());
    let bind: SocketAddr = std::env::var("FAUCET_BIND")
        .unwrap_or_else(|_| "127.0.0.1:8088".into())
        .parse()
        .map_err(|e| format!("FAUCET_BIND: {e}"))?;
    let drip: u128 = std::env::var("FAUCET_DRIP_AMOUNT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_000_000_000_000_000_000);
    let cooldown_secs: u64 = std::env::var("FAUCET_COOLDOWN_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60);
    let cooldown = Duration::from_secs(cooldown_secs.max(1));

    let st = AppState {
        rpc_url,
        drip,
        cooldown,
        limits: Arc::new(Mutex::new(HashMap::new())),
    };

    let app = Router::new()
        .route("/", get(index))
        .route("/health", get(health))
        .route("/fund", post(fund))
        .with_state(st);

    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .map_err(|e| format!("bind {bind}: {e}"))?;
    eprintln!(
        "{}",
        serde_json::json!({
            "step": "faucet_listen",
            "bind": bind.to_string(),
            "rpcUrl": std::env::var("FRACTAL_RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8545".into()),
            "dripAmount": drip.to_string(),
            "cooldownSecs": cooldown_secs,
            "hardhat0": addr_hex(&HARDHAT_DEFAULT_SIGNER_0),
            "treasury": addr_hex(&DEVNET_FAUCET_TREASURY),
        })
    );

    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .map_err(|e| format!("serve: {e}"))?;
    Ok(())
}
