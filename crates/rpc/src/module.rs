use std::net::SocketAddr;
use std::sync::Arc;

use borsh::BorshDeserialize;
use fractal_core::{Address, OwnedObjectId, TxExecutionScope};
use fractal_crypto::hash::keccak256;
use http::Method;
use jsonrpsee::server::{ServerBuilder, ServerHandle};
use jsonrpsee::types::{ErrorObjectOwned, Params};
use jsonrpsee::RpcModule;
use serde::{Deserialize, Serialize};
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

fn parse_u128_quantity_or_decimal(s: &str) -> Result<u128, ErrorObjectOwned> {
    if let Some(hex) = s.strip_prefix("0x") {
        u128::from_str_radix(hex, 16).map_err(|_| err_invalid_params("invalid u128 quantity"))
    } else {
        s.parse::<u128>()
            .map_err(|_| err_invalid_params("invalid u128 quantity"))
    }
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

fn parse_u64_quantity_or_decimal(s: &str) -> Result<u64, ErrorObjectOwned> {
    if let Some(hex) = s.strip_prefix("0x") {
        u64::from_str_radix(hex, 16).map_err(|_| err_invalid_params("invalid u64 quantity"))
    } else {
        s.parse::<u64>()
            .map_err(|_| err_invalid_params("invalid u64 quantity"))
    }
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
    payload_type: String,
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
pub struct RpcConsensusDiagnostics {
    pub height: String,
    pub current_view: String,
    pub validator_index: String,
    pub validator_set_size: String,
    pub quorum_threshold: String,
    pub connected_peer_count: String,
    pub connected_validator_count: String,
    pub current_leader_index: String,
    pub current_leader_fingerprint: String,
    pub height2_votes_received: String,
    pub height2_vote_view: Option<String>,
    pub height2_vote_header_hash: Option<String>,
    pub height2_vote_signers: Vec<String>,
    pub qc_status: String,
    pub qc_reason: String,
    pub qc_height: String,
    pub qc_view: String,
    pub qc_vote_count: String,
    pub qc_threshold: String,
    pub genesis_hash: String,
    pub validator_set_hash: String,
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

#[derive(Clone, Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RpcRoutingDiagnostics {
    pub source_shard: String,
    pub expected_shard: String,
    pub shard_count: String,
    pub route_key: String,
    pub accepted: bool,
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
    pub block_payload_mode: String,
    pub rlvr_enabled: bool,
    pub rlvr_chain_commit_enabled: bool,
    pub rlvr_raw_data_on_chain: bool,
    pub rlvr_raw_data_on_chain_requested: bool,
    pub settlement_finality: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcProofSubmission {
    pub block_hash: String,
    pub finality_status: String,
}

#[derive(Clone, Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RpcOwnedObjectPrecheck {
    pub tx_hash: String,
    pub owner: String,
    pub signer_nonce: String,
    pub object_versions: Vec<String>,
    pub object_versions_borsh: String,
    pub sign_body_borsh: String,
    pub tx_gas: String,
    pub max_fee_per_gas: String,
    pub base_fee_per_gas: String,
}

#[derive(Clone, Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RpcOwnedObjectCountersignature {
    pub validator_index: String,
    pub signature_borsh: String,
    pub sign_body_borsh: String,
}

#[derive(Clone, Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RpcOwnedObjectCertificate {
    pub certificate_hash: String,
    pub certificate_borsh: String,
    pub signer_indices: Vec<String>,
}

#[derive(Clone, Serialize, serde::Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RpcOwnedObjectFinality {
    pub object_version_borsh: String,
    pub finality_status: String,
    pub certificate_hash: Option<String>,
    pub certificate_borsh: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcZoneProofFinalHeight {
    pub zone_id: String,
    pub proof_final_height: Option<String>,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcZoneUpdateFinality {
    pub zone_id: String,
    pub height: String,
    pub finality_status: String,
}

/// Response from `fractal_submitProofHash`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProofCommitmentResponse {
    /// Network name.
    pub network: String,
    /// Transaction hash (0x-prefixed hex).
    pub transaction_hash: String,
    /// Block number containing the commitment.
    pub block_number: u64,
    /// Whether the commitment is finalized.
    pub finalized: bool,
}

/// Allowed values for `promotionDecision` in an RLMF attestation record.
pub const RLMF_PROMOTION_DECISIONS: [&str; 6] = [
    "promote",
    "reject",
    "inconclusive",
    "shadow",
    "canary",
    "rollback",
];

/// Maximum entries accepted in `evidenceHashes` / `lineageHashes`.
pub const RLMF_MAX_HASH_LIST: usize = 64;

/// Schema tag mixed into the canonical RLMF attestation commitment.
pub const RLMF_ATTESTATION_SCHEMA_V2: &str = "rlmf.chain_attestation.v2";

