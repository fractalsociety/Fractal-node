//! Sustained native NoOp load against a live JSON-RPC node (devnet stress / TPS estimate).

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use fractal_core::{
    NativeCall, Transaction, TxBody, VmKind, HARDHAT_DEFAULT_SIGNER_0, HARDHAT_DEFAULT_SIGNER_1,
};
use serde_json::{json, Value};

const SIGNERS: [[u8; 20]; 2] = [HARDHAT_DEFAULT_SIGNER_0, HARDHAT_DEFAULT_SIGNER_1];

fn rpc(agent: &ureq::Agent, url: &str, method: &str, params: Value) -> Result<Value, String> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1u64,
        "method": method,
        "params": params,
    });
    let resp: Value = agent
        .post(url)
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
        .ok_or_else(|| "missing result".into())
}

fn addr_hex(a: &[u8; 20]) -> String {
    format!("0x{}", hex::encode(a))
}

fn parse_hex_u64(s: &str) -> Result<u64, String> {
    let h = s.strip_prefix("0x").unwrap_or(s);
    u64::from_str_radix(h, 16).map_err(|e| format!("parse hex: {e}"))
}

fn fetch_nonce(agent: &ureq::Agent, rpc_url: &str, signer: &[u8; 20]) -> Result<u64, String> {
    let v = rpc(
        agent,
        rpc_url,
        "eth_getTransactionCount",
        json!([addr_hex(signer), "latest"]),
    )?;
    let s = v.as_str().ok_or("nonce not string")?;
    parse_hex_u64(s)
}

