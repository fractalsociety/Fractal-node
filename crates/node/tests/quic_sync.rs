//! QUIC + request-response: follower replays blocks and matches producer tip height.

use std::sync::Arc;
use std::time::Duration;

use fractal_node::{p2p, producer_loop, NodeHandle, NodeInner};
use libp2p::multiaddr::Protocol;
use libp2p::Multiaddr;
use tokio::sync::{oneshot, Mutex};

#[tokio::test]
async fn follower_syncs_blocks_over_quic() {
    let producer: NodeHandle = Arc::new(Mutex::new(NodeInner::devnet()));
    let listen: Multiaddr = "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap();
    let (tx, rx) = oneshot::channel();
    let p = producer.clone();
    let _producer_p2p = tokio::spawn(p2p::producer_network_task(p, listen, Some(tx), None));
    let (addr, peer) = tokio::time::timeout(Duration::from_secs(15), rx)
        .await
        .expect("timeout waiting for listen")
        .expect("ready channel dropped");

    tokio::spawn(producer_loop(producer.clone()));

    let mut bootstrap = addr;
    bootstrap.push(Protocol::P2p(peer));

    let follower: NodeHandle = Arc::new(Mutex::new(NodeInner::devnet()));
    let f = follower.clone();
    let _follow_p2p = tokio::spawn(p2p::follower_network_task(f, vec![bootstrap], None));

    for _ in 0..60 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let ph = producer.lock().await.height;
        let fh = follower.lock().await.height;
        if ph >= 1 && fh == ph {
            return;
        }
    }

    let ph = producer.lock().await.height;
    let fh = follower.lock().await.height;
    assert!(
        ph >= 1,
        "producer should have produced at least one block, got {ph}"
    );
    assert_eq!(
        fh, ph,
        "follower should match producer tip height (after wait): follower={fh} producer={ph}"
    );
}

#[tokio::test]
async fn follower_samples_da_from_independent_peer() {
    let producer: NodeHandle = Arc::new(Mutex::new(NodeInner::devnet()));
    tokio::spawn(producer_loop(producer.clone()));

    let mut mirror_started = false;
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if producer.lock().await.height >= 1 {
            mirror_started = true;
            break;
        }
    }
    assert!(
        mirror_started,
        "producer should create a block for DA mirror"
    );

    let producer_blocks = producer.lock().await.blocks.clone();
    let mut mirror_inner = NodeInner::devnet();
    for block in &producer_blocks {
        mirror_inner
            .apply_synced_block(block)
            .expect("mirror should replay producer block");
    }
    let mirror: NodeHandle = Arc::new(Mutex::new(mirror_inner));

    let producer_listen: Multiaddr = "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap();
    let (producer_tx, producer_rx) = oneshot::channel();
    let _producer_p2p = tokio::spawn(p2p::producer_network_task(
        producer.clone(),
        producer_listen,
        Some(producer_tx),
        None,
    ));
    let (producer_addr, producer_peer) = tokio::time::timeout(Duration::from_secs(15), producer_rx)
        .await
        .expect("timeout waiting for producer listen")
        .expect("producer ready channel dropped");

    let mirror_listen: Multiaddr = "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap();
    let (mirror_tx, mirror_rx) = oneshot::channel();
    let _mirror_p2p = tokio::spawn(p2p::producer_network_task(
        mirror,
        mirror_listen,
        Some(mirror_tx),
        None,
    ));
    let (mirror_addr, mirror_peer) = tokio::time::timeout(Duration::from_secs(15), mirror_rx)
        .await
        .expect("timeout waiting for mirror listen")
        .expect("mirror ready channel dropped");

    let mut sync_bootstrap = producer_addr;
    sync_bootstrap.push(Protocol::P2p(producer_peer));
    let mut da_bootstrap = mirror_addr;
    da_bootstrap.push(Protocol::P2p(mirror_peer));

    let follower: NodeHandle = Arc::new(Mutex::new(NodeInner::devnet()));
    let f = follower.clone();
    let _follow_p2p = tokio::spawn(p2p::follower_network_task_with_da_peers(
        f,
        vec![sync_bootstrap],
        vec![da_bootstrap],
        None,
    ));

    for _ in 0..80 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let fh = follower.lock().await.height;
        if fh >= 1 {
            return;
        }
    }

    assert!(
        follower.lock().await.height >= 1,
        "follower should apply a block after sampling DA from independent peer"
    );
}
