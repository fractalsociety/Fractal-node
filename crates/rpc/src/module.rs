use std::net::SocketAddr;
use std::sync::Arc;

use borsh::BorshDeserialize;
use fractal_core::{Address, OwnedObjectId, TxExecutionScope};
use fractal_crypto::hash::keccak256;
use http::Method;
use jsonrpsee::server::{ServerBuilder, ServerHandle};
use jsonrpsee::types::{ErrorObjectOwned, Params};
use jsonrpsee::RpcModule;
use serde::Serialize;
use tokio::sync::Mutex;
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};

pub(crate) fn err_invalid_params(msg: &'static str) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(-32602, msg, None::<()>)
}

fn exec_error_to_rpc(e: fractal_core::ExecError) -> ErrorObjectOwned {
    match e {
        fractal_core::ExecError::EvmRevert { return_data } => {
            let data_hex = format!("0x{}", hex::encode(return_data));
            ErrorObjectOwned::owned(
                3,
                "execution reverted",
                Some(serde_json::Value::String(data_hex)),
            )
        }
        other => ErrorObjectOwned::owned(-32000, other.to_string(), None::<()>),
    }
}

fn u256_quantity_hex(v: u128) -> String {
    format!("0x{:x}", v)
}

fn parse_address_hex(s: &str) -> Result<Address, ErrorObjectOwned> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s).map_err(|_| err_invalid_params("invalid address hex"))?;
    if bytes.len() != 20 {
        return Err(err_invalid_params("address must be 20 bytes"));
    }
    let mut a = [0u8; 20];
    a.copy_from_slice(&bytes);
    Ok(a)
}

pub(crate) fn parse_hash256_hex(s: &str) -> Result<[u8; 32], ErrorObjectOwned> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s).map_err(|_| err_invalid_params("invalid hash hex"))?;
    if bytes.len() != 32 {
        return Err(err_invalid_params("hash must be 32 bytes"));
    }
    let mut h = [0u8; 32];
    h.copy_from_slice(&bytes);
    Ok(h)
}

fn quantity_hex_u64(v: u64) -> String {
    format!("0x{:x}", v)
}

fn circuit_version_label(version: fractal_consensus::CircuitVersion) -> String {
    match version {
        fractal_consensus::CircuitVersion::DevMixedV1 => "dev_mixed_v1",
        fractal_consensus::CircuitVersion::NativeStateTransitionV1 => "native_state_transition_v1",
        fractal_consensus::CircuitVersion::MixedStateTransitionV1 => "mixed_state_transition_v1",
    }
    .to_owned()
}

fn hash_hex(h: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(h))
}

fn addr_hex(a: &Address) -> String {
    format!("0x{}", hex::encode(a))
}

fn parse_u256_hex_u128(s: &str) -> Result<u128, ErrorObjectOwned> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.len() > 32 {
        return Err(err_invalid_params(
            "value too large (max 128-bit in devnet)",
        ));
    }
    u128::from_str_radix(if s.is_empty() { "0" } else { s }, 16)
        .map_err(|_| err_invalid_params("invalid quantity"))
}

fn parse_bytes_hex(s: &str) -> Result<Vec<u8>, ErrorObjectOwned> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    hex::decode(s).map_err(|_| err_invalid_params("invalid bytes hex"))
}

/// `eth_call` / `eth_estimateGas` transaction object (ethers may send only one top-level param).
#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct EthCallObject {
    #[serde(default)]
    from: Option<String>,
    /// Omitted or null for contract-creation gas estimation (`eth_estimateGas` / some `eth_call` paths).
    #[serde(default)]
    to: Option<String>,
    #[serde(default)]
    data: Option<String>,
    #[serde(default)]
    value: Option<String>,
}

