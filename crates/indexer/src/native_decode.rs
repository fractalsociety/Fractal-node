//! Decode native txs into indexer-friendly labels + JSON payloads.

use fractal_core::{NativeCall, Transaction, TxBody, VmKind};
use serde_json::{json, Value};

pub fn vm_kind_label(vm: &VmKind) -> &'static str {
    match vm {
        VmKind::Native => "Native",
        VmKind::Evm => "Evm",
    }
}

pub fn native_call_kind(call: &NativeCall) -> &'static str {
    match call {
        NativeCall::RegisterAgent { .. } => "RegisterAgent",
        NativeCall::UpdateAgent { .. } => "UpdateAgent",
        NativeCall::SuspendAgent { .. } => "SuspendAgent",
        NativeCall::SettleReceipt(_) => "SettleReceipt",
        NativeCall::SettleBatch(_) => "SettleBatch",
        NativeCall::ClaimPayout { .. } => "ClaimPayout",
        NativeCall::FileDispute { .. } => "FileDispute",
        NativeCall::ResolveDispute { .. } => "ResolveDispute",
        NativeCall::Stake { .. } => "Stake",
        NativeCall::Unstake { .. } => "Unstake",
        NativeCall::Slash { .. } => "Slash",
        NativeCall::Delegate { .. } => "Delegate",
        NativeCall::WithdrawRewards { .. } => "WithdrawRewards",
        NativeCall::NoOp => "NoOp",
        NativeCall::WalletTaskReceiptAnchorV1 { .. } => "WalletTaskReceiptAnchorV1",
        NativeCall::SetChainEconomics { .. } => "SetChainEconomics",
        NativeCall::ProofCommitmentV1 { .. } => "ProofCommitmentV1",
        NativeCall::LifeCommandV1(_) => "LifeCommandV1",
    }
}

pub fn is_wallet_native(kind: &str) -> bool {
    kind.starts_with("Wallet")
}

pub fn tx_payload_json(tx: &Transaction) -> Value {
    match &tx.body {
        TxBody::Transfer { to, amount } => json!({
            "type": "Transfer",
            "to": addr_hex(to),
            "amount": amount.to_string(),
        }),
        TxBody::Native(call) => native_call_payload(call),
        TxBody::EvmCall {
            to,
            value,
            calldata,
            gas_limit,
        } => json!({
            "type": "EvmCall",
            "to": addr_hex(to),
            "value": value.to_string(),
            "calldataLen": calldata.len(),
            "gasLimit": gas_limit,
        }),
        TxBody::EvmCreate {
            value,
            init_code,
            gas_limit,
        } => json!({
            "type": "EvmCreate",
            "value": value.to_string(),
            "initCodeLen": init_code.len(),
            "gasLimit": gas_limit,
        }),
    }
}

