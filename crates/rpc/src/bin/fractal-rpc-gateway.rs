#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let bind = fractal_rpc::gateway_bind_addr_from_env()?;
    let gateway = fractal_rpc::RpcGateway::from_env()?;
    eprintln!(
        "fractal-rpc-gateway: listening on http://{} for {} shards",
        bind,
        gateway.shard_count()
    );
    for endpoint in gateway.endpoints() {
        eprintln!(
            "fractal-rpc-gateway: shard {} -> {}",
            endpoint.shard_id, endpoint.url
        );
    }
    let handle = fractal_rpc::serve_gateway_http(bind, gateway).await?;
    handle.stopped().await;
    Ok(())
}
