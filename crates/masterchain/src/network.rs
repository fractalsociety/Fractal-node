//! Dedicated masterchain BFT gossipsub transport.

use std::time::Duration;

use borsh::BorshDeserialize;
use fractal_network::{MASTERCHAIN_TIMEOUTS_TOPIC_STR, MASTERCHAIN_VOTES_TOPIC_STR};
use futures::StreamExt;
use libp2p::gossipsub::{self, IdentTopic};
use libp2p::{
    Multiaddr, PeerId, Swarm, SwarmBuilder,
    multiaddr::Protocol,
    swarm::{NetworkBehaviour, SwarmEvent},
};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::oneshot;

use crate::MasterchainHandle;
use crate::bft::{MasterchainTimeoutGossipV1, MasterchainVoteGossipV1};

#[derive(NetworkBehaviour)]
pub struct MasterchainNetBehaviour {
    pub gossipsub: gossipsub::Behaviour,
}

fn masterchain_gossipsub_config() -> std::io::Result<gossipsub::Config> {
    gossipsub::ConfigBuilder::default()
        .mesh_n(2)
        .mesh_n_low(1)
        .mesh_n_high(4)
        .mesh_outbound_min(0)
        .heartbeat_initial_delay(Duration::from_millis(0))
        .heartbeat_interval(Duration::from_millis(200))
        .build()
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidInput, e.to_string()))
}

pub fn parse_masterchain_bootstraps(s: &str) -> Result<Vec<Multiaddr>, String> {
    s.split(',')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .map(|p| {
            p.parse::<Multiaddr>()
                .map_err(|e: libp2p::multiaddr::Error| e.to_string())
        })
        .collect()
}

fn enqueue_publish(pending: &mut Option<Vec<u8>>, bytes: Vec<u8>) {
    *pending = Some(bytes);
}

fn try_flush_publish(
    gossip: &mut gossipsub::Behaviour,
    topic: &gossipsub::TopicHash,
    pending: &mut Option<Vec<u8>>,
    label: &str,
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
            eprintln!("fractal-masterchain gossipsub {label} publish: {e:?}");
            pending.take();
        }
    }
}

fn try_flush_all(
    gossip: &mut gossipsub::Behaviour,
    vote_topic: &gossipsub::TopicHash,
    timeout_topic: &gossipsub::TopicHash,
    pending_vote: &mut Option<Vec<u8>>,
    pending_timeout: &mut Option<Vec<u8>>,
) {
    try_flush_publish(gossip, vote_topic, pending_vote, "vote");
    try_flush_publish(gossip, timeout_topic, pending_timeout, "timeout");
}

async fn handle_vote_gossip(node: &MasterchainHandle, data: &[u8]) {
    match MasterchainVoteGossipV1::try_from_slice(data) {
        Ok(msg) => {
            let mut n = node.lock().await;
            let _ = n.ingest_vote_gossip(msg);
        }
        Err(e) => eprintln!("fractal-masterchain gossipsub: invalid vote borsh: {e}"),
    }
}

async fn handle_timeout_gossip(node: &MasterchainHandle, data: &[u8]) {
    match MasterchainTimeoutGossipV1::try_from_slice(data) {
        Ok(msg) => {
            let mut n = node.lock().await;
            let _ = n.ingest_timeout_gossip(msg);
        }
        Err(e) => eprintln!("fractal-masterchain gossipsub: invalid timeout borsh: {e}"),
    }
}

fn routable_udp_addr(address: &Multiaddr) -> bool {
    let routable_ip = !address.iter().any(|p| match p {
        Protocol::Ip4(ip) => ip.is_unspecified(),
        Protocol::Ip6(ip) => ip.is_unspecified(),
        _ => false,
    });
    routable_ip && address.iter().any(|p| matches!(p, Protocol::Udp(_)))
}

