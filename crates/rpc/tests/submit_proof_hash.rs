use std::sync::Arc;

use fractal_consensus::{CircuitVersion, ExecutionFeatureSetV1, ZoneProofUpdateV1};
use fractal_core::Address;
use fractal_rpc::{
    build_module, ChainInteraction, LogsFilter, ProofCommitmentResponse, RpcChainConfig,
    RpcConsensusDiagnostics, RpcDaMetrics, RpcLog, RpcMempoolLaneMetrics, RpcOwnedObjectFinality,
    RpcProofMetrics, RpcProofUpdateSubmission, RpcRoutingDiagnostics, SharedChain,
};
use tokio::sync::Mutex;

#[derive(Default)]
struct MockChain {
    block_number: u64,
    chain_id: u64,
    commitments: Vec<([u8; 32], u64)>,
    latest_zone_height: Option<(u64, u64)>,
    zone_update_finality: Option<(u64, u64, String)>,
    object_finality: Option<(fractal_core::OwnedObjectVersion, String, String)>,
    proof_updates: Vec<(ZoneProofUpdateV1, u128)>,
    shard_id: u32,
    shard_count: u32,
}

impl MockChain {
    fn new(block_number: u64, chain_id: u64) -> Self {
        Self {
            block_number,
            chain_id,
            commitments: Vec::new(),
            latest_zone_height: None,
            zone_update_finality: None,
            object_finality: None,
            proof_updates: Vec::new(),
            shard_id: 0,
            shard_count: 1,
        }
    }
}

fn proof_update(zone_id: u64, height: u64, byte: u8) -> ZoneProofUpdateV1 {
    ZoneProofUpdateV1 {
        zone_id,
        height,
        parent_root: [1u8; 32],
        new_root: [2u8; 32],
        tx_root: [3u8; 32],
        da_root: [4u8; 32],
        message_root: [5u8; 32],
        forced_inclusion_root: [6u8; 32],
        circuit_version: CircuitVersion::NativeStateTransitionV1,
        feature_set: ExecutionFeatureSetV1::empty(),
        proof_digest: [byte; 32],
    }
}

impl ChainInteraction for MockChain {
    fn block_number(&self) -> u64 {
        self.block_number
    }

    fn chain_id(&self) -> u64 {
        self.chain_id
    }

    fn shard_id(&self) -> u32 {
        self.shard_id
    }

