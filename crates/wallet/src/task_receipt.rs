//! TaskReceipt + tool receipt Merkle binding (`docs/wallet.md` §9.1–9.2).

use borsh::{BorshDeserialize, BorshSerialize};
use thiserror::Error;

use crate::merkle;
use crate::types::{Amount, IntentId, PublicKey, ReceiptId, TaskId};

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct ToolReceiptSummary {
    pub receipt_id: ReceiptId,
    pub intent_id: IntentId,
    pub task_id: TaskId,
    pub cost: Amount,
}

pub fn summary_commitment(s: &ToolReceiptSummary) -> [u8; 32] {
    let bytes = borsh::to_vec(s).expect("ToolReceiptSummary borsh");
    *blake3::hash(&bytes).as_bytes()
}

/// Merkle root over `BLAKE3(borsh(summary))` leaves, sorted by commitment bytes (deterministic).
pub fn tool_receipt_root(summaries: &[ToolReceiptSummary]) -> [u8; 32] {
    let mut commits: Vec<[u8; 32]> = summaries.iter().map(|s| summary_commitment(s)).collect();
    commits.sort();
    merkle::root_from_sorted_commitments(&commits)
}

pub fn verify_tool_costs(summaries: &[ToolReceiptSummary], expected_total: Amount) -> bool {
    summaries.iter().map(|s| s.cost).sum::<Amount>() == expected_total
}

#[derive(Clone, Debug, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
pub struct TaskReceipt {
    pub task_id: TaskId,
    pub agent_session: PublicKey,
    pub artifact_commitment: [u8; 32],
    pub artifact_pointer: String,
    pub tool_receipt_root: [u8; 32],
    pub total_tool_cost: Amount,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum TaskReceiptBuildError {
    #[error("tool cost sum mismatch")]
    CostMismatch,
    #[error("tool receipt root mismatch")]
    RootMismatch,
}

pub fn build_task_receipt(
    task_id: TaskId,
    agent_session: PublicKey,
    artifact_commitment: [u8; 32],
    artifact_pointer: String,
    summaries: &[ToolReceiptSummary],
    expected_total: Amount,
    claimed_root: [u8; 32],
) -> Result<TaskReceipt, TaskReceiptBuildError> {
    if !verify_tool_costs(summaries, expected_total) {
        return Err(TaskReceiptBuildError::CostMismatch);
    }
    let root = tool_receipt_root(summaries);
    if root != claimed_root {
        return Err(TaskReceiptBuildError::RootMismatch);
    }
    Ok(TaskReceipt {
        task_id,
        agent_session,
        artifact_commitment,
        artifact_pointer,
        tool_receipt_root: root,
        total_tool_cost: expected_total,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_and_build() {
        let s = vec![
            ToolReceiptSummary {
                receipt_id: [1u8; 32],
                intent_id: [2u8; 32],
                task_id: 9,
                cost: 10,
            },
            ToolReceiptSummary {
                receipt_id: [3u8; 32],
                intent_id: [4u8; 32],
                task_id: 9,
                cost: 20,
            },
        ];
        let root = tool_receipt_root(&s);
        let tr = build_task_receipt(
            9,
            [0u8; 32],
            [5u8; 32],
            "ipfs://x".into(),
            &s,
            30,
            root,
        )
        .unwrap();
        assert_eq!(tr.total_tool_cost, 30);
        assert_eq!(tr.tool_receipt_root, root);
    }
}
