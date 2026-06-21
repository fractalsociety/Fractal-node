use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use borsh::BorshDeserialize;
use fractal_core::{Address, Transaction};
use http::Method;
use jsonrpsee::server::{ServerBuilder, ServerHandle};
use jsonrpsee::types::{ErrorObjectOwned, Params};
use jsonrpsee::RpcModule;
use serde_json::{json, Value};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tower::ServiceBuilder;
use tower_http::cors::{Any, CorsLayer};

use crate::module::{err_invalid_params, parse_hash256_hex};

const DEFAULT_GATEWAY_ADDR: &str = "127.0.0.1:8549";
const DEFAULT_GATEWAY_SHARDS: &str = "0=http://127.0.0.1:8545,1=http://127.0.0.1:8547";
const DEFAULT_GATEWAY_TIMEOUT_MS: u64 = 10_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShardEndpoint {
    pub shard_id: u32,
    pub url: String,
}

#[derive(Clone)]
pub struct RpcGateway {
    endpoints: Arc<Vec<ShardEndpoint>>,
    shard_count: u32,
    timeout: Duration,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GatewayRoute {
    Shard(u32),
    DefaultShard,
    FirstNonNullAcrossShards,
    MergeArraysAcrossShards,
    MaxQuantityAcrossShards,
    LocalShardCount,
    LocalHomeShardForAddress,
    LocalGatewayMap,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct HttpEndpoint {
    host: String,
    port: u16,
    path: String,
}

impl RpcGateway {
    pub fn new(endpoints: Vec<ShardEndpoint>) -> Result<Self, String> {
        Self::with_timeout(endpoints, Duration::from_millis(DEFAULT_GATEWAY_TIMEOUT_MS))
    }

    pub fn with_timeout(endpoints: Vec<ShardEndpoint>, timeout: Duration) -> Result<Self, String> {
        if endpoints.is_empty() {
            return Err("gateway requires at least one shard endpoint".into());
        }
        let mut endpoints = endpoints;
        endpoints.sort_by_key(|e| e.shard_id);
        for (idx, endpoint) in endpoints.iter().enumerate() {
            if endpoint.shard_id as usize != idx {
                return Err(format!(
                    "gateway shard endpoints must be contiguous from 0; missing shard {idx}"
                ));
            }
            parse_http_endpoint(&endpoint.url)?;
        }
        let shard_count = endpoints.len() as u32;
        Ok(Self {
            endpoints: Arc::new(endpoints),
            shard_count,
            timeout,
        })
    }

    pub fn from_env() -> Result<Self, String> {
        let raw = std::env::var("FRACTAL_GATEWAY_SHARDS")
            .or_else(|_| std::env::var("FRACTAL_SHARD_RPC_URLS"))
            .unwrap_or_else(|_| DEFAULT_GATEWAY_SHARDS.to_string());
        let timeout_ms = std::env::var("FRACTAL_GATEWAY_TIMEOUT_MS")
            .ok()
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(DEFAULT_GATEWAY_TIMEOUT_MS);
        Self::with_timeout(
            parse_gateway_endpoints(&raw)?,
            Duration::from_millis(timeout_ms),
        )
    }

    pub fn shard_count(&self) -> u32 {
        self.shard_count
    }

    pub fn endpoints(&self) -> &[ShardEndpoint] {
        &self.endpoints
    }

    pub fn route_for_method(
        &self,
        method: &str,
        params: &Value,
    ) -> Result<GatewayRoute, ErrorObjectOwned> {
        match method {
            "eth_sendRawTransaction" => {
                let raw_hex = first_param_string(params, "expected [rawTxHex]")?;
                let shard = shard_for_raw_tx_hex(raw_hex, self.shard_count)
                    .map_err(|e| ErrorObjectOwned::owned(-32000, e, None::<()>))?;
                Ok(GatewayRoute::Shard(shard))
            }
            "eth_getBalance" | "eth_getTransactionCount" | "eth_getCode" | "eth_getStorageAt" => {
                let addr = first_param_address(params)?;
                Ok(GatewayRoute::Shard(self.home_shard_for_address(&addr)))
            }
            "eth_call" => {
                let addr = eth_call_route_address(params, true)?;
                Ok(GatewayRoute::Shard(self.home_shard_for_address(&addr)))
            }
            "eth_estimateGas" => {
                let addr = eth_call_route_address(params, false)?;
                Ok(GatewayRoute::Shard(self.home_shard_for_address(&addr)))
            }
            "eth_getLogs" => match single_log_filter_address(params)? {
                Some(addr) => Ok(GatewayRoute::Shard(self.home_shard_for_address(&addr))),
                None => Ok(GatewayRoute::MergeArraysAcrossShards),
            },
            "eth_getTransactionByHash" | "eth_getTransactionReceipt" | "eth_getBlockByHash" => {
                let _ = first_param_hash(params)?;
                Ok(GatewayRoute::FirstNonNullAcrossShards)
            }
            "eth_blockNumber" => Ok(GatewayRoute::MaxQuantityAcrossShards),
            "fractal_getShardCount" => Ok(GatewayRoute::LocalShardCount),
            "fractal_getHomeShardForAddress" => Ok(GatewayRoute::LocalHomeShardForAddress),
            "fractal_gatewayShardMap" => Ok(GatewayRoute::LocalGatewayMap),
            _ => Ok(GatewayRoute::DefaultShard),
        }
    }

    async fn dispatch(&self, method: &str, params: Value) -> Result<Value, ErrorObjectOwned> {
        match self.route_for_method(method, &params)? {
            GatewayRoute::Shard(id) => self.call_shard(id, method, params).await,
            GatewayRoute::DefaultShard => self.call_default_shard(method, params).await,
            GatewayRoute::FirstNonNullAcrossShards => {
                for endpoint in self.endpoints.iter() {
                    let v = self
                        .call_shard(endpoint.shard_id, method, params.clone())
                        .await?;
                    if !v.is_null() {
                        return Ok(v);
                    }
                }
                Ok(Value::Null)
            }
            GatewayRoute::MergeArraysAcrossShards => {
                let mut merged = Vec::new();
                for endpoint in self.endpoints.iter() {
                    let v = self
                        .call_shard(endpoint.shard_id, method, params.clone())
                        .await?;
                    let arr = v.as_array().ok_or_else(|| {
                        ErrorObjectOwned::owned(
                            -32000,
                            "shard returned non-array result for merged query",
                            None::<()>,
                        )
                    })?;
                    merged.extend(arr.iter().cloned());
                }
                Ok(Value::Array(merged))
            }
            GatewayRoute::MaxQuantityAcrossShards => {
                let mut max = 0_u64;
                for endpoint in self.endpoints.iter() {
                    let v = self
                        .call_shard(endpoint.shard_id, method, params.clone())
                        .await?;
                    max = max.max(parse_quantity_result(&v)?);
                }
                Ok(Value::String(format!("0x{max:x}")))
            }
            GatewayRoute::LocalShardCount => Ok(Value::String(format!("0x{:x}", self.shard_count))),
            GatewayRoute::LocalHomeShardForAddress => {
                let (addr_hex,): (String,) = serde_json::from_value(params)
                    .map_err(|_| err_invalid_params("expected [addressHex]"))?;
                let addr = parse_address_hex(&addr_hex)?;
                Ok(Value::String(format!(
                    "0x{:x}",
                    self.home_shard_for_address(&addr)
                )))
            }
            GatewayRoute::LocalGatewayMap => Ok(json!({
                "shardCount": format!("0x{:x}", self.shard_count),
                "shards": self.endpoints.iter().map(|e| {
                    json!({
                        "shardId": format!("0x{:x}", e.shard_id),
                        "url": e.url,
                    })
                }).collect::<Vec<_>>()
            })),
        }
    }

    async fn call_default_shard(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Value, ErrorObjectOwned> {
        self.call_shard(0, method, params).await
    }

    async fn call_shard(
        &self,
        shard_id: u32,
        method: &str,
        params: Value,
    ) -> Result<Value, ErrorObjectOwned> {
        let endpoint = self
            .endpoints
            .iter()
            .find(|e| e.shard_id == shard_id)
            .ok_or_else(|| {
                ErrorObjectOwned::owned(
                    -32000,
                    format!("no RPC endpoint configured for shard {shard_id}"),
                    None::<()>,
                )
            })?;
        let response = post_json_rpc(&endpoint.url, method, params, self.timeout).await?;
        if let Some(error) = response.get("error") {
            return Err(error_object_from_json(error));
        }
        response.get("result").cloned().ok_or_else(|| {
            ErrorObjectOwned::owned(-32000, "shard response missing result", Some(response))
        })
    }

    fn home_shard_for_address(&self, addr: &Address) -> u32 {
        fractal_shard::home_shard_for_address(addr, self.shard_count)
    }
}

pub fn parse_gateway_endpoints(raw: &str) -> Result<Vec<ShardEndpoint>, String> {
    let mut endpoints = Vec::new();
    for (idx, part) in raw
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .enumerate()
    {
        let (shard_id, url) = if let Some((left, right)) = part.split_once('=') {
            let shard_id = left
                .trim()
                .parse::<u32>()
                .map_err(|e| format!("invalid shard id `{}`: {e}", left.trim()))?;
            (shard_id, right.trim())
        } else {
            (idx as u32, part)
        };
        if url.is_empty() {
            return Err(format!("missing URL for shard {shard_id}"));
        }
        endpoints.push(ShardEndpoint {
            shard_id,
            url: url.to_string(),
        });
    }
    if endpoints.is_empty() {
        return Err("empty shard endpoint list".into());
    }
    Ok(endpoints)
}

pub fn gateway_bind_addr_from_env() -> Result<SocketAddr, String> {
    std::env::var("FRACTAL_GATEWAY_ADDR")
        .unwrap_or_else(|_| DEFAULT_GATEWAY_ADDR.to_string())
        .parse::<SocketAddr>()
        .map_err(|e| format!("invalid FRACTAL_GATEWAY_ADDR: {e}"))
}

pub fn build_gateway_module(gateway: RpcGateway) -> RpcModule<RpcGateway> {
    let mut module = RpcModule::new(gateway);

    module
        .register_async_method(
            "web3_clientVersion",
            |_params: Params<'static>, _ctx, _| async move {
                Ok::<String, ErrorObjectOwned>("FractalChainGateway/v0.1.0".into())
            },
        )
        .expect("register web3_clientVersion");

    for method in [
        "eth_chainId",
        "net_version",
        "eth_syncing",
        "eth_blockNumber",
        "eth_getBlockByNumber",
        "eth_getBlockByHash",
        "eth_getTransactionByHash",
        "eth_getTransactionReceipt",
        "eth_getBalance",
        "eth_getCode",
        "eth_getStorageAt",
        "eth_getTransactionCount",
        "eth_gasPrice",
        "eth_maxPriorityFeePerGas",
        "eth_feeHistory",
        "eth_sendRawTransaction",
        "eth_call",
        "eth_estimateGas",
        "eth_getLogs",
        "fractal_getShardCount",
        "fractal_getHomeShardForAddress",
        "fractal_gatewayShardMap",
        "fractal_getConsensusMode",
        "fractal_getTargetBlockTimeMs",
        "fractal_getCheckpointProof",
        "fractal_getCheckpointProofDigest",
        "fractal_getMasterchainHead",
        "fractal_getGlobalZkRoot",
        "fractal_getGlobalZkProof",
        "fractal_getLightClientHead",
    ] {
        module
            .register_async_method(method, move |params: Params<'static>, ctx, _| {
                let gateway = ctx.clone();
                async move {
                    let params = params_json(params)?;
                    gateway.dispatch(method, params).await
                }
            })
            .expect("register gateway method");
    }

    module
}

pub async fn serve_gateway_http(
    bind: SocketAddr,
    gateway: RpcGateway,
) -> Result<ServerHandle, Box<dyn std::error::Error + Send + Sync>> {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers(Any);
    let http_middleware = ServiceBuilder::new().layer(cors);
    let server = ServerBuilder::default()
        .set_http_middleware(http_middleware)
        .build(bind)
        .await?;
    let handle = server.start(build_gateway_module(gateway));
    Ok(handle)
}

fn params_json(params: Params<'static>) -> Result<Value, ErrorObjectOwned> {
    match params.as_str() {
        Some(raw) => {
            serde_json::from_str(raw).map_err(|_| err_invalid_params("invalid params JSON"))
        }
        None => Ok(Value::Array(Vec::new())),
    }
}

async fn post_json_rpc(
    url: &str,
    method: &str,
    params: Value,
    request_timeout: Duration,
) -> Result<Value, ErrorObjectOwned> {
    let endpoint =
        parse_http_endpoint(url).map_err(|e| ErrorObjectOwned::owned(-32000, e, None::<()>))?;
    let body = json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params,
        "id": 1,
    })
    .to_string();

    let fut = async {
        let mut stream = TcpStream::connect((endpoint.host.as_str(), endpoint.port)).await?;
        let request = format!(
            "POST {} HTTP/1.1\r\nHost: {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            endpoint.path,
            endpoint.host,
            body.len(),
            body
        );
        stream.write_all(request.as_bytes()).await?;
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await?;
        Ok::<Vec<u8>, std::io::Error>(response)
    };

    let response = timeout(request_timeout, fut)
        .await
        .map_err(|_| ErrorObjectOwned::owned(-32000, "shard RPC request timed out", None::<()>))?
        .map_err(|e| {
            ErrorObjectOwned::owned(-32000, format!("shard RPC I/O error: {e}"), None::<()>)
        })?;

    let body = split_http_body(&response)?;
    serde_json::from_slice(body).map_err(|e| {
        ErrorObjectOwned::owned(
            -32000,
            format!("invalid shard JSON-RPC response: {e}"),
            None::<()>,
        )
    })
}

