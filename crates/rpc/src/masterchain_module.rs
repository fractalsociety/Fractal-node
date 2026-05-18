//! JSON-RPC surface for the dedicated masterchain BFT process (`fractal-masterchain`).

use std::net::SocketAddr;
use std::sync::Arc;

use fractal_shard::{MasterchainBlockV1, ProofSubmissionV1, ShardAnchor};
use futures::future::BoxFuture;
use http::Method;
use jsonrpsee::RpcModule;
use jsonrpsee::server::middleware::rpc::{RpcServiceBuilder, RpcServiceT};
use jsonrpsee::server::{MethodResponse, ServerBuilder, ServerHandle};
use jsonrpsee::types::{ErrorObjectOwned, Params, Request};
use tokio::sync::Mutex;
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};

use crate::RpcCallStats;
use crate::module::{
    err_invalid_params, masterchain_block_to_json, parse_fractal_height_hex, parse_hash256_hex,
    parse_u32_hex, plonky2_bundle_to_json, shard_anchor_to_json,
};

pub trait MasterchainRpc: Send {
    fn masterchain_height(&self) -> u64;
    fn submit_shard_anchor(&mut self, anchor: ShardAnchor) -> Result<(), String>;
    fn submit_validity_proof(&mut self, sub: ProofSubmissionV1) -> Result<(), String>;
    fn register_prover(&mut self, prover: [u8; 20], bond_wei: u128) -> Result<(), String>;
    fn get_prover_identity(&self, prover: [u8; 20]) -> Option<ProverIdentityJson>;
    fn get_invalid_proof_slash_events(&self) -> Vec<InvalidProofSlashEventJson>;
    fn get_masterchain_head(&self) -> Option<MasterchainBlockV1>;
    fn get_global_zk_root(&self) -> Option<[u8; 32]>;
    fn get_global_zk_proof(&self) -> Option<fractal_proof_aggregator::Plonky2ProofBundleV1>;
    fn get_shard_anchor(&self, shard_id: u32, block_height: Option<u64>) -> Option<ShardAnchor>;
}

#[derive(Clone, Debug)]
pub struct ProverIdentityJson {
    pub prover: [u8; 20],
    pub bond_wei: u128,
    pub registered_at_masterchain_height: u64,
    pub active: bool,
}

#[derive(Clone, Debug)]
pub struct InvalidProofSlashEventJson {
    pub masterchain_height: u64,
    pub prover: [u8; 20],
    pub shard_id: u32,
    pub start_block: u64,
    pub end_block: u64,
    pub proof_digest: [u8; 32],
    pub reason_code: u8,
    pub evidence_hash: [u8; 32],
    pub slash_amount_wei: u128,
    pub executed: bool,
    pub burned_bond_wei: u128,
    pub bond_before_wei: u128,
    pub bond_after_wei: u128,
    pub prover_active_after: bool,
}

fn invalid_proof_slash_event_to_json(e: &InvalidProofSlashEventJson) -> serde_json::Value {
    serde_json::json!({
        "masterchainHeight": format!("0x{:x}", e.masterchain_height),
        "prover": format!("0x{}", hex::encode(e.prover)),
        "shardId": format!("0x{:x}", e.shard_id),
        "startBlock": format!("0x{:x}", e.start_block),
        "endBlock": format!("0x{:x}", e.end_block),
        "proofDigest": format!("0x{}", hex::encode(e.proof_digest)),
        "reasonCode": e.reason_code,
        "evidenceHash": format!("0x{}", hex::encode(e.evidence_hash)),
        "slashAmountWei": format!("0x{:x}", e.slash_amount_wei),
        "executed": e.executed,
        "burnedBondWei": format!("0x{:x}", e.burned_bond_wei),
        "bondBeforeWei": format!("0x{:x}", e.bond_before_wei),
        "bondAfterWei": format!("0x{:x}", e.bond_after_wei),
        "proverActiveAfter": e.prover_active_after,
    })
}

