//! Minimal JSON-RPC server (PRD §18 M2 subset: `eth_blockNumber`, `eth_getBalance`,
//! `eth_sendRawTransaction` hex-borsh stub, `eth_call` stub).

mod gateway;
mod module;

pub use gateway::{
    build_gateway_module, gateway_bind_addr_from_env, parse_gateway_endpoints, serve_gateway_http,
    GatewayRoute, RpcGateway, ShardEndpoint,
};
pub use module::{
    build_module, evm_log_matches_topic_filters, logs_bloom_256, logs_bloom_hex, make_rpc_log,
    serve_http, ChainInteraction, LogsFilter, ProofCommitmentResponse, RpcChainConfig,
    RpcConsensusDiagnostics, RpcDaMetrics, RpcLog, RpcMempoolLaneMetrics, RpcProofMetrics,
    RpcProofRejectionMetric, RpcProofSubmission, RpcSettlementBlock, SharedChain, TopicMatch,
};