/// RLMF attestation record submitted by Fractalwork / DataEvol via
/// `fractal_submitRlmfAttestation`. All hash fields are 0x-prefixed 32-byte
/// hex strings. `commitment_hash` must equal the canonical commitment over
/// the remaining fields (see [`RlmfAttestationRecord::canonical_commitment`]).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RlmfAttestationRecord {
    pub commitment_hash: String,
    pub subject_id: String,
    pub source_system: String,
    pub dataset_hash: String,
    pub job_hash: String,
    pub judge_report_hash: String,
    pub benchmark_report_hash: String,
    pub model_artifact_hash: String,
    pub promotion_decision: String,
    #[serde(default)]
    pub evidence_hashes: Vec<String>,
    #[serde(default)]
    pub lineage_hashes: Vec<String>,
}

/// Indexed RLMF attestation as stored by the node and returned by queries.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RlmfAttestationStored {
    pub record: RlmfAttestationRecord,
    pub transaction_hash: String,
    pub block_number: u64,
    pub finalized: bool,
}

/// Response from `fractal_submitRlmfAttestation`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RlmfAttestationResponse {
    pub network: String,
    pub transaction_hash: String,
    pub block_number: u64,
    pub finalized: bool,
    pub attestation: RlmfAttestationRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RpcLifeCommandInput {
    pub command_id: Option<String>,
    pub kind: String,
    pub soul_id: String,
    pub counterparty_id: Option<String>,
    pub epoch: u64,
    #[serde(default)]
    pub amount_micro_credits: u128,
    #[serde(default)]
    pub payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcLifeCommandRecord {
    pub command_id: String,
    pub kind: String,
    pub soul_id_hash: String,
    pub counterparty_hash: Option<String>,
    pub epoch: u64,
    pub amount_micro_credits: String,
    pub payload_hash: String,
    pub signer: Option<String>,
    pub sequence: Option<u64>,
    pub transaction_hash: Option<String>,
    pub block_number: Option<u64>,
    pub finalized: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcLifeCommandResponse {
    pub network: String,
    pub transaction_hash: String,
    pub block_number: u64,
    pub finalized: bool,
    pub command: RpcLifeCommandRecord,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcLifeEventRecord {
    pub event_id: String,
    pub command_id: String,
    pub kind: String,
    pub soul_id_hash: String,
    pub epoch: u64,
    pub amount_micro_credits: String,
    pub payload_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcSupplyResponse {
    pub network: String,
    pub block_number: String,
    pub max_supply_wei: String,
    pub protocol_minted_wei: String,
    pub protocol_burned_wei: String,
    pub circulating_supply_wei: String,
    pub provider_pool_wei: String,
    pub consensus_pool_wei: String,
    pub intelligence_pool_wei: String,
    pub provider_rollover_wei: String,
    pub consensus_rollover_wei: String,
    pub intelligence_rollover_wei: String,
}

fn hash_text(value: &str) -> [u8; 32] {
    keccak256(value.as_bytes())
}

fn parse_life_command_kind(kind: &str) -> Result<fractal_core::LifeCommandKind, &'static str> {
    use fractal_core::LifeCommandKind::*;
    match kind {
        "birth_grant" | "birthGrant" => Ok(BirthGrant),
        "birth_spawn" | "birthSpawn" => Ok(BirthSpawn),
        "birth_player_funded" | "birthPlayerFunded" => Ok(BirthPlayerFunded),
        "rent_charge" | "rentCharge" => Ok(RentCharge),
        "loan_open" | "loanOpen" => Ok(LoanOpen),
        "loan_accept" | "loanAccept" => Ok(LoanAccept),
        "loan_repay" | "loanRepay" => Ok(LoanRepay),
        "extension_purchase" | "extensionPurchase" => Ok(ExtensionPurchase),
        "will_register" | "willRegister" => Ok(WillRegister),
        "will_update" | "willUpdate" => Ok(WillUpdate),
        "owner_topup" | "ownerTopUp" => Ok(OwnerTopUp),
        "withdrawal_request" | "withdrawalRequest" => Ok(WithdrawalRequest),
        "withdrawal_settlement" | "withdrawalSettlement" => Ok(WithdrawalSettlement),
        "sii_commit" | "siiCommit" => Ok(SiiCommit),
        "ladder_commit" | "ladderCommit" => Ok(LadderCommit),
        "benchmark_freeze" | "benchmarkFreeze" => Ok(BenchmarkFreeze),
        "intelligence_payout" | "intelligencePayout" => Ok(IntelligencePayout),
        "provenance_bond" | "provenanceBond" => Ok(ProvenanceBond),
        "feedback_artifact" | "feedbackArtifact" => Ok(FeedbackArtifact),
        "sealed_sale" | "sealedSale" => Ok(SealedSale),
        "reaper_epoch" | "reaperEpoch" => Ok(ReaperEpoch),
        _ => Err("unknown life command kind"),
    }
}

fn build_life_command(
    input: RpcLifeCommandInput,
) -> Result<fractal_core::LifeCommandV1, ErrorObjectOwned> {
    let kind = parse_life_command_kind(&input.kind).map_err(err_invalid_params)?;
    let soul_id_hash = hash_text(&input.soul_id);
    let counterparty_hash = input.counterparty_id.as_deref().map(hash_text);
    let payload_bytes = serde_json::to_vec(&input.payload)
        .map_err(|_| err_invalid_params("life payload must be JSON serializable"))?;
    let payload_hash = keccak256(&payload_bytes);
    let command_id = if let Some(raw) = input.command_id {
        parse_hash32_hex(&raw).map_err(|_| err_invalid_params("invalid commandId hex"))?
    } else {
        keccak256(
            &borsh::to_vec(&(
                b"life.command.v1",
                &kind,
                soul_id_hash,
                counterparty_hash,
                input.epoch,
                input.amount_micro_credits,
                payload_hash,
            ))
            .map_err(|_| err_invalid_params("life command encode failed"))?,
        )
    };
    Ok(fractal_core::LifeCommandV1 {
        command_id,
        kind,
        soul_id_hash,
        counterparty_hash,
        epoch: input.epoch,
        amount_micro_credits: input.amount_micro_credits,
        payload_hash,
    })
}

fn parse_hash32_hex(value: &str) -> Result<[u8; 32], &'static str> {
    let hex_str = value.strip_prefix("0x").unwrap_or(value);
    let bytes = hex::decode(hex_str).map_err(|_| "invalid hash hex")?;
    if bytes.len() != 32 {
        return Err("hash must be 32 bytes");
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Ok(out)
}

impl RlmfAttestationRecord {
    /// Deterministic canonical commitment over every field except
    /// `commitment_hash` itself: keccak256 of the schema tag plus each field
    /// encoded as `len(u32 LE) || bytes` in declaration order; hash lists are
    /// encoded as `count(u32 LE)` followed by raw 32-byte entries.
    ///
    /// Returns an error when any field fails validation, so a canonical
    /// commitment only exists for well-formed records.
    pub fn canonical_commitment(&self) -> Result<[u8; 32], &'static str> {
        self.validate_fields()?;
        let mut buf: Vec<u8> = Vec::with_capacity(512);
        let push_str = |buf: &mut Vec<u8>, value: &str| {
            buf.extend_from_slice(&(value.len() as u32).to_le_bytes());
            buf.extend_from_slice(value.as_bytes());
        };
        push_str(&mut buf, RLMF_ATTESTATION_SCHEMA_V2);
        push_str(&mut buf, &self.subject_id);
        push_str(&mut buf, &self.source_system);
        for field in [
            &self.dataset_hash,
            &self.job_hash,
            &self.judge_report_hash,
            &self.benchmark_report_hash,
            &self.model_artifact_hash,
        ] {
            let hash = parse_hash32_hex(field)?;
            buf.extend_from_slice(&hash);
        }
        push_str(&mut buf, &self.promotion_decision);
        for list in [&self.evidence_hashes, &self.lineage_hashes] {
            buf.extend_from_slice(&(list.len() as u32).to_le_bytes());
            for entry in list {
                let hash = parse_hash32_hex(entry)?;
                buf.extend_from_slice(&hash);
            }
        }
        Ok(keccak256(&buf))
    }

    fn validate_fields(&self) -> Result<(), &'static str> {
        if self.subject_id.is_empty() || self.subject_id.len() > 256 {
            return Err("subjectId must be 1..=256 characters");
        }
        if self.source_system.is_empty() || self.source_system.len() > 256 {
            return Err("sourceSystem must be 1..=256 characters");
        }
        if !RLMF_PROMOTION_DECISIONS.contains(&self.promotion_decision.as_str()) {
            return Err(
                "promotionDecision must be one of promote|reject|inconclusive|shadow|canary|rollback",
            );
        }
        if self.evidence_hashes.len() > RLMF_MAX_HASH_LIST {
            return Err("too many evidenceHashes (max 64)");
        }
        if self.lineage_hashes.len() > RLMF_MAX_HASH_LIST {
            return Err("too many lineageHashes (max 64)");
        }
        Ok(())
    }

    /// Validate the record and verify `commitment_hash` matches the canonical
    /// commitment. Returns the parsed 32-byte commitment.
    pub fn validate(&self) -> Result<[u8; 32], &'static str> {
        let claimed =
            parse_hash32_hex(&self.commitment_hash).map_err(|_| "invalid commitmentHash hex")?;
        let computed = self.canonical_commitment()?;
        if claimed != computed {
            return Err("commitmentHash does not match canonical commitment of record fields");
        }
        Ok(claimed)
    }
}

