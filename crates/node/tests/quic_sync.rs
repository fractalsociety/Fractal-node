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
    let _producer_p2p = tokio::spawn(p2p::producer_network_task(p, listen, Some(tx)));
    let (addr, peer) = tokio::time::timeout(Duration::from_secs(15), rx)
        .await
        .expect("timeout waiting for listen")
        .expect("ready channel dropped");

    tokio::spawn(producer_loop(producer.clone()));

    let mut bootstrap = addr;
    bootstrap.push(Protocol::P2p(peer));

    let follower: NodeHandle = Arc::new(Mutex::new(NodeInner::devnet()));
    let f = follower.clone();
    let _follow_p2p = tokio::spawn(p2p::follower_network_task(f, vec![bootstrap]));

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
    assert!(ph >= 1, "producer should have produced at least one block, got {ph}");
    assert_eq!(
        fh, ph,
        "follower should match producer tip height (after wait): follower={fh} producer={ph}"
    );
}
