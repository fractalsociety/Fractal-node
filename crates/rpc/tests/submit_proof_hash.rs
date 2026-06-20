use std::sync::Arc;

use fractal_core::Address;
use fractal_rpc::{
    build_module, ChainInteraction, LogsFilter, ProofCommitmentResponse, RpcChainConfig,
    RpcConsensusDiagnostics, RpcDaMetrics, RpcLog, RpcMempoolLaneMetrics, RpcProofMetrics,
    SharedChain,
};
use tokio::sync::Mutex;

#[derive(Default)]
struct MockChain {
    block_number: u64,
    chain_id: u64,
    commitments: Vec<([u8; 32], u64)>,
}

impl MockChain {
    fn new(block_number: u64, chain_id: u64) -> Self {
        Self {
            block_number,
            chain_id,
            commitments: Vec::new(),
        }
    }
}

impl ChainInteraction for MockChain {
    fn block_number(&self) -> u64 {
        self.block_number
    }

    fn chain_id(&self) -> u64 {
        self.chain_id
    }

    fn balance_of(&self, _addr: &Address) -> u128 {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn transaction_count(&self, _addr: &Address) -> u64 {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn submit_raw_tx(&mut self, _raw: &[u8]) -> Result<(), String> {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn base_fee_per_gas(&self) -> u128 {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn block_hash_by_number(&self, _number: u64) -> Option<[u8; 32]> {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn block_by_hash(&self, _hash: &[u8; 32]) -> Option<fractal_consensus::Block> {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn tx_by_hash(&self, _hash: &[u8; 32]) -> Option<fractal_core::Transaction> {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn mined_tx_info(&self, _hash: &[u8; 32]) -> Option<(u64, [u8; 32], u32)> {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn eth_signed_raw(&self, _tx_hash: &[u8; 32]) -> Option<Vec<u8>> {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn simulate_eth_call(
        &self,
        _from: Address,
        _to: Option<Address>,
        _value: u128,
        _data: Vec<u8>,
    ) -> Result<Vec<u8>, fractal_core::ExecError> {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn estimate_eth_gas(
        &self,
        _from: Address,
        _to: Option<Address>,
        _value: u128,
        _data: Vec<u8>,
    ) -> Result<u64, fractal_core::ExecError> {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn code_at(&self, _addr: &Address) -> Vec<u8> {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn storage_at(&self, _addr: &Address, _slot: [u8; 32]) -> [u8; 32] {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn gas_used_for_tx(&self, _tx_hash: &[u8; 32]) -> Option<u64> {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn evm_receipt_success(&self, _tx_hash: &[u8; 32]) -> bool {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn logs_for_filter(&self, _filter: &LogsFilter) -> Vec<RpcLog> {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn receipt_rpc_logs(
        &self,
        _tx_hash: &[u8; 32],
        _block_number: u64,
        _block_hash: &[u8; 32],
        _tx_index: u32,
    ) -> (Vec<RpcLog>, [u8; 256]) {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn logs_bloom_for_block(&self, _block: &fractal_consensus::Block) -> [u8; 256] {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn da_metrics(&self) -> RpcDaMetrics {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn da_fee_revenue(&self) -> u128 {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn proof_metrics(&self) -> RpcProofMetrics {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn consensus_diagnostics(&self) -> RpcConsensusDiagnostics {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn mempool_lane_metrics(&self) -> RpcMempoolLaneMetrics {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn chain_config(&self) -> RpcChainConfig {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn submit_validity_proof(
        &mut self,
        _proof: fractal_consensus::BlockValidityProof,
    ) -> Result<[u8; 32], String> {
        unimplemented!("not used by fractal_submitProofHash")
    }

    fn submit_proof_hash(
        &mut self,
        proof_hash: [u8; 32],
    ) -> Result<ProofCommitmentResponse, String> {
        self.commitments.push((proof_hash, self.block_number));
        Ok(ProofCommitmentResponse {
            network: format!("fractalchain-{}", self.chain_id),
            transaction_hash: format!("0x{}", hex::encode(fractal_crypto::sha256(&proof_hash))),
            block_number: self.block_number,
            finalized: true,
        })
    }
}

#[tokio::test]
async fn submit_proof_hash_accepts_object_params() {
    let ctx: SharedChain = Arc::new(Mutex::new(MockChain::new(12, 41)));
    let module = build_module(ctx);
    let proof_hash = format!("0x{}", "ab".repeat(32));

    let response: ProofCommitmentResponse = module
        .call(
            "fractal_submitProofHash",
            [serde_json::json!({ "proof_hash": proof_hash })],
        )
        .await
        .unwrap();

    assert_eq!(response.network, "fractalchain-41");
    assert!(response.transaction_hash.starts_with("0x"));
    assert_eq!(response.transaction_hash.len(), 66);
    assert_eq!(response.block_number, 12);
    assert!(response.finalized);
}

#[tokio::test]
async fn submit_proof_hash_accepts_adapter_positional_params() {
    let ctx: SharedChain = Arc::new(Mutex::new(MockChain::new(14, 41)));
    let module = build_module(ctx);
    let proof_hash = format!("0x{}", "cd".repeat(32));

    let response: ProofCommitmentResponse = module
        .call("fractal_submitProofHash", [proof_hash])
        .await
        .unwrap();

    assert_eq!(response.network, "fractalchain-41");
    assert_eq!(response.block_number, 14);
    assert!(response.finalized);
}