fn parse_eth_call_params(
    params: Params<'static>,
) -> Result<(Address, Option<Address>, u128, Vec<u8>, String), ErrorObjectOwned> {
    let vs: Vec<serde_json::Value> = params
        .parse()
        .map_err(|_| err_invalid_params("expected [callObject] or [callObject, blockTag]"))?;
    if vs.is_empty() {
        return Err(err_invalid_params("empty params"));
    }
    let obj: EthCallObject = serde_json::from_value(vs[0].clone())
        .map_err(|_| err_invalid_params("invalid call object"))?;
    let tag = vs
        .get(1)
        .and_then(|v| {
            if v.is_null() {
                None
            } else {
                v.as_str().map(String::from)
            }
        })
        .unwrap_or_else(|| "latest".into());
    let from = obj
        .from
        .as_deref()
        .map(parse_address_hex)
        .transpose()?
        .unwrap_or([0u8; 20]);
    let to = match obj.to.as_deref() {
        None | Some("") | Some("0x") | Some("0X") => None,
        Some(s) => Some(parse_address_hex(s)?),
    };
    let data = obj
        .data
        .as_deref()
        .map(parse_bytes_hex)
        .transpose()?
        .unwrap_or_default();
    let value = obj
        .value
        .as_deref()
        .map(parse_u256_hex_u128)
        .transpose()?
        .unwrap_or(0);
    Ok((from, to, value, data, tag))
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RpcBlock {
    number: String,
    hash: String,
    parent_hash: String,
    nonce: String,
    sha3_uncles: String,
    logs_bloom: String,
    transactions_root: String,
    state_root: String,
    receipts_root: String,
    zone_namespace: String,
    da_root: String,
    da_bytes: String,
    da_share_count: String,
    da_gas_used: String,
    da_fee_paid: String,
    miner: String,
    difficulty: String,
    total_difficulty: String,
    extra_data: String,
    size: String,
    gas_limit: String,
    gas_used: String,
    timestamp: String,
    /// Post-London field; required for ethers.js / Hardhat to pick EIP-1559 txs.
    base_fee_per_gas: String,
    finality_status: String,
    proof_circuit_version: Option<String>,
    proof_coverage_manifest_digest: Option<String>,
    proof_covered_features: Option<String>,
    transactions: Vec<String>,
    uncles: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcDaMetrics {
    pub committed_blocks: String,
    pub committed_original_bytes: String,
    pub committed_encoded_bytes: String,
    pub committed_da_gas: String,
    pub da_fee_revenue: String,
    pub sampling_success: String,
    pub sampling_failure: String,
    pub reconstruction_success: String,
    pub reconstruction_failure: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcProofRejectionMetric {
    pub reason: String,
    pub count: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcProofMetrics {
    pub proofs_accepted: String,
    pub proofs_rejected: String,
    pub witness_gen_latency_ms: String,
    pub latest_proof_latency_ms: String,
    pub latest_proof_final_lag_ms: String,
    pub average_proof_latency_ms: String,
    pub proof_final_height: String,
    pub unsupported_feature_rejections: String,
    pub latest_rejection_reason: Option<String>,
    pub rejection_reasons: Vec<RpcProofRejectionMetric>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcMempoolLaneMetrics {
    pub pending_total: String,
    pub pending_owned: String,
    pub pending_mixed: String,
    pub pending_consensus: String,
    pub pending_consensus_lane: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcTxScope {
    pub tx_hash: String,
    pub lane: String,
    pub certificate_eligible: bool,
    pub mixed: bool,
    pub owner: Option<String>,
    pub owned_objects: Vec<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcChainConfig {
    pub proof_required_settlement: bool,
    pub native_transition_proofs_enabled: bool,
    pub proofs_required_for_settlement: String,
    pub owned_object_certificates: bool,
    pub da_sampling: bool,
    pub proof_final_settlement: bool,
    pub execution_zones: bool,
    pub forced_inclusion: bool,
    pub prover_rewards: bool,
    pub sequencer_rewards: bool,
    pub settlement_finality: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcProofSubmission {
    pub block_hash: String,
    pub finality_status: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcSettlementBlock {
    pub block_hash: String,
    pub block_number: String,
    pub finality_status: String,
    pub proof_circuit_version: Option<String>,
    pub proof_coverage_manifest_digest: Option<String>,
    pub proof_covered_features: Option<String>,
    pub settlement_allowed: bool,
    pub proof_required_settlement: bool,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RpcTx {
    hash: String,
    nonce: String,
    from: String,
    to: Option<String>,
    value: String,
    input: String,
    gas: String,
    gas_price: String,
    block_hash: Option<String>,
    block_number: Option<String>,
    transaction_index: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RpcReceipt {
    transaction_hash: String,
    transaction_index: String,
    block_hash: String,
    block_number: String,
    from: String,
    to: Option<String>,
    cumulative_gas_used: String,
    gas_used: String,
    contract_address: Option<String>,
    logs: Vec<RpcLog>,
    logs_bloom: String,
    status: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RpcFeeHistory {
    oldest_block: String,
    base_fee_per_gas: Vec<String>,
    gas_used_ratio: Vec<f64>,
    reward: Option<Vec<Vec<String>>>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcLog {
    pub address: String,
    pub topics: Vec<String>,
    pub data: String,
    pub block_hash: String,
    pub block_number: String,
    pub transaction_hash: String,
    pub transaction_index: String,
    pub log_index: String,
    pub removed: bool,
}

/// `eth_getLogs` filter after JSON-RPC parsing (`addresses == None` means any contract).
#[derive(Clone, Debug, Default)]
pub struct LogsFilter {
    pub from_block: u64,
    pub to_block: u64,
    pub addresses: Option<Vec<Address>>,
    pub topic_filters: Vec<Option<TopicMatch>>,
}

/// One indexed topic position in the filter (`eth_getLogs` `topics` array).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TopicMatch {
    Exact([u8; 32]),
    AnyOf(Vec<[u8; 32]>),
}

/// Whether `log` satisfies `topic_filters` (same rules as Ethereum JSON-RPC `topics`).
pub fn evm_log_matches_topic_filters(
    log: &fractal_core::EvmLog,
    topic_filters: &[Option<TopicMatch>],
) -> bool {
    for (i, slot) in topic_filters.iter().enumerate() {
        let Some(tm) = slot else {
            continue;
        };
        let Some(log_topic) = log.topics.get(i) else {
            return false;
        };
        match tm {
            TopicMatch::Exact(h) => {
                if log_topic != h {
                    return false;
                }
            }
            TopicMatch::AnyOf(hs) => {
                if !hs.iter().any(|h| h == log_topic) {
                    return false;
                }
            }
        }
    }
    true
}

fn parse_topic_filters(
    topics: Option<Vec<serde_json::Value>>,
) -> Result<Vec<Option<TopicMatch>>, ErrorObjectOwned> {
    let Some(rows) = topics else {
        return Ok(Vec::new());
    };
    let mut out = Vec::with_capacity(rows.len());
    for val in rows {
        match val {
            serde_json::Value::Null => out.push(None),
            serde_json::Value::String(s) => {
                let h = parse_hash256_hex(&s)?;
                out.push(Some(TopicMatch::Exact(h)));
            }
            serde_json::Value::Array(items) => {
                if items.is_empty() {
                    return Err(err_invalid_params("empty topics OR list"));
                }
                let mut hs = Vec::with_capacity(items.len());
                for it in items {
                    let serde_json::Value::String(s) = it else {
                        return Err(err_invalid_params(
                            "topic OR list must contain only hex strings",
                        ));
                    };
                    hs.push(parse_hash256_hex(&s)?);
                }
                out.push(Some(TopicMatch::AnyOf(hs)));
            }
            _ => return Err(err_invalid_params("invalid topic filter entry")),
        }
    }
    Ok(out)
}

fn parse_filter_addresses(
    v: Option<serde_json::Value>,
) -> Result<Option<Vec<Address>>, ErrorObjectOwned> {
    match v {
        None => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(vec![parse_address_hex(&s)?])),
        Some(serde_json::Value::Array(a)) => {
            let mut out = Vec::with_capacity(a.len());
            for x in a {
                let serde_json::Value::String(s) = x else {
                    return Err(err_invalid_params(
                        "address filter must be string or array of strings",
                    ));
                };
                out.push(parse_address_hex(&s)?);
            }
            Ok(Some(out))
        }
        _ => Err(err_invalid_params(
            "address must be string or array of strings",
        )),
    }
}

fn parse_block_quantity_or_tag(s: &str, latest: u64) -> Result<u64, ErrorObjectOwned> {
    match s {
        "latest" | "pending" => Ok(latest),
        "earliest" => Ok(1),
        s if s.starts_with("0x") => u64::from_str_radix(s.strip_prefix("0x").unwrap_or(s), 16)
            .map_err(|_| err_invalid_params("invalid block quantity hex")),
        _ => Err(err_invalid_params("unsupported block tag")),
    }
}

/// Ethereum 2048-bit logs bloom (same construction as go-ethereum `core/types/bloom9.go`).
pub fn logs_bloom_256(evm_logs: &[fractal_core::EvmLog]) -> [u8; 256] {
    let mut bloom = [0u8; 256];
    for log in evm_logs {
        bloom_add(&mut bloom, &log.address);
        for t in &log.topics {
            bloom_add(&mut bloom, t);
        }
    }
    bloom
}

fn bloom_add(bloom: &mut [u8; 256], data: &[u8]) {
    let h = keccak256(data);
    let v1 = 1u8 << (h[1] & 0x7);
    let v2 = 1u8 << (h[3] & 0x7);
    let v3 = 1u8 << (h[5] & 0x7);
    let u16be = |a: usize| u16::from_be_bytes([h[a], h[a + 1]]);
    let idx = |pair_start: usize| -> usize {
        256usize - (((u16be(pair_start) & 0x7ff) >> 3) as usize) - 1
    };
    let i1 = idx(0);
    let i2 = idx(2);
    let i3 = idx(4);
    bloom[i1] |= v1;
    bloom[i2] |= v2;
    bloom[i3] |= v3;
}

/// `0x` + 512 hex chars (256 bytes).
pub fn logs_bloom_hex(bloom: &[u8; 256]) -> String {
    format!("0x{}", hex::encode(bloom))
}

/// Minimal chain surface for JSON-RPC (implemented by `fractal-node`).
pub trait ChainInteraction: Send {
    fn block_number(&self) -> u64;

    fn chain_id(&self) -> u64;

    fn balance_of(&self, addr: &Address) -> u128;

    fn transaction_count(&self, addr: &Address) -> u64;

    /// Hex is `0x` + raw **borsh** `Transaction` bytes (dev stub until RLP exists).
    fn submit_raw_tx(&mut self, raw: &[u8]) -> Result<(), String>;

    fn base_fee_per_gas(&self) -> u128;

    fn block_hash_by_number(&self, number: u64) -> Option<[u8; 32]>;

    fn block_by_hash(&self, hash: &[u8; 32]) -> Option<fractal_consensus::Block>;

    fn block_is_proof_final(&self, _hash: &[u8; 32]) -> bool {
        false
    }

    fn proof_for_block(&self, _hash: &[u8; 32]) -> Option<fractal_consensus::BlockValidityProof> {
        None
    }

    fn settlement_requires_proof_for_features(
        &self,
        _features: fractal_consensus::ExecutionFeatureSetV1,
    ) -> bool {
        self.chain_config().proof_required_settlement
    }

    fn settlement_finality_for_block_hash(&self, hash: &[u8; 32]) -> Result<(), String> {
        let Some(block) = self.block_by_hash(hash) else {
            return Err("block not found".into());
        };
        let requires = self.settlement_requires_proof_for_features(block.header.feature_set);
        if requires && !self.block_is_proof_final(hash) {
            return Err("block is not proof-final".into());
        }
        Ok(())
    }

    fn tx_by_hash(&self, hash: &[u8; 32]) -> Option<fractal_core::Transaction>;

    fn mined_tx_info(&self, hash: &[u8; 32]) -> Option<(u64, [u8; 32], u32)>;

    /// Signed EIP-1559 bytes for this RPC tx hash, if known (Hardhat / MetaMask).
    fn eth_signed_raw(&self, tx_hash: &[u8; 32]) -> Option<Vec<u8>>;

    fn simulate_eth_call(
        &self,
        from: Address,
        to: Option<Address>,
        value: u128,
        data: Vec<u8>,
    ) -> Result<Vec<u8>, fractal_core::ExecError>;

    fn estimate_eth_gas(
        &self,
        from: Address,
        to: Option<Address>,
        value: u128,
        data: Vec<u8>,
    ) -> Result<u64, fractal_core::ExecError>;

    fn code_at(&self, addr: &Address) -> Vec<u8>;

    fn storage_at(&self, addr: &Address, slot: [u8; 32]) -> [u8; 32];

    fn gas_used_for_tx(&self, tx_hash: &[u8; 32]) -> Option<u64>;

    /// `false` only when a mined EVM tx explicitly failed (reserved); default success for native / legacy.
    fn evm_receipt_success(&self, tx_hash: &[u8; 32]) -> bool;

    fn logs_for_filter(&self, filter: &LogsFilter) -> Vec<RpcLog>;

    /// Logs for `eth_getTransactionReceipt`, with `logIndex` as index within the block,
    /// plus Ethereum `logsBloom` bits for those logs.
    fn receipt_rpc_logs(
        &self,
        tx_hash: &[u8; 32],
        block_number: u64,
        block_hash: &[u8; 32],
        tx_index: u32,
    ) -> (Vec<RpcLog>, [u8; 256]);

    /// Bitwise OR of each mined tx receipt bloom in `block` (from stored execution logs).
    fn logs_bloom_for_block(&self, block: &fractal_consensus::Block) -> [u8; 256];

    fn da_metrics(&self) -> RpcDaMetrics;

    fn da_fee_revenue(&self) -> u128;

    fn proof_metrics(&self) -> RpcProofMetrics;

    fn mempool_lane_metrics(&self) -> RpcMempoolLaneMetrics;

    fn chain_config(&self) -> RpcChainConfig;

    fn submit_validity_proof(
        &mut self,
        proof: fractal_consensus::BlockValidityProof,
    ) -> Result<[u8; 32], String>;
}

pub type SharedChain = Arc<Mutex<dyn ChainInteraction + Send>>;

pub fn build_module(ctx: SharedChain) -> RpcModule<SharedChain> {
    let mut module = RpcModule::new(ctx.clone());

    module
        .register_async_method(
            "eth_syncing",
            |_params: Params<'static>, _ctx, _| async move { Ok::<bool, ErrorObjectOwned>(false) },
        )
        .expect("register eth_syncing");

    module
        .register_async_method(
            "web3_clientVersion",
            |_params: Params<'static>, _ctx, _| async move {
                Ok::<String, ErrorObjectOwned>("FractalChain/v0.1.0".into())
            },
        )
        .expect("register web3_clientVersion");

    module
        .register_async_method("eth_chainId", |_params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                let g = ctx.lock().await;
                Ok::<String, ErrorObjectOwned>(format!("0x{:x}", g.chain_id()))
            }
        })
        .expect("register eth_chainId");

    module
        .register_async_method("net_version", |_params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                let g = ctx.lock().await;
                Ok::<String, ErrorObjectOwned>(format!("{}", g.chain_id()))
            }
        })
        .expect("register net_version");

    module
        .register_async_method("fractal_daMetrics", |_params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                let g = ctx.lock().await;
                Ok::<RpcDaMetrics, ErrorObjectOwned>(g.da_metrics())
            }
        })
        .expect("register fractal_daMetrics");

    module
        .register_async_method(
            "fractal_mempoolLaneMetrics",
            |_params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let g = ctx.lock().await;
                    Ok::<RpcMempoolLaneMetrics, ErrorObjectOwned>(g.mempool_lane_metrics())
                }
            },
        )
        .expect("register fractal_mempoolLaneMetrics");

    module
        .register_async_method(
            "fractal_debugTxScope",
            |params: Params<'static>, _ctx, _| async move {
                let tx_hex: String = params
                    .one()
                    .map_err(|_| err_invalid_params("expected borsh transaction hex"))?;
                let tx_bytes = parse_bytes_hex(&tx_hex)?;
                let tx = fractal_core::Transaction::try_from_slice(&tx_bytes)
                    .map_err(|_| err_invalid_params("invalid Transaction borsh"))?;
                Ok::<RpcTxScope, ErrorObjectOwned>(rpc_tx_scope(&tx, &keccak256(&tx_bytes)))
            },
        )
        .expect("register fractal_debugTxScope");

    module
        .register_async_method(
            "fractal_proofMetrics",
            |_params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let g = ctx.lock().await;
                    Ok::<RpcProofMetrics, ErrorObjectOwned>(g.proof_metrics())
                }
            },
        )
        .expect("register fractal_proofMetrics");

    module
        .register_async_method("fractal_chainConfig", |_params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                let g = ctx.lock().await;
                Ok::<RpcChainConfig, ErrorObjectOwned>(g.chain_config())
            }
        })
        .expect("register fractal_chainConfig");

    module
        .register_async_method(
            "fractal_submitValidityProof",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let proof_hex: String = params
                        .one()
                        .map_err(|_| err_invalid_params("expected borsh proof hex"))?;
                    let proof_bytes = parse_bytes_hex(&proof_hex)?;
                    let proof = fractal_consensus::BlockValidityProof::try_from_slice(&proof_bytes)
                        .map_err(|_| err_invalid_params("invalid BlockValidityProof borsh"))?;
                    let mut g = ctx.lock().await;
                    let block_hash = g
                        .submit_validity_proof(proof)
                        .map_err(|e| ErrorObjectOwned::owned(-32000, e, None::<()>))?;
                    Ok::<RpcProofSubmission, ErrorObjectOwned>(RpcProofSubmission {
                        block_hash: hash_hex(&block_hash),
                        finality_status: "proof".into(),
                    })
                }
            },
        )
        .expect("register fractal_submitValidityProof");

    module
        .register_async_method(
            "fractal_getSettlementBlock",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let hash_hex_param: String = params
                        .one()
                        .map_err(|_| err_invalid_params("expected block hash"))?;
                    let hash = parse_hash256_hex(&hash_hex_param)?;
                    let g = ctx.lock().await;
                    let Some(block) = g.block_by_hash(&hash) else {
                        return Err(ErrorObjectOwned::owned(
                            -32001,
                            "block not found",
                            None::<()>,
                        ));
                    };
                    let proof_final = g.block_is_proof_final(&hash);
                    let proof = g.proof_for_block(&hash);
                    let proof_required =
                        g.settlement_requires_proof_for_features(block.header.feature_set);
                    if let Err(e) = g.settlement_finality_for_block_hash(&hash) {
                        let (code, label) = match e.as_str() {
                            "block is not proof-final" => (-32010, "soft-final"),
                            "block proof circuit does not cover requested settlement features" => {
                                (-32011, "uncovered-circuit")
                            }
                            "block data availability is unavailable" => (-32012, "unavailable-da"),
                            _ => (-32001, "block-not-found"),
                        };
                        return Err(ErrorObjectOwned::owned(code, label, Some(e)));
                    }
                    Ok::<RpcSettlementBlock, ErrorObjectOwned>(RpcSettlementBlock {
                        block_hash: hash_hex(&hash),
                        block_number: quantity_hex_u64(block.header.height),
                        finality_status: if proof_final { "proof" } else { "soft" }.into(),
                        proof_circuit_version: proof
                            .as_ref()
                            .map(|p| circuit_version_label(p.circuit_version)),
                        proof_coverage_manifest_digest: proof
                            .as_ref()
                            .map(|p| hash_hex(&p.coverage_manifest_digest)),
                        proof_covered_features: proof.as_ref().map(|p| {
                            let manifest = fractal_consensus::coverage_manifest_for_circuit_version(
                                p.circuit_version,
                            );
                            quantity_hex_u64(manifest.covered_features.bits)
                        }),
                        settlement_allowed: !proof_required || proof_final,
                        proof_required_settlement: proof_required,
                    })
                }
            },
        )
        .expect("register fractal_getSettlementBlock");

    module
        .register_async_method(
            "fractal_daFeeRevenue",
            |_params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let g = ctx.lock().await;
                    Ok::<String, ErrorObjectOwned>(u256_quantity_hex(g.da_fee_revenue()))
                }
            },
        )
        .expect("register fractal_daFeeRevenue");

    module
        .register_async_method("eth_blockNumber", |_params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                let g = ctx.lock().await;
                Ok::<String, ErrorObjectOwned>(quantity_hex_u64(g.block_number()))
            }
        })
        .expect("register eth_blockNumber");

    module
        .register_async_method("eth_getBlockByNumber", |params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                let (tag, _full): (String, bool) = params
                    .parse()
                    .map_err(|_| err_invalid_params("expected [blockTag, fullTxObjects]"))?;
                let g = ctx.lock().await;
                let number = if tag == "latest" {
                    g.block_number()
                } else if let Some(hex) = tag.strip_prefix("0x") {
                    u64::from_str_radix(hex, 16)
                        .map_err(|_| err_invalid_params("invalid block number"))?
                } else {
                    return Err(err_invalid_params("unsupported blockTag"));
                };
                let h = g.block_hash_by_number(number).ok_or_else(|| {
                    ErrorObjectOwned::owned(-32000, "block not found", None::<()>)
                })?;
                let b = g.block_by_hash(&h).ok_or_else(|| {
                    ErrorObjectOwned::owned(-32000, "block not found", None::<()>)
                })?;
                let lb = g.logs_bloom_for_block(&b);
                Ok::<RpcBlock, ErrorObjectOwned>(rpc_block_from_consensus(
                    &b,
                    Some(h),
                    lb,
                    g.base_fee_per_gas(),
                    g.block_is_proof_final(&h),
                    g.proof_for_block(&h),
                ))
            }
        })
        .expect("register eth_getBlockByNumber");

    module
        .register_async_method("eth_getBlockByHash", |params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                let (hash_hex, _full): (String, bool) = params
                    .parse()
                    .map_err(|_| err_invalid_params("expected [blockHash, fullTxObjects]"))?;
                let h = parse_hash256_hex(&hash_hex)?;
                let g = ctx.lock().await;
                let b = g.block_by_hash(&h).ok_or_else(|| {
                    ErrorObjectOwned::owned(-32000, "block not found", None::<()>)
                })?;
                let lb = g.logs_bloom_for_block(&b);
                Ok::<RpcBlock, ErrorObjectOwned>(rpc_block_from_consensus(
                    &b,
                    Some(h),
                    lb,
                    g.base_fee_per_gas(),
                    g.block_is_proof_final(&h),
                    g.proof_for_block(&h),
                ))
            }
        })
        .expect("register eth_getBlockByHash");

    module
        .register_async_method(
            "eth_getBlockTransactionCountByNumber",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let tag: String = params
                        .one()
                        .map_err(|_| err_invalid_params("expected blockTag"))?;
                    let g = ctx.lock().await;
                    let number = if tag == "latest" {
                        g.block_number()
                    } else if let Some(hex) = tag.strip_prefix("0x") {
                        u64::from_str_radix(hex, 16)
                            .map_err(|_| err_invalid_params("invalid block number"))?
                    } else {
                        return Err(err_invalid_params("unsupported blockTag"));
                    };
                    let h = g.block_hash_by_number(number).ok_or_else(|| {
                        ErrorObjectOwned::owned(-32000, "block not found", None::<()>)
                    })?;
                    let b = g.block_by_hash(&h).ok_or_else(|| {
                        ErrorObjectOwned::owned(-32000, "block not found", None::<()>)
                    })?;
                    Ok::<String, ErrorObjectOwned>(quantity_hex_u64(b.transactions.len() as u64))
                }
            },
        )
        .expect("register eth_getBlockTransactionCountByNumber");

    module
        .register_async_method(
            "eth_getBlockTransactionCountByHash",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let hash_hex: String = params
                        .one()
                        .map_err(|_| err_invalid_params("expected block hash"))?;
                    let h = parse_hash256_hex(&hash_hex)?;
                    let g = ctx.lock().await;
                    let b = g.block_by_hash(&h).ok_or_else(|| {
                        ErrorObjectOwned::owned(-32000, "block not found", None::<()>)
                    })?;
                    Ok::<String, ErrorObjectOwned>(quantity_hex_u64(b.transactions.len() as u64))
                }
            },
        )
        .expect("register eth_getBlockTransactionCountByHash");

    module
        .register_async_method(
            "eth_getTransactionByBlockHashAndIndex",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let (block_hash_hex, idx_hex): (String, String) = params
                        .parse()
                        .map_err(|_| err_invalid_params("expected [blockHash, index]"))?;
                    let bh = parse_hash256_hex(&block_hash_hex)?;
                    let idx = if let Some(hex) = idx_hex.strip_prefix("0x") {
                        u64::from_str_radix(hex, 16)
                            .map_err(|_| err_invalid_params("invalid index"))?
                    } else {
                        return Err(err_invalid_params("index must be hex quantity"));
                    };
                    let g = ctx.lock().await;
                    let b = match g.block_by_hash(&bh) {
                        Some(b) => b,
                        None => return Ok::<Option<RpcTx>, ErrorObjectOwned>(None),
                    };
                    let tx = match b.transactions.get(idx as usize) {
                        Some(t) => t.clone(),
                        None => return Ok::<Option<RpcTx>, ErrorObjectOwned>(None),
                    };
                    let raw = borsh::to_vec(&tx).map_err(|_| {
                        ErrorObjectOwned::owned(-32000, "tx encode failed", None::<()>)
                    })?;
                    let th = keccak256(&raw);
                    let mined = Some((b.header.height, bh, idx as u32));
                    Ok::<Option<RpcTx>, ErrorObjectOwned>(Some(rpc_tx_from_core(
                        &tx,
                        &th,
                        mined,
                        g.base_fee_per_gas(),
                    )))
                }
            },
        )
        .expect("register eth_getTransactionByBlockHashAndIndex");

    module
        .register_async_method(
            "eth_getTransactionByBlockNumberAndIndex",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let (tag, idx_hex): (String, String) = params
                        .parse()
                        .map_err(|_| err_invalid_params("expected [blockTag, index]"))?;
                    let idx = if let Some(hex) = idx_hex.strip_prefix("0x") {
                        u64::from_str_radix(hex, 16)
                            .map_err(|_| err_invalid_params("invalid index"))?
                    } else {
                        return Err(err_invalid_params("index must be hex quantity"));
                    };
                    let g = ctx.lock().await;
                    let number = if tag == "latest" {
                        g.block_number()
                    } else if let Some(hex) = tag.strip_prefix("0x") {
                        u64::from_str_radix(hex, 16)
                            .map_err(|_| err_invalid_params("invalid block number"))?
                    } else {
                        return Err(err_invalid_params("unsupported blockTag"));
                    };
                    let bh = match g.block_hash_by_number(number) {
                        Some(h) => h,
                        None => return Ok::<Option<RpcTx>, ErrorObjectOwned>(None),
                    };
                    let b = match g.block_by_hash(&bh) {
                        Some(b) => b,
                        None => return Ok::<Option<RpcTx>, ErrorObjectOwned>(None),
                    };
                    let tx = match b.transactions.get(idx as usize) {
                        Some(t) => t.clone(),
                        None => return Ok::<Option<RpcTx>, ErrorObjectOwned>(None),
                    };
                    let raw = borsh::to_vec(&tx).map_err(|_| {
                        ErrorObjectOwned::owned(-32000, "tx encode failed", None::<()>)
                    })?;
                    let th = keccak256(&raw);
                    let mined = Some((b.header.height, bh, idx as u32));
                    Ok::<Option<RpcTx>, ErrorObjectOwned>(Some(rpc_tx_from_core(
                        &tx,
                        &th,
                        mined,
                        g.base_fee_per_gas(),
                    )))
                }
            },
        )
        .expect("register eth_getTransactionByBlockNumberAndIndex");

    module
        .register_async_method(
            "eth_getTransactionByHash",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let hash_hex: String = params
                        .one()
                        .map_err(|_| err_invalid_params("expected tx hash"))?;
                    let h = parse_hash256_hex(&hash_hex)?;
                    let g = ctx.lock().await;
                    let tx = match g.tx_by_hash(&h) {
                        Some(t) => t,
                        None => return Ok(serde_json::Value::Null),
                    };
                    let mined = g.mined_tx_info(&h);
                    if let Some(raw) = g.eth_signed_raw(&h) {
                        let v = fractal_eth_wire::eip1559_signed_tx_to_json(&raw, mined).map_err(
                            |e| {
                                ErrorObjectOwned::owned(
                                    -32000,
                                    format!("eth tx decode: {e}"),
                                    None::<()>,
                                )
                            },
                        )?;
                        return Ok(v);
                    }
                    serde_json::to_value(rpc_tx_from_core(&tx, &h, mined, g.base_fee_per_gas()))
                        .map_err(|_| ErrorObjectOwned::owned(-32000, "serialize tx", None::<()>))
                }
            },
        )
        .expect("register eth_getTransactionByHash");

    module
        .register_async_method(
            "eth_getTransactionReceipt",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let hash_hex: String = params
                        .one()
                        .map_err(|_| err_invalid_params("expected tx hash"))?;
                    let h = parse_hash256_hex(&hash_hex)?;
                    let g = ctx.lock().await;
                    let tx = match g.tx_by_hash(&h) {
                        Some(t) => t,
                        None => return Ok::<Option<RpcReceipt>, ErrorObjectOwned>(None),
                    };
                    let Some((bn, bh, idx)) = g.mined_tx_info(&h) else {
                        return Ok::<Option<RpcReceipt>, ErrorObjectOwned>(None);
                    };
                    let gas_used = g
                        .gas_used_for_tx(&h)
                        .unwrap_or_else(|| fractal_core::intrinsic_gas(&tx).unwrap_or(0));
                    let (logs, logs_bloom) = g.receipt_rpc_logs(&h, bn, &bh, idx);
                    let receipt_ok = g.evm_receipt_success(&h);
                    Ok::<Option<RpcReceipt>, ErrorObjectOwned>(Some(rpc_receipt_from_core(
                        &tx, &h, bn, &bh, idx, gas_used, logs, logs_bloom, receipt_ok,
                    )))
                }
            },
        )
        .expect("register eth_getTransactionReceipt");

    module
        .register_async_method("eth_getBalance", |params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                let (addr_hex, _tag): (String, String) = params
                    .parse()
                    .map_err(|_| err_invalid_params("expected [address, blockTag]"))?;
                let addr = parse_address_hex(&addr_hex)?;
                let g = ctx.lock().await;
                let b = g.balance_of(&addr);
                Ok::<String, ErrorObjectOwned>(u256_quantity_hex(b))
            }
        })
        .expect("register eth_getBalance");

    module
        .register_async_method("eth_getCode", |params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                let (addr_hex, _tag): (String, String) = params
                    .parse()
                    .map_err(|_| err_invalid_params("expected [address, blockTag]"))?;
                let addr = parse_address_hex(&addr_hex)?;
                let g = ctx.lock().await;
                let code = g.code_at(&addr);
                Ok::<String, ErrorObjectOwned>(format!("0x{}", hex::encode(code)))
            }
        })
        .expect("register eth_getCode");

    module
        .register_async_method(
            "eth_getStorageAt",
            |params: Params<'static>, _ctx, _| async move {
                // Devnet: reads from `State.evm_storage` (slot -> value).
                let (addr_hex, pos_hex, _tag): (String, String, String) = params
                    .parse()
                    .map_err(|_| err_invalid_params("expected [address, position, blockTag]"))?;
                let addr = parse_address_hex(&addr_hex)?;
                let slot = parse_hash256_hex(&pos_hex)?;
                let v = _ctx.lock().await.storage_at(&addr, slot);
                Ok::<String, ErrorObjectOwned>(hash_hex(&v))
            },
        )
        .expect("register eth_getStorageAt");

    module
        .register_async_method(
            "eth_getTransactionCount",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let (addr_hex, _tag): (String, String) = params
                        .parse()
                        .map_err(|_| err_invalid_params("expected [address, blockTag]"))?;
                    let addr = parse_address_hex(&addr_hex)?;
                    let g = ctx.lock().await;
                    let n = g.transaction_count(&addr);
                    Ok::<String, ErrorObjectOwned>(format!("0x{:x}", n))
                }
            },
        )
        .expect("register eth_getTransactionCount");

    module
        .register_async_method("eth_gasPrice", |_params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                let g = ctx.lock().await;
                Ok::<String, ErrorObjectOwned>(u256_quantity_hex(g.base_fee_per_gas()))
            }
        })
        .expect("register eth_gasPrice");

    module
        .register_async_method(
            "eth_maxPriorityFeePerGas",
            |_params: Params<'static>, _ctx, _| async move {
                // Devnet: fixed small tip suggestion (1 wei-equivalent).
                Ok::<String, ErrorObjectOwned>(u256_quantity_hex(1))
            },
        )
        .expect("register eth_maxPriorityFeePerGas");

    module
        .register_async_method("eth_feeHistory", |params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                // Params: (blockCount, newestBlock, rewardPercentiles?)
                // We'll accept rewardPercentiles but ignore it (reward = null).
                let (block_count_hex, newest_block, _reward): (String, String, Option<Vec<f64>>) =
                    params.parse().map_err(|_| {
                        err_invalid_params("expected [blockCount, newestBlock, rewardPercentiles?]")
                    })?;
                let block_count = if let Some(hex) = block_count_hex.strip_prefix("0x") {
                    u64::from_str_radix(hex, 16)
                        .map_err(|_| err_invalid_params("invalid blockCount"))?
                } else {
                    return Err(err_invalid_params("blockCount must be hex quantity"));
                };
                let g = ctx.lock().await;
                let newest = if newest_block == "latest" {
                    g.block_number()
                } else if let Some(hex) = newest_block.strip_prefix("0x") {
                    u64::from_str_radix(hex, 16)
                        .map_err(|_| err_invalid_params("invalid newestBlock"))?
                } else {
                    return Err(err_invalid_params("unsupported newestBlock"));
                };
                let oldest = newest.saturating_sub(block_count.saturating_sub(1));
                // EIP-1559 requires baseFeePerGas length = blockCount + 1.
                let base = u256_quantity_hex(g.base_fee_per_gas());
                let mut base_fees = Vec::with_capacity(block_count as usize + 1);
                for _ in 0..=block_count {
                    base_fees.push(base.clone());
                }
                let gas_used_ratio = vec![0.0f64; block_count as usize];
                Ok::<RpcFeeHistory, ErrorObjectOwned>(RpcFeeHistory {
                    oldest_block: quantity_hex_u64(oldest),
                    base_fee_per_gas: base_fees,
                    gas_used_ratio,
                    reward: None,
                })
            }
        })
        .expect("register eth_feeHistory");

    module
        .register_async_method(
            "eth_sendRawTransaction",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let hex: String = params
                        .one()
                        .map_err(|_| err_invalid_params("expected raw tx hex"))?;
                    let bytes = hex::decode(hex.trim_start_matches("0x"))
                        .map_err(|_| err_invalid_params("invalid tx hex"))?;
                    let mut g = ctx.lock().await;
                    g.submit_raw_tx(&bytes)
                        .map_err(|e| ErrorObjectOwned::owned(-32000, e, None::<()>))?;
                    // Return keccak hash placeholder of raw bytes (not canonical tx hash yet).
                    let h = keccak256(&bytes);
                    Ok::<String, ErrorObjectOwned>(format!("0x{}", hex::encode(h)))
                }
            },
        )
        .expect("register eth_sendRawTransaction");

    module
        .register_async_method("eth_call", |params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                let (from, to, value, data, _tag) = parse_eth_call_params(params)?;
                let g = ctx.lock().await;
                let out = g
                    .simulate_eth_call(from, to, value, data)
                    .map_err(exec_error_to_rpc)?;
                Ok::<String, ErrorObjectOwned>(format!("0x{}", hex::encode(out)))
            }
        })
        .expect("register eth_call");

    module
        .register_async_method("eth_estimateGas", |params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                let (from, to, value, data, _tag) = parse_eth_call_params(params)?;
                let g = ctx.lock().await;
                let gas = g
                    .estimate_eth_gas(from, to, value, data)
                    .map_err(exec_error_to_rpc)?;
                Ok::<String, ErrorObjectOwned>(quantity_hex_u64(gas))
            }
        })
        .expect("register eth_estimateGas");

    module
        .register_async_method("eth_getLogs", |params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                #[derive(serde::Deserialize)]
                #[serde(rename_all = "camelCase")]
                struct Filter {
                    from_block: Option<String>,
                    to_block: Option<String>,
                    block_hash: Option<String>,
                    address: Option<serde_json::Value>,
                    topics: Option<Vec<serde_json::Value>>,
                }
                let filter: Filter = params
                    .one()
                    .map_err(|_| err_invalid_params("expected filter object"))?;
                let g = ctx.lock().await;

                let latest = g.block_number();

                if filter.block_hash.is_some()
                    && (filter.from_block.is_some() || filter.to_block.is_some())
                {
                    return Err(err_invalid_params(
                        "blockHash is mutually exclusive with fromBlock and toBlock",
                    ));
                }

                let (mut from_block, mut to_block) = if let Some(ref bh) = filter.block_hash {
                    let h = parse_hash256_hex(bh)?;
                    let Some(block) = g.block_by_hash(&h) else {
                        return Ok::<Vec<RpcLog>, ErrorObjectOwned>(Vec::new());
                    };
                    let bn = block.header.height;
                    (bn, bn)
                } else {
                    let from_block = parse_block_quantity_or_tag(
                        filter.from_block.as_deref().unwrap_or("latest"),
                        latest,
                    )?;
                    let to_block = parse_block_quantity_or_tag(
                        filter.to_block.as_deref().unwrap_or("latest"),
                        latest,
                    )?;
                    (from_block, to_block)
                };

                if from_block > to_block {
                    std::mem::swap(&mut from_block, &mut to_block);
                }

                let addresses = parse_filter_addresses(filter.address)?;
                if addresses.as_ref().is_some_and(|a| a.is_empty()) {
                    return Ok::<Vec<RpcLog>, ErrorObjectOwned>(Vec::new());
                }
                let topic_filters = parse_topic_filters(filter.topics)?;

                let lf = LogsFilter {
                    from_block,
                    to_block,
                    addresses,
                    topic_filters,
                };
                let logs = g.logs_for_filter(&lf);
                Ok::<Vec<RpcLog>, ErrorObjectOwned>(logs)
            }
        })
        .expect("register eth_getLogs");

    module
}

