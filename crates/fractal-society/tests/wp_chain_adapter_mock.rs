#![cfg(feature = "live-chain")]

use std::sync::Mutex;

use fractal_society::chain::fractalchain_adapter::{
    FractalChainCommitmentAdapter, FractalChainCommitmentResponse, FractalChainRpc,
    JsonRpseeFractalChainRpc, SUBMIT_PROOF_METHOD, submit_params,
};
use fractal_society::pkgs::chain_commitment::CommitmentAdapter;
use fractal_society::protocol::Hash;
use jsonrpsee::RpcModule;
use jsonrpsee::server::ServerBuilder;
use jsonrpsee::types::ErrorObjectOwned;

#[derive(Debug, Default)]
struct MockRpc {
    calls: Mutex<Vec<(String, serde_json::Value)>>,
}

impl MockRpc {
    fn calls(&self) -> Vec<(String, serde_json::Value)> {
        self.calls.lock().unwrap().clone()
    }
}

impl FractalChainRpc for MockRpc {
    fn call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> fractal_society::Result<FractalChainCommitmentResponse> {
        self.calls
            .lock()
            .unwrap()
            .push((method.to_string(), params));
        Ok(FractalChainCommitmentResponse {
            network: "fractalchain-devnet".to_string(),
            transaction_hash: "0xabc123".to_string(),
            block_number: 42,
            finalized: true,
        })
    }
}

#[test]
fn submit_returns_chain_reference_from_node_response() {
    let proof_hash = Hash::new(b"proof");
    let adapter = FractalChainCommitmentAdapter::new(MockRpc::default());

    let reference = adapter.submit(&proof_hash).unwrap();

    assert_eq!(reference.network, "fractalchain-devnet");
    assert_eq!(reference.transaction_hash, "0xabc123");
    assert_eq!(reference.block_number, 42);
    assert!(reference.finalized);
}

#[test]
fn submit_uses_expected_rpc_method_and_params() {
    let proof_hash = Hash::new(b"proof");
    let rpc = MockRpc::default();
    let adapter = FractalChainCommitmentAdapter::new(&rpc);

    adapter.submit(&proof_hash).unwrap();

    assert_eq!(
        rpc.calls(),
        vec![(SUBMIT_PROOF_METHOD.to_string(), submit_params(&proof_hash))]
    );
}

impl FractalChainRpc for &MockRpc {
    fn call(
        &self,
        method: &str,
        params: serde_json::Value,
    ) -> fractal_society::Result<FractalChainCommitmentResponse> {
        (*self).call(method, params)
    }
}

#[tokio::test]
async fn submit_round_trips_through_in_process_jsonrpsee_server() {
    let proof_hash = Hash::new(b"proof");
    let expected_hash = proof_hash.0.clone();
    let mut module = RpcModule::new(());
    module
        .register_method(SUBMIT_PROOF_METHOD, move |params, _, _| {
            let submitted_hash: String = params.one()?;
            assert_eq!(submitted_hash, expected_hash);
            Ok::<_, ErrorObjectOwned>(FractalChainCommitmentResponse {
                network: "fractalchain-devnet".to_string(),
                transaction_hash: "0xdef456".to_string(),
                block_number: 99,
                finalized: true,
            })
        })
        .unwrap();

    let server = ServerBuilder::default().build("127.0.0.1:0").await.unwrap();
    let addr = server.local_addr().unwrap();
    let handle = server.start(module);
    let adapter = FractalChainCommitmentAdapter::new(
        JsonRpseeFractalChainRpc::connect(format!("http://{addr}")).unwrap(),
    );

    let reference = tokio::task::spawn_blocking(move || adapter.submit(&proof_hash))
        .await
        .unwrap()
        .unwrap();

    assert_eq!(reference.network, "fractalchain-devnet");
    assert_eq!(reference.transaction_hash, "0xdef456");
    assert_eq!(reference.block_number, 99);
    assert!(reference.finalized);
    handle.stop().unwrap();
}
