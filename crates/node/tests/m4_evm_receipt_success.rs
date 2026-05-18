use fractal_node::NodeInner;
use fractal_rpc::ChainInteraction;

#[test]
fn evm_receipt_success_defaults_true_without_entry() {
    let n = NodeInner::devnet();
    let h = [1u8; 32];
    assert!(n.evm_receipt_success(&h));
}

#[test]
fn evm_receipt_success_reads_false_when_stored() {
    let mut n = NodeInner::devnet();
    let h = [2u8; 32];
    n.state.evm_tx_success.insert(h, false);
    assert!(!n.evm_receipt_success(&h));
}
