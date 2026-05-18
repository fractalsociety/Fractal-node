//! Minimal JSON-RPC client for light sync (`fractal_getLightClientHead`).

use serde_json::{json, Value};

use crate::error::LightClientError;
use crate::head::LightClientHeadV1;
use crate::parse::parse_light_client_head_json;

fn rpc_post(url: &str, method: &str, params: Value) -> Result<Value, LightClientError> {
    let body = json!({
        "jsonrpc": "2.0",
        "id": 1u64,
        "method": method,
        "params": params,
    });
    let resp: Value = ureq::post(url)
        .set("Content-Type", "application/json; charset=utf-8")
        .send_json(body)
        .map_err(|e| LightClientError::Rpc(format!("http: {e}")))?
        .into_json()
        .map_err(|e| LightClientError::Rpc(format!("json: {e}")))?;
    if let Some(err) = resp.get("error") {
        return Err(LightClientError::Rpc(format!("rpc error: {err}")));
    }
    resp.get("result")
        .cloned()
        .ok_or_else(|| LightClientError::Rpc("missing result".into()))
}

/// Fetch and parse `fractal_getLightClientHead` from a node JSON-RPC URL.
pub fn fetch_light_client_head(rpc_url: &str) -> Result<LightClientHeadV1, LightClientError> {
    let result = rpc_post(rpc_url, "fractal_getLightClientHead", json!([]))?;
    parse_light_client_head_json(&result)
}

/// Fetch `fractal_getLightClientHead` and verify Plonky2 + `globalStateRoot`.
pub fn fetch_and_verify_light_client_head(
    rpc_url: &str,
) -> Result<crate::VerifiedLightClientHead, LightClientError> {
    let head = fetch_light_client_head(rpc_url)?;
    crate::verify_light_client_head(&head)
}
