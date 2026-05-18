//! Parse `fractal_getLightClientHead` JSON into [`LightClientHeadV1`].

use fractal_proof_aggregator::{
    GlobalZkStatementV1, PLONKY2_AGGREGATOR_VERSION, Plonky2ProofBundleV1,
};
use fractal_shard::{CrossShardMessageV1, MasterchainBlockV1, ProofSubmissionV1, ShardAnchor};
use serde_json::Value;

use crate::error::LightClientError;
use crate::head::LightClientHeadV1;

fn parse_hex_bytes(s: &str, label: &str) -> Result<Vec<u8>, LightClientError> {
    let h = s.strip_prefix("0x").unwrap_or(s);
    hex::decode(h).map_err(|e| LightClientError::Hex(format!("{label}: {e}")))
}

fn parse_hash32(s: &str, label: &str) -> Result<[u8; 32], LightClientError> {
    let b = parse_hex_bytes(s, label)?;
    if b.len() != 32 {
        return Err(LightClientError::Hex(format!(
            "{label}: expected 32 bytes, got {}",
            b.len()
        )));
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&b);
    Ok(out)
}

fn parse_address20(s: &str, label: &str) -> Result<[u8; 20], LightClientError> {
    let b = parse_hex_bytes(s, label)?;
    if b.len() != 20 {
        return Err(LightClientError::Hex(format!(
            "{label}: expected 20 bytes, got {}",
            b.len()
        )));
    }
    let mut out = [0u8; 20];
    out.copy_from_slice(&b);
    Ok(out)
}

fn parse_u64_hex(s: &str, label: &str) -> Result<u64, LightClientError> {
    let h = s.strip_prefix("0x").unwrap_or(s);
    u64::from_str_radix(h, 16).map_err(|e| LightClientError::Json(format!("{label}: {e}")))
}

fn parse_u32_hex(s: &str, label: &str) -> Result<u32, LightClientError> {
    let h = s.strip_prefix("0x").unwrap_or(s);
    u32::from_str_radix(h, 16).map_err(|e| LightClientError::Json(format!("{label}: {e}")))
}

fn parse_shard_anchors(v: &Value) -> Result<Vec<ShardAnchor>, LightClientError> {
    let arr = v
        .as_array()
        .ok_or_else(|| LightClientError::Json("shardAnchors not array".into()))?;
    let mut out = Vec::with_capacity(arr.len());
    for row in arr {
        let shard_id = parse_u32_hex(
            row.get("shardId")
                .and_then(Value::as_str)
                .ok_or_else(|| LightClientError::Json("shardId missing".into()))?,
            "shardId",
        )?;
        let block_height = parse_u64_hex(
            row.get("blockHeight")
                .and_then(Value::as_str)
                .ok_or_else(|| LightClientError::Json("blockHeight missing".into()))?,
            "blockHeight",
        )?;
        let state_root = parse_hash32(
            row.get("stateRoot")
                .and_then(Value::as_str)
                .ok_or_else(|| LightClientError::Json("stateRoot missing".into()))?,
            "stateRoot",
        )?;
        let witness_commitment = parse_hash32(
            row.get("witnessCommitment")
                .and_then(Value::as_str)
                .ok_or_else(|| LightClientError::Json("witnessCommitment missing".into()))?,
            "witnessCommitment",
        )?;
        out.push(ShardAnchor {
            shard_id,
            block_height,
            state_root,
            witness_commitment,
        });
    }
    Ok(out)
}

fn parse_proof_submissions(v: &Value) -> Result<Vec<ProofSubmissionV1>, LightClientError> {
    let arr = v
        .as_array()
        .ok_or_else(|| LightClientError::Json("validityProofs not array".into()))?;
    let mut out = Vec::with_capacity(arr.len());
    for row in arr {
        let shard_id = parse_u32_hex(
            row.get("shardId")
                .and_then(Value::as_str)
                .ok_or_else(|| LightClientError::Json("proof shardId missing".into()))?,
            "proof.shardId",
        )?;
        let start_block = parse_u64_hex(
            row.get("startBlock")
                .and_then(Value::as_str)
                .ok_or_else(|| LightClientError::Json("startBlock missing".into()))?,
            "startBlock",
        )?;
        let end_block = parse_u64_hex(
            row.get("endBlock")
                .and_then(Value::as_str)
                .ok_or_else(|| LightClientError::Json("endBlock missing".into()))?,
            "endBlock",
        )?;
        let prover = parse_address20(
            row.get("prover")
                .and_then(Value::as_str)
                .ok_or_else(|| LightClientError::Json("prover missing".into()))?,
            "prover",
        )?;
        let lag_seconds = row
            .get("lagSeconds")
            .and_then(Value::as_u64)
            .ok_or_else(|| LightClientError::Json("lagSeconds missing".into()))?
            as u32;
        let proof_digest = parse_hash32(
            row.get("proofDigest")
                .and_then(Value::as_str)
                .ok_or_else(|| LightClientError::Json("proofDigest missing".into()))?,
            "proofDigest",
        )?;
        out.push(ProofSubmissionV1 {
            shard_id,
            start_block,
            end_block,
            prover,
            lag_seconds,
            proof_digest,
        });
    }
    Ok(out)
}