fn split_http_body(response: &[u8]) -> Result<&[u8], ErrorObjectOwned> {
    let header_end = response
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| {
            ErrorObjectOwned::owned(-32000, "invalid HTTP response from shard", None::<()>)
        })?;
    let head = String::from_utf8_lossy(&response[..header_end]);
    let status = head.lines().next().unwrap_or("");
    if !(status.starts_with("HTTP/1.1 200") || status.starts_with("HTTP/1.0 200")) {
        return Err(ErrorObjectOwned::owned(
            -32000,
            format!("shard RPC returned non-200 status: {status}"),
            None::<()>,
        ));
    }
    Ok(&response[header_end + 4..])
}

fn parse_http_endpoint(url: &str) -> Result<HttpEndpoint, String> {
    let rest = url
        .strip_prefix("http://")
        .ok_or_else(|| format!("only http:// shard RPC URLs are supported: {url}"))?;
    let (authority, path) = match rest.split_once('/') {
        Some((a, p)) => (a, format!("/{p}")),
        None => (rest, "/".to_string()),
    };
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => {
            let port = port
                .parse::<u16>()
                .map_err(|e| format!("invalid port in {url}: {e}"))?;
            (host.to_string(), port)
        }
        None => (authority.to_string(), 80),
    };
    if host.is_empty() {
        return Err(format!("missing host in {url}"));
    }
    Ok(HttpEndpoint { host, port, path })
}

