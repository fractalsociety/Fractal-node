//! Build [`fractal_wallet::ReputationLedgerSummary`] from on-chain M3 receipts (`SettleBatch` / `SettleReceipt`).

use std::collections::BTreeSet;

use fractal_core::OnChainTaskReceipt;
use fractal_wallet::{ReputationLedgerSummary, SettlementEvent, ToolClass};

/// `final_status == 1` matches MVP / M3 samples for a completed paid receipt.
pub const ONCHAIN_RECEIPT_SUCCESS_STATUS: u8 = 1;
pub const DEFAULT_RECEIPT_TOOL_CLASS: u8 = 0;

#[derive(Clone, Debug, Default)]
pub struct SettlementLedgerSide {
    pub client_requesters: BTreeSet<String>,
}

impl SettlementLedgerSide {
    pub fn insert_requester(&mut self, addr: &[u8; 20]) {
        self.client_requesters
            .insert(format!("0x{}", hex::encode(addr)));
    }

    pub fn sync_into_summary(&self, summary: &mut ReputationLedgerSummary) {
        summary.distinct_client_count = self.client_requesters.len().min(u32::MAX as usize) as u32;
    }
}

#[inline]
pub fn tool_class_from_receipt(receipt: &OnChainTaskReceipt) -> ToolClass {
    let _ = receipt;
    ToolClass::from_discriminant(DEFAULT_RECEIPT_TOOL_CLASS).unwrap_or(ToolClass::Browser)
}

/// Merge one [`OnChainTaskReceipt`] into a §10.4 summary (`now_ms` should be chain / block time).
pub fn apply_onchain_receipt_to_summary(
    summary: &mut ReputationLedgerSummary,
    receipt: &OnChainTaskReceipt,
    now_ms: u64,
    side: &mut SettlementLedgerSide,
    available_stake: u128,
) {
    let tc = tool_class_from_receipt(receipt);
    summary.tool_class = tc;
    summary.now_ms = now_ms;
    summary.first_seen_ms = summary.first_seen_ms.min(receipt.finalized_at).min(now_ms);
    side.insert_requester(&receipt.requester);
    side.sync_into_summary(summary);
    summary.available_stake = available_stake;

    if receipt.final_status == ONCHAIN_RECEIPT_SUCCESS_STATUS && receipt.payout_amount > 0 {
        summary.successful.push(SettlementEvent {
            settled_at_ms: receipt.finalized_at,
            weight: receipt.payout_amount.max(1),
        });
    } else {
        summary.failed_settlements = summary.failed_settlements.saturating_add(1);
    }
}

pub fn row_key_for_settlement(provider_id: &[u8; 32], tool_class: u8) -> String {
    format!("{}:{}", hex::encode(provider_id), tool_class)
}

pub fn provider_and_key_from_receipt(receipt: &OnChainTaskReceipt) -> ([u8; 32], String) {
    let pid = provider_id_from_onchain_worker_agent(receipt.worker);
    let key = row_key_for_settlement(&pid, DEFAULT_RECEIPT_TOOL_CLASS);
    (pid, key)
}

pub fn row_key_for_worker_agent(worker: u64, tool_class: u8) -> String {
    let pid = provider_id_from_onchain_worker_agent(worker);
    row_key_for_settlement(&pid, tool_class)
}

pub fn provider_id_from_onchain_worker_agent(worker: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[24..32].copy_from_slice(&worker.to_be_bytes());
    out
}

/// Governance [`fractal_core::NativeCall::ResolveDispute`] with [`fractal_core::DISPUTE_RESOLUTION_PROVIDER_FAULT`].
/// §14.5 wallet task finalize: credit provider reputation from on-chain task escrow + verifier score.
pub fn apply_wallet_task_finalize_to_summary(
    summary: &mut ReputationLedgerSummary,
    event: &crate::wallet_task_mirror::WalletTaskFinalizeEvent,
    now_ms: u64,
    side: &mut SettlementLedgerSide,
    available_stake: u128,
) {
    summary.tool_class =
        ToolClass::from_discriminant(event.tool_class).unwrap_or(ToolClass::Browser);
    summary.now_ms = now_ms;
    summary.first_seen_ms = summary.first_seen_ms.min(event.finalized_at_ms).min(now_ms);
    side.insert_requester(&event.requester);
    side.sync_into_summary(summary);
    summary.available_stake = available_stake;

    if event.verifier_score > 0 && event.escrow_wei > 0 {
        summary.successful.push(SettlementEvent {
            settled_at_ms: event.finalized_at_ms,
            weight: event.escrow_wei.max(1),
        });
    } else {
        summary.failed_settlements = summary.failed_settlements.saturating_add(1);
    }
}