fn rpc_tx_scope(tx: &fractal_core::Transaction, tx_hash: &[u8; 32]) -> RpcTxScope {
    match tx.execution_scope() {
        TxExecutionScope::Owned { owner, objects } => RpcTxScope {
            tx_hash: hash_hex(tx_hash),
            lane: "owned".into(),
            certificate_eligible: true,
            mixed: false,
            owner: Some(addr_hex(&owner)),
            owned_objects: objects.iter().map(rpc_owned_object_id).collect(),
        },
        TxExecutionScope::Mixed {
            owner,
            owned_objects,
        } => RpcTxScope {
            tx_hash: hash_hex(tx_hash),
            lane: "mixed".into(),
            certificate_eligible: false,
            mixed: true,
            owner: Some(addr_hex(&owner)),
            owned_objects: owned_objects.iter().map(rpc_owned_object_id).collect(),
        },
        TxExecutionScope::Consensus => RpcTxScope {
            tx_hash: hash_hex(tx_hash),
            lane: "consensus".into(),
            certificate_eligible: false,
            mixed: false,
            owner: None,
            owned_objects: Vec::new(),
        },
    }
}

fn rpc_owned_object_id(object_id: &OwnedObjectId) -> String {
    match object_id {
        OwnedObjectId::AccountNonce(address) => format!("accountNonce:{}", addr_hex(address)),
        OwnedObjectId::Agent(agent_id) => format!("agent:{agent_id}"),
        OwnedObjectId::Receipt(receipt_id) => format!("receipt:{}", hash_hex(receipt_id)),
        OwnedObjectId::WalletTaskReceipt(commitment) => {
            format!("walletTaskReceipt:{}", hash_hex(commitment))
        }
    }
}

