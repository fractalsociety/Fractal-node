//! Poll JSON-RPC for new heads and minimal block metadata (`docs/wallet.md` W6-d / W6-e).
//!
//! Optional **`INDEXER_REPUTATION_STORE_PATH`**: JSON file updated from txs in each scanned block:
//! - **`NativeCall::WalletReputationSnapshotV1`**: governance snapshot (borsh summary as committed on-chain).
//! - **`NativeCall::ResolveDispute`** with [`fractal_core::DISPUTE_RESOLUTION_PROVIDER_FAULT`]: increments `slashing_events` for the disputed receipt’s worker (receipt meta must have been seen in a prior settle tx).
//! - **Stake / agent mirror** (persisted in `chainMirror`): `RegisterAgent`, `Stake`, `Unstake`, `Slash` update synthetic provider **`available_stake`** (native `stakes` map keyed by agent address).
//!
//! ```text
//! INDEXER_RPC_URL=http://127.0.0.1:8545 INDEXER_POLL_MS=3000 cargo run -p fractal-indexer-stub
//! INDEXER_JSON_LOG=1 …
//! INDEXER_REPUTATION_STORE_PATH=./target/indexer_reputation.json
//! INDEXER_CATCHUP_BLOCKS=2048     # when store has never scanned (watermark 0), scan at most this many blocks ending at head
//! INDEXER_REPUTATION_MERGE_SETTLEMENTS=0   # optional: disable Settle* merge (on by default)
//! ```

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

mod indexer_mirror;
mod ledger_merge;

use borsh::BorshDeserialize;
use fractal_core::{NativeCall, OnChainTaskReceipt, Transaction, TxBody, VmKind};
use fractal_crypto::hash::keccak256;
use indexer_mirror::IndexerChainMirror;
use ledger_merge::{
    apply_dispute_slash_to_summary, apply_onchain_receipt_to_summary, provider_and_key_from_receipt,
    row_key_for_worker_agent, tool_class_from_receipt, SettlementLedgerSide,
};
use fractal_wallet::{ReputationLedgerSummary, ToolClass};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Serialize, Deserialize, Default)]
struct ReputationStore {
    /// Last block height fully processed for reputation rows (catch-up watermark).
    #[serde(default)]
    last_scanned_block: u64,
    /// Key: `hex32_provider:tool_class_u8`
    #[serde(default)]
    rows: BTreeMap<String, StoreRow>,
    /// Replay-lite agent / stake / dispute state for §10.4 `available_stake` + dispute slash routing.
    #[serde(default, rename = "chainMirror")]
    chain_mirror: IndexerChainMirror,
}

#[derive(Clone, Serialize, Deserialize)]
struct StoreRow {
    pub last_block: u64,
    pub score_milli: String,
    pub ledger_commitment_hex: String,
    #[serde(default)]
    pub ledger_borsh_hex: Option<String>,
    /// Distinct `requester` addresses (hex), for `distinct_client_count`.
    #[serde(default)]
    pub client_requesters_hex: Vec<String>,
    /// `snapshot` | `settlement` | empty (legacy).
    #[serde(default)]
    pub kind: String,
}

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
    m.insert("evt".to_string(), serde_json::Value::String(evt.to_string()));
    if let Value::Object(o) = extra {
        for (k, v) in o {
            m.insert(k, v);
        }
    }
    eprintln!("{}", serde_json::Value::Object(m));
}

fn load_store(path: &Path) -> ReputationStore {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

fn save_store(path: &Path, store: &ReputationStore) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let s = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    std::fs::write(path, s).map_err(|e| e.to_string())
}

fn decode_tx_from_rpc(txv: &Value) -> Option<Transaction> {
    let fb = txv.pointer("/result/fractalTxBorsh")?.as_str()?;
    let hex = fb.trim_start_matches("0x");
    let bytes = hex::decode(hex).ok()?;
    let tx = Transaction::try_from_slice(&bytes).ok()?;
    if tx.vm != VmKind::Native {
        return None;
    }
    Some(tx)
}

