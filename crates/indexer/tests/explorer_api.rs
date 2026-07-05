//! Deep indexer REST API smoke (no live RPC).

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use fractal_indexer::db::{BlockRow, IndexerDb, LifeEventRow, TxRow};
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
        finality_status: "soft".into(),
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
    db.insert_life_event(&LifeEventRow {
        tx_hash: "0xlife12".into(),
        block_number: 12,
        tx_index: 1,
        command_id: "0xcmd".into(),
        kind: "sii_commit".into(),
        soul_id_hash: "0xsoul".into(),
        epoch: 3,
        amount_micro_credits: "0".into(),
        payload_hash: "0xpayload".into(),
    })
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
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let blocks: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(blocks[0]["finality_status"], "soft");

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
        .clone()
        .oneshot(
            Request::builder()
                .uri("/life/events?kind=sii_commit&epoch=3")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
        .await
        .unwrap();
    let life_events: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(life_events[0]["kind"], "sii_commit");

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
