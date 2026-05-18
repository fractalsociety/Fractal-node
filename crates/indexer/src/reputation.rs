//! On-chain reputation rows in SQLite (parity with `fractal-indexer-stub` JSON store).

use borsh::BorshDeserialize;
use fractal_core::{NativeCall, OnChainTaskReceipt, Transaction, TxBody};
use fractal_crypto::hash::keccak256;
use fractal_wallet::{
    compute_reputation_score_milli, ReputationLedgerSummary, ReputationParams, ToolClass,
};

use crate::db::{IndexerDb, ReputationRow};
use crate::ledger_merge::{
    apply_dispute_slash_to_summary, apply_onchain_receipt_to_summary,
    apply_wallet_task_finalize_to_summary, provider_and_key_from_receipt,
    row_key_for_wallet_provider, row_key_for_worker_agent, tool_class_from_receipt,
    SettlementLedgerSide,
};
use crate::wallet_task_mirror::{IndexerChainMirrorV2, WalletChainMirror};

/// Same meta key regardless of DB path; serialized JSON of [`IndexerChainMirror`].
pub const META_REPUTATION_CHAIN_MIRROR: &str = "reputation_chain_mirror_json";

#[derive(Clone, Debug)]
pub struct ReputationSyncConfig {
    pub merge_settlements: bool,
    /// Merge §14.5 `WalletFinalizeTaskV1` into tool-class keyed provider rows (default on).
    pub merge_wallet_tasks: bool,
    pub json_log: bool,
}

impl Default for ReputationSyncConfig {
    /// §10.4 settlement merge on unless `INDEXER_REPUTATION_MERGE_SETTLEMENTS=0`.
    fn default() -> Self {
        Self {
            merge_settlements: true,
            merge_wallet_tasks: true,
            json_log: false,
        }
    }
}

fn log_json(evt: &str, extra: serde_json::Value) {
    let mut m = serde_json::Map::new();
    m.insert("evt".to_string(), serde_json::Value::String(evt.to_string()));
    if let serde_json::Value::Object(o) = extra {
        for (k, v) in o {
            m.insert(k, v);
        }
    }
    eprintln!("{}", serde_json::Value::Object(m));
}

fn load_mirror(db: &IndexerDb) -> Result<IndexerChainMirrorV2, String> {
    let Some(raw) = db.get_meta(META_REPUTATION_CHAIN_MIRROR)? else {
        return Ok(IndexerChainMirrorV2::default());
    };
    if let Ok(v2) = serde_json::from_str::<IndexerChainMirrorV2>(&raw) {
        return Ok(v2);
    }
    let legacy: crate::indexer_mirror::IndexerChainMirror =
        serde_json::from_str(&raw).map_err(|e| format!("reputation chain mirror json: {e}"))?;
    Ok(IndexerChainMirrorV2 {
        legacy,
        wallet: WalletChainMirror::default(),
    })
}

fn save_mirror(db: &IndexerDb, m: &IndexerChainMirrorV2) -> Result<(), String> {
    let s = serde_json::to_string(m).map_err(|e| e.to_string())?;
    db.set_meta(META_REPUTATION_CHAIN_MIRROR, &s)
}