/// Response from `fractal_submitProofUpdate`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcProofUpdateSubmission {
    pub network: String,
    pub proof_update_hash: String,
    pub zone_id: String,
    pub height: String,
    pub pending_proof_updates: String,
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

    fn shard_id(&self) -> u32 {
        0
    }

    fn shard_count(&self) -> u32 {
        1
    }

    fn consensus_mode(&self) -> String {
        "singleton".into()
    }

    fn balance_of(&self, addr: &Address) -> u128;

    fn transaction_count(&self, addr: &Address) -> u64;

    /// Hex is `0x` + raw **borsh** `Transaction` bytes (dev stub until RLP exists).
    fn submit_raw_tx(&mut self, raw: &[u8]) -> Result<(), String>;

    fn routing_diagnostics_for_raw_tx(&self, raw: &[u8]) -> Result<RpcRoutingDiagnostics, String> {
        let tx = fractal_core::Transaction::try_from_slice(raw)
            .map_err(|e| format!("invalid Transaction borsh: {e}"))?;
        Ok(rpc_routing_diagnostics_for_transaction(
            &tx,
            self.shard_id(),
            self.shard_count(),
        ))
    }

    fn base_fee_per_gas(&self) -> u128;

    fn block_hash_by_number(&self, number: u64) -> Option<[u8; 32]>;

    fn block_by_hash(&self, hash: &[u8; 32]) -> Option<fractal_consensus::Block>;

    fn block_is_proof_final(&self, _hash: &[u8; 32]) -> bool {
        false
    }

    fn proof_for_block(&self, _hash: &[u8; 32]) -> Option<fractal_consensus::BlockValidityProof> {
        None
    }

    fn latest_proof_final_height_for_zone(&self, _zone_id: u64) -> Option<u64> {
        None
    }

    fn zone_update_finality(&self, _zone_id: u64, _height: u64) -> Option<String> {
        None
    }

    fn owned_object_finality(
        &self,
        object_version: &fractal_core::OwnedObjectVersion,
    ) -> Option<(String, String)> {
        let _ = object_version;
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

    fn consensus_diagnostics(&self) -> RpcConsensusDiagnostics;

    fn mempool_lane_metrics(&self) -> RpcMempoolLaneMetrics;

    fn chain_config(&self) -> RpcChainConfig;

    fn submit_validity_proof(
        &mut self,
        proof: fractal_consensus::BlockValidityProof,
    ) -> Result<[u8; 32], String>;

    fn owned_object_precheck(
        &self,
        _raw_tx: &[u8],
        _max_fee_per_gas: u128,
    ) -> Result<RpcOwnedObjectPrecheck, String> {
        Err("owned-object precheck unsupported".into())
    }

    fn countersign_owned_object_tx(
        &self,
        _raw_tx: &[u8],
        _max_fee_per_gas: u128,
    ) -> Result<RpcOwnedObjectCountersignature, String> {
        Err("owned-object countersign unsupported".into())
    }

    fn aggregate_owned_object_certificate(
        &self,
        _raw_tx: &[u8],
        _object_versions_borsh: &[u8],
        _signatures_borsh: Vec<Vec<u8>>,
    ) -> Result<RpcOwnedObjectCertificate, String> {
        Err("owned-object certificate aggregation unsupported".into())
    }

    /// Record a research-proof hash commitment. Default: not supported.
    ///
    /// The hash is **generic**: `fractal_submitProofHash` accepts *any* 32-byte
    /// content hash. This covers both pipeline proof hashes (from
    /// `fractal-society`'s trading/research pipeline) and arbitrary research
    /// package content hashes committed via the generic commit service (AR-01,
    /// `commit_research_package`). No new RPC method is required for packages —
    /// they reuse this endpoint.
    fn submit_proof_hash(
        &mut self,
        _proof_hash: [u8; 32],
    ) -> Result<ProofCommitmentResponse, String> {
        Err("fractal_submitProofHash not supported on this node".to_string())
    }

    fn submit_proof_update(
        &mut self,
        _update: fractal_consensus::ZoneProofUpdateV1,
        _max_priority_fee: u128,
    ) -> Result<RpcProofUpdateSubmission, String> {
        Err("fractal_submitProofUpdate not supported on this node".to_string())
    }

    /// Submit an RLMF attestation record. Default: not supported.
    fn submit_rlmf_attestation(
        &mut self,
        _record: RlmfAttestationRecord,
    ) -> Result<RlmfAttestationResponse, String> {
        Err("fractal_submitRlmfAttestation not supported on this node".to_string())
    }

    /// Look up an indexed RLMF attestation by its 32-byte commitment hash.
    fn rlmf_attestation_by_commitment(&self, _hash: [u8; 32]) -> Option<RlmfAttestationStored> {
        None
    }

    /// List indexed RLMF attestations with optional filters, newest block first.
    fn list_rlmf_attestations(
        &self,
        _subject_id: Option<&str>,
        _source_system: Option<&str>,
        _block_number: Option<u64>,
        _transaction_hash: Option<&str>,
        _limit: usize,
    ) -> Vec<RlmfAttestationStored> {
        Vec::new()
    }

    fn submit_life_command(
        &mut self,
        _command: fractal_core::LifeCommandV1,
    ) -> Result<RpcLifeCommandResponse, String> {
        Err("fractal_submitLifeCommand not supported on this node".to_string())
    }

    fn life_command_by_id(&self, _command_id: [u8; 32]) -> Option<RpcLifeCommandRecord> {
        None
    }

    fn list_life_events(
        &self,
        _kind: Option<&str>,
        _epoch: Option<u64>,
        _limit: usize,
    ) -> Vec<RpcLifeEventRecord> {
        Vec::new()
    }

    fn supply(&self) -> RpcSupplyResponse {
        RpcSupplyResponse {
            network: "unsupported".to_string(),
            block_number: "0x0".to_string(),
            max_supply_wei: "0x0".to_string(),
            protocol_minted_wei: "0x0".to_string(),
            protocol_burned_wei: "0x0".to_string(),
            circulating_supply_wei: "0x0".to_string(),
            provider_pool_wei: "0x0".to_string(),
            consensus_pool_wei: "0x0".to_string(),
            intelligence_pool_wei: "0x0".to_string(),
            provider_rollover_wei: "0x0".to_string(),
            consensus_rollover_wei: "0x0".to_string(),
            intelligence_rollover_wei: "0x0".to_string(),
        }
    }
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
        .register_async_method("fractal_getShardId", |_params: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                let g = ctx.lock().await;
                Ok::<String, ErrorObjectOwned>(format!("0x{:x}", g.shard_id()))
            }
        })
        .expect("register fractal_getShardId");

    module
        .register_async_method(
            "fractal_getShardCount",
            |_params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let g = ctx.lock().await;
                    Ok::<String, ErrorObjectOwned>(format!("0x{:x}", g.shard_count()))
                }
            },
        )
        .expect("register fractal_getShardCount");

    module
        .register_async_method(
            "fractal_getConsensusMode",
            |_params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let g = ctx.lock().await;
                    Ok::<String, ErrorObjectOwned>(g.consensus_mode())
                }
            },
        )
        .expect("register fractal_getConsensusMode");

    module
        .register_async_method(
            "fractal_consensusDiagnostics",
            |_params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let g = ctx.lock().await;
                    Ok::<RpcConsensusDiagnostics, ErrorObjectOwned>(g.consensus_diagnostics())
                }
            },
        )
        .expect("register fractal_consensusDiagnostics");

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
            "fractal_debugTxRouting",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let tx_hex: String = params
                        .one()
                        .map_err(|_| err_invalid_params("expected raw transaction hex"))?;
                    let tx_bytes = parse_bytes_hex(&tx_hex)?;
                    let g = ctx.lock().await;
                    let diagnostics = g
                        .routing_diagnostics_for_raw_tx(&tx_bytes)
                        .map_err(|e| ErrorObjectOwned::owned(-32602, e, None::<()>))?;
                    Ok::<RpcRoutingDiagnostics, ErrorObjectOwned>(diagnostics)
                }
            },
        )
        .expect("register fractal_debugTxRouting");

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
            "fractal_ownedObjectPrecheck",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let args: Vec<serde_json::Value> = params
                        .parse()
                        .map_err(|_| err_invalid_params("expected [rawTxHex, maxFeePerGas?]"))?;
                    let raw_hex = args
                        .first()
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| err_invalid_params("missing rawTxHex"))?;
                    let max_fee = args
                        .get(1)
                        .and_then(|v| v.as_str())
                        .map(parse_u128_quantity_or_decimal)
                        .transpose()?
                        .unwrap_or(1);
                    let raw = parse_bytes_hex(raw_hex)?;
                    let g = ctx.lock().await;
                    let response = g
                        .owned_object_precheck(&raw, max_fee)
                        .map_err(|e| ErrorObjectOwned::owned(-32000, e, None::<()>))?;
                    Ok::<RpcOwnedObjectPrecheck, ErrorObjectOwned>(response)
                }
            },
        )
        .expect("register fractal_ownedObjectPrecheck");

    module
        .register_async_method(
            "fractal_countersignOwnedObjectTx",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let args: Vec<serde_json::Value> = params
                        .parse()
                        .map_err(|_| err_invalid_params("expected [rawTxHex, maxFeePerGas?]"))?;
                    let raw_hex = args
                        .first()
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| err_invalid_params("missing rawTxHex"))?;
                    let max_fee = args
                        .get(1)
                        .and_then(|v| v.as_str())
                        .map(parse_u128_quantity_or_decimal)
                        .transpose()?
                        .unwrap_or(1);
                    let raw = parse_bytes_hex(raw_hex)?;
                    let g = ctx.lock().await;
                    let response = g
                        .countersign_owned_object_tx(&raw, max_fee)
                        .map_err(|e| ErrorObjectOwned::owned(-32000, e, None::<()>))?;
                    Ok::<RpcOwnedObjectCountersignature, ErrorObjectOwned>(response)
                }
            },
        )
        .expect("register fractal_countersignOwnedObjectTx");

    module
        .register_async_method(
            "fractal_aggregateOwnedObjectCertificate",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let args: Vec<serde_json::Value> = params.parse().map_err(|_| {
                        err_invalid_params(
                            "expected [rawTxHex, objectVersionsBorshHex, signatureBorshHexArray]",
                        )
                    })?;
                    let raw_hex = args
                        .first()
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| err_invalid_params("missing rawTxHex"))?;
                    let versions_hex = args
                        .get(1)
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| err_invalid_params("missing objectVersionsBorshHex"))?;
                    let signatures = args
                        .get(2)
                        .and_then(|v| v.as_array())
                        .ok_or_else(|| err_invalid_params("missing signatureBorshHexArray"))?
                        .iter()
                        .map(|v| {
                            v.as_str()
                                .ok_or_else(|| err_invalid_params("signature must be hex"))
                                .and_then(parse_bytes_hex)
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let raw = parse_bytes_hex(raw_hex)?;
                    let object_versions = parse_bytes_hex(versions_hex)?;
                    let g = ctx.lock().await;
                    let response = g
                        .aggregate_owned_object_certificate(&raw, &object_versions, signatures)
                        .map_err(|e| ErrorObjectOwned::owned(-32000, e, None::<()>))?;
                    Ok::<RpcOwnedObjectCertificate, ErrorObjectOwned>(response)
                }
            },
        )
        .expect("register fractal_aggregateOwnedObjectCertificate");

    module
        .register_async_method(
            "fractal_getOwnedObjectFinality",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let object_version_hex: String = params
                        .one()
                        .map_err(|_| err_invalid_params("expected borsh object version hex"))?;
                    let object_version_bytes = parse_bytes_hex(&object_version_hex)?;
                    let object_version =
                        fractal_core::OwnedObjectVersion::try_from_slice(&object_version_bytes)
                            .map_err(|_| err_invalid_params("invalid OwnedObjectVersion borsh"))?;
                    let g = ctx.lock().await;
                    let (finality_status, certificate_hash, certificate_borsh) = g
                        .owned_object_finality(&object_version)
                        .map(|(hash, cert)| ("certificate".to_owned(), Some(hash), Some(cert)))
                        .unwrap_or_else(|| ("none".to_owned(), None, None));
                    Ok::<RpcOwnedObjectFinality, ErrorObjectOwned>(RpcOwnedObjectFinality {
                        object_version_borsh: format!("0x{}", hex::encode(object_version_bytes)),
                        finality_status,
                        certificate_hash,
                        certificate_borsh,
                    })
                }
            },
        )
        .expect("register fractal_getOwnedObjectFinality");

    module
        .register_async_method(
            "fractal_getProofFinalHeight",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let zone_id_raw: String = params
                        .one()
                        .map_err(|_| err_invalid_params("expected zone id"))?;
                    let zone_id = parse_u64_quantity_or_decimal(&zone_id_raw)?;
                    let g = ctx.lock().await;
                    Ok::<RpcZoneProofFinalHeight, ErrorObjectOwned>(RpcZoneProofFinalHeight {
                        zone_id: quantity_hex_u64(zone_id),
                        proof_final_height: g
                            .latest_proof_final_height_for_zone(zone_id)
                            .map(quantity_hex_u64),
                    })
                }
            },
        )
        .expect("register fractal_getProofFinalHeight");

    module
        .register_async_method(
            "fractal_getZoneUpdateFinality",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let (zone_id_raw, height_raw): (String, String) = params
                        .parse()
                        .map_err(|_| err_invalid_params("expected [zoneId, height]"))?;
                    let zone_id = parse_u64_quantity_or_decimal(&zone_id_raw)?;
                    let height = parse_u64_quantity_or_decimal(&height_raw)?;
                    let g = ctx.lock().await;
                    let finality = g
                        .zone_update_finality(zone_id, height)
                        .unwrap_or_else(|| "none".into());
                    Ok::<RpcZoneUpdateFinality, ErrorObjectOwned>(RpcZoneUpdateFinality {
                        zone_id: quantity_hex_u64(zone_id),
                        height: quantity_hex_u64(height),
                        finality_status: finality,
                    })
                }
            },
        )
        .expect("register fractal_getZoneUpdateFinality");

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
        .register_async_method(
            "fractal_submitProofUpdate",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let arr: Vec<serde_json::Value> = params
                        .parse()
                        .map_err(|_| err_invalid_params("expected proof update borsh hex"))?;
                    let first = arr
                        .first()
                        .ok_or(err_invalid_params("expected proof update borsh hex"))?;
                    let update_hex = first
                        .get("proofUpdate")
                        .and_then(|v| v.as_str())
                        .or_else(|| first.get("proof_update").and_then(|v| v.as_str()))
                        .or_else(|| first.get("proofUpdateBorsh").and_then(|v| v.as_str()))
                        .or_else(|| first.get("proof_update_borsh").and_then(|v| v.as_str()))
                        .or_else(|| first.as_str())
                        .ok_or(err_invalid_params("missing proof update borsh hex"))?;
                    let max_priority_fee = first
                        .get("maxPriorityFee")
                        .and_then(|v| v.as_str())
                        .or_else(|| first.get("max_priority_fee").and_then(|v| v.as_str()))
                        .map(parse_u128_quantity_or_decimal)
                        .transpose()?
                        .unwrap_or(0);
                    let update_bytes = parse_bytes_hex(update_hex)?;
                    let update =
                        fractal_consensus::ZoneProofUpdateV1::try_from_slice(&update_bytes)
                            .map_err(|_| err_invalid_params("invalid ZoneProofUpdateV1 borsh"))?;
                    let mut g = ctx.lock().await;
                    let response = g
                        .submit_proof_update(update, max_priority_fee)
                        .map_err(|e| ErrorObjectOwned::owned(-32603, e, None::<()>))?;
                    Ok::<RpcProofUpdateSubmission, ErrorObjectOwned>(response)
                }
            },
        )
        .expect("register fractal_submitProofUpdate");

    module
        .register_async_method(
            "fractal_submitProofHash",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let arr: Vec<serde_json::Value> = params
                        .parse()
                        .map_err(|_| err_invalid_params("expected proof_hash object"))?;
                    let obj = arr
                        .first()
                        .ok_or(err_invalid_params("expected proof_hash object"))?;
                    let hex = obj
                        .get("proof_hash")
                        .and_then(|v| v.as_str())
                        .or_else(|| obj.as_str())
                        .ok_or(err_invalid_params("missing proof_hash string"))?;
                    let hex = hex.strip_prefix("0x").unwrap_or(hex);
                    let bytes = hex::decode(hex)
                        .map_err(|_| err_invalid_params("invalid proof_hash hex"))?;
                    if bytes.len() != 32 {
                        return Err(err_invalid_params("proof_hash must be 32 bytes"));
                    }
                    let mut hash = [0u8; 32];
                    hash.copy_from_slice(&bytes);
                    let mut g = ctx.lock().await;
                    let response = g
                        .submit_proof_hash(hash)
                        .map_err(|e| ErrorObjectOwned::owned(-32603, e, None::<()>))?;
                    Ok::<ProofCommitmentResponse, ErrorObjectOwned>(response)
                }
            },
        )
        .expect("register fractal_submitProofHash");

    module
        .register_async_method(
            "fractal_submitRlmfAttestation",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let arr: Vec<serde_json::Value> = params
                        .parse()
                        .map_err(|_| err_invalid_params("expected attestation record object"))?;
                    let obj = arr
                        .first()
                        .cloned()
                        .ok_or(err_invalid_params("expected attestation record object"))?;
                    let record: RlmfAttestationRecord = serde_json::from_value(obj)
                        .map_err(|_| err_invalid_params("malformed attestation record"))?;
                    record.validate().map_err(err_invalid_params)?;
                    let mut g = ctx.lock().await;
                    let response = g
                        .submit_rlmf_attestation(record)
                        .map_err(|e| ErrorObjectOwned::owned(-32603, e, None::<()>))?;
                    Ok::<RlmfAttestationResponse, ErrorObjectOwned>(response)
                }
            },
        )
        .expect("register fractal_submitRlmfAttestation");

    module
        .register_async_method(
            "fractal_getRlmfAttestation",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let arr: Vec<serde_json::Value> = params
                        .parse()
                        .map_err(|_| err_invalid_params("expected commitment hash"))?;
                    let obj = arr
                        .first()
                        .ok_or(err_invalid_params("expected commitment hash"))?;
                    let hex_value = obj
                        .get("commitmentHash")
                        .and_then(|v| v.as_str())
                        .or_else(|| obj.as_str())
                        .ok_or(err_invalid_params("missing commitmentHash string"))?;
                    let hash = {
                        let hex_str = hex_value.strip_prefix("0x").unwrap_or(hex_value);
                        let bytes = hex::decode(hex_str)
                            .map_err(|_| err_invalid_params("invalid commitmentHash hex"))?;
                        if bytes.len() != 32 {
                            return Err(err_invalid_params("commitmentHash must be 32 bytes"));
                        }
                        let mut hash = [0u8; 32];
                        hash.copy_from_slice(&bytes);
                        hash
                    };
                    let g = ctx.lock().await;
                    Ok::<Option<RlmfAttestationStored>, ErrorObjectOwned>(
                        g.rlmf_attestation_by_commitment(hash),
                    )
                }
            },
        )
        .expect("register fractal_getRlmfAttestation");

    module
        .register_async_method(
            "fractal_listRlmfAttestations",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let arr: Vec<serde_json::Value> = params.parse().unwrap_or_default();
                    let filter = arr.first().cloned().unwrap_or(serde_json::Value::Null);
                    let subject_id = filter
                        .get("subjectId")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    let source_system = filter
                        .get("sourceSystem")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    let block_number = filter.get("blockNumber").and_then(|v| {
                        v.as_u64().or_else(|| {
                            v.as_str().and_then(|raw| {
                                let raw = raw.strip_prefix("0x").unwrap_or(raw);
                                u64::from_str_radix(raw, 16).ok()
                            })
                        })
                    });
                    let transaction_hash = filter
                        .get("transactionHash")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    let limit = filter
                        .get("limit")
                        .and_then(|v| v.as_u64())
                        .map(|v| v.min(256) as usize)
                        .unwrap_or(64);
                    let g = ctx.lock().await;
                    Ok::<Vec<RlmfAttestationStored>, ErrorObjectOwned>(g.list_rlmf_attestations(
                        subject_id.as_deref(),
                        source_system.as_deref(),
                        block_number,
                        transaction_hash.as_deref(),
                        limit,
                    ))
                }
            },
        )
        .expect("register fractal_listRlmfAttestations");

    module
        .register_async_method(
            "fractal_submitLifeCommand",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let arr: Vec<serde_json::Value> = params
                        .parse()
                        .map_err(|_| err_invalid_params("expected life command object"))?;
                    let obj = arr
                        .first()
                        .cloned()
                        .ok_or(err_invalid_params("expected life command object"))?;
                    let input: RpcLifeCommandInput = serde_json::from_value(obj)
                        .map_err(|_| err_invalid_params("malformed life command"))?;
                    let command = build_life_command(input)?;
                    let mut g = ctx.lock().await;
                    let response = g
                        .submit_life_command(command)
                        .map_err(|e| ErrorObjectOwned::owned(-32603, e, None::<()>))?;
                    Ok::<RpcLifeCommandResponse, ErrorObjectOwned>(response)
                }
            },
        )
        .expect("register fractal_submitLifeCommand");

    module
        .register_async_method("fractal_getSupply", |_: Params<'static>, ctx, _| {
            let ctx = ctx.clone();
            async move {
                let g = ctx.lock().await;
                Ok::<RpcSupplyResponse, ErrorObjectOwned>(g.supply())
            }
        })
        .expect("register fractal_getSupply");

    module
        .register_async_method(
            "fractal_getLifeCommand",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let arr: Vec<serde_json::Value> = params
                        .parse()
                        .map_err(|_| err_invalid_params("expected command id"))?;
                    let obj = arr
                        .first()
                        .ok_or(err_invalid_params("expected command id"))?;
                    let raw = obj
                        .get("commandId")
                        .and_then(|v| v.as_str())
                        .or_else(|| obj.as_str())
                        .ok_or(err_invalid_params("missing commandId string"))?;
                    let command_id = parse_hash32_hex(raw)
                        .map_err(|_| err_invalid_params("invalid commandId hex"))?;
                    let g = ctx.lock().await;
                    Ok::<Option<RpcLifeCommandRecord>, ErrorObjectOwned>(
                        g.life_command_by_id(command_id),
                    )
                }
            },
        )
        .expect("register fractal_getLifeCommand");

    module
        .register_async_method(
            "fractal_listLifeEvents",
            |params: Params<'static>, ctx, _| {
                let ctx = ctx.clone();
                async move {
                    let arr: Vec<serde_json::Value> = params.parse().unwrap_or_default();
                    let filter = arr.first().cloned().unwrap_or(serde_json::Value::Null);
                    let kind = filter
                        .get("kind")
                        .and_then(|v| v.as_str())
                        .map(str::to_string);
                    let epoch = filter.get("epoch").and_then(|v| {
                        v.as_u64().or_else(|| {
                            v.as_str().and_then(|raw| {
                                let raw = raw.strip_prefix("0x").unwrap_or(raw);
                                u64::from_str_radix(raw, 16).ok()
                            })
                        })
                    });
                    let limit = filter
                        .get("limit")
                        .and_then(|v| v.as_u64())
                        .map(|v| v.min(512) as usize)
                        .unwrap_or(128);
                    let g = ctx.lock().await;
                    Ok::<Vec<RpcLifeEventRecord>, ErrorObjectOwned>(g.list_life_events(
                        kind.as_deref(),
                        epoch,
                        limit,
                    ))
                }
            },
        )
        .expect("register fractal_listLifeEvents");

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
        OwnedObjectId::ProofCommitment(proof_hash) => {
            format!("proofCommitment:{}", hash_hex(proof_hash))
        }
        OwnedObjectId::LifeCommand(command_id) => {
            format!("lifeCommand:{}", hash_hex(command_id))
        }
    }
}

pub fn rpc_routing_diagnostics_for_transaction(
    tx: &fractal_core::Transaction,
    source_shard: u32,
    shard_count: u32,
) -> RpcRoutingDiagnostics {
    let topology = fractal_shard::ShardTopology { shard_count };
    let diagnostics =
        fractal_shard::routing_diagnostics_for_transaction(tx, source_shard, &topology);
    RpcRoutingDiagnostics {
        source_shard: quantity_hex_u64(u64::from(diagnostics.source_shard)),
        expected_shard: quantity_hex_u64(u64::from(diagnostics.expected_shard)),
        shard_count: quantity_hex_u64(u64::from(diagnostics.shard_count)),
        route_key: diagnostics.route_key,
        accepted: diagnostics.accepted,
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
        payload_type: b.payload_kind().as_str().into(),
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
        fractal_core::TxBody::Native(c) => (
            None,
            u256_quantity_hex(0),
            format!("0x{}", hex::encode(borsh::to_vec(c).unwrap_or_default())),
            quantity_hex_u64(fractal_core::intrinsic_gas(tx).unwrap_or(0)),
        ),
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