fn rpc_block_from_consensus(
    b: &fractal_consensus::Block,
    hash: Option<[u8; 32]>,
    logs_bloom: [u8; 256],
    base_fee_per_gas: u128,
    proof_final: bool,
    proof: Option<fractal_consensus::BlockValidityProof>,
) -> RpcBlock {
    let h = hash.unwrap_or([0u8; 32]);
    let tx_hashes: Vec<String> = b
        .transactions
        .iter()
        .map(|tx| {
            let raw = borsh::to_vec(tx).unwrap_or_default();
            hash_hex(&keccak256(&raw))
        })
        .collect();
    RpcBlock {
        number: quantity_hex_u64(b.header.height),
        hash: hash_hex(&h),
        parent_hash: hash_hex(&b.header.parent_hash),
        nonce: "0x0000000000000000".into(),
        sha3_uncles: hash_hex(&[0u8; 32]),
        logs_bloom: logs_bloom_hex(&logs_bloom),
        transactions_root: hash_hex(&b.header.tx_root),
        state_root: hash_hex(&b.header.state_root),
        receipts_root: hash_hex(&[0u8; 32]),
        zone_namespace: format!("0x{}", hex::encode(b.header.zone_namespace)),
        da_root: hash_hex(&b.header.da_root),
        da_bytes: quantity_hex_u64(b.header.da_bytes),
        da_share_count: quantity_hex_u64(u64::from(b.header.da_share_count)),
        da_gas_used: quantity_hex_u64(b.header.da_gas_used),
        da_fee_paid: u256_quantity_hex(b.header.da_fee_paid),
        miner: "0x0000000000000000000000000000000000000000".into(),
        difficulty: u256_quantity_hex(0),
        total_difficulty: u256_quantity_hex(0),
        extra_data: format!("0x{}", hex::encode(b.header.extra)),
        size: quantity_hex_u64(0),
        gas_limit: quantity_hex_u64(b.header.gas_limit),
        gas_used: quantity_hex_u64(b.header.gas_used),
        timestamp: quantity_hex_u64(b.header.timestamp_ms / 1000),
        base_fee_per_gas: u256_quantity_hex(base_fee_per_gas),
        finality_status: if proof_final { "proof" } else { "soft" }.into(),
        proof_circuit_version: proof
            .as_ref()
            .map(|p| circuit_version_label(p.circuit_version)),
        proof_coverage_manifest_digest: proof
            .as_ref()
            .map(|p| hash_hex(&p.coverage_manifest_digest)),
        proof_covered_features: proof.as_ref().map(|p| {
            let manifest =
                fractal_consensus::coverage_manifest_for_circuit_version(p.circuit_version);
            quantity_hex_u64(manifest.covered_features.bits)
        }),
        transactions: tx_hashes,
        uncles: Vec::new(),
    }
}

