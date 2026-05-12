//! Minimal JSON-RPC server (PRD §18 M2 subset: `eth_blockNumber`, `eth_getBalance`,
//! `eth_sendRawTransaction` hex-borsh stub, `eth_call` stub).

mod module;

pub use module::{build_module, serve_http, ChainInteraction, SharedChain};
