//! Fractal native event indexer library (`docs/prd.md` §14.4).

pub mod db;
pub mod explorer_api;
pub mod graphql;
mod indexer_mirror;
mod ledger_merge;
pub mod native_decode;
pub mod reputation;
pub mod rpc;
pub mod sync;
pub mod wallet_task_mirror;