fn rpc_tx_from_core(
    tx: &fractal_core::Transaction,
    hash: &[u8; 32],
    mined: Option<(u64, [u8; 32], u32)>,
    base_fee: u128,
) -> RpcTx {
    let (to, value, input, gas) = match &tx.body {
        fractal_core::TxBody::Transfer { to, amount } => (
            Some(addr_hex(to)),
            u256_quantity_hex(*amount),
            "0x".into(),
            quantity_hex_u64(fractal_core::TRANSFER_GAS),
        ),
        fractal_core::TxBody::Native(_c) => {
            (None, u256_quantity_hex(0), "0x".into(), quantity_hex_u64(0))
        }
        fractal_core::TxBody::EvmCall {
            to,
            value,
            calldata,
            gas_limit,
        } => (
            Some(addr_hex(to)),
            u256_quantity_hex(*value),
            format!("0x{}", hex::encode(calldata)),
            quantity_hex_u64(*gas_limit),
        ),
        fractal_core::TxBody::EvmCreate {
            value,
            init_code,
            gas_limit,
        } => (
            None,
            u256_quantity_hex(*value),
            format!("0x{}", hex::encode(init_code)),
            quantity_hex_u64(*gas_limit),
        ),
    };
    let (block_number, block_hash, tx_index) = mined
        .map(|(bn, bh, i)| {
            (
                Some(quantity_hex_u64(bn)),
                Some(hash_hex(&bh)),
                Some(quantity_hex_u64(i as u64)),
            )
        })
        .unwrap_or((None, None, None));
    RpcTx {
        hash: hash_hex(hash),
        nonce: quantity_hex_u64(tx.nonce),
        from: addr_hex(&tx.signer),
        to,
        value,
        input,
        gas,
        gas_price: u256_quantity_hex(base_fee),
        block_hash,
        block_number,
        transaction_index: tx_index,
    }
}