fn error_object_from_json(v: &Value) -> ErrorObjectOwned {
    let code = v
        .get("code")
        .and_then(|c| c.as_i64())
        .and_then(|c| i32::try_from(c).ok())
        .unwrap_or(-32000);
    let message = v
        .get("message")
        .and_then(|m| m.as_str())
        .unwrap_or("shard RPC error")
        .to_string();
    let data = v.get("data").cloned();
    ErrorObjectOwned::owned(code, message, data)
}

fn parse_quantity_result(v: &Value) -> Result<u64, ErrorObjectOwned> {
    let s = v.as_str().ok_or_else(|| {
        ErrorObjectOwned::owned(-32000, "expected hex quantity result", None::<()>)
    })?;
    let hex = s.strip_prefix("0x").unwrap_or(s);
    u64::from_str_radix(if hex.is_empty() { "0" } else { hex }, 16)
        .map_err(|_| ErrorObjectOwned::owned(-32000, "invalid hex quantity from shard", None::<()>))
}

fn shard_for_raw_tx_hex(raw_hex: &str, shard_count: u32) -> Result<u32, String> {
    let raw = hex::decode(raw_hex.trim_start_matches("0x"))
        .map_err(|e| format!("invalid raw tx hex: {e}"))?;
    if let Ok(tx) = Transaction::try_from_slice(&raw) {
        return Ok(fractal_shard::home_shard_for_transaction(&tx, shard_count));
    }
    let env = fractal_eth_wire::decode_eip1559(&raw)?;
    let signer = fractal_eth_wire::recover_sender_eip1559(&raw, &env)?;
    Ok(fractal_shard::home_shard_for_address(&signer, shard_count))
}