fn load_summary_and_side(
    row: &ReputationRow,
) -> Option<(ReputationLedgerSummary, SettlementLedgerSide)> {
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

fn persist_row(
    db: &IndexerDb,
    key: &str,
    block_number: u64,
    now_ms: u64,
    summary: &ReputationLedgerSummary,
    side: &SettlementLedgerSide,
    kind: &str,
) -> Result<(), String> {
    let summary_borsh = borsh::to_vec(summary).map_err(|e| e.to_string())?;
    let score = compute_reputation_score_milli(summary, &ReputationParams::default());
    let commitment = keccak256(&summary_borsh);
    let mut clients: Vec<String> = side.client_requesters.iter().cloned().collect();
    clients.sort();
    db.upsert_reputation_row(&ReputationRow {
        row_key: key.to_string(),
        last_block: block_number,
        score_milli: score.to_string(),
        ledger_commitment_hex: format!("0x{}", hex::encode(commitment)),
        ledger_borsh_hex: Some(format!("0x{}", hex::encode(&summary_borsh))),
        client_requesters_hex: clients,
        kind: kind.to_string(),
        updated_at_ms: now_ms,
    })
}

fn apply_settlement_receipts(
    db: &IndexerDb,
    mirror: &IndexerChainMirrorV2,
    block_number: u64,
    now_ms: u64,
    receipts: &[OnChainTaskReceipt],
    cfg: &ReputationSyncConfig,
) -> Result<(), String> {
    for r in receipts {
        let (_pid, key) = provider_and_key_from_receipt(r);
        let tc = tool_class_from_receipt(r);
        let stake = mirror.legacy.available_stake_wei_for_worker(r.worker);
        let mut summary;
        let mut side;
        if let Some(existing) = db.reputation_row(&key)? {
            if let Some((s, sd)) = load_summary_and_side(&existing) {
                summary = s;
                side = sd;
            } else {
                eprintln!(
                    "fractal-indexer: skip settlement merge key={key} (missing ledger_borsh_hex on existing row)"
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
        let score = compute_reputation_score_milli(&summary, &ReputationParams::default());
        if cfg.json_log {
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
                "fractal-indexer: settlement merge block={block_number} key={key} score_milli={score} worker={} tool_class={}",
                r.worker,
                r.tool_class
            );
        }
        persist_row(db, &key, block_number, now_ms, &summary, &side, "settlement")?;
    }
    Ok(())
}

fn apply_wallet_task_finalize_row(
    db: &IndexerDb,
    mirror: &mut IndexerChainMirrorV2,
    block_number: u64,
    now_ms: u64,
    cfg: &ReputationSyncConfig,
) -> Result<(), String> {
    let Some(event) = mirror.wallet.take_pending_finalize() else {
        return Ok(());
    };
    let key = row_key_for_wallet_provider(&event.provider_id, event.tool_class);
    let stake = mirror
        .wallet
        .available_stake_wei(&event.provider_id, event.tool_class);
    let tc = fractal_wallet::ToolClass::from_discriminant(event.tool_class)
        .unwrap_or(fractal_wallet::ToolClass::Browser);
    let mut summary;
    let mut side;
    if let Some(existing) = db.reputation_row(&key)? {
        if let Some((s, sd)) = load_summary_and_side(&existing) {
            summary = s;
            side = sd;
        } else {
            eprintln!(
                "fractal-indexer: skip wallet task finalize key={key} (missing ledger_borsh_hex)"
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
    apply_wallet_task_finalize_to_summary(&mut summary, &event, now_ms, &mut side, stake);
    let score = compute_reputation_score_milli(&summary, &ReputationParams::default());
    if cfg.json_log {
        log_json(
            "reputation_wallet_task_finalize",
            serde_json::json!({
                "block": block_number,
                "key": key,
                "taskId": event.task_id,
                "scoreMilli": score.to_string(),
                "toolClass": event.tool_class,
                "escrowWei": event.escrow_wei.to_string(),
            }),
        );
    } else {
        eprintln!(
            "fractal-indexer: wallet task finalize block={block_number} key={key} task_id={} score_milli={score}",
            event.task_id
        );
    }
    persist_row(
        db,
        &key,
        block_number,
        now_ms,
        &summary,
        &side,
        "wallet_task",
    )?;
    Ok(())
}

fn apply_dispute_slash_row(
    db: &IndexerDb,
    mirror: &IndexerChainMirrorV2,
    worker: u64,
    tool_class_u8: u8,
    block_number: u64,
    now_ms: u64,
    cfg: &ReputationSyncConfig,
) -> Result<(), String> {
    let key = row_key_for_worker_agent(worker, tool_class_u8);
    let tc = ToolClass::from_discriminant(tool_class_u8).unwrap_or(ToolClass::Browser);
    let stake = mirror.legacy.available_stake_wei_for_worker(worker);
    let mut summary;
    let mut side;
    if let Some(existing) = db.reputation_row(&key)? {
        if let Some((s, sd)) = load_summary_and_side(&existing) {
            summary = s;
            side = sd;
        } else {
            eprintln!(
                "fractal-indexer: skip dispute slash key={key} (missing ledger_borsh_hex on existing row)"
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
    let score = compute_reputation_score_milli(&summary, &ReputationParams::default());
    if cfg.json_log {
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
            "fractal-indexer: dispute slash block={block_number} key={key} score_milli={score} worker={worker}"
        );
    }
    persist_row(
        db,
        &key,
        block_number,
        now_ms,
        &summary,
        &side,
        "dispute_slash",
    )?;
    Ok(())
}

fn process_wallet_reputation_snapshot(
    db: &IndexerDb,
    block_number: u64,
    tx_index: u32,
    tx_hash: &str,
    provider_id: &[u8; 32],
    tool_class: u8,
    summary_borsh: &[u8],
    cfg: &ReputationSyncConfig,
) -> Result<(), String> {
    let summary = ReputationLedgerSummary::try_from_slice(summary_borsh)
        .map_err(|_| "invalid ReputationLedgerSummary borsh".to_string())?;
    let score = compute_reputation_score_milli(&summary, &ReputationParams::default());
    let key = format!("{}:{}", hex::encode(provider_id), tool_class);
    let commitment = keccak256(summary_borsh);
    if cfg.json_log {
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
            "fractal-indexer: reputation snapshot block={block_number} key={key} score_milli={score}"
        );
    }
    db.upsert_reputation_row(&ReputationRow {
        row_key: key,
        last_block: block_number,
        score_milli: score.to_string(),
        ledger_commitment_hex: format!("0x{}", hex::encode(commitment)),
        ledger_borsh_hex: Some(format!("0x{}", hex::encode(summary_borsh))),
        client_requesters_hex: vec![],
        kind: "snapshot".into(),
        updated_at_ms: summary.now_ms,
    })
}

/// Advance chain mirror + optional ledger merge for one native transaction (`docs/wallet.md` §10.4).
pub fn process_tx_for_reputation(
    db: &IndexerDb,
    block_number: u64,
    now_ms: u64,
    tx_index: u32,
    tx_hash: &str,
    tx: &Transaction,
    cfg: &ReputationSyncConfig,
) -> Result<(), String> {
    let TxBody::Native(call) = &tx.body else {
        return Ok(());
    };

    let mut mirror = load_mirror(db)?;
    let slash_target = mirror.apply_native(tx.signer, call, now_ms);
    if cfg.merge_wallet_tasks {
        apply_wallet_task_finalize_row(db, &mut mirror, block_number, now_ms, cfg)?;
    }
    save_mirror(db, &mirror)?;

    match call {
        NativeCall::WalletReputationSnapshotV1 {
            provider_id,
            tool_class,
            summary_borsh,
        } => {
            process_wallet_reputation_snapshot(
                db,
                block_number,
                tx_index,
                tx_hash,
                provider_id,
                *tool_class,
                summary_borsh.as_slice(),
                cfg,
            )?;
        }
        NativeCall::SettleBatch(p) if cfg.merge_settlements => {
            apply_settlement_receipts(db, &mirror, block_number, now_ms, &p.receipts, cfg)?;
        }
        NativeCall::SettleReceipt(r) if cfg.merge_settlements => {
            apply_settlement_receipts(
                db,
                &mirror,
                block_number,
                now_ms,
                std::slice::from_ref(r),
                cfg,
            )?;
        }
        _ => {}
    }

    if let Some((w, tc_u8)) = slash_target {
        apply_dispute_slash_row(db, &mirror, w, tc_u8, block_number, now_ms, cfg)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::IndexerDb;
    use fractal_core::{NativeCall, VmKind};
    use tempfile::tempdir;

    #[test]
    fn reputation_sync_config_default_merges_settlements() {
        let d = ReputationSyncConfig::default();
        assert!(d.merge_settlements);
        assert!(d.merge_wallet_tasks);
        assert!(!d.json_log);
    }

    #[test]
    fn wallet_task_finalize_merge_persists_row() {
        use fractal_core::ProviderRegistration;

        let dir = tempdir().unwrap();
        let db = IndexerDb::open(&dir.path().join("wt.db")).unwrap();
        let mut mirror = IndexerChainMirrorV2::default();
        let owner = [0x01u8; 20];
        let provider = [0x02u8; 20];
        let mut pid = [0u8; 32];
        pid[0] = 0xbb;
        let root = [0x44u8; 32];
        mirror.wallet.record_provider_registration(&ProviderRegistration {
            provider_id: pid,
            owner: provider,
            public_key: [5u8; 32],
            encryption_pubkey: [6u8; 32],
            metadata_uri: String::new(),
            endpoint_uri: String::new(),
            tool_classes: vec![2],
            tee_attestation_hash: None,
            registration_bond: 0,
        });
        mirror.wallet.apply_wallet_native(
            provider,
            &NativeCall::WalletBatchSettleV1(fractal_core::WalletToolBatchSettlePayload {
                batch_id: [8u8; 32],
                provider_id: pid,
                provider_public_key: [5u8; 32],
                tool_class: 2,
                receipt_root: root,
                total_cost: 1,
                payout_to: provider,
                receipts_borsh: vec![vec![1]],
                submitted_at: 0,
                provider_batch_sig: [0u8; 64],
            }),
            1000,
        );
        mirror.wallet.apply_wallet_native(
            owner,
            &NativeCall::WalletPostTaskV1 {
                metadata_uri: "m".into(),
                bounty_budget: 30,
                tool_budget: 10,
                verifier_budget: 5,
            },
            1000,
        );
        mirror.wallet.apply_wallet_native(
            provider,
            &NativeCall::WalletCheckoutTaskV1 {
                task_id: 1,
                agent_session: [0u8; 32],
                expiry_ms: 9_999,
            },
            1000,
        );
        mirror.wallet.apply_wallet_native(
            provider,
            &NativeCall::WalletSubmitTaskV1 {
                task_id: 1,
                artifact_pointer: "a".into(),
                tool_receipt_root: root,
            },
            1000,
        );
        mirror.wallet.apply_wallet_native(
            owner,
            &NativeCall::WalletVerifyTaskV1 {
                task_id: 1,
                verifier_sig: [0u8; 64],
                score: 80,
            },
            2000,
        );
        mirror.wallet.apply_wallet_native(
            provider,
            &NativeCall::WalletFinalizeTaskV1 { task_id: 1 },
            3000,
        );
        save_mirror(&db, &mirror).unwrap();
        let cfg = ReputationSyncConfig::default();
        apply_wallet_task_finalize_row(&db, &mut mirror, 7, 3000, &cfg).unwrap();
        let key = row_key_for_wallet_provider(&pid, 2);
        let row = db.reputation_row(&key).unwrap().unwrap();
        assert_eq!(row.kind, "wallet_task");
        assert_eq!(row.last_block, 7);
    }

    #[test]
    fn settlement_merge_persists_row() {
        let dir = tempdir().unwrap();
        let db = IndexerDb::open(&dir.path().join("t.db")).unwrap();
        let mut rid = [0u8; 32];
        rid[0] = 9;
        let receipt = OnChainTaskReceipt {
            receipt_id: rid,
            job_id: rid,
            requester: [7u8; 20],
            worker: 1,
            verifier: 0,
            artifact_root: [0u8; 32],
            output_hash: [0u8; 32],
            score: 1,
            payout_amount: 100,
            verifier_fee: 0,
            protocol_fee: 0,
            final_status: crate::ledger_merge::ONCHAIN_RECEIPT_SUCCESS_STATUS,
            finalized_at: 11_000,
            schema_version: 2,
            tool_class: 0,
        };
        let key = provider_and_key_from_receipt(&receipt).1;
        let tx = Transaction {
            signer: [0xcdu8; 20],
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::SettleReceipt(receipt)),
        };
        let cfg = ReputationSyncConfig::default();
        process_tx_for_reputation(&db, 3, 30_000, 0, "0xab", &tx, &cfg).unwrap();
        let row = db.reputation_row(&key).unwrap().unwrap();
        assert_eq!(row.kind, "settlement");
        assert!(row.ledger_borsh_hex.is_some());
        assert_eq!(row.last_block, 3);
    }
}