pub fn make_rpc_log(
    l: &fractal_core::EvmLog,
    block_hash: &[u8; 32],
    block_number: u64,
    tx_hash: &[u8; 32],
    tx_index: u32,
    log_index: u64,
) -> RpcLog {
    RpcLog {
        address: format!("0x{}", hex::encode(l.address)),
        topics: l
            .topics
            .iter()
            .map(|t| format!("0x{}", hex::encode(t)))
            .collect(),
        data: format!("0x{}", hex::encode(&l.data)),
        block_hash: hash_hex(block_hash),
        block_number: quantity_hex_u64(block_number),
        transaction_hash: hash_hex(tx_hash),
        transaction_index: quantity_hex_u64(tx_index as u64),
        log_index: quantity_hex_u64(log_index),
        removed: false,
    }
}

fn rpc_receipt_from_core(
    tx: &fractal_core::Transaction,
    hash: &[u8; 32],
    block_number: u64,
    block_hash: &[u8; 32],
    tx_index: u32,
    gas_used: u64,
    logs: Vec<RpcLog>,
    logs_bloom: [u8; 256],
    success: bool,
) -> RpcReceipt {
    let to = match &tx.body {
        fractal_core::TxBody::Transfer { to, .. } => Some(addr_hex(to)),
        fractal_core::TxBody::EvmCall { to, .. } => Some(addr_hex(to)),
        fractal_core::TxBody::Native(_) => None,
        fractal_core::TxBody::EvmCreate { .. } => None,
    };
    let contract_address = match &tx.body {
        fractal_core::TxBody::EvmCreate { .. } => {
            let a = fractal_core::create_contract_address(tx.signer, tx.nonce);
            Some(addr_hex(&a))
        }
        _ => None,
    };
    RpcReceipt {
        transaction_hash: hash_hex(hash),
        transaction_index: quantity_hex_u64(tx_index as u64),
        block_hash: hash_hex(block_hash),
        block_number: quantity_hex_u64(block_number),
        from: addr_hex(&tx.signer),
        to,
        cumulative_gas_used: quantity_hex_u64(gas_used),
        gas_used: quantity_hex_u64(gas_used),
        contract_address,
        logs,
        logs_bloom: logs_bloom_hex(&logs_bloom),
        status: if success { "0x1".into() } else { "0x0".into() },
    }
}

