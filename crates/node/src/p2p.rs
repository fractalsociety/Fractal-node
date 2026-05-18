//! libp2p QUIC + request-response block sync (PRD §18 M2) + gossipsub votes (M7-d-5) + timeouts (M7-f).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use fractal_consensus::{Block, Timeout, Vote};
use fractal_network::{BorshSyncCodec, SyncRequest, SyncResponse};
use fractal_shard::MasterchainBlockV1;
use futures::StreamExt;
use libp2p::gossipsub::{self, IdentTopic};
use libp2p::{
    Multiaddr, PeerId, Swarm, SwarmBuilder,
    multiaddr::Protocol,
    request_response::{self, Message},
    swarm::{NetworkBehaviour, SwarmEvent},
};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::oneshot;

use crate::NodeHandle;
use crate::metrics::MetricsState;

fn follower_fast_sync_enabled() -> bool {
    match std::env::var("FRACTAL_FAST_SYNC") {
        Ok(v) => {
            let t = v.trim().to_ascii_lowercase();
            !(t.is_empty() || t == "0" || t == "false" || t == "no" || t == "off")
        }
        Err(_) => true,
    }
}

fn p2p_dec_connection_count(c: &AtomicUsize) {
    let mut cur = c.load(Ordering::Relaxed);
    while cur > 0 {
        match c.compare_exchange_weak(cur, cur - 1, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return,
            Err(x) => cur = x,
        }
    }
}

/// Deterministic libp2p Ed25519 keys for **`testnets/devnet/docker-compose.yml`** (`profile: follower`).
///
/// Set **`FRACTAL_P2P_DOCKER_FIXTURE=producer`** on the producer and **`follower`** on the follower so
/// `FRACTAL_BOOTSTRAP` can embed a **stable** `/p2p/<PeerId>` without mounting key material.
///
/// Seeds are arbitrary valid Ed25519 scalars (not for mainnet).
const DEV_DOCKER_PRODUCER_SEED: [u8; 32] = [
    0x9d, 0x61, 0xb1, 0x9d, 0xef, 0xfd, 0x5a, 0x60, 0xba, 0x84, 0x4a, 0xf4, 0x92, 0xec, 0x2c, 0xc4,
    0x44, 0x29, 0xf4, 0x96, 0x8b, 0x18, 0xde, 0x28, 0x68, 0x99, 0x07, 0xab, 0xe8, 0x05, 0x9b, 0x2e,
];
const DEV_DOCKER_FOLLOWER_SEED: [u8; 32] = [
    0x4c, 0xc3, 0xc2, 0x35, 0x94, 0x0b, 0xe1, 0x11, 0x88, 0x40, 0x1d, 0x3a, 0x08, 0x53, 0x15, 0x9b,
    0x7a, 0x01, 0x14, 0x8d, 0xad, 0x22, 0x16, 0x41, 0x2a, 0x3d, 0x7a, 0x43, 0x9d, 0x03, 0x08, 0x57,
];

/// Deterministic keypair for docker-compose devnet (`FRACTAL_P2P_DOCKER_FIXTURE`).
pub fn p2p_keypair_docker_fixture(role: &str) -> std::io::Result<libp2p::identity::Keypair> {
    use libp2p::identity::{Keypair, ed25519};
    let seed = match role {
        "producer" | "1" => DEV_DOCKER_PRODUCER_SEED,
        "follower" | "2" => DEV_DOCKER_FOLLOWER_SEED,
        _ => {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                format!("FRACTAL_P2P_DOCKER_FIXTURE: expected producer|follower|1|2, got {role:?}"),
            ));
        }
    };
    let sk = ed25519::SecretKey::try_from_bytes(seed).map_err(|e| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("FRACTAL_P2P_DOCKER_FIXTURE ed25519 seed: {e}"),
        )
    })?;
    Ok(Keypair::from(ed25519::Keypair::from(sk)))
}

/// PRD §8.1 (peer identity): load or create a libp2p **Ed25519** keypair stored as protobuf private key bytes.
///
/// On first use, creates parent directories, writes `path.identity.tmp` then renames to `path` (atomic on same filesystem).
pub fn load_or_create_p2p_keypair(path: &Path) -> std::io::Result<libp2p::identity::Keypair> {
    use libp2p::identity::Keypair;
    fn decode_err(e: libp2p::identity::DecodingError) -> std::io::Error {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("FRACTAL_P2P_IDENTITY_PATH decode: {e}"),
        )
    }
    fn encode_err(e: libp2p::identity::DecodingError) -> std::io::Error {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("encode libp2p keypair: {e}"),
        )
    }
    if path.exists() {
        let bytes = std::fs::read(path)?;
        return Keypair::from_protobuf_encoding(&bytes).map_err(decode_err);
    }
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent)?;
        }
    }
    let kp = Keypair::generate_ed25519();
    let enc = kp.to_protobuf_encoding().map_err(encode_err)?;
    let tmp = path.with_extension("identity.tmp");
    std::fs::write(&tmp, &enc)?;
    std::fs::rename(&tmp, path)?;
    Ok(kp)
}

/// Host keypair for the libp2p swarm: stable [`PeerId`] when **`FRACTAL_P2P_IDENTITY_PATH`** is set;
/// deterministic dev fixture when **`FRACTAL_P2P_DOCKER_FIXTURE`** is set; else ephemeral Ed25519.
pub fn p2p_keypair_from_env() -> std::io::Result<libp2p::identity::Keypair> {
    use libp2p::identity::Keypair;
    if let Some(raw) = std::env::var_os("FRACTAL_P2P_IDENTITY_PATH") {
        if !raw.is_empty() {
            return load_or_create_p2p_keypair(&PathBuf::from(raw));
        }
    }
    if let Ok(role) = std::env::var("FRACTAL_P2P_DOCKER_FIXTURE") {
        let t = role.trim();
        if !t.is_empty() {
            return p2p_keypair_docker_fixture(t);
        }
    }
    Ok(Keypair::generate_ed25519())
}

