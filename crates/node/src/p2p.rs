//! libp2p QUIC + request-response block sync (PRD §18 M2) + gossipsub votes (M7-d-5).

use std::path::{Path, PathBuf};
use std::time::Duration;

use fractal_consensus::{Block, Vote};
use fractal_network::{BorshSyncCodec, SyncRequest, SyncResponse};
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
            }
        } else {
            let ev = swarm.select_next_some().await;
            handle_producer_swarm_event(ev, &mut swarm, &node, &mut ready, &votes_topic_hash).await;
        }
    }
}

async fn handle_producer_swarm_event(
    ev: SwarmEvent<NodeBehaviourEvent>,
    swarm: &mut Swarm<NodeBehaviour>,
    node: &NodeHandle,
    ready: &mut Option<oneshot::Sender<(Multiaddr, PeerId)>>,
    votes_topic_hash: &libp2p::gossipsub::TopicHash,
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
                        request,
                        channel,
                        ..
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
        SwarmEvent::Behaviour(NodeBehaviourEvent::Gossipsub(
            gossipsub::Event::Message { message, .. },
        )) => {
            if message.topic == *votes_topic_hash {
                record_vote_from_gossip_bytes(node, &message.data).await;
            }
        }
        SwarmEvent::Behaviour(NodeBehaviourEvent::Gossipsub(_)) => {}
        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
            swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
        }
        SwarmEvent::IncomingConnection { .. }
        | SwarmEvent::IncomingConnectionError { .. }
        | SwarmEvent::ConnectionClosed { .. } => {}
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
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if bootstraps.is_empty() {
        return Err("bootstraps is empty".into());
    }
    let producer_peer = peer_id_from_multiaddr(&bootstraps[0])
        .ok_or("FRACTAL_BOOTSTRAP multiaddr must include /p2p/<PeerId>")?;

    let votes_topic = IdentTopic::new(fractal_network::VOTES_TOPIC_STR);
    let votes_topic_hash = votes_topic.hash();

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

    let mut poll = tokio::time::interval(Duration::from_millis(600));
    let mut connected = false;
    let mut outstanding = false;
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
                        &mut connected,
                        &mut outstanding,
                        &votes_topic_hash,
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
                        &mut connected,
                        &mut outstanding,
                        &votes_topic_hash,
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
    connected: &mut bool,
    outstanding: &mut bool,
    votes_topic_hash: &libp2p::gossipsub::TopicHash,
) {
    match ev {
        SwarmEvent::ConnectionEstablished { peer_id, .. } if peer_id == producer_peer => {
            swarm.behaviour_mut().gossipsub.add_explicit_peer(&producer_peer);
            *connected = true;
            *outstanding = true;
            swarm
                .behaviour_mut()
                .sync
                .send_request(&producer_peer, SyncRequest::GetTip);
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
                    for b in &blocks {
                        let mut g = node.lock().await;
                        if let Err(e) = g.apply_synced_block(b) {
                            eprintln!("fractal-node follower: apply_synced_block: {e}");
                        }
                    }
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
        SwarmEvent::Behaviour(NodeBehaviourEvent::Sync(request_response::Event::OutboundFailure {
            error,
            ..
        })) => {
            *outstanding = false;
            eprintln!("fractal-node follower: outbound failure: {error:?}");
        }
        SwarmEvent::Behaviour(NodeBehaviourEvent::Gossipsub(
            gossipsub::Event::Message { message, .. },
        )) => {
            if message.topic == *votes_topic_hash {
                record_vote_from_gossip_bytes(node, &message.data).await;
            }
        }
        SwarmEvent::Behaviour(NodeBehaviourEvent::Gossipsub(_)) => {}
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
