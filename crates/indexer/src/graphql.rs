//! GraphQL query surface (`docs/prd.md` §14.4 subgraph-class).

use std::sync::Arc;

use async_graphql::{Context, EmptyMutation, EmptySubscription, Object, Schema, SimpleObject};
use serde_json::Value;

use crate::db::{BlockRow, IndexerDb, LifeEventRow, ReputationRow, TxRow};

pub struct AppState {
    pub db: Arc<IndexerDb>,
    pub rpc_url: String,
}

#[derive(SimpleObject)]
pub struct GqlIndexerStatus {
    pub last_indexed_block: u64,
    pub chain_rpc_url: String,
    pub tx_count: u64,
    pub wallet_event_count: u64,
    pub reputation_row_count: u64,
    pub life_event_count: u64,
}

#[derive(SimpleObject)]
pub struct GqlBlock {
    pub number: u64,
    pub hash: String,
    pub timestamp_ms: u64,
    pub tx_count: u32,
}

#[derive(SimpleObject)]
pub struct GqlReputationRow {
    pub row_key: String,
    pub last_block: u64,
    pub score_milli: String,
    pub ledger_commitment_hex: String,
    pub ledger_borsh_hex: Option<String>,
    pub client_requesters_hex: Vec<String>,
    pub kind: String,
    pub updated_at_ms: u64,
}

#[derive(SimpleObject)]
pub struct GqlTransaction {
    pub hash: String,
    pub block_number: u64,
    pub tx_index: u32,
    pub signer: String,
    pub vm_kind: String,
    pub call_kind: Option<String>,
    pub payload: Value,
}

#[derive(SimpleObject)]
pub struct GqlLifeEvent {
    pub tx_hash: String,
    pub block_number: u64,
    pub tx_index: u32,
    pub command_id: String,
    pub kind: String,
    pub soul_id_hash: String,
    pub epoch: u64,
    pub amount_micro_credits: String,
    pub payload_hash: String,
}

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn indexer_status(&self, ctx: &Context<'_>) -> Result<GqlIndexerStatus, String> {
        let st = ctx.data_unchecked::<AppState>();
        let s = st.db.status().map_err(|e| e.to_string())?;
        Ok(GqlIndexerStatus {
            last_indexed_block: s.last_indexed_block,
            chain_rpc_url: st.rpc_url.clone(),
            tx_count: s.tx_count,
            wallet_event_count: s.wallet_event_count,
            reputation_row_count: s.reputation_row_count,
            life_event_count: s.life_event_count,
        })
    }

    async fn block(&self, ctx: &Context<'_>, number: u64) -> Result<Option<GqlBlock>, String> {
        let st = ctx.data_unchecked::<AppState>();
        Ok(st
            .db
            .block(number)
            .map_err(|e| e.to_string())?
            .map(block_gql))
    }

    async fn blocks(
        &self,
        ctx: &Context<'_>,
        #[graphql(default = 20)] first: i32,
        #[graphql(default = 0)] skip: i32,
    ) -> Result<Vec<GqlBlock>, String> {
        let st = ctx.data_unchecked::<AppState>();
        let limit = first.clamp(1, 500) as i64;
        let offset = skip.max(0) as i64;
        Ok(st
            .db
            .blocks(limit, offset)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(block_gql)
            .collect())
    }

    async fn transaction(
        &self,
        ctx: &Context<'_>,
        hash: String,
    ) -> Result<Option<GqlTransaction>, String> {
        let st = ctx.data_unchecked::<AppState>();
        Ok(st
            .db
            .transaction(&hash)
            .map_err(|e| e.to_string())?
            .map(|r| tx_gql(&r)))
    }

    async fn transactions(
        &self,
        ctx: &Context<'_>,
        #[graphql(default = 20)] first: i32,
        #[graphql(default = 0)] skip: i32,
        call_kind: Option<String>,
        #[graphql(default = false)] wallet_only: bool,
    ) -> Result<Vec<GqlTransaction>, String> {
        let st = ctx.data_unchecked::<AppState>();
        let limit = first.clamp(1, 500) as i64;
        let offset = skip.max(0) as i64;
        let ck = call_kind.as_deref();
        Ok(st
            .db
            .transactions(limit, offset, ck, wallet_only)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(|r| tx_gql(&r))
            .collect())
    }

    async fn reputation_rows(
        &self,
        ctx: &Context<'_>,
        #[graphql(default = 20)] first: i32,
        #[graphql(default = 0)] skip: i32,
    ) -> Result<Vec<GqlReputationRow>, String> {
        let st = ctx.data_unchecked::<AppState>();
        let limit = first.clamp(1, 500) as i64;
        let offset = skip.max(0) as i64;
        Ok(st
            .db
            .reputation_rows(limit, offset)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(reputation_row_gql)
            .collect())
    }

    /// Alias for wallet-native txs (`call_kind` prefix `Wallet`).
    async fn wallet_events(
        &self,
        ctx: &Context<'_>,
        #[graphql(default = 20)] first: i32,
        #[graphql(default = 0)] skip: i32,
        kind: Option<String>,
    ) -> Result<Vec<GqlTransaction>, String> {
        self.transactions(ctx, first, skip, kind, true).await
    }

    async fn life_events(
        &self,
        ctx: &Context<'_>,
        #[graphql(default = 20)] first: i32,
        #[graphql(default = 0)] skip: i32,
        kind: Option<String>,
        epoch: Option<u64>,
    ) -> Result<Vec<GqlLifeEvent>, String> {
        let st = ctx.data_unchecked::<AppState>();
        let limit = first.clamp(1, 500) as i64;
        let offset = skip.max(0) as i64;
        Ok(st
            .db
            .life_events(limit, offset, kind.as_deref(), epoch)
            .map_err(|e| e.to_string())?
            .into_iter()
            .map(life_event_gql)
            .collect())
    }
}

fn block_gql(b: BlockRow) -> GqlBlock {
    GqlBlock {
        number: b.number,
        hash: b.hash,
        timestamp_ms: b.timestamp_ms,
        tx_count: b.tx_count,
    }
}

fn reputation_row_gql(r: ReputationRow) -> GqlReputationRow {
    GqlReputationRow {
        row_key: r.row_key,
        last_block: r.last_block,
        score_milli: r.score_milli,
        ledger_commitment_hex: r.ledger_commitment_hex,
        ledger_borsh_hex: r.ledger_borsh_hex,
        client_requesters_hex: r.client_requesters_hex,
        kind: r.kind,
        updated_at_ms: r.updated_at_ms,
    }
}

fn tx_gql(r: &TxRow) -> GqlTransaction {
    GqlTransaction {
        hash: r.hash.clone(),
        block_number: r.block_number,
        tx_index: r.tx_index,
        signer: r.signer.clone(),
        vm_kind: r.vm_kind.clone(),
        call_kind: r.call_kind.clone(),
        payload: IndexerDb::payload_value(r),
    }
}

fn life_event_gql(r: LifeEventRow) -> GqlLifeEvent {
    GqlLifeEvent {
        tx_hash: r.tx_hash,
        block_number: r.block_number,
        tx_index: r.tx_index,
        command_id: r.command_id,
        kind: r.kind,
        soul_id_hash: r.soul_id_hash,
        epoch: r.epoch,
        amount_micro_credits: r.amount_micro_credits,
        payload_hash: r.payload_hash,
    }
}

pub type IndexerSchema = Schema<QueryRoot, EmptyMutation, EmptySubscription>;

pub fn build_schema(state: AppState) -> IndexerSchema {
    Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
        .data(state)
        .finish()
}