fn prover_identity_to_json(id: &ProverIdentityJson) -> serde_json::Value {
    serde_json::json!({
        "prover": format!("0x{}", hex::encode(id.prover)),
        "bondWei": format!("0x{:x}", id.bond_wei),
        "registeredAtMasterchainHeight": format!("0x{:x}", id.registered_at_masterchain_height),
        "active": id.active,
    })
}

fn parse_shard_anchor_json(obj: &serde_json::Value) -> Result<ShardAnchor, ErrorObjectOwned> {
    let shard_id = obj
        .get("shardId")
        .and_then(|v| v.as_str())
        .ok_or(err_invalid_params("missing shardId"))?;
    let block_height = obj
        .get("blockHeight")
        .and_then(|v| v.as_str())
        .ok_or(err_invalid_params("missing blockHeight"))?;
    let state_root = obj
        .get("stateRoot")
        .and_then(|v| v.as_str())
        .ok_or(err_invalid_params("missing stateRoot"))?;
    let witness = obj
        .get("witnessCommitment")
        .and_then(|v| v.as_str())
        .ok_or(err_invalid_params("missing witnessCommitment"))?;
    Ok(ShardAnchor {
        shard_id: parse_u32_hex(shard_id)?,
        block_height: parse_fractal_height_hex(block_height)?,
        state_root: parse_hash256_hex(state_root)?,
        witness_commitment: parse_hash256_hex(witness)?,
    })
}

