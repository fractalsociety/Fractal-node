use std::net::SocketAddr;
use std::sync::Arc;

use fractal_core::Address;
use fractal_crypto::hash::keccak256;
use jsonrpsee::server::{ServerBuilder, ServerHandle};
use jsonrpsee::types::{ErrorObjectOwned, Params};
use jsonrpsee::RpcModule;
use serde::Serialize;
use tokio::sync::Mutex;

fn err_invalid_params(msg: &'static str) -> ErrorObjectOwned {
    ErrorObjectOwned::owned(-32602, msg, None::<()>)
}

fn u256_quantity_hex(v: u128) -> String {
    format!("0x{:064x}", v)
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

fn parse_hash256_hex(s: &str) -> Result<[u8; 32], ErrorObjectOwned> {
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

fn hash_hex(h: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(h))
}

fn addr_hex(a: &Address) -> String {
    format!("0x{}", hex::encode(a))
}

fn bytes32_zero_hex() -> String {
    format!("0x{:0width$x}", 0u8, width = 64)
}

fn parse_u256_hex_u128(s: &str) -> Result<u128, ErrorObjectOwned> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.len() > 32 {
        return Err(err_invalid_params("value too large (max 128-bit in devnet)"));
    }
    u128::from_str_radix(if s.is_empty() { "0" } else { s }, 16)
        .map_err(|_| err_invalid_params("invalid quantity"))
}

fn parse_bytes_hex(s: &str) -> Result<Vec<u8>, ErrorObjectOwned> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    hex::decode(s).map_err(|_| err_invalid_params("invalid bytes hex"))
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
    miner: String,
    difficulty: String,
    total_difficulty: String,
    extra_data: String,
    size: String,
    gas_limit: String,
    gas_used: String,
    timestamp: String,
    transactions: Vec<String>,
    uncles: Vec<String>,
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
    logs: Vec<String>,
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

    fn tx_by_hash(&self, hash: &[u8; 32]) -> Option<fractal_core::Transaction>;

    fn mined_tx_info(&self, hash: &[u8; 32]) -> Option<(u64, [u8; 32], u32)>;

    fn simulate_eth_call(&self, from: Address, to: Address, value: u128, data: Vec<u8>) -> Result<Vec<u8>, String>;

    fn estimate_eth_gas(&self, from: Address, to: Address, value: u128, data: Vec<u8>) -> Result<u64, String>;

    fn code_at(&self, addr: &Address) -> Vec<u8>;
}

pub type SharedChain = Arc<Mutex<dyn ChainInteraction + Send>>;