fn first_param_string<'a>(
    params: &'a Value,
    msg: &'static str,
) -> Result<&'a str, ErrorObjectOwned> {
    params
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_str())
        .ok_or_else(|| err_invalid_params(msg))
}

fn first_param_hash(params: &Value) -> Result<[u8; 32], ErrorObjectOwned> {
    let h = first_param_string(params, "expected [hashHex]")?;
    parse_hash256_hex(h)
}

fn first_param_address(params: &Value) -> Result<Address, ErrorObjectOwned> {
    let a = first_param_string(params, "expected [address, ...]")?;
    parse_address_hex(a)
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

fn eth_call_route_address(params: &Value, prefer_to: bool) -> Result<Address, ErrorObjectOwned> {
    let obj = params
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_object())
        .ok_or_else(|| err_invalid_params("expected [callObject]"))?;

    let preferred = if prefer_to {
        ["to", "from"]
    } else {
        ["from", "to"]
    };
    for key in preferred {
        if let Some(s) = obj.get(key).and_then(|v| v.as_str()) {
            if s.is_empty() || s == "0x" || s == "0X" {
                continue;
            }
            return parse_address_hex(s);
        }
    }
    Ok([0u8; 20])
}

fn single_log_filter_address(params: &Value) -> Result<Option<Address>, ErrorObjectOwned> {
    let Some(filter) = params
        .as_array()
        .and_then(|a| a.first())
        .and_then(|v| v.as_object())
    else {
        return Ok(None);
    };
    let Some(address) = filter.get("address") else {
        return Ok(None);
    };
    if let Some(s) = address.as_str() {
        return parse_address_hex(s).map(Some);
    }
    if let Some(arr) = address.as_array() {
        if arr.len() == 1 {
            if let Some(s) = arr[0].as_str() {
                return parse_address_hex(s).map(Some);
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use fractal_core::{NativeCall, TxBody, VmKind};

    fn gateway() -> RpcGateway {
        RpcGateway::new(vec![
            ShardEndpoint {
                shard_id: 0,
                url: "http://127.0.0.1:8545".into(),
            },
            ShardEndpoint {
                shard_id: 1,
                url: "http://127.0.0.1:8547".into(),
            },
        ])
        .unwrap()
    }

    fn addr_with_home(want: u32) -> Address {
        for i in 0u8..=255 {
            let mut a = [0u8; 20];
            a[19] = i;
            if fractal_shard::home_shard_for_address(&a, 2) == want {
                return a;
            }
        }
        panic!("no address for shard {want}");
    }

    #[test]
    fn parses_gateway_endpoint_lists() {
        let got =
            parse_gateway_endpoints("0=http://127.0.0.1:8545,1=http://127.0.0.1:8547").unwrap();
        assert_eq!(got[0].shard_id, 0);
        assert_eq!(got[1].shard_id, 1);

        let got = parse_gateway_endpoints("http://127.0.0.1:8545,http://127.0.0.1:8547").unwrap();
        assert_eq!(got[0].shard_id, 0);
        assert_eq!(got[1].shard_id, 1);
    }

    #[test]
    fn routes_raw_borsh_tx_by_signer_home_shard() {
        let g = gateway();
        let signer = addr_with_home(1);
        let tx = Transaction {
            signer,
            nonce: 0,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::NoOp),
        };
        let raw = format!("0x{}", hex::encode(borsh::to_vec(&tx).unwrap()));
        let route = g
            .route_for_method("eth_sendRawTransaction", &json!([raw]))
            .unwrap();
        assert_eq!(route, GatewayRoute::Shard(1));
    }

    #[test]
    fn routes_address_queries_by_queried_address_home_shard() {
        let g = gateway();
        let addr = addr_with_home(1);
        let route = g
            .route_for_method(
                "eth_getBalance",
                &json!([format!("0x{}", hex::encode(addr)), "latest"]),
            )
            .unwrap();
        assert_eq!(route, GatewayRoute::Shard(1));
    }

    #[test]
    fn routes_eth_call_to_contract_address_when_present() {
        let g = gateway();
        let from = addr_with_home(0);
        let to = addr_with_home(1);
        let route = g
            .route_for_method(
                "eth_call",
                &json!([{
                    "from": format!("0x{}", hex::encode(from)),
                    "to": format!("0x{}", hex::encode(to)),
                    "data": "0x"
                }, "latest"]),
            )
            .unwrap();
        assert_eq!(route, GatewayRoute::Shard(1));
    }

    #[test]
    fn routes_estimate_gas_to_sender_home_shard() {
        let g = gateway();
        let from = addr_with_home(1);
        let to = addr_with_home(0);
        let route = g
            .route_for_method(
                "eth_estimateGas",
                &json!([{
                    "from": format!("0x{}", hex::encode(from)),
                    "to": format!("0x{}", hex::encode(to)),
                    "data": "0x"
                }]),
            )
            .unwrap();
        assert_eq!(route, GatewayRoute::Shard(1));
    }

    #[test]
    fn routes_single_address_logs_to_home_and_wildcard_logs_to_merge() {
        let g = gateway();
        let addr = addr_with_home(1);
        let route = g
            .route_for_method(
                "eth_getLogs",
                &json!([{"address": format!("0x{}", hex::encode(addr))}]),
            )
            .unwrap();
        assert_eq!(route, GatewayRoute::Shard(1));

        let route = g.route_for_method("eth_getLogs", &json!([{}])).unwrap();
        assert_eq!(route, GatewayRoute::MergeArraysAcrossShards);
    }
}
