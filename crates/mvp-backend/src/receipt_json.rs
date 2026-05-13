//! Load PRD M5 settle batch from JSON (off-chain receipt export shape).

use fractal_core::{Address, OnChainTaskReceipt, PayoutEntry, SettleBatchPayload};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MvpSettleFile {
    batch_id: String,
    operator: String,
    claim_agent: String,
    #[serde(default)]
    submitted_at: u64,
    receipts: Vec<ReceiptJson>,
    #[serde(default)]
    operator_sig: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReceiptJson {
    receipt_id: String,
    job_id: String,
    requester: String,
    worker: u64,
    verifier: u64,
    artifact_root: String,
    output_hash: String,
    score: u8,
    payout_amount: serde_json::Value,
    #[serde(default)]
    verifier_fee: Option<serde_json::Value>,
    #[serde(default)]
    protocol_fee: Option<serde_json::Value>,
    final_status: u8,
    finalized_at: u64,
    schema_version: u16,
}

fn hex20(s: &str, field: &str) -> Result<Address, String> {
    let t = s.trim().strip_prefix("0x").unwrap_or(s.trim());
    if t.len() != 40 {
        return Err(format!("{field}: expected 20-byte hex, got {} nibbles", t.len()));
    }
    let b = hex::decode(t).map_err(|e| format!("{field}: {e}"))?;
    let mut a = [0u8; 20];
    a.copy_from_slice(&b);
    Ok(a)
}

fn hex32(s: &str, field: &str) -> Result<[u8; 32], String> {
    let t = s.trim().strip_prefix("0x").unwrap_or(s.trim());
    if t.len() != 64 {
        return Err(format!("{field}: expected 32-byte hex, got {} nibbles", t.len()));
    }
    let b = hex::decode(t).map_err(|e| format!("{field}: {e}"))?;
    let mut h = [0u8; 32];
    h.copy_from_slice(&b);
    Ok(h)
}

fn hex64(s: &str, field: &str) -> Result<[u8; 64], String> {
    let t = s.trim().strip_prefix("0x").unwrap_or(s.trim());
    if t.len() != 128 {
        return Err(format!("{field}: expected 64-byte hex, got {} nibbles", t.len()));
    }
    let b = hex::decode(t).map_err(|e| format!("{field}: {e}"))?;
    let mut h = [0u8; 64];
    h.copy_from_slice(&b);
    Ok(h)
}

fn u128_field(v: &serde_json::Value, field: &str) -> Result<u128, String> {
    match v {
        serde_json::Value::String(s) => s
            .parse::<u128>()
            .map_err(|e| format!("{field}: parse u128: {e}")),
        serde_json::Value::Number(n) => n
            .as_u64()
            .map(|x| x as u128)
            .ok_or_else(|| format!("{field}: number out of range")),
        serde_json::Value::Null => Ok(0),
        _ => Err(format!("{field}: expected string or number")),
    }
}

fn receipt_from_json(r: ReceiptJson) -> Result<OnChainTaskReceipt, String> {
    let verifier_fee = u128_field(r.verifier_fee.as_ref().unwrap_or(&serde_json::Value::Null), "verifierFee")?;
    let protocol_fee = u128_field(r.protocol_fee.as_ref().unwrap_or(&serde_json::Value::Null), "protocolFee")?;
    Ok(OnChainTaskReceipt {
        receipt_id: hex32(&r.receipt_id, "receiptId")?,
        job_id: hex32(&r.job_id, "jobId")?,
        requester: hex20(&r.requester, "requester")?,
        worker: r.worker,
        verifier: r.verifier,
        artifact_root: hex32(&r.artifact_root, "artifactRoot")?,
        output_hash: hex32(&r.output_hash, "outputHash")?,
        score: r.score,
        payout_amount: u128_field(&r.payout_amount, "payoutAmount")?,
        verifier_fee,
        protocol_fee,
        final_status: r.final_status,
        finalized_at: r.finalized_at,
        schema_version: r.schema_version,
    })
}

/// Read [`SettleBatchPayload`] from a JSON file (see `testdata/mvp_receipts_sample.json`).
///
/// Payout leaves are one per receipt, index `i`, account `claim_agent`, amount `receipt.payout_amount`.
pub fn load_settle_payload_from_json(path: &str) -> Result<(SettleBatchPayload, Address), String> {
    let raw = std::fs::read_to_string(path).map_err(|e| format!("read {path}: {e}"))?;
    let f: MvpSettleFile = serde_json::from_str(&raw).map_err(|e| format!("json: {e}"))?;
    let operator = hex20(&f.operator, "operator")?;
    let claim_agent = hex20(&f.claim_agent, "claimAgent")?;
    let batch_id = hex32(&f.batch_id, "batchId")?;
    if f.receipts.is_empty() {
        return Err("receipts array is empty".into());
    }
    let mut receipts = Vec::with_capacity(f.receipts.len());
    for r in f.receipts {
        receipts.push(receipt_from_json(r)?);
    }
    let payout_entries: Vec<PayoutEntry> = receipts
        .iter()
        .enumerate()
        .map(|(i, rc)| PayoutEntry {
            index: i as u32,
            account: claim_agent,
            amount: rc.payout_amount,
        })
        .collect();
    let operator_sig = match &f.operator_sig {
        Some(s) => hex64(s, "operatorSig")?,
        None => [0u8; 64],
    };
    Ok((
        SettleBatchPayload {
            batch_id,
            operator,
            receipts,
            payout_entries,
            submitted_at: f.submitted_at,
            operator_sig,
        },
        claim_agent,
    ))
}