pub fn build_module(ctx: SharedChain) -> RpcModule<SharedChain> {
    let mut module = RpcModule::new(ctx.clone());

    module
        .register_async_method("eth_syncing", |_params: Params<'static>, _ctx, _| async move {
            Ok::<bool, ErrorObjectOwned>(false)
        })
        .expect("register eth_syncing");

    module
        .register_async_method("web3_clientVersion", |_params: Params<'static>, _ctx, _| async move {
            Ok::<String, ErrorObjectOwned>("FractalChain/v0.1.0".into())
        })
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
                    u64::from_str_radix(hex, 16).map_err(|_| err_invalid_params("invalid block number"))?
                } else {
                    return Err(err_invalid_params("unsupported blockTag"));
                };
                let h = g.block_hash_by_number(number).ok_or_else(|| ErrorObjectOwned::owned(-32000, "block not found", None::<()>))?;
                let b = g.block_by_hash(&h).ok_or_else(|| ErrorObjectOwned::owned(-32000, "block not found", None::<()>))?;
                Ok::<RpcBlock, ErrorObjectOwned>(rpc_block_from_consensus(&b, Some(h)))
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
                let b = g.block_by_hash(&h).ok_or_else(|| ErrorObjectOwned::owned(-32000, "block not found", None::<()>))?;
                Ok::<RpcBlock, ErrorObjectOwned>(rpc_block_from_consensus(&b, Some(h)))
            }
        })
        .expect("register eth_getBlockByHash");

    module
        .register_async_method("eth_getBlockTransactionCountByNumber", |params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                let tag: String = params.one().map_err(|_| err_invalid_params("expected blockTag"))?;
                let g = ctx.lock().await;
                let number = if tag == "latest" {
                    g.block_number()
                } else if let Some(hex) = tag.strip_prefix("0x") {
                    u64::from_str_radix(hex, 16).map_err(|_| err_invalid_params("invalid block number"))?
                } else {
                    return Err(err_invalid_params("unsupported blockTag"));
                };
                let h = g
                    .block_hash_by_number(number)
                    .ok_or_else(|| ErrorObjectOwned::owned(-32000, "block not found", None::<()>))?;
                let b = g
                    .block_by_hash(&h)
                    .ok_or_else(|| ErrorObjectOwned::owned(-32000, "block not found", None::<()>))?;
                Ok::<String, ErrorObjectOwned>(quantity_hex_u64(b.transactions.len() as u64))
            }
        })
        .expect("register eth_getBlockTransactionCountByNumber");

    module
        .register_async_method("eth_getBlockTransactionCountByHash", |params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                let hash_hex: String = params.one().map_err(|_| err_invalid_params("expected block hash"))?;
                let h = parse_hash256_hex(&hash_hex)?;
                let g = ctx.lock().await;
                let b = g
                    .block_by_hash(&h)
                    .ok_or_else(|| ErrorObjectOwned::owned(-32000, "block not found", None::<()>))?;
                Ok::<String, ErrorObjectOwned>(quantity_hex_u64(b.transactions.len() as u64))
            }
        })
        .expect("register eth_getBlockTransactionCountByHash");

    module
        .register_async_method("eth_getTransactionByBlockHashAndIndex", |params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                let (block_hash_hex, idx_hex): (String, String) = params
                    .parse()
                    .map_err(|_| err_invalid_params("expected [blockHash, index]"))?;
                let bh = parse_hash256_hex(&block_hash_hex)?;
                let idx = if let Some(hex) = idx_hex.strip_prefix("0x") {
                    u64::from_str_radix(hex, 16).map_err(|_| err_invalid_params("invalid index"))?
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
                let raw = borsh::to_vec(&tx).map_err(|_| ErrorObjectOwned::owned(-32000, "tx encode failed", None::<()>))?;
                let th = keccak256(&raw);
                let mined = Some((b.header.height, bh, idx as u32));
                Ok::<Option<RpcTx>, ErrorObjectOwned>(Some(rpc_tx_from_core(&tx, &th, mined, g.base_fee_per_gas())))
            }
        })
        .expect("register eth_getTransactionByBlockHashAndIndex");

    module
        .register_async_method("eth_getTransactionByBlockNumberAndIndex", |params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                let (tag, idx_hex): (String, String) = params
                    .parse()
                    .map_err(|_| err_invalid_params("expected [blockTag, index]"))?;
                let idx = if let Some(hex) = idx_hex.strip_prefix("0x") {
                    u64::from_str_radix(hex, 16).map_err(|_| err_invalid_params("invalid index"))?
                } else {
                    return Err(err_invalid_params("index must be hex quantity"));
                };
                let g = ctx.lock().await;
                let number = if tag == "latest" {
                    g.block_number()
                } else if let Some(hex) = tag.strip_prefix("0x") {
                    u64::from_str_radix(hex, 16).map_err(|_| err_invalid_params("invalid block number"))?
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
                let raw = borsh::to_vec(&tx)
                    .map_err(|_| ErrorObjectOwned::owned(-32000, "tx encode failed", None::<()>))?;
                let th = keccak256(&raw);
                let mined = Some((b.header.height, bh, idx as u32));
                Ok::<Option<RpcTx>, ErrorObjectOwned>(Some(rpc_tx_from_core(
                    &tx,
                    &th,
                    mined,
                    g.base_fee_per_gas(),
                )))
            }
        })
        .expect("register eth_getTransactionByBlockNumberAndIndex");

    module
        .register_async_method("eth_getTransactionByHash", |params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                let hash_hex: String = params
                    .one()
                    .map_err(|_| err_invalid_params("expected tx hash"))?;
                let h = parse_hash256_hex(&hash_hex)?;
                let g = ctx.lock().await;
                let tx = match g.tx_by_hash(&h) {
                    Some(t) => t,
                    None => return Ok::<Option<RpcTx>, ErrorObjectOwned>(None),
                };
                let mined = g.mined_tx_info(&h);
                Ok::<Option<RpcTx>, ErrorObjectOwned>(Some(rpc_tx_from_core(&tx, &h, mined, g.base_fee_per_gas())))
            }
        })
        .expect("register eth_getTransactionByHash");

    module
        .register_async_method("eth_getTransactionReceipt", |params: Params<'static>, ctx, _| {
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
                let gas_used = fractal_core::intrinsic_gas(&tx).unwrap_or(0);
                Ok::<Option<RpcReceipt>, ErrorObjectOwned>(Some(rpc_receipt_from_core(
                    &tx,
                    &h,
                    bn,
                    &bh,
                    idx,
                    gas_used,
                )))
            }
        })
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
        .register_async_method("eth_getStorageAt", |params: Params<'static>, _ctx, _| async move {
            // Devnet stub: EVM storage not yet exposed via unified trie.
            // Return zero slot.
            let (_addr_hex, _pos_hex, _tag): (String, String, String) = params
                .parse()
                .map_err(|_| err_invalid_params("expected [address, position, blockTag]"))?;
            Ok::<String, ErrorObjectOwned>(bytes32_zero_hex())
        })
        .expect("register eth_getStorageAt");

    module
        .register_async_method("eth_getTransactionCount", |params: Params<'static>, ctx, _| {
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
        })
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
        .register_async_method("eth_maxPriorityFeePerGas", |_params: Params<'static>, _ctx, _| async move {
            // Devnet: fixed small tip suggestion (1 wei-equivalent).
            Ok::<String, ErrorObjectOwned>(u256_quantity_hex(1))
        })
        .expect("register eth_maxPriorityFeePerGas");

    module
        .register_async_method("eth_feeHistory", |params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                // Params: (blockCount, newestBlock, rewardPercentiles?)
                // We'll accept rewardPercentiles but ignore it (reward = null).
                let (block_count_hex, newest_block, _reward): (String, String, Option<Vec<f64>>) = params
                    .parse()
                    .map_err(|_| err_invalid_params("expected [blockCount, newestBlock, rewardPercentiles?]"))?;
                let block_count = if let Some(hex) = block_count_hex.strip_prefix("0x") {
                    u64::from_str_radix(hex, 16).map_err(|_| err_invalid_params("invalid blockCount"))?
                } else {
                    return Err(err_invalid_params("blockCount must be hex quantity"));
                };
                let g = ctx.lock().await;
                let newest = if newest_block == "latest" {
                    g.block_number()
                } else if let Some(hex) = newest_block.strip_prefix("0x") {
                    u64::from_str_radix(hex, 16).map_err(|_| err_invalid_params("invalid newestBlock"))?
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
        .register_async_method("eth_sendRawTransaction", |params: Params<'static>, ctx, _| {
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
        })
        .expect("register eth_sendRawTransaction");

    module
        .register_async_method("eth_call", |params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                #[derive(serde::Deserialize)]
                struct CallObj {
                    #[serde(default)]
                    from: Option<String>,
                    to: String,
                    #[serde(default)]
                    data: Option<String>,
                    #[serde(default)]
                    value: Option<String>,
                }
                let (obj, _tag): (CallObj, String) = params
                    .parse()
                    .map_err(|_| err_invalid_params("expected [callObject, blockTag]"))?;
                let from = obj
                    .from
                    .as_deref()
                    .map(parse_address_hex)
                    .transpose()?
                    .unwrap_or([0u8; 20]);
                let to = parse_address_hex(&obj.to)?;
                let data = obj.data.as_deref().map(parse_bytes_hex).transpose()?.unwrap_or_default();
                let value = obj.value.as_deref().map(parse_u256_hex_u128).transpose()?.unwrap_or(0);
                let g = ctx.lock().await;
                let out = g
                    .simulate_eth_call(from, to, value, data)
                    .map_err(|e| ErrorObjectOwned::owned(-32000, e, None::<()>))?;
                Ok::<String, ErrorObjectOwned>(format!("0x{}", hex::encode(out)))
            }
        })
        .expect("register eth_call");

    module
        .register_async_method("eth_estimateGas", |params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                #[derive(serde::Deserialize)]
                struct CallObj {
                    #[serde(default)]
                    from: Option<String>,
                    to: String,
                    #[serde(default)]
                    data: Option<String>,
                    #[serde(default)]
                    value: Option<String>,
                }
                let (obj, _tag): (CallObj, String) = params
                    .parse()
                    .map_err(|_| err_invalid_params("expected [callObject, blockTag]"))?;
                let from = obj
                    .from
                    .as_deref()
                    .map(parse_address_hex)
                    .transpose()?
                    .unwrap_or([0u8; 20]);
                let to = parse_address_hex(&obj.to)?;
                let data = obj.data.as_deref().map(parse_bytes_hex).transpose()?.unwrap_or_default();
                let value = obj.value.as_deref().map(parse_u256_hex_u128).transpose()?.unwrap_or(0);
                let g = ctx.lock().await;
                let gas = g
                    .estimate_eth_gas(from, to, value, data)
                    .map_err(|e| ErrorObjectOwned::owned(-32000, e, None::<()>))?;
                Ok::<String, ErrorObjectOwned>(quantity_hex_u64(gas))
            }
        })
        .expect("register eth_estimateGas");

    module
        .register_async_method("eth_getLogs", |_params: Params<'static>, _ctx, _| async move {
            // Devnet stub: we don't emit EVM logs yet (no MPT / receipts trie / log indexer).
            // Returning an empty list keeps MetaMask/ethers happy for now.
            Ok::<Vec<serde_json::Value>, ErrorObjectOwned>(Vec::new())
        })
        .expect("register eth_getLogs");

    module
}

