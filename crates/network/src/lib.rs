//! libp2p + QUIC sync layer for PRD §18 M2 (`/fractalchain/sync/1.0.0` request-response).

mod codec;

pub use codec::BorshSyncCodec;

use borsh::{BorshDeserialize, BorshSerialize};
use libp2p::request_response::{self, ProtocolSupport};
use libp2p::swarm::StreamProtocol;
use std::time::Duration;

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct DaProviderAnnouncement {
    pub chain_id: u64,
    pub height: u64,
    pub head_hash: [u8; 32],
    pub namespaces: Vec<[u8; 8]>,
}

/// Wire requests (borsh + length prefix on substreams).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum SyncRequest {
    GetTip,
    /// Inclusive lower bound on `BlockHeader::height` (canonical chain order in producer `blocks` vec).
    GetBlocks {
        from_height: u64,
        max_blocks: u32,
    },
    /// Fetch DA shares by committed block hash and sidecar share indexes.
    GetDaShares {
        block_hash: [u8; 32],
        indexes: Vec<u32>,
    },
}

/// `Blocks` carries `borsh::to_vec` of `Vec<fractal_consensus::Block>` and `DaShares` carries
/// `borsh::to_vec` of `Vec<fractal_consensus::DaShare>` (keeps this crate free of consensus types).
#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub enum SyncResponse {
    Tip {
        height: u64,
        head_hash: [u8; 32],
    },
    Blocks(Vec<u8>),
    DaShares {
        block_hash: [u8; 32],
        indexes: Vec<u32>,
        shares: Vec<u8>,
    },
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

/// Gossipsub topic for HotStuff-2 votes (`docs/prd.md` §18 M7-d-5).
pub const VOTES_TOPIC_STR: &str = "/fractalchain/votes/1.0.0";

/// Gossipsub topic where DA-serving peers advertise custody for namespaces/heights.
pub const DA_PROVIDERS_TOPIC_STR: &str = "/fractalchain/da-providers/1.0.0";
