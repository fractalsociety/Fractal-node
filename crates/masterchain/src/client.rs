//! Shard → dedicated masterchain JSON-RPC client.

use fractal_shard::ShardAnchor;

/// POST `fractal_submitShardAnchor` to a dedicated masterchain node.
pub fn submit_shard_anchor_sync(masterchain_rpc: &str, anchor: &ShardAnchor) -> Result<(), String> {
    let url = masterchain_rpc.trim_end_matches('/');
    let body = serde_json::json!({
        "shardId": format!("0x{:x}", anchor.shard_id),
        "blockHeight": format!("0x{:x}", anchor.block_height),
        "stateRoot": format!("0x{}", hex::encode(anchor.state_root)),
        "witnessCommitment": format!("0x{}", hex::encode(anchor.witness_commitment)),
    });
    let req = serde_json::json!({
        "jsonrpc": "2.0",
        "method": "fractal_submitShardAnchor",
        "params": [body],
        "id": 1u64,
    });
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(std::time::Duration::from_secs(3))
        .timeout_read(std::time::Duration::from_secs(10))
        .build();
    let resp = agent
        .post(url)
        .set("Content-Type", "application/json")
        .send_json(req)
        .map_err(|e| format!("masterchain RPC POST failed: {e}"))?;
    let status = resp.status();
    if !(200..300).contains(&status) {
        return Err(format!("masterchain RPC HTTP {status}"));
    }
    let v: serde_json::Value = resp
        .into_json()
        .map_err(|e| format!("masterchain RPC json: {e}"))?;
    if let Some(err) = v.get("error") {
        return Err(format!("masterchain RPC error: {err}"));
    }
    Ok(())
}