fn persist_row(
    store: &mut ReputationStore,
    path: &Path,
    key: &str,
    block_number: u64,
    summary: &ReputationLedgerSummary,
    side: &SettlementLedgerSide,
    kind: &str,
) -> Result<(), String> {
    let summary_borsh = borsh::to_vec(summary).map_err(|e| e.to_string())?;
    let score = fractal_wallet::compute_reputation_score_milli(
        summary,
        &fractal_wallet::ReputationParams::default(),
    );
    let commitment = keccak256(&summary_borsh);
    let mut clients: Vec<String> = side.client_requesters.iter().cloned().collect();
    clients.sort();
    store.rows.insert(
        key.to_string(),
        StoreRow {
            last_block: block_number,
            score_milli: score.to_string(),
            ledger_commitment_hex: format!("0x{}", hex::encode(commitment)),
            ledger_borsh_hex: Some(format!("0x{}", hex::encode(&summary_borsh))),
            client_requesters_hex: clients,
            kind: kind.to_string(),
        },
    );
    save_store(path, store)
}

fn load_summary_and_side(row: &StoreRow) -> Option<(ReputationLedgerSummary, SettlementLedgerSide)> {
    let mut side = SettlementLedgerSide::default();
    for h in &row.client_requesters_hex {
        side.client_requesters.insert(h.clone());
    }
    let summary = if let Some(ref hx) = row.ledger_borsh_hex {
        let raw = hex::decode(hx.trim_start_matches("0x")).ok()?;
        ReputationLedgerSummary::try_from_slice(&raw).ok()?
    } else {
        return None;
    };
    Some((summary, side))
}

fn apply_settlement_receipts(
    store: &mut ReputationStore,
    path: &Path,
    block_number: u64,
    now_ms: u64,
    receipts: &[OnChainTaskReceipt],
    json_log: bool,
) -> Result<(), String> {
    for r in receipts {
        let (_pid, key) = provider_and_key_from_receipt(r);
        let tc = tool_class_from_receipt(r);
        let stake = store.chain_mirror.available_stake_wei_for_worker(r.worker);
        let mut summary;
        let mut side;
        if let Some(existing) = store.rows.get(&key) {
            if let Some((s, sd)) = load_summary_and_side(existing) {
                summary = s;
                side = sd;
            } else {
                eprintln!(
                    "fractal-indexer-stub: skip settlement merge for key={key} (missing ledger_borsh_hex on existing row)"
                );
                continue;
            }
        } else {
            summary = ReputationLedgerSummary {
                tool_class: tc,
                successful: vec![],
                failed_settlements: 0,
                slashing_events: 0,
                first_seen_ms: now_ms,
                now_ms,
                available_stake: stake,
                distinct_client_count: 0,
            };
            side = SettlementLedgerSide::default();
        }
        apply_onchain_receipt_to_summary(&mut summary, r, now_ms, &mut side, stake);
        let score = fractal_wallet::compute_reputation_score_milli(
            &summary,
            &fractal_wallet::ReputationParams::default(),
        );
        if json_log {
            log_json(
                "reputation_settlement_merge",
                serde_json::json!({
                    "block": block_number,
                    "key": key,
                    "scoreMilli": score.to_string(),
                    "worker": r.worker,
                    "toolClass": r.tool_class,
                    "availableStakeWei": stake.to_string(),
                }),
            );
        } else {
            eprintln!(
                "fractal-indexer-stub: settlement merge block={block_number} key={key} score_milli={score} worker={} tool_class={}",
                r.worker,
                r.tool_class
            );
        }
        persist_row(store, path, &key, block_number, &summary, &side, "settlement")?;
    }
    Ok(())
}