fn fractal_gossipsub_config() -> std::io::Result<gossipsub::Config> {
    gossipsub::ConfigBuilder::default()
        .mesh_n(2)
        .mesh_n_low(1)
        .mesh_n_high(2)
        .mesh_outbound_min(0)
        .heartbeat_initial_delay(Duration::from_millis(0))
        .heartbeat_interval(Duration::from_millis(200))
        .build()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()))
}

#[derive(NetworkBehaviour)]
pub struct NodeBehaviour {
    pub sync: request_response::Behaviour<BorshSyncCodec>,
    pub gossipsub: gossipsub::Behaviour,
}

async fn sync_response_for_request(node: &NodeHandle, req: SyncRequest) -> SyncResponse {
    let g = node.lock().await;
    match req {
        SyncRequest::GetTip => SyncResponse::Tip {
            height: g.height,
            head_hash: g.head_hash,
        },
        SyncRequest::GetBlocks {
            from_height,
            max_blocks,
        } => {
            let list: Vec<Block> = g
                .blocks
                .iter()
                .filter(|b| b.header.height >= from_height)
                .take(max_blocks as usize)
                .cloned()
                .collect();
            match borsh::to_vec(&list) {
                Ok(b) => SyncResponse::Blocks(b),
                Err(e) => SyncResponse::ErrMsg(format!("encode blocks: {e}")),
            }
        }
        SyncRequest::GetSnapshot => match g.encode_preferred_chain_snapshot() {
            Ok(b) => SyncResponse::Snapshot(b),
            Err(e) => SyncResponse::ErrMsg(format!("encode snapshot: {e}")),
        },
        SyncRequest::GetMasterchainTip => SyncResponse::MasterchainTip {
            height: g.masterchain_ledger.masterchain_height,
        },
        SyncRequest::GetMasterchainBlocks {
            from_height,
            max_blocks,
        } => {
            let mut list: Vec<MasterchainBlockV1> = g
                .masterchain_ledger
                .blocks
                .iter()
                .filter(|b| b.height >= from_height)
                .take(max_blocks as usize)
                .cloned()
                .collect();
            if list.is_empty() {
                if let Some(db) = g.chain_store.as_ref() {
                    for height in from_height..from_height.saturating_add(max_blocks as u64) {
                        match db.get_masterchain_block_v1(height) {
                            Ok(Some(block)) => list.push(block),
                            Ok(None) => break,
                            Err(e) => {
                                return SyncResponse::ErrMsg(format!(
                                    "read masterchain block {height}: {e}"
                                ));
                            }
                        }
                    }
                }
            }
            match borsh::to_vec(&list) {
                Ok(b) => SyncResponse::MasterchainBlocks(b),
                Err(e) => SyncResponse::ErrMsg(format!("encode masterchain blocks: {e}")),
            }
        }
    }
}

async fn record_timeout_from_gossip_bytes(node: &NodeHandle, data: &[u8]) {
    match borsh::from_slice::<Timeout>(data) {
        Ok(t) => {
            let mut g = node.lock().await;
            let out = g.record_timeout(t);
            match out {
                fractal_consensus::RecordTimeoutOutcome::BadSignature
                | fractal_consensus::RecordTimeoutOutcome::OutOfRange => {
                    eprintln!("fractal-node gossipsub: timeout rejected ({out:?})");
                }
                _ => {}
            }
        }
        Err(e) => eprintln!("fractal-node gossipsub: invalid timeout borsh: {e}"),
    }
}

