//! Deep indexer REST API smoke (no live RPC).

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use fractal_indexer::db::{BlockRow, IndexerDb, TxRow};
use fractal_indexer::explorer_api::{explorer_router, ExplorerApiState};
use tower::ServiceExt;

#[tokio::test]
async fn explorer_api_blocks_and_search() {
    let dir = tempfile::tempdir().unwrap();
    let db = Arc::new(IndexerDb::open(&dir.path().join("e.db")).unwrap());
    db.insert_block(&BlockRow {
        number: 12,
        hash: "0xblock12".into(),
        timestamp_ms: 1_700_000_000_000,
        tx_count: 1,
    })
    .unwrap();
    db.insert_tx(
        &TxRow {
            hash: "0xtx12".into(),
            block_number: 12,
            tx_index: 0,
            signer: "0x1111111111111111111111111111111111111111".into(),
            vm_kind: "Native".into(),
            call_kind: Some("NoOp".into()),
            payload_json: r#"{"type":"NoOp"}"#.into(),
            receipt_status: Some(1),
            gas_used: Some(99),
            transfer_to: None,
        },
        false,
    )
    .unwrap();

    let app = explorer_router(ExplorerApiState {
        db,
        rpc_url: "http://127.0.0.1:8545".into(),
    });

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/blocks?first=5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/search?q=12")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    let resp = app
        .oneshot(
            Request::builder()
                .uri("/transactions/0xtx12")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
}
