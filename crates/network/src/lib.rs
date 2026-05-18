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
    /// PRD §10.4 fast-sync snapshot blob (proof-chain snapshot or `ChainSyncSnapshotV2`; v1 accepted by old peers).
    GetSnapshot,
    /// Latest local masterchain coordination height.
    GetMasterchainTip,
    /// Inclusive lower bound on `MasterchainBlockV1::height`.
    GetMasterchainBlocks {
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
    /// `borsh` of a fractal-node chain snapshot (opaque to this crate).
    Snapshot(Vec<u8>),
    MasterchainTip {
        height: u64,
    },
    /// `borsh::to_vec(Vec<fractal_shard::MasterchainBlockV1>)` (opaque to this crate).
    MasterchainBlocks(Vec<u8>),
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

/// Gossipsub topic for view-timeout messages (`docs/prd.md` §7.4 / §18 M7-f).
pub const TIMEOUTS_TOPIC_STR: &str = "/fractalchain/timeouts/1.0.0";

/// Gossipsub topic for dedicated masterchain BFT vote envelopes.
pub const MASTERCHAIN_VOTES_TOPIC_STR: &str = "/fractalchain/masterchain/votes/1.0.0";

/// Gossipsub topic for dedicated masterchain BFT timeout envelopes.
pub const MASTERCHAIN_TIMEOUTS_TOPIC_STR: &str = "/fractalchain/masterchain/timeouts/1.0.0";

/// Per-shard vote gossip (`docs/prd.md` §8, Track B). Monolith uses [`VOTES_TOPIC_STR`].
#[must_use]
pub fn shard_votes_topic(shard_id: u32) -> String {
    format!("/fractalchain/shard/{shard_id}/votes/1.0.0")
}

#[must_use]
pub fn shard_timeouts_topic(shard_id: u32) -> String {
    format!("/fractalchain/shard/{shard_id}/timeouts/1.0.0")
}

#[must_use]
pub fn shard_sync_protocol(shard_id: u32) -> StreamProtocol {
    let name: &'static str =
        Box::leak(format!("/fractalchain/shard/{shard_id}/sync/1.0.0").into_boxed_str());
    StreamProtocol::new(name)
}

/// Vote topic for this process: global monolith or shard-scoped mesh.
#[must_use]
pub fn votes_topic_for_shard(shard_id: u32, shard_count: u32) -> String {
    if shard_count <= 1 {
        VOTES_TOPIC_STR.to_string()
    } else {
        shard_votes_topic(shard_id)
    }
}

#[must_use]
pub fn timeouts_topic_for_shard(shard_id: u32, shard_count: u32) -> String {
    if shard_count <= 1 {
        TIMEOUTS_TOPIC_STR.to_string()
    } else {
        shard_timeouts_topic(shard_id)
    }
}

#[must_use]
pub fn sync_protocols_for_shard(
    shard_id: u32,
    shard_count: u32,
) -> Vec<(StreamProtocol, ProtocolSupport)> {
    if shard_count <= 1 {
        sync_protocols()
    } else {
        vec![(shard_sync_protocol(shard_id), ProtocolSupport::Full)]
    }
}