/// Pull blocks from `peer` when its tip is ahead of our local chain (any validator may lead).
async fn handle_outbound_sync_response(
    swarm: &mut Swarm<NodeBehaviour>,
    node: &NodeHandle,
    peer: PeerId,
    response: SyncResponse,
    outstanding: &mut bool,
) {
    match response {
        SyncResponse::Tip { height, .. } => {
            let (local_h, fast_on) = {
                let g = node.lock().await;
                (g.height, follower_fast_sync_enabled())
            };
            let next = local_h.saturating_add(1);
            if height >= next {
                *outstanding = true;
                if fast_on && local_h == 0 && height > 0 {
                    swarm
                        .behaviour_mut()
                        .sync
                        .send_request(&peer, SyncRequest::GetSnapshot);
                } else {
                    swarm.behaviour_mut().sync.send_request(
                        &peer,
                        SyncRequest::GetBlocks {
                            from_height: next,
                            max_blocks: 64,
                        },
                    );
                }
            } else {
                *outstanding = true;
                swarm
                    .behaviour_mut()
                    .sync
                    .send_request(&peer, SyncRequest::GetMasterchainTip);
            }
        }
        SyncResponse::Blocks(bytes) => {
            let blocks: Vec<Block> = match borsh::from_slice(&bytes) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("fractal-node: decode blocks from {peer}: {e}");
                    *outstanding = false;
                    return;
                }
            };
            if blocks.is_empty() {
                *outstanding = true;
                swarm
                    .behaviour_mut()
                    .sync
                    .send_request(&peer, SyncRequest::GetTip);
                return;
            }
            for b in &blocks {
                let mut g = node.lock().await;
                if let Err(e) = g.apply_synced_block(b) {
                    eprintln!("fractal-node: apply_synced_block from {peer}: {e}");
                }
            }
            *outstanding = true;
            swarm
                .behaviour_mut()
                .sync
                .send_request(&peer, SyncRequest::GetTip);
        }
        SyncResponse::Snapshot(bytes) => {
            {
                let mut g = node.lock().await;
                if let Err(e) = g.apply_chain_snapshot_auto(&bytes) {
                    if crate::monolith_migration_from_env() {
                        match g.apply_monolith_snapshot_to_shard0_v1(&bytes) {
                            Ok(()) => eprintln!(
                                "fractal-node: migrated Track A snapshot from {peer} into shard 0"
                            ),
                            Err(mig) => eprintln!(
                                "fractal-node: apply_chain_snapshot_v1 from {peer}: {e}; monolith migration failed: {mig}"
                            ),
                        }
                    } else {
                        eprintln!("fractal-node: apply_chain_snapshot_v1 from {peer}: {e}");
                    }
                }
            }
            *outstanding = true;
            swarm
                .behaviour_mut()
                .sync
                .send_request(&peer, SyncRequest::GetTip);
        }
        SyncResponse::ErrMsg(m) => {
            eprintln!("fractal-node: sync peer {peer} error: {m}");
            *outstanding = false;
        }
        SyncResponse::MasterchainTip { height } => {
            let local_h = {
                let g = node.lock().await;
                g.masterchain_ledger.masterchain_height
            };
            let next = local_h.saturating_add(1);
            if height >= next {
                *outstanding = true;
                swarm.behaviour_mut().sync.send_request(
                    &peer,
                    SyncRequest::GetMasterchainBlocks {
                        from_height: next,
                        max_blocks: 64,
                    },
                );
            } else {
                *outstanding = false;
            }
        }
        SyncResponse::MasterchainBlocks(bytes) => {
            let blocks: Vec<MasterchainBlockV1> = match borsh::from_slice(&bytes) {
                Ok(b) => b,
                Err(e) => {
                    eprintln!("fractal-node: decode masterchain blocks from {peer}: {e}");
                    *outstanding = false;
                    return;
                }
            };
            if blocks.is_empty() {
                *outstanding = false;
                return;
            }
            for block in &blocks {
                let mut g = node.lock().await;
                if let Err(e) = g.apply_synced_masterchain_block(block) {
                    eprintln!("fractal-node: apply_synced_masterchain_block from {peer}: {e}");
                }
            }
            *outstanding = true;
            swarm
                .behaviour_mut()
                .sync
                .send_request(&peer, SyncRequest::GetMasterchainTip);
        }
    }
}

fn note_p2p_peer_connected(
    swarm: &mut Swarm<NodeBehaviour>,
    peer_id: PeerId,
    connected: &mut bool,
    outstanding: &mut bool,
) {
    swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
    *connected = true;
    *outstanding = true;
    swarm
        .behaviour_mut()
        .sync
        .send_request(&peer_id, SyncRequest::GetTip);
}

async fn record_vote_from_gossip_bytes(node: &NodeHandle, data: &[u8]) {
    match borsh::from_slice::<Vote>(data) {
        Ok(vote) => {
            let mut g = node.lock().await;
            let out = g.record_vote(vote);
            match out {
                fractal_consensus::RecordVoteOutcome::BadSignature
                | fractal_consensus::RecordVoteOutcome::OutOfRange => {
                    eprintln!("fractal-node gossipsub: vote rejected ({out:?})");
                }
                _ => {}
            }
        }
        Err(e) => eprintln!("fractal-node gossipsub: invalid vote borsh: {e}"),
    }
}

/// `gossipsub::publish` returns [`gossipsub::PublishError::NoPeersSubscribedToTopic`] until the
/// mesh has grafted a peer. Queue one pending payload and retry after every swarm poll.
fn enqueue_timeout_publish(pending: &mut Option<Vec<u8>>, bytes: Vec<u8>) {
    *pending = Some(bytes);
}

fn try_flush_timeout_publish(
    gossip: &mut gossipsub::Behaviour,
    topic: &gossipsub::TopicHash,
    topic_label: &str,
    metrics: &Arc<MetricsState>,
    pending: &mut Option<Vec<u8>>,
) {
    let Some(bytes) = pending.as_ref() else {
        return;
    };
    match gossip.publish(topic.clone(), bytes.clone()) {
        Ok(_) => {
            metrics.p2p_topic_messages.record(topic_label, "out");
            pending.take();
        }
        Err(gossipsub::PublishError::NoPeersSubscribedToTopic) => {}
        Err(e) => {
            eprintln!("fractal-node gossipsub timeout publish: {e:?}");
            pending.take();
        }
    }
}

fn enqueue_vote_publish(pending: &mut Option<Vec<u8>>, bytes: Vec<u8>) {
    *pending = Some(bytes);
}

fn try_flush_vote_publish(
    gossip: &mut gossipsub::Behaviour,
    topic: &gossipsub::TopicHash,
    topic_label: &str,
    metrics: &Arc<MetricsState>,
    pending: &mut Option<Vec<u8>>,
) {
    let Some(bytes) = pending.as_ref() else {
        return;
    };
    match gossip.publish(topic.clone(), bytes.clone()) {
        Ok(_) => {
            metrics.p2p_topic_messages.record(topic_label, "out");
            pending.take();
        }
        Err(gossipsub::PublishError::NoPeersSubscribedToTopic) => {}
        Err(e) => {
            eprintln!("fractal-node gossipsub publish: {e:?}");
            pending.take();
        }
    }
}

fn try_flush_vote_and_timeout(
    gossip: &mut gossipsub::Behaviour,
    votes_th: &gossipsub::TopicHash,
    votes_label: &str,
    timeouts_th: &gossipsub::TopicHash,
    timeouts_label: &str,
    metrics: &Arc<MetricsState>,
    pending_vote: &mut Option<Vec<u8>>,
    pending_timeout: &mut Option<Vec<u8>>,
) {
    try_flush_vote_publish(gossip, votes_th, votes_label, metrics, pending_vote);
    try_flush_timeout_publish(
        gossip,
        timeouts_th,
        timeouts_label,
        metrics,
        pending_timeout,
    );
}