fn rpc_block_from_consensus(b: &fractal_consensus::Block, hash: Option<[u8; 32]>) -> RpcBlock {
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
        logs_bloom: format!("0x{:0width$x}", 0u8, width = 512),
        transactions_root: hash_hex(&b.header.tx_root),
        state_root: hash_hex(&b.header.state_root),
        receipts_root: hash_hex(&[0u8; 32]),
        miner: "0x0000000000000000000000000000000000000000".into(),
        difficulty: u256_quantity_hex(0),
        total_difficulty: u256_quantity_hex(0),
        extra_data: format!("0x{}", hex::encode(b.header.extra)),
        size: quantity_hex_u64(0),
        gas_limit: quantity_hex_u64(b.header.gas_limit),
        gas_used: quantity_hex_u64(b.header.gas_used),
        timestamp: quantity_hex_u64(b.header.timestamp_ms / 1000),
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
        fractal_core::TxBody::Transfer { to, amount } => (Some(addr_hex(to)), u256_quantity_hex(*amount), "0x".into(), quantity_hex_u64(fractal_core::TRANSFER_GAS)),
        fractal_core::TxBody::Native(_c) => (None, u256_quantity_hex(0), "0x".into(), quantity_hex_u64(0)),
        fractal_core::TxBody::EvmCall { to, value, calldata, gas_limit } => (
            Some(addr_hex(to)),
            u256_quantity_hex(*value),
            format!("0x{}", hex::encode(calldata)),
            quantity_hex_u64(*gas_limit),
        ),
        fractal_core::TxBody::EvmCreate { value, init_code, gas_limit } => (
            None,
            u256_quantity_hex(*value),
            format!("0x{}", hex::encode(init_code)),
            quantity_hex_u64(*gas_limit),
        ),
    };
    let (block_number, block_hash, tx_index) = mined
        .map(|(bn, bh, i)| (Some(quantity_hex_u64(bn)), Some(hash_hex(&bh)), Some(quantity_hex_u64(i as u64))))
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

