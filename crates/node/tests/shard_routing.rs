//! M10 shard routing: txs whose home shard ≠ this node are rejected at RPC.

use fractal_core::{Account, NativeCall, Transaction, TxBody, VmKind};
use fractal_node::NodeInner;
use fractal_rpc::ChainInteraction;
use fractal_shard::{home_shard_for_signer, ShardTopology};

#[test]
fn submit_rejects_tx_for_wrong_home_shard() {
    let mut node = NodeInner::devnet();
    node.shard_topology = ShardTopology { shard_count: 4 };
    node.shard_id = 0;

    let mut signer = [0u8; 20];
    for b in 0u8..=255 {
        signer[19] = b;
        if home_shard_for_signer(&signer, 4) != 0 {
            break;
        }
    }
    assert_ne!(
        home_shard_for_signer(&signer, 4),
        0,
        "could not find a signer routed away from shard 0"
    );

    node.state.accounts.insert(
        signer,
        Account {
            nonce: 0,
            balance: 1_000_000_000_000_000_000u128,
        },
    );

    let tx = Transaction {
        signer,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let raw = borsh::to_vec(&tx).expect("borsh");
    let err = node.submit_raw_tx(&raw).expect_err("wrong shard");
    assert!(
        err.contains("does not match") || err.contains("WrongShard"),
        "unexpected error: {err}"
    );
}

#[test]
fn submit_accepts_tx_on_home_shard() {
    let mut node = NodeInner::devnet();
    let signer = fractal_node::HARDHAT_DEFAULT_SIGNER_0;
    node.shard_topology = ShardTopology { shard_count: 4 };
    node.shard_id = home_shard_for_signer(&signer, 4);

    let tx = Transaction {
        signer,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let raw = borsh::to_vec(&tx).expect("borsh");
    node.submit_raw_tx(&raw).expect("home shard accepts");
}
