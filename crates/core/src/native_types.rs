//! On-chain native shapes for PRD §9.2 / §18 M3 (compact borsh).

use borsh::{BorshDeserialize, BorshSerialize};

use crate::address::Address;
use fractal_crypto::Hash256;

/// Native opcode bytes (PRD §9.2 table).
pub const OP_REGISTER_AGENT: u8 = 0x01;
pub const OP_UPDATE_AGENT: u8 = 0x02;
pub const OP_SUSPEND_AGENT: u8 = 0x03;
pub const OP_SETTLE_RECEIPT: u8 = 0x04;
pub const OP_SETTLE_BATCH: u8 = 0x05;
pub const OP_CLAIM_PAYOUT: u8 = 0x06;
pub const OP_FILE_DISPUTE: u8 = 0x07;
pub const OP_RESOLVE_DISPUTE: u8 = 0x08;
pub const OP_STAKE: u8 = 0x09;
pub const OP_UNSTAKE: u8 = 0x0a;
pub const OP_SLASH: u8 = 0x0b;
pub const OP_DELEGATE: u8 = 0x0c;
pub const OP_WITHDRAW_REWARDS: u8 = 0x0d;
/// W6-d: anchor `task_receipt_commitment` on-chain (`docs/wallet.md` §9.2, `wallet_anchor`).
pub const OP_WALLET_TASK_RECEIPT_ANCHOR_V1: u8 = 0x0e;

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct OnChainTaskReceipt {
    pub receipt_id: Hash256,
    pub job_id: Hash256,
    pub requester: Address,
    pub worker: u64,
    pub verifier: u64,
    pub artifact_root: Hash256,
    pub output_hash: Hash256,
    pub score: u8,
    pub payout_amount: u128,
    pub verifier_fee: u128,
    pub protocol_fee: u128,
    pub final_status: u8,
    pub finalized_at: u64,
    pub schema_version: u16,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct PayoutEntry {
    pub index: u32,
    pub account: Address,
    pub amount: u128,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct SettleBatchPayload {
    pub batch_id: Hash256,
    pub operator: Address,
    pub receipts: Vec<OnChainTaskReceipt>,
    pub payout_entries: Vec<PayoutEntry>,
    pub submitted_at: u64,
    pub operator_sig: [u8; 64],
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentRecord {
    pub agent_id: u64,
    pub address: Address,
    pub operator: Address,
    pub pubkey: [u8; 32],
    pub kind: u8,
    pub metadata_uri: String,
    pub reputation_score: u32,
    pub completed_jobs: u32,
    pub status: u8,
    pub registered_at: u64,
    pub schema_version: u16,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct StoredBatch {
    pub operator: Address,
    pub receipt_root: Hash256,
    pub payout_root: Hash256,
    pub receipt_count: u32,
    pub payout_count: u32,
    pub total_payout: u128,
    pub submitted_at: u64,
}

#[derive(BorshSerialize, BorshDeserialize, Clone, Debug, PartialEq, Eq)]
pub struct DisputeRecord {
    pub receipt_id: Hash256,
    pub filer: Address,
    pub reason_code: u32,
    pub evidence_hash: Hash256,
    pub status: u8,
}
