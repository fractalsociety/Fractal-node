//! QUIC + request-response: follower replays blocks and matches producer tip height.

use std::sync::Arc;
use std::time::Duration;

use fractal_node::{NodeHandle, NodeInner, p2p, producer_loop};
use libp2p::Multiaddr;
use libp2p::multiaddr::Protocol;
use tokio::sync::{Mutex, oneshot};

#[tokio::test]
async fn follower_syncs_blocks_over_quic() {
    let producer: NodeHandle = Arc::new(Mutex::new(NodeInner::devnet()));
    producer.lock().await.anchor_interval = 1;
    let listen: Multiaddr = "/ip4/127.0.0.1/udp/0/quic-v1".parse().unwrap();
    let (tx, rx) = oneshot::channel();
    let p = producer.clone();
    let _producer_p2p = tokio::spawn(async move {
        if let Err(e) = p2p::producer_network_task(p, listen, Some(tx), None, None).await {
            eprintln!("producer p2p task exited: {e:?}");
        }
    });
    let (addr, peer) = tokio::time::timeout(Duration::from_secs(15), rx)
        .await
        .expect("timeout waiting for listen")
        .expect("ready channel dropped");

    tokio::spawn(producer_loop(producer.clone()));

    let mut bootstrap = addr;
    bootstrap.push(Protocol::P2p(peer));

    let follower: NodeHandle = Arc::new(Mutex::new(NodeInner::devnet()));
    let f = follower.clone();
    let _follow_p2p = tokio::spawn(async move {
        if let Err(e) = p2p::follower_network_task(f, vec![bootstrap], None, None).await {
            eprintln!("follower p2p task exited: {e:?}");
        }
    });

    for _ in 0..60 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let ph = producer.lock().await.height;
        let fh = follower.lock().await.height;
        let pmh = producer.lock().await.masterchain_ledger.masterchain_height;
        let fmh = follower.lock().await.masterchain_ledger.masterchain_height;
        if ph >= 1 && fh == ph && pmh >= 1 && fmh == pmh {
            return;
        }
    }

    let ph = producer.lock().await.height;
    let fh = follower.lock().await.height;
    let pmh = producer.lock().await.masterchain_ledger.masterchain_height;
    let fmh = follower.lock().await.masterchain_ledger.masterchain_height;
    assert!(
        ph >= 1,
        "producer should have produced at least one block, got {ph}"
    );
    assert_eq!(
        fh, ph,
        "follower should match producer tip height (after wait): follower={fh} producer={ph}"
    );
    assert!(
        pmh >= 1,
        "producer should have sealed at least one masterchain block, got {pmh}"
    );
    assert_eq!(
        fmh, pmh,
        "follower should match producer masterchain height (after wait): follower={fmh} producer={pmh}"
    );
}
