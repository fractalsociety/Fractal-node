//! libp2p QUIC + request-response block sync (PRD §18 M2) + gossipsub votes (M7-d-5).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;
use std::time::Duration;

use fractal_consensus::{header_hash, Block, DaShare, Vote};
use fractal_network::{BorshSyncCodec, DaProviderAnnouncement, SyncRequest, SyncResponse};
use futures::StreamExt;
use libp2p::gossipsub::{self, IdentTopic};
use libp2p::{
    multiaddr::Protocol,
    request_response::{self, Message},
    swarm::{NetworkBehaviour, SwarmEvent},
    Multiaddr, PeerId, Swarm, SwarmBuilder,
};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::oneshot;

use crate::NodeHandle;

const DEFAULT_NETWORK_DA_SAMPLE_COUNT: usize = 8;
const DA_PROVIDER_ANNOUNCE_INTERVAL: Duration = Duration::from_millis(800);

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

/// Host keypair for the libp2p swarm: stable [`PeerId`] when **`FRACTAL_P2P_IDENTITY_PATH`** is set, else ephemeral Ed25519.
pub fn p2p_keypair_from_env() -> std::io::Result<libp2p::identity::Keypair> {
    use libp2p::identity::Keypair;
    match std::env::var_os("FRACTAL_P2P_IDENTITY_PATH") {
        None => Ok(Keypair::generate_ed25519()),
        Some(raw) if raw.is_empty() => Ok(Keypair::generate_ed25519()),
        Some(raw) => load_or_create_p2p_keypair(&PathBuf::from(raw)),
    }
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
        SyncRequest::GetDaShares {
            block_hash,
            indexes,
        } => match g.da_shares_by_block_hash(&block_hash, &indexes) {
            Some(shares) => match borsh::to_vec(&shares) {
                Ok(b) => SyncResponse::DaShares {
                    block_hash,
                    indexes,
                    shares: b,
                },
                Err(e) => SyncResponse::ErrMsg(format!("encode DA shares: {e}")),
            },
            None => SyncResponse::ErrMsg("DA shares not found".into()),
        },
    }
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

async fn record_connected_peer_count(node: &NodeHandle, count: usize) {
    let g = node.lock().await;
    g.p2p_connected_peers.store(count as u64, Ordering::Relaxed);
}

/// `gossipsub::publish` returns [`gossipsub::PublishError::NoPeersSubscribedToTopic`] until the
/// mesh has grafted a peer. Queue one pending payload and retry after every swarm poll.
fn enqueue_vote_publish(pending: &mut Option<Vec<u8>>, bytes: Vec<u8>) {
    *pending = Some(bytes);
}

fn try_flush_vote_publish(
    gossip: &mut gossipsub::Behaviour,
    topic: &gossipsub::TopicHash,
    pending: &mut Option<Vec<u8>>,
) {
    let Some(bytes) = pending.as_ref() else {
        return;
    };
    match gossip.publish(topic.clone(), bytes.clone()) {
        Ok(_) => {
            pending.take();
        }
        Err(gossipsub::PublishError::NoPeersSubscribedToTopic) => {}
        Err(e) => {
            eprintln!("fractal-node gossipsub publish: {e:?}");
            pending.take();
        }
    }
}

fn da_provider_announcement_from_node(g: &crate::NodeInner) -> DaProviderAnnouncement {
    let mut namespaces = BTreeSet::new();
    for block in &g.blocks {
        namespaces.insert(block.header.zone_namespace);
    }
    if namespaces.is_empty() {
        namespaces.insert(fractal_consensus::MASTERCHAIN_ZONE_NAMESPACE);
    }
    DaProviderAnnouncement {
        chain_id: g.chain_id,
        height: g.height,
        head_hash: g.head_hash,
        namespaces: namespaces.into_iter().collect(),
    }
}

async fn publish_da_provider_announcement(
    node: &NodeHandle,
    gossip: &mut gossipsub::Behaviour,
    topic: &gossipsub::TopicHash,
) {
    let ann = {
        let g = node.lock().await;
        da_provider_announcement_from_node(&g)
    };
    let Ok(bytes) = borsh::to_vec(&ann) else {
        return;
    };
    match gossip.publish(topic.clone(), bytes) {
        Ok(_) | Err(gossipsub::PublishError::NoPeersSubscribedToTopic) => {}
        Err(e) => eprintln!("fractal-node DA provider announcement publish: {e:?}"),
    }
}

