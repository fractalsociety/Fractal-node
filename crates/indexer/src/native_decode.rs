//! Decode native txs into indexer-friendly labels + JSON payloads.

use fractal_core::{NativeCall, Transaction, TxBody, VmKind};
use serde_json::{Value, json};

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
        NativeCall::DepositConsensusStake { .. } => "DepositConsensusStake",
        NativeCall::WithdrawConsensusStake { .. } => "WithdrawConsensusStake",
        NativeCall::CommitSlashingEvidence { .. } => "CommitSlashingEvidence",
        NativeCall::SlashConsensusStake { .. } => "SlashConsensusStake",
        NativeCall::SlashConsensusStakeVerified { .. } => "SlashConsensusStakeVerified",
        NativeCall::WalletReputationSnapshotV1 { .. } => "WalletReputationSnapshotV1",
        NativeCall::SetValidatorCommission { .. } => "SetValidatorCommission",
        NativeCall::RegisterValidator { .. } => "RegisterValidator",
        NativeCall::Redelegate { .. } => "Redelegate",
        NativeCall::SetChainEconomics { .. } => "SetChainEconomics",
        NativeCall::WalletMintCapabilityV1 { .. } => "WalletMintCapabilityV1",
        NativeCall::WalletCreateBudgetAccountV1 { .. } => "WalletCreateBudgetAccountV1",
        NativeCall::WalletFundBudgetAccountV1 { .. } => "WalletFundBudgetAccountV1",
        NativeCall::WalletCloseBudgetAccountV1 { .. } => "WalletCloseBudgetAccountV1",
        NativeCall::WalletRevokeCapabilityV1 { .. } => "WalletRevokeCapabilityV1",
        NativeCall::WalletPostTaskV1 { .. } => "WalletPostTaskV1",
        NativeCall::WalletCheckoutTaskV1 { .. } => "WalletCheckoutTaskV1",
        NativeCall::WalletRenewCheckoutV1 { .. } => "WalletRenewCheckoutV1",
        NativeCall::WalletSubmitTaskV1 { .. } => "WalletSubmitTaskV1",
        NativeCall::WalletVerifyTaskV1 { .. } => "WalletVerifyTaskV1",
        NativeCall::WalletFinalizeTaskV1 { .. } => "WalletFinalizeTaskV1",
        NativeCall::WalletEmergencyStopV1 { .. } => "WalletEmergencyStopV1",
        NativeCall::WalletBatchSettleV1 { .. } => "WalletBatchSettleV1",
        NativeCall::WalletRegisterProviderV1 { .. } => "WalletRegisterProviderV1",
        NativeCall::WalletStakeForClassV1 { .. } => "WalletStakeForClassV1",
        NativeCall::WalletProviderUnstakeRequestV1 { .. } => "WalletProviderUnstakeRequestV1",
        NativeCall::WalletProviderUnstakeFinalizeV1 { .. } => "WalletProviderUnstakeFinalizeV1",
        NativeCall::WalletSlashProviderV1 { .. } => "WalletSlashProviderV1",
        NativeCall::WalletUpdateProviderV1 { .. } => "WalletUpdateProviderV1",
        NativeCall::WalletDeregisterProviderV1 { .. } => "WalletDeregisterProviderV1",
        NativeCall::WalletScopedEmergencyStopV1 { .. } => "WalletScopedEmergencyStopV1",
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
            "toolClass": r.tool_class,
            "payoutAmount": r.payout_amount.to_string(),
        }),
        NativeCall::SettleBatch(p) => json!({
            "type": kind,
            "batchId": hex32(&p.batch_id),
            "receiptCount": p.receipts.len(),
        }),
        NativeCall::WalletReputationSnapshotV1 {
            provider_id,
            tool_class,
            summary_borsh,
        } => json!({
            "type": kind,
            "providerId": hex32(provider_id),
            "toolClass": tool_class,
            "summaryBorshLen": summary_borsh.len(),
        }),
        NativeCall::WalletMintCapabilityV1 {
            parent_cap_id,
            child_token_borsh,
            budget_seed,
            revocation_proof_borsh,
        } => json!({
            "type": kind,
            "parentCapId": parent_cap_id.as_ref().map(hex32),
            "childTokenBorshLen": child_token_borsh.len(),
            "hasBudgetSeed": budget_seed.is_some(),
            "revocationProofBorshLen": revocation_proof_borsh.len(),
        }),
        NativeCall::WalletRevokeCapabilityV1 {
            cap_id,
            reason_code,
            cascade,
            ..
        } => json!({
            "type": kind,
            "capId": hex32(cap_id),
            "reasonCode": reason_code,
            "cascade": cascade,
        }),
        NativeCall::WalletCreateBudgetAccountV1 {
            parent,
            initial_deposit,
        } => json!({
            "type": kind,
            "parent": parent,
            "initialDeposit": initial_deposit.to_string(),
        }),
        NativeCall::WalletFundBudgetAccountV1 {
            budget,
            amount,
            source_budget,
        } => json!({
            "type": kind,
            "budget": budget,
            "amount": amount.to_string(),
            "sourceBudget": source_budget,
        }),
        NativeCall::WalletCloseBudgetAccountV1 { budget } => json!({
            "type": kind,
            "budget": budget,
        }),
        NativeCall::WalletPostTaskV1 {
            metadata_uri,
            bounty_budget,
            tool_budget,
            verifier_budget,
        } => json!({
            "type": kind,
            "metadataUri": metadata_uri,
            "bountyBudget": bounty_budget.to_string(),
            "toolBudget": tool_budget.to_string(),
            "verifierBudget": verifier_budget.to_string(),
        }),
        NativeCall::WalletCheckoutTaskV1 {
            task_id,
            agent_session,
            expiry_ms,
        } => json!({
            "type": kind,
            "taskId": task_id,
            "agentSession": hex32(agent_session),
            "expiryMs": expiry_ms,
        }),
        NativeCall::WalletRenewCheckoutV1 {
            task_id,
            evidence_uri,
            new_expiry_ms,
        } => json!({
            "type": kind,
            "taskId": task_id,
            "evidenceUri": evidence_uri,
            "newExpiryMs": new_expiry_ms,
        }),
        NativeCall::WalletSubmitTaskV1 {
            task_id,
            artifact_pointer,
            tool_receipt_root,
        } => json!({
            "type": kind,
            "taskId": task_id,
            "artifactPointer": artifact_pointer,
            "toolReceiptRoot": hex32(tool_receipt_root),
        }),
        NativeCall::WalletVerifyTaskV1 { task_id, score, .. } => json!({
            "type": kind,
            "taskId": task_id,
            "score": score,
        }),
        NativeCall::WalletFinalizeTaskV1 { task_id } => json!({
            "type": kind,
            "taskId": task_id,
        }),
        NativeCall::WalletEmergencyStopV1 { engage } => json!({
            "type": kind,
            "engage": engage,
        }),
        NativeCall::WalletBatchSettleV1(p) => json!({
            "type": kind,
            "batchId": hex32(&p.batch_id),
            "providerId": hex32(&p.provider_id),
            "toolClass": p.tool_class,
            "receiptRoot": hex32(&p.receipt_root),
            "totalCost": p.total_cost.to_string(),
            "payoutTo": addr_hex(&p.payout_to),
            "receiptCount": p.receipts_borsh.len(),
        }),
        NativeCall::WalletRegisterProviderV1 { registration } => json!({
            "type": kind,
            "providerId": hex32(&registration.provider_id),
            "owner": addr_hex(&registration.owner),
            "toolClasses": registration.tool_classes,
            "registrationBond": registration.registration_bond.to_string(),
            "metadataUri": registration.metadata_uri,
            "endpointUri": registration.endpoint_uri,
        }),
        NativeCall::WalletStakeForClassV1 {
            provider_id,
            tool_class,
            amount,
        } => json!({
            "type": kind,
            "providerId": hex32(provider_id),
            "toolClass": tool_class,
            "amount": amount.to_string(),
        }),
        NativeCall::WalletProviderUnstakeRequestV1 {
            provider_id,
            tool_class,
            amount,
        } => json!({
            "type": kind,
            "providerId": hex32(provider_id),
            "toolClass": tool_class,
            "amount": amount.to_string(),
        }),
        NativeCall::WalletProviderUnstakeFinalizeV1 { request_id } => json!({
            "type": kind,
            "requestId": request_id,
        }),
        NativeCall::WalletSlashProviderV1 { provider_id, slash } => json!({
            "type": kind,
            "providerId": hex32(provider_id),
            "toolClass": slash.tool_class,
            "amount": slash.amount.to_string(),
            "reasonCode": slash.reason_code,
            "evidenceHash": hex32(&slash.evidence_hash),
            "challenger": addr_hex(&slash.challenger),
        }),
        NativeCall::WalletUpdateProviderV1 {
            provider_id,
            metadata_uri,
            endpoint_uri,
            active,
        } => json!({
            "type": kind,
            "providerId": hex32(provider_id),
            "metadataUri": metadata_uri,
            "endpointUri": endpoint_uri,
            "active": active,
        }),
        NativeCall::WalletDeregisterProviderV1 { provider_id } => json!({
            "type": kind,
            "providerId": hex32(provider_id),
        }),
        NativeCall::WalletScopedEmergencyStopV1 {
            engage,
            scope,
            master_public_key,
            ..
        } => json!({
            "type": kind,
            "engage": engage,
            "masterPublicKey": hex32(master_public_key),
            "scope": {
                "workspaceId": scope.workspace_id,
                "projectId": scope.project_id,
                "taskId": scope.task_id,
                "toolClassMask": format!("0x{:x}", scope.tool_class_mask),
                "providerId": scope.provider_id.as_ref().map(hex32),
            },
        }),
        NativeCall::Stake { amount } | NativeCall::Unstake { amount } => {
            json!({ "type": kind, "amount": amount.to_string() })
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wallet_kinds_detected() {
        assert!(is_wallet_native("WalletMintCapabilityV1"));
        assert!(is_wallet_native("WalletPostTaskV1"));
        assert!(!is_wallet_native("Stake"));
    }
}