pub fn row_key_for_wallet_provider(provider_id: &[u8; 32], tool_class: u8) -> String {
    row_key_for_settlement(provider_id, tool_class)
}

pub fn apply_dispute_slash_to_summary(
    summary: &mut ReputationLedgerSummary,
    now_ms: u64,
    available_stake: u128,
    side: &mut SettlementLedgerSide,
    tool_class: ToolClass,
) {
    summary.tool_class = tool_class;
    summary.now_ms = now_ms;
    summary.available_stake = available_stake;
    summary.slashing_events = summary.slashing_events.saturating_add(1);
    side.sync_into_summary(summary);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_receipt(final_status: u8, payout: u128) -> OnChainTaskReceipt {
        let mut rid = [0u8; 32];
        rid[31] = 7;
        OnChainTaskReceipt {
            receipt_id: rid,
            job_id: rid,
            requester: [1u8; 20],
            worker: 42,
            verifier: 0,
            artifact_root: [2u8; 32],
            output_hash: [3u8; 32],
            score: 100,
            payout_amount: payout,
            verifier_fee: 0,
            protocol_fee: 0,
            final_status,
            finalized_at: 10_000,
            schema_version: 2,
        }
    }

    #[test]
    fn success_appends_settlement_event() {
        let mut s = ReputationLedgerSummary {
            tool_class: ToolClass::Browser,
            successful: vec![],
            failed_settlements: 0,
            slashing_events: 0,
            first_seen_ms: u64::MAX,
            now_ms: 0,
            available_stake: 0,
            distinct_client_count: 0,
        };
        let mut side = SettlementLedgerSide::default();
        let r = sample_receipt(ONCHAIN_RECEIPT_SUCCESS_STATUS, 500);
        apply_onchain_receipt_to_summary(&mut s, &r, 20_000, &mut side, 99);
        assert_eq!(s.successful.len(), 1);
        assert_eq!(s.successful[0].weight, 500);
        assert_eq!(s.failed_settlements, 0);
        assert_eq!(s.distinct_client_count, 1);
        assert_eq!(s.available_stake, 99);
    }

    #[test]
    fn failure_increments_failed_counter() {
        let mut s = ReputationLedgerSummary {
            tool_class: ToolClass::Browser,
            successful: vec![],
            failed_settlements: 0,
            slashing_events: 0,
            first_seen_ms: u64::MAX,
            now_ms: 0,
            available_stake: 0,
            distinct_client_count: 0,
        };
        let mut side = SettlementLedgerSide::default();
        let r = sample_receipt(0, 0);
        apply_onchain_receipt_to_summary(&mut s, &r, 20_000, &mut side, 0);
        assert!(s.successful.is_empty());
        assert_eq!(s.failed_settlements, 1);
    }

    #[test]
    fn current_receipts_default_to_browser_tool_class() {
        let r = sample_receipt(ONCHAIN_RECEIPT_SUCCESS_STATUS, 1);
        assert_eq!(tool_class_from_receipt(&r), ToolClass::Browser);
    }

    #[test]
    fn wallet_task_finalize_success_counts_escrow_weight() {
        use crate::wallet_task_mirror::WalletTaskFinalizeEvent;

        let mut s = ReputationLedgerSummary {
            tool_class: ToolClass::Browser,
            successful: vec![],
            failed_settlements: 0,
            slashing_events: 0,
            first_seen_ms: u64::MAX,
            now_ms: 0,
            available_stake: 0,
            distinct_client_count: 0,
        };
        let mut side = SettlementLedgerSide::default();
        let ev = WalletTaskFinalizeEvent {
            task_id: 1,
            provider_id: [0xabu8; 32],
            tool_class: 1,
            requester: [1u8; 20],
            escrow_wei: 175,
            verifier_score: 90,
            finalized_at_ms: 5_000,
        };
        apply_wallet_task_finalize_to_summary(&mut s, &ev, 6_000, &mut side, 50);
        assert_eq!(s.successful.len(), 1);
        assert_eq!(s.successful[0].weight, 175);
        assert_eq!(s.tool_class, ToolClass::LlmInference);
    }
}