pub async fn masterchain_gossip_task(
    node: MasterchainHandle,
    listen: Multiaddr,
    bootstraps: Vec<Multiaddr>,
    ready: Option<oneshot::Sender<(Multiaddr, PeerId)>>,
    vote_wire_out: Option<UnboundedReceiver<Vec<u8>>>,
    timeout_wire_out: Option<UnboundedReceiver<Vec<u8>>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let vote_topic = IdentTopic::new(MASTERCHAIN_VOTES_TOPIC_STR);
    let vote_topic_hash = vote_topic.hash();
    let timeout_topic = IdentTopic::new(MASTERCHAIN_TIMEOUTS_TOPIC_STR);
    let timeout_topic_hash = timeout_topic.hash();
    let keypair = libp2p::identity::Keypair::generate_ed25519();
    let mut swarm: Swarm<MasterchainNetBehaviour> = SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_quic()
        .with_behaviour(|key| {
            let gossipsub = gossipsub::Behaviour::new(
                gossipsub::MessageAuthenticity::Signed(key.clone()),
                masterchain_gossipsub_config()?,
            )
            .map_err(std::io::Error::other)?;
            let mut gossipsub = gossipsub;
            gossipsub
                .subscribe(&vote_topic)
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            gossipsub
                .subscribe(&timeout_topic)
                .map_err(|e| std::io::Error::other(e.to_string()))?;
            Ok(MasterchainNetBehaviour { gossipsub })
        })?
        .build();

    swarm.listen_on(listen)?;
    for peer in bootstraps {
        swarm.dial(peer)?;
    }

    let mut ready = ready;
    let mut vote_rx = vote_wire_out;
    let mut timeout_rx = timeout_wire_out;
    let mut pending_vote: Option<Vec<u8>> = None;
    let mut pending_timeout: Option<Vec<u8>> = None;
    let mut flush = tokio::time::interval(Duration::from_millis(200));

    loop {
        match (&mut vote_rx, &mut timeout_rx) {
            (Some(vr), Some(tr)) => {
                tokio::select! {
                    ev = swarm.select_next_some() => {
                        handle_swarm_event(ev, &mut swarm, &node, &mut ready, &vote_topic_hash, &timeout_topic_hash).await;
                        try_flush_all(&mut swarm.behaviour_mut().gossipsub, &vote_topic_hash, &timeout_topic_hash, &mut pending_vote, &mut pending_timeout);
                    }
                    opt = vr.recv() => {
                        if let Some(bytes) = opt {
                            enqueue_publish(&mut pending_vote, bytes);
                            try_flush_all(&mut swarm.behaviour_mut().gossipsub, &vote_topic_hash, &timeout_topic_hash, &mut pending_vote, &mut pending_timeout);
                        }
                    }
                    opt = tr.recv() => {
                        if let Some(bytes) = opt {
                            enqueue_publish(&mut pending_timeout, bytes);
                            try_flush_all(&mut swarm.behaviour_mut().gossipsub, &vote_topic_hash, &timeout_topic_hash, &mut pending_vote, &mut pending_timeout);
                        }
                    }
                    _ = flush.tick() => {
                        try_flush_all(&mut swarm.behaviour_mut().gossipsub, &vote_topic_hash, &timeout_topic_hash, &mut pending_vote, &mut pending_timeout);
                    }
                }
            }
            (Some(vr), None) => {
                tokio::select! {
                    ev = swarm.select_next_some() => {
                        handle_swarm_event(ev, &mut swarm, &node, &mut ready, &vote_topic_hash, &timeout_topic_hash).await;
                        try_flush_all(&mut swarm.behaviour_mut().gossipsub, &vote_topic_hash, &timeout_topic_hash, &mut pending_vote, &mut pending_timeout);
                    }
                    opt = vr.recv() => {
                        if let Some(bytes) = opt {
                            enqueue_publish(&mut pending_vote, bytes);
                            try_flush_all(&mut swarm.behaviour_mut().gossipsub, &vote_topic_hash, &timeout_topic_hash, &mut pending_vote, &mut pending_timeout);
                        }
                    }
                }
            }
            (None, Some(tr)) => {
                tokio::select! {
                    ev = swarm.select_next_some() => {
                        handle_swarm_event(ev, &mut swarm, &node, &mut ready, &vote_topic_hash, &timeout_topic_hash).await;
                        try_flush_all(&mut swarm.behaviour_mut().gossipsub, &vote_topic_hash, &timeout_topic_hash, &mut pending_vote, &mut pending_timeout);
                    }
                    opt = tr.recv() => {
                        if let Some(bytes) = opt {
                            enqueue_publish(&mut pending_timeout, bytes);
                            try_flush_all(&mut swarm.behaviour_mut().gossipsub, &vote_topic_hash, &timeout_topic_hash, &mut pending_vote, &mut pending_timeout);
                        }
                    }
                }
            }
            (None, None) => {
                let ev = swarm.select_next_some().await;
                handle_swarm_event(
                    ev,
                    &mut swarm,
                    &node,
                    &mut ready,
                    &vote_topic_hash,
                    &timeout_topic_hash,
                )
                .await;
                try_flush_all(
                    &mut swarm.behaviour_mut().gossipsub,
                    &vote_topic_hash,
                    &timeout_topic_hash,
                    &mut pending_vote,
                    &mut pending_timeout,
                );
            }
        }
    }
}

async fn handle_swarm_event(
    ev: SwarmEvent<MasterchainNetBehaviourEvent>,
    swarm: &mut Swarm<MasterchainNetBehaviour>,
    node: &MasterchainHandle,
    ready: &mut Option<oneshot::Sender<(Multiaddr, PeerId)>>,
    vote_topic_hash: &gossipsub::TopicHash,
    timeout_topic_hash: &gossipsub::TopicHash,
) {
    match ev {
        SwarmEvent::NewListenAddr { address, .. } => {
            eprintln!(
                "fractal-masterchain gossipsub: listening on {}/p2p/{}",
                address,
                swarm.local_peer_id()
            );
            if routable_udp_addr(&address) {
                if let Some(tx) = ready.take() {
                    let _ = tx.send((address, *swarm.local_peer_id()));
                }
            }
        }
        SwarmEvent::ConnectionEstablished { peer_id, .. } => {
            swarm.behaviour_mut().gossipsub.add_explicit_peer(&peer_id);
        }
        SwarmEvent::Behaviour(MasterchainNetBehaviourEvent::Gossipsub(
            gossipsub::Event::Message { message, .. },
        )) => {
            if message.topic == *vote_topic_hash {
                handle_vote_gossip(node, &message.data).await;
            } else if message.topic == *timeout_topic_hash {
                handle_timeout_gossip(node, &message.data).await;
            }
        }
        _ => {}
    }
}
