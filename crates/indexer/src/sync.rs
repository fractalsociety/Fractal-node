//! Block polling and SQLite ingest.

use std::sync::Arc;

use fractal_core::{TxBody, Transaction};
use serde_json::Value;

use crate::db::{BlockRow, IndexerDb, TxRow};
use crate::native_decode::{is_wallet_native, native_call_kind, tx_payload_json, vm_kind_label};
use crate::reputation::{process_tx_for_reputation, ReputationSyncConfig};
use crate::rpc::{block_timestamp_ms, decode_tx_from_rpc, head_number, rpc_post};

#[derive(Clone)]
pub struct SyncConfig {
    pub rpc_url: String,
    pub catchup_blocks: u64,
    pub reputation: ReputationSyncConfig,
}

pub fn sync_to_head(db: &Arc<IndexerDb>, cfg: &SyncConfig) -> Result<u64, String> {
    let head = head_number(&cfg.rpc_url)?;
    let start = {
        let last = db.last_indexed_block()?;
        if last == 0 {
            head.saturating_sub(cfg.catchup_blocks)
        } else {
            last.saturating_add(1)
        }
    };
    if start > head {
        return Ok(head);
    }
    for bn in start..=head {
        sync_block(db, cfg, bn)?;
    }
    db.set_last_indexed_block(head)?;
    Ok(head)
}

pub fn sync_block(db: &IndexerDb, cfg: &SyncConfig, block_number: u64) -> Result<(), String> {
    let url = &cfg.rpc_url;
    let tag = format!("0x{:x}", block_number);
    let bv = rpc_post(
        url,
        "eth_getBlockByNumber",
        serde_json::json!([tag, false]),
    )?;
    let res = bv.get("result").cloned().unwrap_or(Value::Null);
    let hash = res
        .get("hash")
        .and_then(|x| x.as_str())
        .unwrap_or("")
        .to_string();
    let ts_ms = block_timestamp_ms(&bv);
    let txs = res
        .get("transactions")
        .and_then(|t| t.as_array())
        .cloned()
        .unwrap_or_default();
    db.insert_block(&BlockRow {
        number: block_number,
        hash,
        timestamp_ms: ts_ms,
        tx_count: txs.len() as u32,
    })?;
    for (i, txh) in txs.iter().enumerate() {
        let Some(hash_str) = txh.as_str() else {
            continue;
        };
        if let Ok(txv) = rpc_post(
            url,
            "eth_getTransactionByHash",
            serde_json::json!([hash_str]),
        ) {
            let receipt_v = rpc_post(
                url,
                "eth_getTransactionReceipt",
                serde_json::json!([hash_str]),
            )
            .ok();
            if let Some(tx) = decode_tx_from_rpc(&txv) {
                ingest_tx(
                    db,
                    block_number,
                    ts_ms,
                    i as u32,
                    hash_str,
                    &tx,
                    receipt_v.as_ref(),
                    &cfg.reputation,
                )?;
            }
        }
    }
    Ok(())
}

fn ingest_tx(
    db: &IndexerDb,
    block_number: u64,
    block_ts_ms: u64,
    tx_index: u32,
    hash: &str,
    tx: &Transaction,
    receipt_v: Option<&Value>,
    rep_cfg: &ReputationSyncConfig,
) -> Result<(), String> {
    let vm = vm_kind_label(&tx.vm).to_string();
    let (call_kind, is_wallet) = match &tx.body {
        TxBody::Native(call) => {
            let k = native_call_kind(call).to_string();
            (Some(k.clone()), is_wallet_native(&k))
        }
        _ => (None, false),
    };
    let transfer_to = match &tx.body {
        TxBody::Transfer { to, .. } => Some(format!("0x{}", hex::encode(to))),
        _ => None,
    };
    let (receipt_status, gas_used) = parse_receipt_fields(receipt_v);
    let payload = tx_payload_json(tx);
    db.insert_tx(
        &TxRow {
            hash: hash.to_string(),
            block_number,
            tx_index,
            signer: format!("0x{}", hex::encode(tx.signer)),
            vm_kind: vm,
            call_kind,
            payload_json: payload.to_string(),
            receipt_status,
            gas_used,
            transfer_to,
        },
        is_wallet,
    )?;
    process_tx_for_reputation(
        db,
        block_number,
        block_ts_ms,
        tx_index,
        hash,
        tx,
        rep_cfg,
    )?;
    Ok(())
}

fn parse_receipt_fields(receipt_v: Option<&Value>) -> (Option<u32>, Option<u64>) {
    let Some(v) = receipt_v else {
        return (None, None);
    };
    let res = v.get("result").unwrap_or(v);
    if res.is_null() {
        return (None, None);
    }
    let status = res.get("status").and_then(|s| {
        let hex = s.as_str()?;
        u32::from_str_radix(hex.trim_start_matches("0x"), 16).ok()
    });
    let gas = res.get("gasUsed").and_then(|g| {
        let hex = g.as_str()?;
        u64::from_str_radix(hex.trim_start_matches("0x"), 16).ok()
    });
    (status, gas)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fractal_core::{NativeCall, TxBody, VmKind};

    #[test]
    fn ingest_native_wallet_tx_flags_wallet() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        let db = IndexerDb::open(&path).unwrap();
        let tx = Transaction {
            signer: [1u8; 20],
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::WalletCloseBudgetAccountV1 { budget: 3 }),
        };
        ingest_tx(
            &db,
            1,
            0,
            0,
            "0xabc",
            &tx,
            None,
            &ReputationSyncConfig::default(),
        )
        .unwrap();
        let st = db.status().unwrap();
        assert_eq!(st.wallet_event_count, 1);
    }
}