#[cfg(test)]
mod eth_get_logs_filter_tests {
    use super::*;
    use fractal_core::EvmLog;

    fn log_with_topics(topics: Vec<[u8; 32]>) -> EvmLog {
        EvmLog {
            address: [1u8; 20],
            topics,
            data: vec![],
        }
    }

    #[test]
    fn topic_match_exact() {
        let t0 = [2u8; 32];
        let log = log_with_topics(vec![t0]);
        let f = vec![Some(TopicMatch::Exact(t0))];
        assert!(evm_log_matches_topic_filters(&log, &f));
        let f2 = vec![Some(TopicMatch::Exact([0u8; 32]))];
        assert!(!evm_log_matches_topic_filters(&log, &f2));
    }

    #[test]
    fn topic_match_wildcard_second_position() {
        let log = log_with_topics(vec![[5u8; 32], [7u8; 32]]);
        let f = vec![None, Some(TopicMatch::Exact([7u8; 32]))];
        assert!(evm_log_matches_topic_filters(&log, &f));
    }

    #[test]
    fn topic_match_any_of() {
        let log = log_with_topics(vec![[1u8; 32]]);
        let f = vec![Some(TopicMatch::AnyOf(vec![[2u8; 32], [1u8; 32]]))];
        assert!(evm_log_matches_topic_filters(&log, &f));
    }