/// QUIC listener for sync + gossipsub vote and timeout topics; answers [`SyncRequest`] against `node`.
///
/// `vote_wire_out` / `timeout_wire_out`: when set, payloads from [`crate::NodeInner::vote_sink`] /
/// [`crate::NodeInner::timeout_sink`] are published on [`fractal_network::VOTES_TOPIC_STR`] /
/// [`fractal_network::TIMEOUTS_TOPIC_STR`].
pub async fn producer_network_task(
    node: NodeHandle,
    listen: Multiaddr,
    ready: Option<oneshot::Sender<(Multiaddr, PeerId)>>,
    vote_wire_out: Option<UnboundedReceiver<Vec<u8>>>,
    timeout_wire_out: Option<UnboundedReceiver<Vec<u8>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (shard_id, shard_count) = {
        let n = node.lock().await;
        (n.shard_id, n.shard_topology.shard_count)
    };
    let votes_topic_str = fractal_network::votes_topic_for_shard(shard_id, shard_count);
    let timeouts_topic_str = fractal_network::timeouts_topic_for_shard(shard_id, shard_count);
    eprintln!(
        "fractal-node p2p: shard_id={shard_id} shard_count={shard_count} votes_topic={votes_topic_str}"
    );
    let votes_topic = IdentTopic::new(&votes_topic_str);
    let votes_topic_hash = votes_topic.hash();
    let timeouts_topic = IdentTopic::new(&timeouts_topic_str);
    let timeouts_topic_hash = timeouts_topic.hash();
    let sync_protocols = fractal_network::sync_protocols_for_shard(shard_id, shard_count);

    let keypair = p2p_keypair_from_env()?;
    let mut swarm: Swarm<NodeBehaviour> = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_quic()
        .with_behaviour(|key| {
            let gossipsub_config = fractal_gossipsub_config()?;
            let gossipsub = gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(key.clone()),
                gossipsub_config,
            )
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            let mut gossipsub = gossipsub;
            gossipsub
                .subscribe(&votes_topic)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
            gossipsub
                .subscribe(&timeouts_topic)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

            Ok(NodeBehaviour {
                sync: request_response::Behaviour::with_codec(
                    BorshSyncCodec,
                    sync_protocols,
                    fractal_network::sync_request_response_config(),
                ),
                gossipsub,
            })
        })?
        .build();

    let mut ready = ready;
    let mut vote_rx = vote_wire_out;
    let mut timeout_rx = timeout_wire_out;
    let mut pending_vote: Option<Vec<u8>> = None;
    let mut pending_timeout: Option<Vec<u8>> = None;
    let mut sync_connected = false;
    let mut sync_outstanding = false;
    swarm.listen_on(listen)?;

    let p2p_peers = {
        let g = node.lock().await;
        g.p2p_connected_peers.clone()
    };
    let p2p_metrics = {
        let g = node.lock().await;
        g.metrics.clone()
    };

    loop {
        match (&mut vote_rx, &mut timeout_rx) {
            (Some(vr), Some(tr)) => {
                tokio::select! {
                    ev = swarm.select_next_some() => {
                        handle_producer_swarm_event(
                            ev,
                            &mut swarm,
                            &node,
                            &mut ready,
                            &votes_topic_hash,
                            &votes_topic_str,
                            &timeouts_topic_hash,
                            &timeouts_topic_str,
                            &p2p_peers,
                            &mut sync_connected,
                            &mut sync_outstanding,
                        )
                        .await;
                        try_flush_vote_and_timeout(
                            &mut swarm.behaviour_mut().gossipsub,
                            &votes_topic_hash,
                            &votes_topic_str,
                            &timeouts_topic_hash,
                            &timeouts_topic_str,
                            &p2p_metrics,
                            &mut pending_vote,
                            &mut pending_timeout,
                        );
                    }
                    opt = vr.recv() => {
                        if let Some(bytes) = opt {
                            enqueue_vote_publish(&mut pending_vote, bytes);
                            try_flush_vote_and_timeout(
                                &mut swarm.behaviour_mut().gossipsub,
                                &votes_topic_hash,
                                &votes_topic_str,
                                &timeouts_topic_hash,
                                &timeouts_topic_str,
                                &p2p_metrics,
                                &mut pending_vote,
                                &mut pending_timeout,
                            );
                        }
                    }
                    opt = tr.recv() => {
                        if let Some(bytes) = opt {
                            enqueue_timeout_publish(&mut pending_timeout, bytes);
                            try_flush_vote_and_timeout(
                                &mut swarm.behaviour_mut().gossipsub,
                                &votes_topic_hash,
                                &votes_topic_str,
                                &timeouts_topic_hash,
                                &timeouts_topic_str,
                                &p2p_metrics,
                                &mut pending_vote,
                                &mut pending_timeout,
                            );
                        }
                    }
                }
            }
            (Some(vr), None) => {
                tokio::select! {
                    ev = swarm.select_next_some() => {
                        handle_producer_swarm_event(
                            ev,
                            &mut swarm,
                            &node,
                            &mut ready,
                            &votes_topic_hash,
                            &votes_topic_str,
                            &timeouts_topic_hash,
                            &timeouts_topic_str,
                            &p2p_peers,
                            &mut sync_connected,
                            &mut sync_outstanding,
                        )
                        .await;
                        try_flush_vote_and_timeout(
                            &mut swarm.behaviour_mut().gossipsub,
                            &votes_topic_hash,
                            &votes_topic_str,
                            &timeouts_topic_hash,
                            &timeouts_topic_str,
                            &p2p_metrics,
                            &mut pending_vote,
                            &mut pending_timeout,
                        );
                    }
                    opt = vr.recv() => {
                        if let Some(bytes) = opt {
                            enqueue_vote_publish(&mut pending_vote, bytes);
                            try_flush_vote_and_timeout(
                                &mut swarm.behaviour_mut().gossipsub,
                                &votes_topic_hash,
                                &votes_topic_str,
                                &timeouts_topic_hash,
                                &timeouts_topic_str,
                                &p2p_metrics,
                                &mut pending_vote,
                                &mut pending_timeout,
                            );
                        }
                    }
                }
            }
            (None, Some(tr)) => {
                tokio::select! {
                    ev = swarm.select_next_some() => {
                        handle_producer_swarm_event(
                            ev,
                            &mut swarm,
                            &node,
                            &mut ready,
                            &votes_topic_hash,
                            &votes_topic_str,
                            &timeouts_topic_hash,
                            &timeouts_topic_str,
                            &p2p_peers,
                            &mut sync_connected,
                            &mut sync_outstanding,
                        )
                        .await;
                        try_flush_vote_and_timeout(
                            &mut swarm.behaviour_mut().gossipsub,
                            &votes_topic_hash,
                            &votes_topic_str,
                            &timeouts_topic_hash,
                            &timeouts_topic_str,
                            &p2p_metrics,
                            &mut pending_vote,
                            &mut pending_timeout,
                        );
                    }
                    opt = tr.recv() => {
                        if let Some(bytes) = opt {
                            enqueue_timeout_publish(&mut pending_timeout, bytes);
                            try_flush_vote_and_timeout(
                                &mut swarm.behaviour_mut().gossipsub,
                                &votes_topic_hash,
                                &votes_topic_str,
                                &timeouts_topic_hash,
                                &timeouts_topic_str,
                                &p2p_metrics,
                                &mut pending_vote,
                                &mut pending_timeout,
                            );
                        }
                    }
                }
            }
            (None, None) => {
                let ev = swarm.select_next_some().await;
                handle_producer_swarm_event(
                    ev,
                    &mut swarm,
                    &node,
                    &mut ready,
                    &votes_topic_hash,
                    &votes_topic_str,
                    &timeouts_topic_hash,
                    &timeouts_topic_str,
                    &p2p_peers,
                    &mut sync_connected,
                    &mut sync_outstanding,
                )
                .await;
                try_flush_vote_and_timeout(
                    &mut swarm.behaviour_mut().gossipsub,
                    &votes_topic_hash,
                    &votes_topic_str,
                    &timeouts_topic_hash,
                    &timeouts_topic_str,
                    &p2p_metrics,
                    &mut pending_vote,
                    &mut pending_timeout,
                );
            }
        }
    }
}