fn rpc_receipt_from_core(
    tx: &fractal_core::Transaction,
    hash: &[u8; 32],
    block_number: u64,
    block_hash: &[u8; 32],
    tx_index: u32,
    gas_used: u64,
) -> RpcReceipt {
    let to = match &tx.body {
        fractal_core::TxBody::Transfer { to, .. } => Some(addr_hex(to)),
        fractal_core::TxBody::EvmCall { to, .. } => Some(addr_hex(to)),
        fractal_core::TxBody::Native(_) => None,
        fractal_core::TxBody::EvmCreate { .. } => None,
    };
    let contract_address = match &tx.body {
        fractal_core::TxBody::EvmCreate { .. } => {
            // Duplicate of State's CREATE address derivation (devnet).
            let mut s = rlp::RlpStream::new_list(2);
            s.append(&tx.signer.as_slice());
            s.append(&tx.nonce);
            let h = keccak256(&s.out());
            let mut a = [0u8; 20];
            a.copy_from_slice(&h[12..]);
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
        logs: Vec::new(),
        logs_bloom: format!("0x{:0width$x}", 0u8, width = 512),
        status: "0x1".into(),
    }
}

pub async fn serve_http(addr: SocketAddr, ctx: SharedChain) -> Result<(ServerHandle, SocketAddr), std::io::Error> {
    let module = build_module(ctx);
    let server = ServerBuilder::default().build(addr).await?;
    let bound = server.local_addr()?;
    let handle = server.start(module);
    Ok((handle, bound))
}
