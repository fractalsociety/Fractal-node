//! Minimal JSON-RPC server (PRD §18 M2 subset: `eth_blockNumber`, `eth_getBalance`,
//! `eth_sendRawTransaction` hex-borsh stub, `eth_call` stub).

mod module;

pub use module::{
    build_module, evm_log_matches_topic_filters, logs_bloom_256, logs_bloom_hex, make_rpc_log,
    serve_http, ChainInteraction, LogsFilter, RpcChainConfig, RpcDaMetrics, RpcLog,
    RpcProofMetrics, RpcProofRejectionMetric, RpcProofSubmission, RpcSettlementBlock, SharedChain,
    TopicMatch,
};
