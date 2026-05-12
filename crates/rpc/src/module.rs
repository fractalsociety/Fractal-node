use std::net::SocketAddr;
use std::sync::Arc;

use fractal_core::Address;
use fractal_crypto::hash::keccak256;
use jsonrpsee::server::{ServerBuilder, ServerHandle};
use jsonrpsee::types::{ErrorObjectOwned, Params};
use jsonrpsee::RpcModule;
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

/// Minimal chain surface for JSON-RPC (implemented by `fractal-node`).
pub trait ChainInteraction: Send {
    fn block_number(&self) -> u64;

    fn balance_of(&self, addr: &Address) -> u128;

    /// Hex is `0x` + raw **borsh** `Transaction` bytes (dev stub until RLP exists).
    fn submit_raw_tx(&mut self, raw: &[u8]) -> Result<(), String>;

    fn base_fee_per_gas(&self) -> u128;
}

pub type SharedChain = Arc<Mutex<dyn ChainInteraction + Send>>;

pub fn build_module(ctx: SharedChain) -> RpcModule<SharedChain> {
    let mut module = RpcModule::new(ctx.clone());

    module
        .register_async_method("eth_blockNumber", |_params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                let g = ctx.lock().await;
                Ok::<String, ErrorObjectOwned>(format!("0x{:x}", g.block_number()))
            }
        })
        .expect("register eth_blockNumber");

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
        .register_async_method("eth_gasPrice", |_params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                let g = ctx.lock().await;
                Ok::<String, ErrorObjectOwned>(u256_quantity_hex(g.base_fee_per_gas()))
            }
        })
        .expect("register eth_gasPrice");

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
        .register_async_method("eth_call", |_params: Params<'static>, _ctx, _| async move {
            Ok::<String, ErrorObjectOwned>("0x".into())
        })
        .expect("register eth_call");

    module
}

pub async fn serve_http(addr: SocketAddr, ctx: SharedChain) -> Result<(ServerHandle, SocketAddr), std::io::Error> {
    let module = build_module(ctx);
    let server = ServerBuilder::default().build(addr).await?;
    let bound = server.local_addr()?;
    let handle = server.start(module);
    Ok((handle, bound))
}
