//! Gossipsub vote wire (`docs/prd.md` §18 M7-d-5): two Swarms on QUIC exchange `Vote` on
//! [`fractal_network::VOTES_TOPIC_STR`] while block sync runs on request-response.

use std::sync::Arc;
use std::time::Duration;

use fractal_consensus::{header_hash, ValidatorSet};
use fractal_node::{p2p, producer_loop, NodeHandle, NodeInner};
use libp2p::multiaddr::Protocol;
use libp2p::Multiaddr;
use tokio::sync::{oneshot, Mutex};

#[tokio::test]
async fn producer_and_follower_exchange_votes_over_gossipsub() {
    let validators = ValidatorSet::phase2_bft7_fixture();

    let (prod_vote_tx, prod_vote_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut prod_inner = NodeInner::devnet_with_validator_index(validators.clone(), 0);
    prod_inner.set_vote_sink(Some(prod_vote_tx));
    let producer: NodeHandle = Arc::new(Mutex::new(prod_inner));

    let (fol_vote_tx, fol_vote_rx) = tokio::sync::mpsc::unbounded_channel();
    let mut fol_inner = NodeInner::devnet_with_validator_index(validators, 1);
    fol_inner.set_vote_sink(Some(fol_vote_tx));
    let follower: NodeHandle = Arc::new(Mutex::new(fol_inner));

    let listen: Multiaddr = "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap();
    let (tx, rx) = oneshot::channel();
    let p = producer.clone();
    let _producer_p2p = tokio::spawn(p2p::producer_network_task(
        p,
        listen,
        Some(tx),
        Some(prod_vote_rx),
    ));

    let (addr, peer) = tokio::time::timeout(Duration::from_secs(15), rx)
        .await
        .expect("timeout waiting for listen")
        .expect("ready channel dropped");

    tokio::spawn(producer_loop(producer.clone()));

    let mut bootstrap = addr;
    bootstrap.push(Protocol::P2p(peer));

    let f = follower.clone();
    let _follow_p2p = tokio::spawn(p2p::follower_network_task(
        f,
        vec![bootstrap],
        Some(fol_vote_rx),
    ));

    for _ in 0..120 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let ph = producer.lock().await.height;
        let fh = follower.lock().await.height;
        if ph < 1 || fh != ph {
            continue;
        }
        let (view, hh) = {
            let g = producer.lock().await;
            let tip = &g.blocks[(ph - 1) as usize];
            let hh = header_hash(&tip.header).expect("header hash");
            (tip.header.view, hh)
        };
        let pc = producer.lock().await.vote_pool.count(view, hh);
        let fc = follower.lock().await.vote_pool.count(view, hh);
        if pc >= 2 && fc >= 2 {
            return;
        }
    }

    let ph = producer.lock().await.height;
    let fh = follower.lock().await.height;
    let mut fc = 0;
    let mut pc = 0;
    let mut view = 0u64;
    if ph >= 1 {
        let g = producer.lock().await;
        let tip = &g.blocks[(ph - 1) as usize];
        let hh = header_hash(&tip.header).unwrap_or([0u8; 32]);
        view = tip.header.view;
        pc = g.vote_pool.count(view, hh);
        fc = follower.lock().await.vote_pool.count(view, hh);
    }
    panic!(
        "expected both nodes to collect votes from each other on gossipsub: producer_height={ph} follower_height={fh} tip_view={view} vote_pool_counts producer={pc} follower={fc}"
    );
}
