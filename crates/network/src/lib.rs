//! libp2p + QUIC sync layer for PRD §18 M2 (`/fractalchain/sync/1.0.0` request-response).

mod codec;

pub use codec::BorshSyncCodec;

use borsh::{BorshDeserialize, BorshSerialize};
use libp2p::request_response::{self, ProtocolSupport};
use libp2p::swarm::StreamProtocol;
use std::time::Duration;

/// Wire requests (borsh + length prefix on substreams).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum SyncRequest {
    GetTip,
    /// Inclusive lower bound on `BlockHeader::height` (canonical chain order in producer `blocks` vec).
    GetBlocks {
        from_height: u64,
        max_blocks: u32,
    },
}

/// `Blocks` carries `borsh::to_vec` of `Vec<fractal_consensus::Block>` (keeps this crate free of consensus types).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum SyncResponse {
    Tip {
        height: u64,
        head_hash: [u8; 32],
    },
    Blocks(Vec<u8>),
    ErrMsg(String),
}

pub fn sync_stream_protocol() -> StreamProtocol {
    StreamProtocol::new("/fractalchain/sync/1.0.0")
}

pub fn sync_protocols() -> Vec<(StreamProtocol, ProtocolSupport)> {
    vec![(sync_stream_protocol(), ProtocolSupport::Full)]
}

pub fn sync_request_response_config() -> request_response::Config {
    request_response::Config::default()
        .with_request_timeout(Duration::from_secs(120))
        .with_max_concurrent_streams(32)
}
