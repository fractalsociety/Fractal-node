//! Snapshot-only indexer path: governance `WalletReputationSnapshotV1` rows (no RPC).

use std::path::PathBuf;

use borsh::BorshDeserialize;
use fractal_wallet::{ReputationLedgerSummary, SettlementEvent, ToolClass};

#[test]
fn reputation_store_roundtrip_snapshot_kind() {
    let dir = std::env::temp_dir().join(format!(
        "fractal_indexer_rep_{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("reputation.json");

    let summary = ReputationLedgerSummary {
        tool_class: ToolClass::Browser,
        successful: vec![SettlementEvent {
            settled_at_ms: 9,
            weight: 1,
        }],
        failed_settlements: 0,
        slashing_events: 0,
        first_seen_ms: 1,
        now_ms: 10,
        available_stake: 5,
        distinct_client_count: 1,
    };
    let summary_borsh = borsh::to_vec(&summary).unwrap();
    let provider_id = [0x11u8; 32];

    // Mirror main.rs `process_wallet_reputation_snapshot` shape via a minimal inline store.
    let score = fractal_wallet::compute_reputation_score_milli(
        &summary,
        &fractal_wallet::ReputationParams::default(),
    );
    let row = serde_json::json!({
        "last_block": 7,
        "score_milli": score.to_string(),
        "ledger_commitment_hex": "0x00",
        "ledger_borsh_hex": format!("0x{}", hex::encode(&summary_borsh)),
        "client_requesters_hex": [],
        "kind": "snapshot",
    });
    let store = serde_json::json!({
        "last_scanned_block": 7,
        "rows": {
            format!("{}:0", hex::encode(provider_id)): row,
        },
        "chainMirror": serde_json::json!({}),
    });
    std::fs::write(&path, serde_json::to_string_pretty(&store).unwrap()).unwrap();

    let loaded: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let key = format!("{}:0", hex::encode(provider_id));
    assert_eq!(
        loaded["rows"][&key]["kind"].as_str(),
        Some("snapshot")
    );
    let hx = loaded["rows"][&key]["ledger_borsh_hex"]
        .as_str()
        .unwrap()
        .trim_start_matches("0x");
    let decoded = ReputationLedgerSummary::try_from_slice(&hex::decode(hx).unwrap()).unwrap();
    assert_eq!(decoded.successful.len(), 1);
    let _ = PathBuf::from(path); // silence unused in case of platform quirks
}