fn send_noop(
    agent: &ureq::Agent,
    rpc_url: &str,
    signer: [u8; 20],
    nonce: u64,
) -> Result<(), String> {
    let tx = Transaction {
        signer,
        nonce,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let raw = borsh::to_vec(&tx).map_err(|e| format!("borsh: {e}"))?;
    let hex_raw = format!("0x{}", hex::encode(raw));
    rpc(agent, rpc_url, "eth_sendRawTransaction", json!([hex_raw]))?;
    Ok(())
}

fn block_tx_count(agent: &ureq::Agent, rpc_url: &str, height: u64) -> Result<usize, String> {
    let b = rpc(
        agent,
        rpc_url,
        "eth_getBlockByNumber",
        json!([format!("0x{height:x}"), false]),
    )?;
    let n = b
        .get("transactions")
        .and_then(|t| t.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    Ok(n)
}

fn head_height(agent: &ureq::Agent, rpc_url: &str) -> Result<u64, String> {
    let v = rpc(agent, rpc_url, "eth_blockNumber", json!([]))?;
    parse_hex_u64(v.as_str().ok_or("blockNumber not string")?)
}

fn env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(default)
}

/// Optional cap on submit rate per worker (microseconds sleep after each successful send).
fn submit_pause_micros() -> u64 {
    env_u64("LOAD_SUBMIT_PAUSE_US", 200)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let rpc_url =
        std::env::var("FRACTAL_RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8545".into());
    let duration_secs = env_u64("LOAD_DURATION_SECS", 30);
    let requested_workers = env_usize("LOAD_WORKERS", SIGNERS.len());
    let allow_shared_signer_workers =
        std::env::var("LOAD_ALLOW_SHARED_SIGNER_WORKERS").as_deref() == Ok("1");
    let workers = if allow_shared_signer_workers {
        requested_workers
    } else {
        requested_workers.clamp(1, SIGNERS.len())
    };
    let warmup_secs = env_u64("LOAD_WARMUP_SECS", 3);

    println!("fractal-load-tps: rpc={rpc_url} duration={duration_secs}s workers={workers}");
    if workers != requested_workers {
        println!(
            "  requested_workers={requested_workers} capped to {workers}; set LOAD_ALLOW_SHARED_SIGNER_WORKERS=1 to share signers"
        );
    }

    let agent = ureq::AgentBuilder::new()
        .max_idle_connections_per_host(workers.saturating_add(4))
        .timeout_connect(Duration::from_secs(5))
        .timeout_read(Duration::from_secs(30))
        .build();

    let mut start_nonces = Vec::new();
    for signer in SIGNERS {
        let n = fetch_nonce(&agent, &rpc_url, &signer).unwrap_or_else(|e| {
            eprintln!("warn: nonce for {}: {e}", addr_hex(&signer));
            0
        });
        start_nonces.push(n);
        println!("  signer {} start_nonce={n}", addr_hex(&signer));
    }

    let submitted = Arc::new(AtomicU64::new(0));
    let submit_errors = Arc::new(AtomicU64::new(0));
    let nonce_counters: Vec<Arc<AtomicU64>> = start_nonces
        .iter()
        .map(|&n| Arc::new(AtomicU64::new(n)))
        .collect();

    let measure_start_height =
        head_height(&agent, &rpc_url).map_err(|e| format!("head before load: {e}"))?;

    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_workers = stop.clone();
    let rpc_workers = rpc_url.clone();
    let agent_workers = agent.clone();
    let submitters: Vec<_> = (0..workers)
        .map(|wid| {
            let submitted = submitted.clone();
            let submit_errors = submit_errors.clone();
            let stop = stop_workers.clone();
            let rpc_url = rpc_workers.clone();
            let agent = agent_workers.clone();
            let signer = SIGNERS[wid % SIGNERS.len()];
            let nonce_ctr = nonce_counters[wid % nonce_counters.len()].clone();
            thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let nonce = nonce_ctr.fetch_add(1, Ordering::Relaxed);
                    match send_noop(&agent, &rpc_url, signer, nonce) {
                        Ok(()) => {
                            submitted.fetch_add(1, Ordering::Relaxed);
                            // Avoid starving the node's producer/RPC threads on localhost.
                            thread::sleep(Duration::from_micros(submit_pause_micros()));
                        }
                        Err(_) => {
                            submit_errors.fetch_add(1, Ordering::Relaxed);
                            thread::sleep(Duration::from_millis(5));
                        }
                    }
                }
            })
        })
        .collect();

    let t0 = Instant::now();
    thread::sleep(Duration::from_secs(warmup_secs));
    let chain_t0 = Instant::now();
    let chain_start_height =
        head_height(&agent, &rpc_url).map_err(|e| format!("head at measure start: {e}"))?;

    thread::sleep(Duration::from_secs(
        duration_secs.saturating_sub(warmup_secs),
    ));
    stop.store(true, Ordering::Relaxed);
    for h in submitters {
        let _ = h.join();
    }
    let elapsed = chain_t0.elapsed();
    let total_elapsed = t0.elapsed();

    let chain_end_height =
        head_height(&agent, &rpc_url).map_err(|e| format!("head after load: {e}"))?;

    let mut nonce_confirmed = 0u64;
    for (signer, start) in SIGNERS.iter().zip(start_nonces.iter()) {
        let end = fetch_nonce(&agent, &rpc_url, signer).unwrap_or(*start);
        nonce_confirmed = nonce_confirmed.saturating_add(end.saturating_sub(*start));
    }

    let mut included_txs = 0u64;
    let mut blocks_with_txs = 0u64;
    let start = chain_start_height.saturating_add(1);
    let end = chain_end_height;
    for h in start..=end {
        if let Ok(n) = block_tx_count(&agent, &rpc_url, h) {
            if n > 0 {
                blocks_with_txs += 1;
            }
            included_txs += n as u64;
        }
    }
    let blocks_measured = end.saturating_sub(chain_start_height);
    let secs = elapsed.as_secs_f64().max(0.001);
    let chain_tps = included_txs as f64 / secs;
    let submit_tps = submitted.load(Ordering::Relaxed) as f64 / secs;
    let block_rate = blocks_measured as f64 / secs;
    let avg_tx_per_block = if blocks_measured > 0 {
        included_txs as f64 / blocks_measured as f64
    } else {
        0.0
    };

    println!();
    println!("=== load-tps results ===");
    println!(
        "measure window:     {:.1}s (warmup {}s excluded)",
        secs, warmup_secs
    );
    println!(
        "chain heights:      {} -> {} ({} new blocks, started from {})",
        chain_start_height, chain_end_height, blocks_measured, measure_start_height
    );
    println!("submitted (rpc):    {}", submitted.load(Ordering::Relaxed));
    println!(
        "submit errors:      {}",
        submit_errors.load(Ordering::Relaxed)
    );
    println!("included in chain:  {included_txs} txs in {blocks_with_txs} nonempty blocks");
    println!("confirmed (nonce):  {nonce_confirmed} txs (signer nonce delta)");
    let nonce_tps = nonce_confirmed as f64 / secs;
    println!("submit TPS:         {submit_tps:.1}");
    println!("confirmed chain TPS:{chain_tps:.1}  (txs in new blocks / measure window)");
    println!("confirmed nonce TPS:{nonce_tps:.1}  (on-chain nonce advance / measure window)");
    println!("block rate:         {block_rate:.2} blocks/s");
    println!("avg tx/block:       {avg_tx_per_block:.2}");
    println!("total elapsed:      {:.1}s", total_elapsed.as_secs_f64());
    println!();
    println!("Note: chain TPS is the meaningful ceiling for this node config;");
    println!("submit TPS can be higher if mempool backs up or leader cadence limits inclusion.");
    Ok(())
}