pub fn native_call_payload(call: &NativeCall) -> Value {
    let kind = native_call_kind(call);
    let mut v = match call {
        NativeCall::RegisterAgent {
            operator,
            pubkey,
            kind: agent_kind,
            metadata_uri,
        } => json!({
            "type": kind,
            "operator": addr_hex(operator),
            "pubkey": hex32(pubkey),
            "agentKind": agent_kind,
            "metadataUri": metadata_uri,
        }),
        NativeCall::SettleReceipt(r) => json!({
            "type": kind,
            "worker": r.worker,
            "requester": addr_hex(&r.requester),
            "payoutAmount": r.payout_amount.to_string(),
            "score": r.score,
            "finalStatus": r.final_status,
        }),
        NativeCall::SettleBatch(p) => json!({
            "type": kind,
            "batchId": hex32(&p.batch_id),
            "operator": addr_hex(&p.operator),
            "receiptCount": p.receipts.len(),
        }),
        NativeCall::WalletTaskReceiptAnchorV1 {
            commitment,
            receipt_witness,
        } => json!({
            "type": kind,
            "commitment": hex32(commitment),
            "receiptWitnessLen": receipt_witness.len(),
        }),
        NativeCall::ProofCommitmentV1 { proof_hash } => json!({
            "type": kind,
            "proofHash": hex32(proof_hash),
        }),
        NativeCall::ClaimPayout {
            batch_id,
            account,
            amount,
            leaf_index,
            proof,
        } => json!({
            "type": kind,
            "batchId": hex32(batch_id),
            "account": addr_hex(account),
            "amount": amount.to_string(),
            "leafIndex": leaf_index,
            "proofLen": proof.len(),
        }),
        NativeCall::FileDispute {
            receipt_id,
            reason_code,
            evidence_hash,
        } => json!({
            "type": kind,
            "receiptId": hex32(receipt_id),
            "reasonCode": reason_code,
            "evidenceHash": hex32(evidence_hash),
        }),
        NativeCall::ResolveDispute {
            dispute_id,
            resolution,
            payouts_diff,
        } => json!({
            "type": kind,
            "disputeId": dispute_id,
            "resolution": resolution,
            "payoutsDiff": payouts_diff.to_string(),
        }),
        NativeCall::LifeCommandV1(command) => json!({
            "type": kind,
            "commandId": hex32(&command.command_id),
            "kind": life_kind(&command.kind),
            "soulIdHash": hex32(&command.soul_id_hash),
            "counterpartyHash": command.counterparty_hash.as_ref().map(hex32),
            "epoch": command.epoch,
            "amountMicroCredits": command.amount_micro_credits.to_string(),
            "payloadHash": hex32(&command.payload_hash),
        }),
        NativeCall::Stake { amount } | NativeCall::Unstake { amount } => {
            json!({ "type": kind, "amount": amount.to_string() })
        }
        NativeCall::Delegate { validator, amount } => json!({
            "type": kind,
            "validator": addr_hex(validator),
            "amount": amount.to_string(),
        }),
        NativeCall::WithdrawRewards { validator } => json!({
            "type": kind,
            "validator": addr_hex(validator),
        }),
        NativeCall::Slash {
            validator_id,
            evidence_hash,
        } => json!({
            "type": kind,
            "validatorId": addr_hex(validator_id),
            "evidenceHash": hex32(evidence_hash),
        }),
        _ => json!({ "type": kind }),
    };
    if let Value::Object(ref mut m) = v {
        m.insert("type".into(), json!(kind));
    }
    v
}

fn addr_hex(a: &fractal_core::Address) -> String {
    format!("0x{}", hex::encode(a))
}

fn hex32(b: &[u8; 32]) -> String {
    format!("0x{}", hex::encode(b))
}

pub fn life_kind(kind: &fractal_core::LifeCommandKind) -> &'static str {
    use fractal_core::LifeCommandKind::*;
    match kind {
        BirthGrant => "birth_grant",
        BirthSpawn => "birth_spawn",
        BirthPlayerFunded => "birth_player_funded",
        RentCharge => "rent_charge",
        LoanOpen => "loan_open",
        LoanAccept => "loan_accept",
        LoanRepay => "loan_repay",
        ExtensionPurchase => "extension_purchase",
        WillRegister => "will_register",
        WillUpdate => "will_update",
        OwnerTopUp => "owner_topup",
        WithdrawalRequest => "withdrawal_request",
        WithdrawalSettlement => "withdrawal_settlement",
        SiiCommit => "sii_commit",
        LadderCommit => "ladder_commit",
        BenchmarkFreeze => "benchmark_freeze",
        IntelligencePayout => "intelligence_payout",
        ProvenanceBond => "provenance_bond",
        FeedbackArtifact => "feedback_artifact",
        SealedSale => "sealed_sale",
        ReaperEpoch => "reaper_epoch",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallet_anchor_kind_detected() {
        assert!(is_wallet_native("WalletTaskReceiptAnchorV1"));
        assert!(!is_wallet_native("Stake"));
    }
}