/// QUIC listener for sync + gossipsub vote topic; answers [`SyncRequest`] against `node`.
///
/// `vote_wire_out`: when set, vote payloads from [`crate::NodeInner::vote_sink`] are published on
/// [`fractal_network::VOTES_TOPIC_STR`].
pub async fn producer_network_task(
    node: NodeHandle,
    listen: Multiaddr,
    ready: Option<oneshot::Sender<(Multiaddr, PeerId)>>,
    vote_wire_out: Option<UnboundedReceiver<Vec<u8>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let votes_topic = IdentTopic::new(fractal_network::VOTES_TOPIC_STR);
    let votes_topic_hash = votes_topic.hash();
    let da_topic = IdentTopic::new(fractal_network::DA_PROVIDERS_TOPIC_STR);
    let da_topic_hash = da_topic.hash();

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
                .subscribe(&da_topic)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

            Ok(NodeBehaviour {
                sync: request_response::Behaviour::with_codec(
                    BorshSyncCodec,
                    fractal_network::sync_protocols(),
                    fractal_network::sync_request_response_config(),
                ),
                gossipsub,
            })
        })?
        .build();

    let mut ready = ready;
    let mut vote_rx = vote_wire_out;
    let mut pending_vote: Option<Vec<u8>> = None;
    let mut da_announce = tokio::time::interval(DA_PROVIDER_ANNOUNCE_INTERVAL);
    swarm.listen_on(listen)?;

    loop {
        if let Some(ref mut rx) = vote_rx {
            tokio::select! {
                ev = swarm.select_next_some() => {
                    handle_producer_swarm_event(
                        ev,
                        &mut swarm,
                        &node,
                        &mut ready,
                        &votes_topic_hash,
                        &da_topic_hash,
                    )
                    .await;
                    try_flush_vote_publish(
                        &mut swarm.behaviour_mut().gossipsub,
                        &votes_topic_hash,
                        &mut pending_vote,
                    );
                }
                wire = rx.recv() => {
                    if let Some(bytes) = wire {
                        enqueue_vote_publish(&mut pending_vote, bytes);
                        try_flush_vote_publish(
                            &mut swarm.behaviour_mut().gossipsub,
                            &votes_topic_hash,
                            &mut pending_vote,
                        );
                    }
                }
                _ = da_announce.tick() => {
                    publish_da_provider_announcement(
                        &node,
                        &mut swarm.behaviour_mut().gossipsub,
                        &da_topic_hash,
                    )
                    .await;
                }
            }
        } else {
            tokio::select! {
                ev = swarm.select_next_some() => {
                    handle_producer_swarm_event(
                        ev,
                        &mut swarm,
                        &node,
                        &mut ready,
                        &votes_topic_hash,
                        &da_topic_hash,
                    )
                    .await;
                }
                _ = da_announce.tick() => {
                    publish_da_provider_announcement(
                        &node,
                        &mut swarm.behaviour_mut().gossipsub,
                        &da_topic_hash,
                    )
                    .await;
                }
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
    da_topic_hash: &libp2p::gossipsub::TopicHash,
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
                message:
                    Message::Request {
                        request, channel, ..
                    },
                ..
            } => {
                let resp = sync_response_for_request(node, request).await;
                let _ = swarm.behaviour_mut().sync.send_response(channel, resp);
            }
            request_response::Event::InboundFailure { error, .. } => {
                eprintln!("fractal-node p2p inbound failure: {error:?}");
            }
            request_response::Event::OutboundFailure { error, .. } => {
                eprintln!("fractal-node p2p outbound failure (unexpected on producer): {error:?}");
            }
            _ => {}
        },
        SwarmEvent::Behaviour(NodeBehaviourEvent::Gossipsub(gossipsub::Event::Message {
            message,
            ..
        })) => {
            if message.topic == *votes_topic_hash {
                record_vote_from_gossip_bytes(node, &message.data).await;
            } else if message.topic == *da_topic_hash {
                // Producers subscribe too so the mesh forms; followers consume announcements.
            }
        }
        SwarmEvent::Behaviour(NodeBehaviourEvent::Gossipsub(_)) => {}
        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
            swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
            record_connected_peer_count(node, swarm.connected_peers().count()).await;
            publish_da_provider_announcement(
                node,
                &mut swarm.behaviour_mut().gossipsub,
                da_topic_hash,
            )
            .await;
        }
        SwarmEvent::ConnectionClosed { .. } => {
            record_connected_peer_count(node, swarm.connected_peers().count()).await;
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

/// Parse comma-separated DA serving peers. Unlike `FRACTAL_BOOTSTRAP`, these may be distinct peers.
pub fn parse_fractal_da_bootstraps(s: &str) -> Result<Vec<Multiaddr>, String> {
    let parts: Vec<&str> = s
        .split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect();
    if parts.is_empty() {
        return Err("FRACTAL_DA_BOOTSTRAP is empty".into());
    }
    let mut out = Vec::with_capacity(parts.len());
    for p in parts {
        let m: Multiaddr = p
            .parse()
            .map_err(|e: libp2p::multiaddr::Error| e.to_string())?;
        peer_id_from_multiaddr(&m)
            .ok_or_else(|| format!("FRACTAL_DA_BOOTSTRAP entry has no /p2p/: {m}"))?;
        out.push(m);
    }
    Ok(out)
}

/// Dial each bootstrap multiaddr (same [`PeerId`]) and pull blocks until caught up with producer tip.
pub async fn follower_network_task(
    node: NodeHandle,
    bootstraps: Vec<Multiaddr>,
    vote_wire_out: Option<UnboundedReceiver<Vec<u8>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    follower_network_task_with_da_peers(node, bootstraps, Vec::new(), vote_wire_out).await
}

/// Dial the sync producer plus optional independent DA-serving peers.
pub async fn follower_network_task_with_da_peers(
    node: NodeHandle,
    bootstraps: Vec<Multiaddr>,
    da_bootstraps: Vec<Multiaddr>,
    vote_wire_out: Option<UnboundedReceiver<Vec<u8>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if bootstraps.is_empty() {
        return Err("bootstraps is empty".into());
    }
    let producer_peer = peer_id_from_multiaddr(&bootstraps[0])
        .ok_or("FRACTAL_BOOTSTRAP multiaddr must include /p2p/<PeerId>")?;
    let votes_topic = IdentTopic::new(fractal_network::VOTES_TOPIC_STR);
    let votes_topic_hash = votes_topic.hash();
    let da_topic = IdentTopic::new(fractal_network::DA_PROVIDERS_TOPIC_STR);
    let da_topic_hash = da_topic.hash();

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
                .subscribe(&da_topic)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

            Ok(NodeBehaviour {
                sync: request_response::Behaviour::with_codec(
                    BorshSyncCodec,
                    fractal_network::sync_protocols(),
                    fractal_network::sync_request_response_config(),
                ),
                gossipsub,
            })
        })?
        .build();

    for b in &bootstraps {
        swarm.dial(b.clone())?;
    }
    for b in &da_bootstraps {
        swarm.dial(b.clone())?;
    }

    let mut poll = tokio::time::interval(Duration::from_millis(600));
    let mut connected = false;
    let mut connected_da_peers = BTreeSet::new();
    let mut outstanding = false;
    let mut pending_da_blocks: Vec<Block> = Vec::new();
    let mut pending_da_index: usize = 0;
    let mut pending_da_sample_indexes: Vec<u32> = Vec::new();
    let mut pending_da_verified_indexes: Vec<u32> = Vec::new();
    let mut pending_da_verified_shares: Vec<DaShare> = Vec::new();
    let mut pending_da_responses: usize = 0;
    let mut pending_da_expected_responses: usize = 0;
    let mut vote_rx = vote_wire_out;
    let mut pending_vote: Option<Vec<u8>> = None;

    loop {
        if let Some(ref mut rx) = vote_rx {
            tokio::select! {
                ev = swarm.select_next_some() => {
                    handle_follower_swarm_event(
                        ev,
                        &mut swarm,
                        &node,
                        producer_peer,
                        &mut connected_da_peers,
                        &mut connected,
                        &mut outstanding,
                        &mut pending_da_blocks,
                        &mut pending_da_index,
                        &mut pending_da_sample_indexes,
                        &mut pending_da_verified_indexes,
                        &mut pending_da_verified_shares,
                        &mut pending_da_responses,
                        &mut pending_da_expected_responses,
                        &votes_topic_hash,
                        &da_topic_hash,
                    )
                    .await;
                    try_flush_vote_publish(
                        &mut swarm.behaviour_mut().gossipsub,
                        &votes_topic_hash,
                        &mut pending_vote,
                    );
                }
                wire = rx.recv() => {
                    if let Some(bytes) = wire {
                        enqueue_vote_publish(&mut pending_vote, bytes);
                        try_flush_vote_publish(
                            &mut swarm.behaviour_mut().gossipsub,
                            &votes_topic_hash,
                            &mut pending_vote,
                        );
                    }
                }
                _ = poll.tick(), if connected && !outstanding => {
                    outstanding = true;
                    swarm.behaviour_mut().sync.send_request(&producer_peer, SyncRequest::GetTip);
                    try_flush_vote_publish(
                        &mut swarm.behaviour_mut().gossipsub,
                        &votes_topic_hash,
                        &mut pending_vote,
                    );
                }
            }
        } else {
            tokio::select! {
                ev = swarm.select_next_some() => {
                    handle_follower_swarm_event(
                        ev,
                        &mut swarm,
                        &node,
                        producer_peer,
                        &mut connected_da_peers,
                        &mut connected,
                        &mut outstanding,
                        &mut pending_da_blocks,
                        &mut pending_da_index,
                        &mut pending_da_sample_indexes,
                        &mut pending_da_verified_indexes,
                        &mut pending_da_verified_shares,
                        &mut pending_da_responses,
                        &mut pending_da_expected_responses,
                        &votes_topic_hash,
                        &da_topic_hash,
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

async fn handle_follower_swarm_event(
    ev: SwarmEvent<NodeBehaviourEvent>,
    swarm: &mut Swarm<NodeBehaviour>,
    node: &NodeHandle,
    producer_peer: PeerId,
    connected_da_peers: &mut BTreeSet<PeerId>,
    connected: &mut bool,
    outstanding: &mut bool,
    pending_da_blocks: &mut Vec<Block>,
    pending_da_index: &mut usize,
    pending_da_sample_indexes: &mut Vec<u32>,
    pending_da_verified_indexes: &mut Vec<u32>,
    pending_da_verified_shares: &mut Vec<DaShare>,
    pending_da_responses: &mut usize,
    pending_da_expected_responses: &mut usize,
    votes_topic_hash: &libp2p::gossipsub::TopicHash,
    da_topic_hash: &libp2p::gossipsub::TopicHash,
) {
    match ev {
        SwarmEvent::ConnectionEstablished { peer_id, .. } if peer_id == producer_peer => {
            swarm
                .behaviour_mut()
                .gossipsub
                .add_explicit_peer(&producer_peer);
            record_connected_peer_count(node, swarm.connected_peers().count()).await;
            *connected = true;
            *outstanding = true;
            swarm
                .behaviour_mut()
                .sync
                .send_request(&producer_peer, SyncRequest::GetTip);
        }
        SwarmEvent::ConnectionEstablished { .. } => {
            record_connected_peer_count(node, swarm.connected_peers().count()).await;
        }
        SwarmEvent::ConnectionClosed { peer_id, .. } => {
            connected_da_peers.remove(&peer_id);
            record_connected_peer_count(node, swarm.connected_peers().count()).await;
        }
        SwarmEvent::Behaviour(NodeBehaviourEvent::Sync(request_response::Event::Message {
            message: Message::Response { response, .. },
            ..
        })) => {
            *outstanding = false;
            match response {
                SyncResponse::Tip { height, .. } => {
                    let next = {
                        let g = node.lock().await;
                        g.height.saturating_add(1)
                    };
                    if height >= next {
                        *outstanding = true;
                        swarm.behaviour_mut().sync.send_request(
                            &producer_peer,
                            SyncRequest::GetBlocks {
                                from_height: next,
                                max_blocks: 64,
                            },
                        );
                    }
                }
                SyncResponse::Blocks(bytes) => {
                    let blocks: Vec<Block> = match borsh::from_slice(&bytes) {
                        Ok(b) => b,
                        Err(e) => {
                            eprintln!("fractal-node follower: decode blocks: {e}");
                            return;
                        }
                    };
                    if blocks.is_empty() {
                        *outstanding = true;
                        swarm
                            .behaviour_mut()
                            .sync
                            .send_request(&producer_peer, SyncRequest::GetTip);
                        return;
                    }
                    *pending_da_blocks = blocks;
                    *pending_da_index = 0;
                    request_next_da_sample(
                        swarm,
                        producer_peer,
                        connected_da_peers,
                        pending_da_blocks,
                        pending_da_index,
                        pending_da_sample_indexes,
                        pending_da_verified_indexes,
                        pending_da_verified_shares,
                        pending_da_responses,
                        pending_da_expected_responses,
                        outstanding,
                    );
                }
                SyncResponse::DaShares {
                    block_hash,
                    indexes,
                    shares,
                } => {
                    let shares: Vec<DaShare> = match borsh::from_slice(&shares) {
                        Ok(s) => s,
                        Err(e) => {
                            eprintln!("fractal-node follower: decode DA shares: {e}");
                            return;
                        }
                    };
                    let Some(block) = pending_da_blocks.get(*pending_da_index) else {
                        eprintln!("fractal-node follower: unexpected DA shares response");
                        return;
                    };
                    let expected_hash = match header_hash(&block.header) {
                        Ok(h) => h,
                        Err(e) => {
                            eprintln!("fractal-node follower: header hash for DA response: {e}");
                            return;
                        }
                    };
                    if block_hash != expected_hash {
                        eprintln!("fractal-node follower: DA shares for wrong block");
                        return;
                    }
                    if let Err(e) =
                        crate::NodeInner::verify_da_sampled_shares(block, &indexes, &shares)
                    {
                        node.lock().await.record_da_sampling_result(false);
                        eprintln!("fractal-node follower: DA sample verification failed: {e}");
                        return;
                    }
                    node.lock().await.record_da_sampling_result(true);
                    pending_da_verified_indexes.extend_from_slice(&indexes);
                    pending_da_verified_shares.extend(shares);
                    *pending_da_responses += 1;
                    if *pending_da_responses < *pending_da_expected_responses {
                        *outstanding = true;
                        return;
                    }
                    if !same_index_multiset(pending_da_sample_indexes, pending_da_verified_indexes)
                    {
                        eprintln!("fractal-node follower: incomplete DA sample response set");
                        return;
                    }
                    {
                        let mut g = node.lock().await;
                        if let Err(e) = g.apply_synced_block(block) {
                            eprintln!("fractal-node follower: apply_synced_block: {e}");
                        }
                    }
                    *pending_da_index += 1;
                    if *pending_da_index < pending_da_blocks.len() {
                        request_next_da_sample(
                            swarm,
                            producer_peer,
                            connected_da_peers,
                            pending_da_blocks,
                            pending_da_index,
                            pending_da_sample_indexes,
                            pending_da_verified_indexes,
                            pending_da_verified_shares,
                            pending_da_responses,
                            pending_da_expected_responses,
                            outstanding,
                        );
                        return;
                    }
                    pending_da_blocks.clear();
                    pending_da_sample_indexes.clear();
                    pending_da_verified_indexes.clear();
                    pending_da_verified_shares.clear();
                    *pending_da_responses = 0;
                    *pending_da_expected_responses = 0;
                    *outstanding = true;
                    swarm
                        .behaviour_mut()
                        .sync
                        .send_request(&producer_peer, SyncRequest::GetTip);
                }
                SyncResponse::ErrMsg(m) => {
                    eprintln!("fractal-node follower: peer error: {m}");
                }
            }
        }
        SwarmEvent::Behaviour(NodeBehaviourEvent::Sync(
            request_response::Event::OutboundFailure { error, .. },
        )) => {
            *outstanding = false;
            eprintln!("fractal-node follower: outbound failure: {error:?}");
        }
        SwarmEvent::Behaviour(NodeBehaviourEvent::Gossipsub(gossipsub::Event::Message {
            propagation_source,
            message,
            ..
        })) => {
            if message.topic == *votes_topic_hash {
                record_vote_from_gossip_bytes(node, &message.data).await;
            } else if message.topic == *da_topic_hash {
                match borsh::from_slice::<DaProviderAnnouncement>(&message.data) {
                    Ok(ann) => {
                        let chain_id = { node.lock().await.chain_id };
                        if ann.chain_id == chain_id && !ann.namespaces.is_empty() {
                            connected_da_peers.insert(propagation_source);
                        }
                    }
                    Err(e) => {
                        eprintln!("fractal-node follower: invalid DA provider announcement: {e}")
                    }
                }
            }
        }
        SwarmEvent::Behaviour(NodeBehaviourEvent::Gossipsub(_)) => {}
        _ => {}
    }
}

fn request_next_da_sample(
    swarm: &mut Swarm<NodeBehaviour>,
    producer_peer: PeerId,
    connected_da_peers: &BTreeSet<PeerId>,
    pending_da_blocks: &[Block],
    pending_da_index: &usize,
    pending_da_sample_indexes: &mut Vec<u32>,
    pending_da_verified_indexes: &mut Vec<u32>,
    pending_da_verified_shares: &mut Vec<DaShare>,
    pending_da_responses: &mut usize,
    pending_da_expected_responses: &mut usize,
    outstanding: &mut bool,
) {
    let Some(block) = pending_da_blocks.get(*pending_da_index) else {
        return;
    };
    let indexes = crate::NodeInner::da_sample_indexes_for_block(
        block,
        DEFAULT_NETWORK_DA_SAMPLE_COUNT.min(block.da_sidecar.shares.len()),
        block.header.height,
    );
    *pending_da_sample_indexes = indexes.clone();
    pending_da_verified_indexes.clear();
    pending_da_verified_shares.clear();
    *pending_da_responses = 0;
    let block_hash = match header_hash(&block.header) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("fractal-node follower: header hash for DA sample: {e}");
            *outstanding = false;
            return;
        }
    };
    let mut peers: Vec<PeerId> = connected_da_peers.iter().copied().collect();
    if peers.is_empty() {
        peers.push(producer_peer);
    }
    let chunks = split_indexes_across_peers(&indexes, peers.len());
    *pending_da_expected_responses = chunks.len();
    *outstanding = true;
    for (peer, indexes) in peers.into_iter().zip(chunks) {
        swarm.behaviour_mut().sync.send_request(
            &peer,
            SyncRequest::GetDaShares {
                block_hash,
                indexes,
            },
        );
    }
}

fn split_indexes_across_peers(indexes: &[u32], peer_count: usize) -> Vec<Vec<u32>> {
    if indexes.is_empty() || peer_count == 0 {
        return Vec::new();
    }
    let n = peer_count.min(indexes.len());
    let mut out = vec![Vec::new(); n];
    for (i, index) in indexes.iter().copied().enumerate() {
        out[i % n].push(index);
    }
    out
}

fn same_index_multiset(left: &[u32], right: &[u32]) -> bool {
    let mut left = left.to_vec();
    let mut right = right.to_vec();
    left.sort_unstable();
    right.sort_unstable();
    left == right
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

    #[test]
    fn parse_fractal_da_bootstraps_accepts_distinct_peers() {
        let id1 = PeerId::random();
        let id2 = PeerId::random();
        let a: Multiaddr = format!("/ip4/127.0.0.1/tcp/10001/p2p/{id1}")
            .parse()
            .unwrap();
        let b: Multiaddr = format!("/ip4/127.0.0.1/tcp/10002/p2p/{id2}")
            .parse()
            .unwrap();
        let s = format!("{a},{b}");

        let v = parse_fractal_da_bootstraps(&s).unwrap();
        assert_eq!(v, vec![a, b]);
    }

    #[test]
    fn split_indexes_across_peers_round_robins_samples() {
        let chunks = split_indexes_across_peers(&[0, 1, 2, 3, 4], 2);
        assert_eq!(chunks, vec![vec![0, 2, 4], vec![1, 3]]);
    }

    #[test]
    fn same_index_multiset_ignores_response_order() {
        assert!(same_index_multiset(&[1, 2, 1], &[2, 1, 1]));
        assert!(!same_index_multiset(&[1, 2, 1], &[1, 2]));
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