async fn handle_producer_swarm_event(
    ev: SwarmEvent<NodeBehaviourEvent>,
    swarm: &mut Swarm<NodeBehaviour>,
    node: &NodeHandle,
    ready: &mut Option<oneshot::Sender<(Multiaddr, PeerId)>>,
    votes_topic_hash: &libp2p::gossipsub::TopicHash,
    votes_topic_label: &str,
    timeouts_topic_hash: &libp2p::gossipsub::TopicHash,
    timeouts_topic_label: &str,
    p2p_peers: &Arc<AtomicUsize>,
    sync_connected: &mut bool,
    sync_outstanding: &mut bool,
) {
    match ev {
        SwarmEvent::NewListenAddr { address, .. } => {
            let routable_ip = !address.iter().any(|p| match p {
                Protocol::Ip4(ip) => ip.is_unspecified(),
                Protocol::Ip6(ip) => ip.is_unspecified(),
                _ => false,
            });
            if routable_ip && address.iter().any(|p| matches!(p, Protocol::Udp(_))) {
                if let Some(tx) = ready.take() {
                    let _ = tx.send((address, swarm.local_peer_id().clone()));
                }
            }
        }
        SwarmEvent::Behaviour(NodeBehaviourEvent::Sync(ev)) => match ev {
            request_response::Event::Message {
                peer: _,
                message:
                    Message::Request {
                        request, channel, ..
                    },
                ..
            } => {
                let resp = sync_response_for_request(node, request).await;
                let _ = swarm.behaviour_mut().sync.send_response(channel, resp);
            }
            request_response::Event::Message {
                peer,
                message: Message::Response { response, .. },
                ..
            } => {
                handle_outbound_sync_response(swarm, node, peer, response, sync_outstanding).await;
            }
            request_response::Event::InboundFailure { error, .. } => {
                eprintln!("fractal-node p2p inbound failure: {error:?}");
            }
            request_response::Event::OutboundFailure { error, .. } => {
                eprintln!("fractal-node p2p outbound sync failure: {error:?}");
                *sync_outstanding = false;
            }
            _ => {}
        },
        SwarmEvent::Behaviour(NodeBehaviourEvent::Gossipsub(gossipsub::Event::Message {
            message,
            ..
        })) => {
            if message.topic == *votes_topic_hash {
                if let Ok(g) = node.try_lock() {
                    g.metrics.p2p_topic_messages.record(votes_topic_label, "in");
                }
                record_vote_from_gossip_bytes(node, &message.data).await;
            } else if message.topic == *timeouts_topic_hash {
                if let Ok(g) = node.try_lock() {
                    g.metrics
                        .p2p_topic_messages
                        .record(timeouts_topic_label, "in");
                }
                record_timeout_from_gossip_bytes(node, &message.data).await;
            }
        }
        SwarmEvent::Behaviour(NodeBehaviourEvent::Gossipsub(_)) => {}
        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
            p2p_peers.fetch_add(1, Ordering::Relaxed);
            note_p2p_peer_connected(swarm, peer_id, sync_connected, sync_outstanding);
        }
        SwarmEvent::ConnectionClosed { .. } => {
            p2p_dec_connection_count(p2p_peers);
        }
        SwarmEvent::IncomingConnection { .. } | SwarmEvent::IncomingConnectionError { .. } => {}
        _ => {}
    }
}