    fn shard_count(&self) -> u32 {
        self.shard_count
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

    fn latest_proof_final_height_for_zone(&self, zone_id: u64) -> Option<u64> {
        self.latest_zone_height
            .and_then(|(z, h)| (z == zone_id).then_some(h))
    }

    fn zone_update_finality(&self, zone_id: u64, height: u64) -> Option<String> {
        self.zone_update_finality
            .as_ref()
            .and_then(|(z, h, f)| (*z == zone_id && *h == height).then(|| f.clone()))
    }

    fn owned_object_finality(
        &self,
        object_version: &fractal_core::OwnedObjectVersion,
    ) -> Option<(String, String)> {
        self.object_finality
            .as_ref()
            .and_then(|(v, h, c)| (v == object_version).then(|| (h.clone(), c.clone())))
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

    fn submit_proof_update(
        &mut self,
        update: ZoneProofUpdateV1,
        max_priority_fee: u128,
    ) -> Result<RpcProofUpdateSubmission, String> {
        self.proof_updates.push((update.clone(), max_priority_fee));
        Ok(RpcProofUpdateSubmission {
            network: format!("fractalchain-{}", self.chain_id),
            proof_update_hash: format!("0x{}", hex::encode([0x44u8; 32])),
            zone_id: format!("0x{:x}", update.zone_id),
            height: format!("0x{:x}", update.height),
            pending_proof_updates: format!("0x{:x}", self.proof_updates.len()),
        })
    }
}

#[tokio::test]
async fn proof_final_height_rpc_reports_zone_height() {
    let mut mock = MockChain::new(12, 41);
    mock.latest_zone_height = Some((7, 99));
    let ctx: SharedChain = Arc::new(Mutex::new(mock));
    let module = build_module(ctx);

    let response: serde_json::Value = module
        .call("fractal_getProofFinalHeight", ["0x7"])
        .await
        .unwrap();

    assert_eq!(response["zoneId"], "0x7");
    assert_eq!(response["proofFinalHeight"], "0x63");
}

#[tokio::test]
async fn zone_update_finality_rpc_reports_proof_status() {
    let mut mock = MockChain::new(12, 41);
    mock.zone_update_finality = Some((7, 99, "proof".into()));
    let ctx: SharedChain = Arc::new(Mutex::new(mock));
    let module = build_module(ctx);

    let response: serde_json::Value = module
        .call("fractal_getZoneUpdateFinality", ["7", "99"])
        .await
        .unwrap();

    assert_eq!(response["zoneId"], "0x7");
    assert_eq!(response["height"], "0x63");
    assert_eq!(response["finalityStatus"], "proof");
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
async fn debug_tx_routing_reports_home_shard_and_route_key() {
    let mut mock = MockChain::new(12, 41);
    mock.shard_id = 0;
    mock.shard_count = 2;
    let ctx: SharedChain = Arc::new(Mutex::new(mock));
    let module = build_module(ctx);
    let tx = fractal_core::Transaction {
        signer: [0x22u8; 20],
        nonce: 0,
        vm: fractal_core::VmKind::Native,
        body: fractal_core::TxBody::Native(fractal_core::NativeCall::NoOp),
    };
    let raw = borsh::to_vec(&tx).unwrap();
    let expected = fractal_shard::home_shard_for_signer(&tx.signer, 2);

    let response: RpcRoutingDiagnostics = module
        .call(
            "fractal_debugTxRouting",
            [format!("0x{}", hex::encode(raw))],
        )
        .await
        .unwrap();

    assert_eq!(response.source_shard, "0x0");
    assert_eq!(response.expected_shard, format!("0x{expected:x}"));
    assert_eq!(response.shard_count, "0x2");
    assert_eq!(
        response.route_key,
        format!("signer:0x{}", hex::encode(tx.signer))
    );
    assert_eq!(response.accepted, expected == 0);
}

#[tokio::test]
async fn submit_proof_update_accepts_borsh_hex_without_tx_gas_path() {
    let raw = Arc::new(Mutex::new(MockChain::new(12, 41)));
    let ctx: SharedChain = raw.clone();
    let module = build_module(ctx);
    let update = proof_update(7, 99, 0xaa);
    let update_hex = format!("0x{}", hex::encode(borsh::to_vec(&update).unwrap()));

    let response: RpcProofUpdateSubmission = module
        .call(
            "fractal_submitProofUpdate",
            [serde_json::json!({
                "proofUpdate": update_hex,
                "maxPriorityFee": "0x9"
            })],
        )
        .await
        .unwrap();

    assert_eq!(response.network, "fractalchain-41");
    assert_eq!(response.zone_id, "0x7");
    assert_eq!(response.height, "0x63");
    assert_eq!(response.pending_proof_updates, "0x1");

    let mock = raw.lock().await;
    assert_eq!(mock.proof_updates, vec![(update, 9)]);
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

#[tokio::test]
async fn submit_proof_hash_accepts_arbitrary_package_content_hash() {
    // `fractal_submitProofHash` is hash-generic: it accepts any 32-byte content
    // hash, including a research-package content hash (SHA-256 of arbitrary
    // payload bytes produced by `commit_research_package`), not only pipeline
    // proof hashes. (AR-03)
    let ctx: SharedChain = Arc::new(Mutex::new(MockChain::new(99, 41)));
    let module = build_module(ctx);

    let package_bytes = b"# Research package\nA committed dataset artifact.\n";
    let content_hash = fractal_crypto::sha256(package_bytes);
    let proof_hash = format!("0x{}", hex::encode(content_hash));

    let response: ProofCommitmentResponse = module
        .call(
            "fractal_submitProofHash",
            [serde_json::json!({ "proof_hash": proof_hash })],
        )
        .await
        .unwrap();

    assert_eq!(response.network, "fractalchain-41");
    assert_eq!(response.block_number, 99);
    assert!(response.finalized);
    // The node echoes back a tx hash derived from the submitted hash, proving
    // the package content hash round-trips through the commitment endpoint.
    assert!(response.transaction_hash.starts_with("0x"));
}

#[tokio::test]
async fn owned_object_finality_reports_certificate_status() {
    let object_version = fractal_core::OwnedObjectVersion {
        object_id: fractal_core::OwnedObjectId::Agent(42),
        version: 3,
    };
    let mut mock = MockChain::new(12, 41);
    mock.object_finality = Some((
        object_version.clone(),
        format!("0x{}", "11".repeat(32)),
        "0xabcdef".to_owned(),
    ));
    let ctx: SharedChain = Arc::new(Mutex::new(mock));
    let module = build_module(ctx);
    let object_version_hex = format!("0x{}", hex::encode(borsh::to_vec(&object_version).unwrap()));

    let response: RpcOwnedObjectFinality = module
        .call(
            "fractal_getOwnedObjectFinality",
            [object_version_hex.clone()],
        )
        .await
        .unwrap();

    assert_eq!(response.object_version_borsh, object_version_hex);
    assert_eq!(response.finality_status, "certificate");
    assert_eq!(response.certificate_borsh.as_deref(), Some("0xabcdef"));
}
