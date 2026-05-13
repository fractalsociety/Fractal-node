//! libp2p QUIC + request-response block sync (PRD §18 M2).

use std::time::Duration;

use fractal_consensus::Block;
use fractal_network::{
    BorshSyncCodec, SyncRequest, SyncResponse,
};
use futures::StreamExt;
use libp2p::{
    multiaddr::Protocol,
    request_response::{self, Message},
    swarm::{NetworkBehaviour, SwarmEvent},
    Multiaddr, PeerId, Swarm, SwarmBuilder,
};
use tokio::sync::oneshot;

use crate::NodeHandle;

#[derive(NetworkBehaviour)]
pub struct NodeBehaviour {
    pub sync: request_response::Behaviour<BorshSyncCodec>,
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

/// QUIC listener for sync; answers [`SyncRequest`] against `node`.
pub async fn producer_network_task(
    node: NodeHandle,
    listen: Multiaddr,
    ready: Option<oneshot::Sender<(Multiaddr, PeerId)>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let mut swarm: Swarm<NodeBehaviour> = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_quic()
        .with_behaviour(|_key| {
            Ok(NodeBehaviour {
                sync: request_response::Behaviour::with_codec(
                    BorshSyncCodec,
                    fractal_network::sync_protocols(),
                    fractal_network::sync_request_response_config(),
                ),
            })
        })?
        .build();

    let mut ready = ready;
    swarm.listen_on(listen)?;

    loop {
        match swarm.select_next_some().await {
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
                    let resp = sync_response_for_request(&node, request).await;
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
            SwarmEvent::IncomingConnection { .. }
            | SwarmEvent::IncomingConnectionError { .. }
            | SwarmEvent::ConnectionClosed { .. } => {}
            _ => {}
        }
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
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if bootstraps.is_empty() {
        return Err("bootstraps is empty".into());
    }
    let producer_peer = peer_id_from_multiaddr(&bootstraps[0])
        .ok_or("FRACTAL_BOOTSTRAP multiaddr must include /p2p/<PeerId>")?;

    let mut swarm: Swarm<NodeBehaviour> = SwarmBuilder::with_new_identity()
        .with_tokio()
        .with_quic()
        .with_behaviour(|_key| {
            Ok(NodeBehaviour {
                sync: request_response::Behaviour::with_codec(
                    BorshSyncCodec,
                    fractal_network::sync_protocols(),
                    fractal_network::sync_request_response_config(),
                ),
            })
        })?
        .build();

    for b in &bootstraps {
        swarm.dial(b.clone())?;
    }

    let mut poll = tokio::time::interval(Duration::from_millis(600));
    let mut connected = false;
    // Avoid overlapping request-response RPCs (out-of-order Tips vs Blocks can duplicate work).
    let mut outstanding = false;

    loop {
        tokio::select! {
            ev = swarm.select_next_some() => {
                match ev {
                    SwarmEvent::ConnectionEstablished { peer_id, .. } if peer_id == producer_peer => {
                        connected = true;
                        outstanding = true;
                        swarm.behaviour_mut().sync.send_request(&producer_peer, SyncRequest::GetTip);
                    }
                    SwarmEvent::Behaviour(NodeBehaviourEvent::Sync(request_response::Event::Message {
                        message: Message::Response { response, .. },
                        ..
                    })) => {
                        outstanding = false;
                        match response {
                            SyncResponse::Tip { height, .. } => {
                                let next = {
                                    let g = node.lock().await;
                                    g.height.saturating_add(1)
                                };
                                if height >= next {
                                    outstanding = true;
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
                                let blocks: Vec<Block> =
                                    borsh::from_slice(&bytes).map_err(|e| {
                                        std::io::Error::new(std::io::ErrorKind::InvalidData, e)
                                    })?;
                                if blocks.is_empty() {
                                    outstanding = true;
                                    swarm.behaviour_mut().sync.send_request(
                                        &producer_peer,
                                        SyncRequest::GetTip,
                                    );
                                    continue;
                                }
                                for b in &blocks {
                                    let mut g = node.lock().await;
                                    g.apply_synced_block(b)?;
                                }
                                outstanding = true;
                                swarm.behaviour_mut().sync.send_request(
                                    &producer_peer,
                                    SyncRequest::GetTip,
                                );
                            }
                            SyncResponse::ErrMsg(m) => {
                                eprintln!("fractal-node follower: peer error: {m}");
                            }
                        }
                    }
                    SwarmEvent::Behaviour(NodeBehaviourEvent::Sync(request_response::Event::OutboundFailure {
                        error, ..
                    })) => {
                        outstanding = false;
                        eprintln!("fractal-node follower: outbound failure: {error:?}");
                    }
                    _ => {}
                }
            }
            _ = poll.tick(), if connected && !outstanding => {
                outstanding = true;
                swarm.behaviour_mut().sync.send_request(&producer_peer, SyncRequest::GetTip);
            }
        }
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