    #[test]
    fn topic_filter_requires_topic_at_index() {
        let log = log_with_topics(vec![[1u8; 32]]);
        let f = vec![None, Some(TopicMatch::Exact([2u8; 32]))];
        assert!(!evm_log_matches_topic_filters(&log, &f));
    }

    #[test]
    fn logs_bloom_empty_is_zero() {
        assert_eq!(logs_bloom_256(&[]), [0u8; 256]);
    }

    #[test]
    fn logs_bloom_merge_matches_concat() {
        let l1 = EvmLog {
            address: [1u8; 20],
            topics: vec![[9u8; 32]],
            data: vec![],
        };
        let l2 = EvmLog {
            address: [2u8; 20],
            topics: vec![],
            data: vec![],
        };
        let mut or_manual = logs_bloom_256(std::slice::from_ref(&l1));
        let b2 = logs_bloom_256(std::slice::from_ref(&l2));
        for i in 0..256 {
            or_manual[i] |= b2[i];
        }
        let merged = logs_bloom_256(&[l1, l2]);
        assert_eq!(or_manual, merged);
    }
}

pub async fn serve_http(
    addr: SocketAddr,
    ctx: SharedChain,
) -> Result<(ServerHandle, SocketAddr), std::io::Error> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any);
    let http_middleware = ServiceBuilder::new().layer(cors);

    let module = build_module(ctx);
    let server = ServerBuilder::default()
        .set_http_middleware(http_middleware)
        .build(addr)
        .await?;
    let bound = server.local_addr()?;
    let handle = server.start(module);
    Ok((handle, bound))
}
