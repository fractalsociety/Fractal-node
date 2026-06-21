use fractal_core::{NativeCall, Transaction, TxBody, VmKind};
use fractal_node::{NodeInner, HARDHAT_DEFAULT_SIGNER_0};
use fractal_rpc::ChainInteraction;

fn noop_tx(signer: [u8; 20]) -> Transaction {
    Transaction {
        signer,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    }
}

#[test]
fn submit_raw_tx_wrong_shard_error_includes_route_details() {
    let tx = noop_tx(HARDHAT_DEFAULT_SIGNER_0);
    let expected = fractal_shard::home_shard_for_signer(&tx.signer, 2);
    let wrong = (expected + 1) % 2;
    let mut node = NodeInner::devnet();
    node.shard_count = 2;
    node.shard_id = wrong;

    let raw = borsh::to_vec(&tx).unwrap();
    let err = node.submit_raw_tx(&raw).unwrap_err();

    assert!(err.contains(&format!("source_shard=0x{wrong:x}")), "{err}");
    assert!(
        err.contains(&format!("expected_shard=0x{expected:x}")),
        "{err}"
    );
    assert!(err.contains("shard_count=0x2"), "{err}");
    assert!(
        err.contains(&format!("route_key=signer:0x{}", hex::encode(tx.signer))),
        "{err}"
    );
    assert_eq!(node.mempool.len(), 0);
}

#[test]
fn raw_tx_routing_diagnostics_reports_acceptance_without_inserting() {
    let tx = noop_tx(HARDHAT_DEFAULT_SIGNER_0);
    let expected = fractal_shard::home_shard_for_signer(&tx.signer, 2);
    let mut node = NodeInner::devnet();
    node.shard_count = 2;
    node.shard_id = expected;
    let raw = borsh::to_vec(&tx).unwrap();

    let diagnostics = node.routing_diagnostics_for_raw_tx(&raw).unwrap();

    assert_eq!(diagnostics.source_shard, format!("0x{expected:x}"));
    assert_eq!(diagnostics.expected_shard, format!("0x{expected:x}"));
    assert!(diagnostics.accepted);
    assert_eq!(node.mempool.len(), 0);
}
