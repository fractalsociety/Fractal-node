//! GraphQL schema smoke test (no live RPC).

use std::sync::Arc;

use async_graphql::Request;
use fractal_indexer::db::{BlockRow, IndexerDb, TxRow};
use fractal_indexer::graphql::{build_schema, AppState};

#[tokio::test]
async fn indexer_status_query() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(IndexerDb::open(&dir.path().join("g.db")).unwrap());
    db.set_last_indexed_block(42).unwrap();
    db.insert_block(&BlockRow {
        number: 42,
        hash: "0xb".into(),
        timestamp_ms: 0,
        tx_count: 0,
        finality_status: "unknown".into(),
    })
    .unwrap();
    db.insert_tx(
        &TxRow {
            hash: "0xt".into(),
            block_number: 42,
            tx_index: 0,
            signer: "0xs".into(),
            vm_kind: "Native".into(),
            call_kind: Some("WalletMintCapabilityV1".into()),
            payload_json: r#"{"type":"WalletMintCapabilityV1"}"#.into(),
            receipt_status: None,
            gas_used: None,
            transfer_to: None,
        },
        true,
    )
    .unwrap();

    let schema = build_schema(AppState {
        db,
        rpc_url: "http://127.0.0.1:8545".into(),
    });
    let resp = schema
        .execute(
            Request::new(
                r#"{ indexerStatus { lastIndexedBlock txCount walletEventCount reputationRowCount chainRpcUrl } }"#,
            ),
        )
        .await;
    assert!(resp.errors.is_empty(), "{:?}", resp.errors);
    let data = resp.data.into_json().unwrap();
    assert_eq!(data["indexerStatus"]["lastIndexedBlock"], 42);
    assert_eq!(data["indexerStatus"]["txCount"], 1);
    assert_eq!(data["indexerStatus"]["walletEventCount"], 1);
    assert_eq!(data["indexerStatus"]["reputationRowCount"], 0);
}