fn peer_id_from_multiaddr(addr: &Multiaddr) -> Option<PeerId> {
    addr.iter().find_map(|p| {
        if let Protocol::P2p(pid) = p {
            Some(pid)
        } else {
            None
        }
    })
}

/// Parse `FRACTAL_BOOTSTRAP`: comma-separated multiaddrs (whitespace trimmed), each with `/p2p/<PeerId>`.
/// Every entry must use the **same** [`PeerId`] so the follower has a single logical producer.
pub fn parse_fractal_bootstraps(s: &str) -> Result<Vec<Multiaddr>, String> {
    let parts: Vec<&str> = s
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        return Err("FRACTAL_BOOTSTRAP is empty".into());
    }
    let mut out = Vec::with_capacity(parts.len());
    let mut peer: Option<PeerId> = None;
    for p in parts {
        let m: Multiaddr = p
            .parse()
            .map_err(|e: libp2p::multiaddr::Error| e.to_string())?;
        let pid = peer_id_from_multiaddr(&m)
            .ok_or_else(|| format!("FRACTAL_BOOTSTRAP entry has no /p2p/: {m}"))?;
        match peer {
            None => peer = Some(pid),
            Some(ep) if ep != pid => {
                return Err(format!(
                    "FRACTAL_BOOTSTRAP: mismatched PeerId {pid} vs expected {ep} ({m})"
                ));
            }
            _ => {}
        }
        out.push(m);
    }
    Ok(out)
}