pub fn build_masterchain_module<T>(ctx: Arc<Mutex<T>>) -> RpcModule<Arc<Mutex<T>>>
where
    T: MasterchainRpc + Send + 'static,
{
    let mut module = RpcModule::new(ctx.clone());

    module
        .register_async_method(
            "web3_clientVersion",
            |_params: Params<'static>, _ctx, _| async move {
                Ok::<String, ErrorObjectOwned>("FractalMasterchain/v0.1.0".into())
            },
        )
        .expect("register web3_clientVersion");

    module
        .register_async_method(
            "fractal_getMasterchainHeight",
            |_params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let g = ctx.lock().await;
                    Ok::<String, ErrorObjectOwned>(format!("0x{:x}", g.masterchain_height()))
                }
            },
        )
        .expect("register fractal_getMasterchainHeight");

    module
        .register_async_method(
            "fractal_submitShardAnchor",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let arr: Vec<serde_json::Value> = params
                        .parse()
                        .map_err(|_| err_invalid_params("expected anchor object"))?;
                    let obj = arr
                        .first()
                        .ok_or(err_invalid_params("expected anchor object"))?;
                    let anchor = parse_shard_anchor_json(obj)?;
                    let mut g = ctx.lock().await;
                    g.submit_shard_anchor(anchor)
                        .map_err(|e| ErrorObjectOwned::owned(-32602, e, None::<()>))?;
                    Ok::<bool, ErrorObjectOwned>(true)
                }
            },
        )
        .expect("register fractal_submitShardAnchor");

    module
        .register_async_method(
            "fractal_getMasterchainHead",
            |_params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let g = ctx.lock().await;
                    let head = g
                        .get_masterchain_head()
                        .ok_or(err_invalid_params("no masterchain head"))?;
                    Ok::<serde_json::Value, ErrorObjectOwned>(masterchain_block_to_json(&head))
                }
            },
        )
        .expect("register fractal_getMasterchainHead");

    module
        .register_async_method(
            "fractal_submitValidityProof",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let obj: serde_json::Value = params
                        .parse()
                        .map_err(|_| err_invalid_params("expected validity proof object"))?;
                    let shard_id = obj
                        .get("shardId")
                        .and_then(|v| v.as_str())
                        .ok_or(err_invalid_params("missing shardId"))?;
                    let start_block = obj
                        .get("startBlock")
                        .and_then(|v| v.as_str())
                        .ok_or(err_invalid_params("missing startBlock"))?;
                    let end_block = obj
                        .get("endBlock")
                        .and_then(|v| v.as_str())
                        .ok_or(err_invalid_params("missing endBlock"))?;
                    let proof_digest = obj
                        .get("proofDigest")
                        .and_then(|v| v.as_str())
                        .ok_or(err_invalid_params("missing proofDigest"))?;
                    let prover_hex = obj.get("prover").and_then(|v| v.as_str());
                    let submission = ProofSubmissionV1 {
                        shard_id: parse_u32_hex(shard_id)?,
                        start_block: parse_fractal_height_hex(start_block)?,
                        end_block: parse_fractal_height_hex(end_block)?,
                        prover: if let Some(p) = prover_hex {
                            parse_address_hex_masterchain(p)?
                        } else {
                            [0u8; 20]
                        },
                        lag_seconds: obj.get("lagSeconds").and_then(|v| v.as_u64()).unwrap_or(0)
                            as u32,
                        proof_digest: parse_hash256_hex(proof_digest)?,
                    };
                    let mut g = ctx.lock().await;
                    g.submit_validity_proof(submission)
                        .map_err(|e| ErrorObjectOwned::owned(-32602, e, None::<()>))?;
                    Ok::<bool, ErrorObjectOwned>(true)
                }
            },
        )
        .expect("register fractal_submitValidityProof");

    module
        .register_async_method(
            "fractal_registerProver",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let obj: serde_json::Value = params
                        .parse()
                        .map_err(|_| err_invalid_params("expected prover registration object"))?;
                    let prover = obj
                        .get("prover")
                        .and_then(|v| v.as_str())
                        .ok_or(err_invalid_params("missing prover"))?;
                    let bond_wei = obj
                        .get("bondWei")
                        .and_then(|v| v.as_str())
                        .ok_or(err_invalid_params("missing bondWei"))?;
                    let mut g = ctx.lock().await;
                    g.register_prover(
                        parse_address_hex_masterchain(prover)?,
                        parse_u128_hex_or_dec(bond_wei)?,
                    )
                    .map_err(|e| ErrorObjectOwned::owned(-32602, e, None::<()>))?;
                    Ok::<bool, ErrorObjectOwned>(true)
                }
            },
        )
        .expect("register fractal_registerProver");

    module
        .register_async_method(
            "fractal_getProverIdentity",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let arr: Vec<String> = params
                        .parse()
                        .map_err(|_| err_invalid_params("expected [prover]"))?;
                    let prover = arr
                        .first()
                        .map(|s| parse_address_hex_masterchain(s))
                        .transpose()?
                        .ok_or(err_invalid_params("missing prover"))?;
                    let g = ctx.lock().await;
                    let Some(id) = g.get_prover_identity(prover) else {
                        return Ok::<serde_json::Value, ErrorObjectOwned>(serde_json::Value::Null);
                    };
                    Ok::<serde_json::Value, ErrorObjectOwned>(prover_identity_to_json(&id))
                }
            },
        )
        .expect("register fractal_getProverIdentity");

    module
        .register_async_method(
            "fractal_getInvalidProofSlashEvents",
            |_params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let g = ctx.lock().await;
                    let events: Vec<serde_json::Value> = g
                        .get_invalid_proof_slash_events()
                        .iter()
                        .map(invalid_proof_slash_event_to_json)
                        .collect();
                    Ok::<Vec<serde_json::Value>, ErrorObjectOwned>(events)
                }
            },
        )
        .expect("register fractal_getInvalidProofSlashEvents");

    module
        .register_async_method(
            "fractal_getGlobalZkRoot",
            |_params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let g = ctx.lock().await;
                    let root = g
                        .get_global_zk_root()
                        .ok_or(err_invalid_params("no globalZkRoot yet"))?;
                    Ok::<String, ErrorObjectOwned>(format!("0x{}", hex::encode(root)))
                }
            },
        )
        .expect("register fractal_getGlobalZkRoot");

    module
        .register_async_method(
            "fractal_getGlobalZkProof",
            |_params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let g = ctx.lock().await;
                    let bundle = g
                        .get_global_zk_proof()
                        .ok_or(err_invalid_params("no Plonky2 bundle"))?;
                    Ok::<serde_json::Value, ErrorObjectOwned>(plonky2_bundle_to_json(&bundle))
                }
            },
        )
        .expect("register fractal_getGlobalZkProof");

    module
        .register_async_method(
            "fractal_getShardAnchor",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let arr: Vec<String> = params
                        .parse()
                        .map_err(|_| err_invalid_params("expected [shardId, blockHeight?]"))?;
                    let shard_id = arr
                        .first()
                        .map(|s| parse_u32_hex(s))
                        .transpose()?
                        .ok_or(err_invalid_params("missing shardId"))?;
                    let block_height = arr
                        .get(1)
                        .map(|s| parse_fractal_height_hex(s))
                        .transpose()?;
                    let g = ctx.lock().await;
                    let anchor = g
                        .get_shard_anchor(shard_id, block_height)
                        .ok_or(err_invalid_params("shard anchor not found"))?;
                    Ok::<serde_json::Value, ErrorObjectOwned>(shard_anchor_to_json(&anchor))
                }
            },
        )
        .expect("register fractal_getShardAnchor");

    module
}

