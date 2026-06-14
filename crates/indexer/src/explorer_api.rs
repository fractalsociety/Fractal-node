//! Blockscout-class REST surface over the deep SQLite index (`tools/explorer`).

use std::sync::Arc;

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use serde::Serialize;
use serde_json::Value;

use crate::db::{BlockRow, IndexerDb, SearchResult, TxRow};

#[derive(Clone)]
pub struct ExplorerApiState {
    pub db: Arc<IndexerDb>,
    pub rpc_url: String,
}

#[derive(Serialize)]
pub struct ExplorerStatusJson {
    pub last_indexed_block: u64,
    pub tx_count: u64,
    pub wallet_event_count: u64,
    pub reputation_row_count: u64,
    pub chain_rpc_url: String,
}

#[derive(Serialize)]
pub struct ExplorerBlockJson {
    pub number: u64,
    pub hash: String,
    pub timestamp_ms: u64,
    pub tx_count: u32,
    pub finality_status: String,
}

#[derive(Serialize)]
pub struct ExplorerTxJson {
    pub hash: String,
    pub block_number: u64,
    pub tx_index: u32,
    pub signer: String,
    pub vm_kind: String,
    pub call_kind: Option<String>,
    pub payload: Value,
    pub receipt_status: Option<u32>,
    pub gas_used: Option<u64>,
    pub transfer_to: Option<String>,
}

#[derive(Serialize)]
#[serde(tag = "kind")]
pub enum ExplorerSearchHit {
    #[serde(rename = "block")]
    Block { block: ExplorerBlockJson },
    #[serde(rename = "transaction")]
    Transaction { transaction: ExplorerTxJson },
    #[serde(rename = "address")]
    Address {
        address: String,
        transactions: Vec<ExplorerTxJson>,
    },
}

#[derive(Debug, serde::Deserialize)]
pub struct ListQuery {
    pub first: Option<i32>,
    pub skip: Option<i32>,
}

fn limit_skip(q: &ListQuery) -> (i64, i64) {
    let limit = q.first.unwrap_or(25).clamp(1, 500) as i64;
    let offset = q.skip.unwrap_or(0).max(0) as i64;
    (limit, offset)
}

fn block_json(b: BlockRow) -> ExplorerBlockJson {
    ExplorerBlockJson {
        number: b.number,
        hash: b.hash,
        timestamp_ms: b.timestamp_ms,
        tx_count: b.tx_count,
        finality_status: b.finality_status,
    }
}

fn tx_json(r: &TxRow) -> ExplorerTxJson {
    ExplorerTxJson {
        hash: r.hash.clone(),
        block_number: r.block_number,
        tx_index: r.tx_index,
        signer: r.signer.clone(),
        vm_kind: r.vm_kind.clone(),
        call_kind: r.call_kind.clone(),
        payload: IndexerDb::payload_value(r),
        receipt_status: r.receipt_status,
        gas_used: r.gas_used,
        transfer_to: r.transfer_to.clone(),
    }
}

pub async fn explorer_status(
    State(st): State<ExplorerApiState>,
) -> Result<Json<ExplorerStatusJson>, StatusCode> {
    let s = st
        .db
        .status()
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(ExplorerStatusJson {
        last_indexed_block: s.last_indexed_block,
        tx_count: s.tx_count,
        wallet_event_count: s.wallet_event_count,
        reputation_row_count: s.reputation_row_count,
        chain_rpc_url: st.rpc_url.clone(),
    }))
}

pub async fn explorer_blocks(
    State(st): State<ExplorerApiState>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<ExplorerBlockJson>>, StatusCode> {
    let (limit, offset) = limit_skip(&q);
    let rows = st
        .db
        .blocks(limit, offset)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.into_iter().map(block_json).collect()))
}

pub async fn explorer_block_by_number(
    State(st): State<ExplorerApiState>,
    Path(number): Path<u64>,
) -> Result<Json<ExplorerBlockJson>, StatusCode> {
    let b = st
        .db
        .block(number)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(block_json(b)))
}

pub async fn explorer_block_by_hash(
    State(st): State<ExplorerApiState>,
    Path(hash): Path<String>,
) -> Result<Json<ExplorerBlockJson>, StatusCode> {
    let b = st
        .db
        .block_by_hash(&hash)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(block_json(b)))
}

pub async fn explorer_block_transactions(
    State(st): State<ExplorerApiState>,
    Path(number): Path<u64>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<ExplorerTxJson>>, StatusCode> {
    let (limit, offset) = limit_skip(&q);
    let rows = st
        .db
        .transactions_for_block(number, limit, offset)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.iter().map(tx_json).collect()))
}

pub async fn explorer_transaction(
    State(st): State<ExplorerApiState>,
    Path(hash): Path<String>,
) -> Result<Json<ExplorerTxJson>, StatusCode> {
    let tx = st
        .db
        .transaction(&hash)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(tx_json(&tx)))
}

pub async fn explorer_address_transactions(
    State(st): State<ExplorerApiState>,
    Path(address): Path<String>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<ExplorerTxJson>>, StatusCode> {
    let (limit, offset) = limit_skip(&q);
    let rows = st
        .db
        .transactions_for_address(&address, limit, offset)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(rows.iter().map(tx_json).collect()))
}

#[derive(Debug, serde::Deserialize)]
pub struct SearchQuery {
    pub q: String,
}

pub async fn explorer_search(
    State(st): State<ExplorerApiState>,
    Query(q): Query<SearchQuery>,
) -> Result<Json<ExplorerSearchHit>, StatusCode> {
    let raw = q.q.trim();
    if raw.is_empty() {
        return Err(StatusCode::BAD_REQUEST);
    }
    let hit = st
        .db
        .search(raw)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;
    Ok(Json(search_hit_json(hit)))
}

fn search_hit_json(hit: SearchResult) -> ExplorerSearchHit {
    match hit {
        SearchResult::Block(b) => ExplorerSearchHit::Block {
            block: block_json(b),
        },
        SearchResult::Transaction(tx) => ExplorerSearchHit::Transaction {
            transaction: tx_json(&tx),
        },
        SearchResult::Address {
            address,
            transactions,
        } => ExplorerSearchHit::Address {
            address,
            transactions: transactions.iter().map(tx_json).collect(),
        },
    }
}

pub fn explorer_router(state: ExplorerApiState) -> axum::Router {
    use axum::routing::get;
    axum::Router::new()
        .route("/status", get(explorer_status))
        .route("/blocks", get(explorer_blocks))
        .route("/blocks/{number}", get(explorer_block_by_number))
        .route("/blocks/hash/{hash}", get(explorer_block_by_hash))
        .route(
            "/blocks/{number}/transactions",
            get(explorer_block_transactions),
        )
        .route("/transactions/{hash}", get(explorer_transaction))
        .route(
            "/address/{address}/transactions",
            get(explorer_address_transactions),
        )
        .route("/search", get(explorer_search))
        .with_state(state)
}