/// Dial each bootstrap multiaddr (same [`PeerId`]) and pull blocks until caught up with producer tip.
pub async fn follower_network_task(
    node: NodeHandle,
    bootstraps: Vec<Multiaddr>,
    vote_wire_out: Option<UnboundedReceiver<Vec<u8>>>,
    timeout_wire_out: Option<UnboundedReceiver<Vec<u8>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if bootstraps.is_empty() {
        return Err("bootstraps is empty".into());
    }
    let producer_peer = peer_id_from_multiaddr(&bootstraps[0])
        .ok_or("FRACTAL_BOOTSTRAP multiaddr must include /p2p/<PeerId>")?;

    let (shard_id, shard_count) = {
        let n = node.lock().await;
        (n.shard_id, n.shard_topology.shard_count)
    };
    let votes_topic_str = fractal_network::votes_topic_for_shard(shard_id, shard_count);
    let timeouts_topic_str = fractal_network::timeouts_topic_for_shard(shard_id, shard_count);
    let votes_topic = IdentTopic::new(&votes_topic_str);
    let votes_topic_hash = votes_topic.hash();
    let timeouts_topic = IdentTopic::new(&timeouts_topic_str);
    let timeouts_topic_hash = timeouts_topic.hash();
    let sync_protocols = fractal_network::sync_protocols_for_shard(shard_id, shard_count);

    let keypair = p2p_keypair_from_env()?;
    let mut swarm: Swarm<NodeBehaviour> = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_quic()
        .with_behaviour(|key| {
            let gossipsub_config = fractal_gossipsub_config()?;
            let gossipsub = gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(key.clone()),
                gossipsub_config,
            )
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
            let mut gossipsub = gossipsub;
            gossipsub
                .subscribe(&votes_topic)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
            gossipsub
                .subscribe(&timeouts_topic)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

            Ok(NodeBehaviour {
                sync: request_response::Behaviour::with_codec(
                    BorshSyncCodec,
                    sync_protocols,
                    fractal_network::sync_request_response_config(),
                ),
                gossipsub,
            })
        })?
        .build();

    for b in &bootstraps {
        swarm.dial(b.clone())?;
    }

    let p2p_peers = {
        let g = node.lock().await;
        g.p2p_connected_peers.clone()
    };
    let p2p_metrics = {
        let g = node.lock().await;
        g.metrics.clone()
    };

    let mut poll = tokio::time::interval(Duration::from_millis(600));
    let mut connected = false;
    let mut outstanding = false;
    let mut vote_rx = vote_wire_out;
    let mut timeout_rx = timeout_wire_out;
    let mut pending_vote: Option<Vec<u8>> = None;
    let mut pending_timeout: Option<Vec<u8>> = None;

    loop {
        match (&mut vote_rx, &mut timeout_rx) {
            (Some(vr), Some(tr)) => {
                tokio::select! {
                    ev = swarm.select_next_some() => {
                        handle_follower_swarm_event(
                            ev,
                            &mut swarm,
                            &node,
                            producer_peer,
                            &mut connected,
                            &mut outstanding,
                            &votes_topic_hash,
                            &votes_topic_str,
                            &timeouts_topic_hash,
                            &timeouts_topic_str,
                            &p2p_peers,
                        )
                        .await;
                        try_flush_vote_and_timeout(
                            &mut swarm.behaviour_mut().gossipsub,
                            &votes_topic_hash,
                            &votes_topic_str,
                            &timeouts_topic_hash,
                            &timeouts_topic_str,
                            &p2p_metrics,
                            &mut pending_vote,
                            &mut pending_timeout,
                        );
                    }
                    opt = vr.recv() => {
                        if let Some(bytes) = opt {
                            enqueue_vote_publish(&mut pending_vote, bytes);
                            try_flush_vote_and_timeout(
                                &mut swarm.behaviour_mut().gossipsub,
                                &votes_topic_hash,
                                &votes_topic_str,
                                &timeouts_topic_hash,
                                &timeouts_topic_str,
                                &p2p_metrics,
                                &mut pending_vote,
                                &mut pending_timeout,
                            );
                        }
                    }
                    opt = tr.recv() => {
                        if let Some(bytes) = opt {
                            enqueue_timeout_publish(&mut pending_timeout, bytes);
                            try_flush_vote_and_timeout(
                                &mut swarm.behaviour_mut().gossipsub,
                                &votes_topic_hash,
                                &votes_topic_str,
                                &timeouts_topic_hash,
                                &timeouts_topic_str,
                                &p2p_metrics,
                                &mut pending_vote,
                                &mut pending_timeout,
                            );
                        }
                    }
                    _ = poll.tick(), if connected && !outstanding => {
                        outstanding = true;
                        swarm.behaviour_mut().sync.send_request(&producer_peer, SyncRequest::GetTip);
                        try_flush_vote_and_timeout(
                            &mut swarm.behaviour_mut().gossipsub,
                            &votes_topic_hash,
                            &votes_topic_str,
                            &timeouts_topic_hash,
                            &timeouts_topic_str,
                            &p2p_metrics,
                            &mut pending_vote,
                            &mut pending_timeout,
                        );
                    }
                }
            }
            (Some(vr), None) => {
                tokio::select! {
                    ev = swarm.select_next_some() => {
                        handle_follower_swarm_event(
                            ev,
                            &mut swarm,
                            &node,
                            producer_peer,
                            &mut connected,
                            &mut outstanding,
                            &votes_topic_hash,
                            &votes_topic_str,
                            &timeouts_topic_hash,
                            &timeouts_topic_str,
                            &p2p_peers,
                        )
                        .await;
                        try_flush_vote_and_timeout(
                            &mut swarm.behaviour_mut().gossipsub,
                            &votes_topic_hash,
                            &votes_topic_str,
                            &timeouts_topic_hash,
                            &timeouts_topic_str,
                            &p2p_metrics,
                            &mut pending_vote,
                            &mut pending_timeout,
                        );
                    }
                    opt = vr.recv() => {
                        if let Some(bytes) = opt {
                            enqueue_vote_publish(&mut pending_vote, bytes);
                            try_flush_vote_and_timeout(
                                &mut swarm.behaviour_mut().gossipsub,
                                &votes_topic_hash,
                                &votes_topic_str,
                                &timeouts_topic_hash,
                                &timeouts_topic_str,
                                &p2p_metrics,
                                &mut pending_vote,
                                &mut pending_timeout,
                            );
                        }
                    }
                    _ = poll.tick(), if connected && !outstanding => {
                        outstanding = true;
                        swarm.behaviour_mut().sync.send_request(&producer_peer, SyncRequest::GetTip);
                        try_flush_vote_and_timeout(
                            &mut swarm.behaviour_mut().gossipsub,
                            &votes_topic_hash,
                            &votes_topic_str,
                            &timeouts_topic_hash,
                            &timeouts_topic_str,
                            &p2p_metrics,
                            &mut pending_vote,
                            &mut pending_timeout,
                        );
                    }
                }
            }
            (None, Some(tr)) => {
                tokio::select! {
                    ev = swarm.select_next_some() => {
                        handle_follower_swarm_event(
                            ev,
                            &mut swarm,
                            &node,
                            producer_peer,
                            &mut connected,
                            &mut outstanding,
                            &votes_topic_hash,
                            &votes_topic_str,
                            &timeouts_topic_hash,
                            &timeouts_topic_str,
                            &p2p_peers,
                        )
                        .await;
                        try_flush_vote_and_timeout(
                            &mut swarm.behaviour_mut().gossipsub,
                            &votes_topic_hash,
                            &votes_topic_str,
                            &timeouts_topic_hash,
                            &timeouts_topic_str,
                            &p2p_metrics,
                            &mut pending_vote,
                            &mut pending_timeout,
                        );
                    }
                    opt = tr.recv() => {
                        if let Some(bytes) = opt {
                            enqueue_timeout_publish(&mut pending_timeout, bytes);
                            try_flush_vote_and_timeout(
                                &mut swarm.behaviour_mut().gossipsub,
                                &votes_topic_hash,
                                &votes_topic_str,
                                &timeouts_topic_hash,
                                &timeouts_topic_str,
                                &p2p_metrics,
                                &mut pending_vote,
                                &mut pending_timeout,
                            );
                        }
                    }
                    _ = poll.tick(), if connected && !outstanding => {
                        outstanding = true;
                        swarm.behaviour_mut().sync.send_request(&producer_peer, SyncRequest::GetTip);
                        try_flush_vote_and_timeout(
                            &mut swarm.behaviour_mut().gossipsub,
                            &votes_topic_hash,
                            &votes_topic_str,
                            &timeouts_topic_hash,
                            &timeouts_topic_str,
                            &p2p_metrics,
                            &mut pending_vote,
                            &mut pending_timeout,
                        );
                    }
                }
            }
            (None, None) => {
                tokio::select! {
                    ev = swarm.select_next_some() => {
                        handle_follower_swarm_event(
                            ev,
                            &mut swarm,
                            &node,
                            producer_peer,
                            &mut connected,
                            &mut outstanding,
                            &votes_topic_hash,
                            &votes_topic_str,
                            &timeouts_topic_hash,
                            &timeouts_topic_str,
                            &p2p_peers,
                        )
                        .await;
                    }
                    _ = poll.tick(), if connected && !outstanding => {
                        outstanding = true;
                        swarm.behaviour_mut().sync.send_request(&producer_peer, SyncRequest::GetTip);
                    }
                }
            }
        }
    }
}