fn parse_address_hex_masterchain(s: &str) -> Result<[u8; 20], ErrorObjectOwned> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    let bytes = hex::decode(s).map_err(|_| err_invalid_params("invalid address hex"))?;
    if bytes.len() != 20 {
        return Err(err_invalid_params("address must be 20 bytes"));
    }
    let mut a = [0u8; 20];
    a.copy_from_slice(&bytes);
    Ok(a)
}

fn parse_u128_hex_or_dec(s: &str) -> Result<u128, ErrorObjectOwned> {
    if let Some(hex) = s.strip_prefix("0x") {
        u128::from_str_radix(hex, 16).map_err(|_| err_invalid_params("invalid u128 hex"))
    } else {
        s.parse::<u128>()
            .map_err(|_| err_invalid_params("invalid u128 decimal"))
    }
}

struct McRpcCountingService<S> {
    inner: S,
    stats: RpcCallStats,
}

impl<'a, S> RpcServiceT<'a> for McRpcCountingService<S>
where
    S: RpcServiceT<'a> + Send + Sync + Clone + 'static,
{
    type Future = BoxFuture<'a, MethodResponse>;

    fn call(&self, req: Request<'a>) -> Self::Future {
        let method = req.method_name().to_string();
        let stats = self.stats.clone();
        let inner = self.inner.clone();
        Box::pin(async move {
            let started = std::time::Instant::now();
            let rp = inner.call(req).await;
            stats.record_with_latency_ms(
                &method,
                rp.is_success(),
                started.elapsed().as_millis() as u64,
            );
            rp
        })
    }
}

pub async fn serve_masterchain_http<T>(
    addr: SocketAddr,
    ctx: Arc<Mutex<T>>,
    rpc_stats: RpcCallStats,
) -> Result<(ServerHandle, SocketAddr), std::io::Error>
where
    T: MasterchainRpc + Send + 'static,
{
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any);
    let http_middleware = ServiceBuilder::new().layer(cors);
    let stats_layer = rpc_stats.clone();
    let rpc_middleware = RpcServiceBuilder::new().layer_fn(move |inner| McRpcCountingService {
        inner,
        stats: stats_layer.clone(),
    });
    let module = build_masterchain_module(ctx);
    let server = ServerBuilder::default()
        .set_rpc_middleware(rpc_middleware)
        .set_http_middleware(http_middleware)
        .build(addr)
        .await?;
    let bound = server.local_addr()?;
    let handle = server.start(module);
    Ok((handle, bound))
}
