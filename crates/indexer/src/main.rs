//! Native event indexer + GraphQL (`docs/prd.md` §14.4, `tools/indexer`).
//!
//! ```text
//! INDEXER_RPC_URL=http://127.0.0.1:8545 \
//! INDEXER_DB_PATH=./target/fractal_indexer.db \
//! INDEXER_GRAPHQL_BIND=0.0.0.0:8088 \
//! INDEXER_JSON_LOG=0 \
//! INDEXER_REPUTATION_MERGE_SETTLEMENTS=0 \  # optional: disable Settle* → reputation merge (enabled by default)
//! INDEXER_REPUTATION_MERGE_WALLET_TASKS=0 \  # optional: disable WalletFinalizeTask → reputation mirror (enabled by default)
//! cargo run -p fractal-indexer
//! ```

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_graphql_axum::{GraphQLRequest, GraphQLResponse};
use axum::{
    routing::{get, post},
    Router,
};
use fractal_indexer::explorer_api::{explorer_router, ExplorerApiState};
use fractal_indexer::graphql::{build_schema, AppState};
use fractal_indexer::reputation::ReputationSyncConfig;
use fractal_indexer::sync::{sync_to_head, SyncConfig};
use fractal_indexer::db::IndexerDb;
use tower_http::cors::CorsLayer;

/// Unset or empty → **true** (merge `SettleBatch` / `SettleReceipt` into `reputation_rows`). Use `0`, `false`, `no`, or `off` to disable.
fn env_reputation_merge_wallet_tasks() -> bool {
    match std::env::var("INDEXER_REPUTATION_MERGE_WALLET_TASKS") {
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

async fn graphql_handler(
    schema: axum::extract::State<fractal_indexer::graphql::IndexerSchema>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    schema.execute(req.into_inner()).await.into()
}

const GRAPHIQL_HTML: &str = include_str!("../../../tools/indexer/graphiql.html");

async fn health() -> &'static str {
    "ok"
}

async fn graphiql() -> impl axum::response::IntoResponse {
    axum::response::Html(GRAPHIQL_HTML)
}

#[tokio::main]
async fn main() {
    let rpc_url =
        std::env::var("INDEXER_RPC_URL").unwrap_or_else(|_| "http://127.0.0.1:8545".into());
    let db_path = std::env::var("INDEXER_DB_PATH")
        .unwrap_or_else(|_| "./target/fractal_indexer.db".into());
    let bind = std::env::var("INDEXER_GRAPHQL_BIND").unwrap_or_else(|_| "0.0.0.0:8088".into());
    let poll_ms: u64 = std::env::var("INDEXER_POLL_MS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5_000);
    let catchup: u64 = std::env::var("INDEXER_CATCHUP_BLOCKS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2_048);

    let json_log = std::env::var("INDEXER_JSON_LOG")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let merge_settlements = env_reputation_merge_settlements();
    let merge_wallet_tasks = env_reputation_merge_wallet_tasks();

    let db = Arc::new(
        IndexerDb::open(PathBuf::from(&db_path).as_path())
            .expect("open INDEXER_DB_PATH"),
    );
    db.set_meta("chain_rpc_url", &rpc_url)
        .expect("meta chain_rpc_url");

    let cfg = SyncConfig {
        rpc_url: rpc_url.clone(),
        catchup_blocks: catchup,
        reputation: ReputationSyncConfig {
            merge_settlements,
            merge_wallet_tasks,
            json_log,
        },
    };

    let db_sync = db.clone();
    let cfg_sync = cfg.clone();
    std::thread::spawn(move || {
        loop {
            match sync_to_head(&db_sync, &cfg_sync) {
                Ok(h) => eprintln!("fractal-indexer: synced to block {h}"),
                Err(e) => eprintln!("fractal-indexer: sync error: {e}"),
            }
            std::thread::sleep(Duration::from_millis(poll_ms));
        }
    });

    let schema = build_schema(AppState {
        db: db.clone(),
        rpc_url: rpc_url.clone(),
    });

    let explorer = explorer_router(ExplorerApiState {
        db: db.clone(),
        rpc_url: rpc_url.clone(),
    });

    let graphql = Router::new()
        .route("/graphql", post(graphql_handler).get(graphql_handler))
        .route("/graphiql", get(graphiql))
        .route("/", get(graphiql))
        .with_state(schema);

    let app = Router::new()
        .route("/health", get(health))
        .nest("/api/v1/explorer", explorer)
        .merge(graphql)
        .layer(CorsLayer::permissive());

    let addr: SocketAddr = bind.parse().expect("INDEXER_GRAPHQL_BIND");
    eprintln!(
        "fractal-indexer: GraphQL http://{addr}/graphql  Explorer API http://{addr}/api/v1/explorer/status  GraphiQL http://{addr}/graphiql  db={db_path} rpc={rpc_url} reputation_merge_settlements={merge_settlements} reputation_merge_wallet_tasks={merge_wallet_tasks} json_log={json_log}"
    );
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("bind INDEXER_GRAPHQL_BIND");
    axum::serve(listener, app).await.expect("serve graphql");
}