async fn handle_follower_swarm_event(
    ev: SwarmEvent<NodeBehaviourEvent>,
    swarm: &mut Swarm<NodeBehaviour>,
    node: &NodeHandle,
    producer_peer: PeerId,
    connected: &mut bool,
    outstanding: &mut bool,
    votes_topic_hash: &libp2p::gossipsub::TopicHash,
    votes_topic_label: &str,
    timeouts_topic_hash: &libp2p::gossipsub::TopicHash,
    timeouts_topic_label: &str,
    p2p_peers: &Arc<AtomicUsize>,
) {
    match ev {
        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
            p2p_peers.fetch_add(1, Ordering::Relaxed);
            let _ = producer_peer;
            note_p2p_peer_connected(swarm, peer_id, connected, outstanding);
        }
        SwarmEvent::Behaviour(NodeBehaviourEvent::Sync(ev)) => match ev {
            request_response::Event::Message {
                peer: _,
                message:
                    Message::Request {
                        request, channel, ..
                    },
                ..
            } => {
                let resp = sync_response_for_request(node, request).await;
                let _ = swarm.behaviour_mut().sync.send_response(channel, resp);
            }
            request_response::Event::Message {
                peer,
                message: Message::Response { response, .. },
                ..
            } => {
                handle_outbound_sync_response(swarm, node, peer, response, outstanding).await;
            }
            request_response::Event::OutboundFailure { error, .. } => {
                *outstanding = false;
                eprintln!("fractal-node follower: outbound failure: {error:?}");
            }
            request_response::Event::InboundFailure { error, .. } => {
                eprintln!("fractal-node follower: inbound sync failure: {error:?}");
            }
            _ => {}
        },
        SwarmEvent::Behaviour(NodeBehaviourEvent::Gossipsub(gossipsub::Event::Message {
            message,
            ..
        })) => {
            if message.topic == *votes_topic_hash {
                if let Ok(g) = node.try_lock() {
                    g.metrics.p2p_topic_messages.record(votes_topic_label, "in");
                }
                record_vote_from_gossip_bytes(node, &message.data).await;
            } else if message.topic == *timeouts_topic_hash {
                if let Ok(g) = node.try_lock() {
                    g.metrics
                        .p2p_topic_messages
                        .record(timeouts_topic_label, "in");
                }
                record_timeout_from_gossip_bytes(node, &message.data).await;
            }
        }
        SwarmEvent::Behaviour(NodeBehaviourEvent::Gossipsub(_)) => {}
        SwarmEvent::ConnectionClosed { .. } => {
            p2p_dec_connection_count(p2p_peers);
        }
        _ => {}
    }
}

#[cfg(test)]
mod bootstrap_parse_tests {
    use super::*;

    #[test]
    fn parse_fractal_bootstraps_accepts_comma_separated_same_peer() {
        let id = PeerId::random();
        let a: Multiaddr = format!("/ip4/127.0.0.1/tcp/10001/p2p/{id}")
            .parse()
            .unwrap();
        let b: Multiaddr = format!("/ip4/127.0.0.1/tcp/10002/p2p/{id}")
            .parse()
            .unwrap();
        let s = format!("{a}, {b}");
        let v = parse_fractal_bootstraps(&s).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0], a);
        assert_eq!(v[1], b);
    }

    #[test]
    fn parse_fractal_bootstraps_rejects_mismatched_peer() {
        let id1 = PeerId::random();
        let id2 = PeerId::random();
        let a: Multiaddr = format!("/ip4/127.0.0.1/tcp/10001/p2p/{id1}")
            .parse()
            .unwrap();
        let b: Multiaddr = format!("/ip4/127.0.0.1/tcp/10002/p2p/{id2}")
            .parse()
            .unwrap();
        let s = format!("{a},{b}");
        assert!(parse_fractal_bootstraps(&s).is_err());
    }
}

#[cfg(test)]
mod docker_fixture_tests {
    use super::p2p_keypair_docker_fixture;
    use libp2p::PeerId;

    /// Stable producer [`PeerId`] when using **`FRACTAL_P2P_DOCKER_FIXTURE=producer`** (devnet Docker
    /// only; must match `testnets/devnet/docker-compose.yml` **`follower`** `FRACTAL_BOOTSTRAP`).
    pub const DEV_DOCKER_PRODUCER_PEER_ID: &str =
        "12D3KooWSM8gM58U7tv7pS4ggWFqbGtJU7MRgYUjJKsJA6uSTkh3";

    #[test]
    fn docker_fixture_producer_peer_id_matches_devnet_compose() {
        let kp = p2p_keypair_docker_fixture("producer").unwrap();
        let got = PeerId::from_public_key(&kp.public());
        let expected: PeerId = DEV_DOCKER_PRODUCER_PEER_ID
            .parse()
            .expect("DEV_DOCKER_PRODUCER_PEER_ID must be valid base58 PeerId");
        assert_eq!(
            got, expected,
            "update DEV_DOCKER_PRODUCER_PEER_ID + docker-compose FRACTAL_BOOTSTRAP if fixture seed changes"
        );
    }

    #[test]
    fn docker_fixture_peer_ids_are_distinct() {
        let p = p2p_keypair_docker_fixture("producer").unwrap();
        let f = p2p_keypair_docker_fixture("follower").unwrap();
        assert_ne!(
            PeerId::from_public_key(&p.public()),
            PeerId::from_public_key(&f.public())
        );
    }
}

#[cfg(test)]
mod p2p_identity_tests {
    use super::load_or_create_p2p_keypair;
    use libp2p::PeerId;

    #[test]
    fn load_or_create_stable_peer_id() {
        let dir = std::env::temp_dir().join(format!(
            "fractal_p2p_id_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("host.key");
        let k1 = load_or_create_p2p_keypair(&path).unwrap();
        let pid1 = PeerId::from_public_key(&k1.public());
        let k2 = load_or_create_p2p_keypair(&path).unwrap();
        let pid2 = PeerId::from_public_key(&k2.public());
        assert_eq!(pid1, pid2);
        assert!(path.is_file());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