fn apply_dispute_slash_row(
    store: &mut ReputationStore,
    path: &Path,
    worker: u64,
    tool_class_u8: u8,
    block_number: u64,
    now_ms: u64,
    json_log: bool,
) -> Result<(), String> {
    let key = row_key_for_worker_agent(worker, tool_class_u8);
    let tc = ToolClass::from_discriminant(tool_class_u8).unwrap_or(ToolClass::Browser);
    let stake = store.chain_mirror.available_stake_wei_for_worker(worker);
    let mut summary;
    let mut side;
    if let Some(existing) = store.rows.get(&key) {
        if let Some((s, sd)) = load_summary_and_side(existing) {
            summary = s;
            side = sd;
        } else {
            eprintln!(
                "fractal-indexer-stub: skip dispute slash for key={key} (missing ledger_borsh_hex on existing row)"
            );
            return Ok(());
        }
    } else {
        summary = ReputationLedgerSummary {
            tool_class: tc,
            successful: vec![],
            failed_settlements: 0,
            slashing_events: 0,
            first_seen_ms: now_ms,
            now_ms,
            available_stake: stake,
            distinct_client_count: 0,
        };
        side = SettlementLedgerSide::default();
    }
    apply_dispute_slash_to_summary(&mut summary, now_ms, stake, &mut side, tc);
    let score = fractal_wallet::compute_reputation_score_milli(
        &summary,
        &fractal_wallet::ReputationParams::default(),
    );
    if json_log {
        log_json(
            "reputation_dispute_slash",
            serde_json::json!({
                "block": block_number,
                "key": key,
                "scoreMilli": score.to_string(),
                "worker": worker,
            }),
        );
    } else {
        eprintln!(
            "fractal-indexer-stub: dispute slash block={block_number} key={key} score_milli={score} worker={worker}"
        );
    }
    persist_row(
        store,
        path,
        &key,
        block_number,
        &summary,
        &side,
        "dispute_slash",
    )?;
    Ok(())
}

fn process_wallet_reputation_snapshot(
    store: &mut ReputationStore,
    path: &Path,
    block_number: u64,
    tx_index: usize,
    tx_hash: &str,
    provider_id: &[u8; 32],
    tool_class: u8,
    summary_borsh: &[u8],
    json_log: bool,
) -> Result<(), String> {
    let summary = ReputationLedgerSummary::try_from_slice(summary_borsh)
        .map_err(|_| "invalid ReputationLedgerSummary borsh".to_string())?;
    let score = fractal_wallet::compute_reputation_score_milli(
        &summary,
        &fractal_wallet::ReputationParams::default(),
    );
    let key = format!("{}:{}", hex::encode(provider_id), tool_class);
    let commitment = keccak256(summary_borsh);
    if json_log {
        log_json(
            "wallet_reputation_snapshot",
            serde_json::json!({
                "block": block_number,
                "txIndex": tx_index,
                "txHash": tx_hash,
                "key": key,
                "scoreMilli": score.to_string(),
            }),
        );
    } else {
        eprintln!(
            "fractal-indexer-stub: reputation snapshot block={block_number} key={key} score_milli={score}"
        );
    }
    store.rows.insert(
        key,
        StoreRow {
            last_block: block_number,
            score_milli: score.to_string(),
            ledger_commitment_hex: format!("0x{}", hex::encode(commitment)),
            ledger_borsh_hex: Some(format!("0x{}", hex::encode(summary_borsh))),
            client_requesters_hex: vec![],
            kind: "snapshot".into(),
        },
    );
    save_store(path, store)
}

fn process_block_txs(
    url: &str,
    block_number: u64,
    block_ts_ms: u64,
    store_path: &Path,
    json_log: bool,
    merge_settlements: bool,
    store: &mut ReputationStore,
) -> Result<(), String> {
    let tag = format!("0x{:x}", block_number);
    let bv = rpc_value(
        url,
        "eth_getBlockByNumber",
        serde_json::json!([tag, false]),
    )?;
    let Some(txs) = bv
        .pointer("/result/transactions")
        .and_then(|t| t.as_array())
    else {
        return Ok(());
    };
    for (i, txh) in txs.iter().enumerate() {
        let Some(hash_str) = txh.as_str() else {
            continue;
        };
        let Ok(txv) = rpc_value(
            url,
            "eth_getTransactionByHash",
            serde_json::json!([hash_str]),
        ) else {
            continue;
        };
        let Some(tx) = decode_tx_from_rpc(&txv) else {
            continue;
        };
        let TxBody::Native(call) = &tx.body else {
            continue;
        };
        let slash_target = store.chain_mirror.apply_native(tx.signer, &call);
        match &call {
            NativeCall::WalletReputationSnapshotV1 {
                provider_id,
                tool_class,
                summary_borsh,
            } => {
                process_wallet_reputation_snapshot(
                    store,
                    store_path,
                    block_number,
                    i,
                    hash_str,
                    provider_id,
                    *tool_class,
                    summary_borsh.as_slice(),
                    json_log,
                )?;
            }
            NativeCall::SettleBatch(p) if merge_settlements => {
                apply_settlement_receipts(
                    store,
                    store_path,
                    block_number,
                    block_ts_ms,
                    &p.receipts,
                    json_log,
                )?;
            }
            NativeCall::SettleReceipt(ref r) if merge_settlements => {
                apply_settlement_receipts(
                    store,
                    store_path,
                    block_number,
                    block_ts_ms,
                    std::slice::from_ref(r),
                    json_log,
                )?;
            }
            _ => {}
        }
        if let Some((w, tc_u8)) = slash_target {
            apply_dispute_slash_row(
                store,
                store_path,
                w,
                tc_u8,
                block_number,
                block_ts_ms,
                json_log,
            )?;
        }
    }
    Ok(())
}

