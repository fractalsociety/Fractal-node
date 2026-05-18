//! Minimal JSON-RPC server (PRD §18 M2 subset: `eth_blockNumber`, `eth_getBalance`,
//! `eth_sendRawTransaction` (borsh or EIP-1559; returns canonical `keccak256(raw)` tx hash),
//! `eth_call`, and related methods).

mod gateway;
mod masterchain_module;
mod module;
mod rpc_call_stats;

pub use gateway::{
    GatewayRoute, RpcGateway, ShardEndpoint, build_gateway_module, gateway_bind_addr_from_env,
    parse_gateway_endpoints, serve_gateway_http,
};
pub use masterchain_module::{
    InvalidProofSlashEventJson, MasterchainRpc, ProverIdentityJson, build_masterchain_module,
    serve_masterchain_http,
};
pub use module::{
    ChainInteraction, LogsFilter, RpcLog, SharedChain, TopicMatch, build_module,
    evm_log_matches_topic_filters, logs_bloom_256, logs_bloom_hex, make_rpc_log, serve_http,
};
pub use rpc_call_stats::RpcCallStats;