fn parse_cross_shard_messages(v: &Value) -> Result<Vec<CrossShardMessageV1>, LightClientError> {
    let arr = v
        .as_array()
        .ok_or_else(|| LightClientError::Json("crossShardMessages not array".into()))?;
    let mut out = Vec::with_capacity(arr.len());
    for row in arr {
        let from_shard = parse_u32_hex(
            row.get("fromShard")
                .and_then(Value::as_str)
                .ok_or_else(|| LightClientError::Json("fromShard missing".into()))?,
            "fromShard",
        )?;
        let to_shard = parse_u32_hex(
            row.get("toShard")
                .and_then(Value::as_str)
                .ok_or_else(|| LightClientError::Json("toShard missing".into()))?,
            "toShard",
        )?;
        let payload_hash = parse_hash32(
            row.get("payloadHash")
                .and_then(Value::as_str)
                .ok_or_else(|| LightClientError::Json("payloadHash missing".into()))?,
            "payloadHash",
        )?;
        let payload_hex = row.get("payload").and_then(Value::as_str).unwrap_or("0x");
        let payload = parse_hex_bytes(payload_hex, "payload")?;
        out.push(CrossShardMessageV1 {
            from_shard,
            to_shard,
            payload_hash,
            payload,
        });
    }
    Ok(out)
}

fn parse_plonky2_bundle(v: &Value) -> Result<Plonky2ProofBundleV1, LightClientError> {
    let version =
        v.get("version")
            .and_then(Value::as_u64)
            .ok_or_else(|| LightClientError::Json("plonky2.version missing".into()))? as u8;
    if version != PLONKY2_AGGREGATOR_VERSION {
        return Err(LightClientError::Aggregator(
            fractal_proof_aggregator::AggregatorError::UnsupportedVersion(version),
        ));
    }
    let masterchain_height = parse_u64_hex(
        v.get("masterchainHeight")
            .and_then(Value::as_str)
            .ok_or_else(|| LightClientError::Json("masterchainHeight missing".into()))?,
        "masterchainHeight",
    )?;
    let global_state_root = parse_hash32(
        v.get("globalStateRoot")
            .and_then(Value::as_str)
            .ok_or_else(|| LightClientError::Json("plonky2.globalStateRoot missing".into()))?,
        "globalStateRoot",
    )?;
    let global_zk_root = parse_hash32(
        v.get("globalZkRoot")
            .and_then(Value::as_str)
            .ok_or_else(|| LightClientError::Json("plonky2.globalZkRoot missing".into()))?,
        "globalZkRoot",
    )?;
    let validity_proofs = parse_proof_submissions(
        v.get("validityProofs")
            .ok_or_else(|| LightClientError::Json("plonky2.validityProofs missing".into()))?,
    )?;
    let snark_hex = v
        .get("snarkBytes")
        .and_then(Value::as_str)
        .ok_or_else(|| LightClientError::Json("snarkBytes missing".into()))?;
    let snark_bytes = parse_hex_bytes(snark_hex, "snarkBytes")?;
    Ok(Plonky2ProofBundleV1 {
        version,
        masterchain_height,
        statement: GlobalZkStatementV1 {
            global_state_root,
            global_zk_root,
            validity_proofs,
            verified_stwo_statements: vec![],
        },
        snark_bytes,
    })
}

/// Parse JSON-RPC `result` for `fractal_getLightClientHead`.
pub fn parse_light_client_head_json(v: &Value) -> Result<LightClientHeadV1, LightClientError> {
    let height = parse_u64_hex(
        v.get("height")
            .and_then(Value::as_str)
            .ok_or_else(|| LightClientError::Json("height missing".into()))?,
        "height",
    )?;
    let shard_anchors = parse_shard_anchors(
        v.get("shardAnchors")
            .ok_or_else(|| LightClientError::Json("shardAnchors missing".into()))?,
    )?;
    let validity_proofs = parse_proof_submissions(
        v.get("validityProofs")
            .ok_or_else(|| LightClientError::Json("validityProofs missing".into()))?,
    )?;
    let global_state_root = parse_hash32(
        v.get("globalStateRoot")
            .and_then(Value::as_str)
            .ok_or_else(|| LightClientError::Json("globalStateRoot missing".into()))?,
        "globalStateRoot",
    )?;
    let global_zk_root = parse_hash32(
        v.get("globalZkRoot")
            .and_then(Value::as_str)
            .ok_or_else(|| LightClientError::Json("globalZkRoot missing".into()))?,
        "globalZkRoot",
    )?;
    let cross_shard_messages =
        parse_cross_shard_messages(v.get("crossShardMessages").unwrap_or(&Value::Array(vec![])))?;
    let plonky2 = v.get("plonky2").map(parse_plonky2_bundle).transpose()?;
    let execution_shard_id = v
        .get("executionShardId")
        .and_then(Value::as_str)
        .map(|s| parse_u32_hex(s, "executionShardId"))
        .transpose()?;
    let execution_tip_height = v
        .get("executionTipHeight")
        .and_then(Value::as_str)
        .map(|s| parse_u64_hex(s, "executionTipHeight"))
        .transpose()?;
    let execution_tip_state_root = v
        .get("executionTipStateRoot")
        .and_then(Value::as_str)
        .map(|s| parse_hash32(s, "executionTipStateRoot"))
        .transpose()?;
    Ok(LightClientHeadV1 {
        masterchain: MasterchainBlockV1 {
            height,
            shard_anchors,
            validity_proofs,
            global_state_root,
            global_zk_root,
            cross_shard_messages,
        },
        plonky2,
        execution_shard_id,
        execution_tip_height,
        execution_tip_state_root,
    })
}