fn env_reputation_merge_settlements() -> bool {
    match std::env::var("INDEXER_REPUTATION_MERGE_SETTLEMENTS") {
        Ok(v) => {
            let v = v.trim();
            if v.is_empty() {
                return true;
            }
            !(v == "0"
                || v.eq_ignore_ascii_case("false")
                || v.eq_ignore_ascii_case("no")
                || v.eq_ignore_ascii_case("off"))
        }
        Err(_) => true,
    }
}

fn block_timestamp_ms(bv: &Value) -> u64 {
    let hex = bv
        .pointer("/result/timestamp")
        .and_then(|x| x.as_str())
        .unwrap_or("0x0");
    let sec = u64::from_str_radix(hex.trim_start_matches("0x"), 16).unwrap_or(0);
    sec.saturating_mul(1000)
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
    let catchup: u64 = std::env::var("INDEXER_CATCHUP_BLOCKS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2_048);
    let merge_settlements = env_reputation_merge_settlements();
    let store_path = std::env::var("INDEXER_REPUTATION_STORE_PATH").ok();
    let mut store = store_path
        .as_ref()
        .map(|p| load_store(Path::new(p)))
        .unwrap_or_default();
    eprintln!(
        "fractal-indexer-stub: rpc={url} poll_ms={poll_ms} json_log={json_log} reputation_store={:?} catchup_blocks={catchup} merge_settlements={merge_settlements}",
        store_path.as_deref(),
    );

    let mut last_reported_head: Option<u64> = None;
    loop {
        match rpc_str(&url, "eth_blockNumber", serde_json::json!([])) {
            Ok(hex) => {
                let h = u64::from_str_radix(hex.trim_start_matches("0x"), 16).unwrap_or(0);
                if last_reported_head != Some(h) {
                    if json_log {
                        log_json("head", serde_json::json!({ "number": h, "hex": hex }));
                    } else {
                        eprintln!("fractal-indexer-stub: head {hex} ({h})");
                    }
                    last_reported_head = Some(h);
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
                            let bh = res
                                .get("hash")
                                .and_then(|x| x.as_str())
                                .unwrap_or("");
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
                                eprintln!(
                                    "fractal-indexer-stub: block {tag} hash={bh} txs={n_tx}"
                                );
                            }
                            if let Some(ref p) = store_path {
                                let path = Path::new(p);
                                let start = if store.last_scanned_block == 0 {
                                    h.saturating_sub(catchup)
                                } else {
                                    store.last_scanned_block.saturating_add(1)
                                };
                                let end = h;
                                if start <= end {
                                    for bn in start..=end {
                                        let ttag = format!("0x{:x}", bn);
                                        if let Ok(bvv) = rpc_value(
                                            &url,
                                            "eth_getBlockByNumber",
                                            serde_json::json!([ttag, false]),
                                        ) {
                                            let ts_ms = block_timestamp_ms(&bvv);
                                            if let Err(e) = process_block_txs(
                                                &url,
                                                bn,
                                                ts_ms,
                                                path,
                                                json_log,
                                                merge_settlements,
                                                &mut store,
                                            ) {
                                                eprintln!(
                                                    "fractal-indexer-stub: process_block_txs bn={bn}: {e}"
                                                );
                                            }
                                        }
                                    }
                                    store.last_scanned_block = end;
                                    let _ = save_store(path, &store);
                                }
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
