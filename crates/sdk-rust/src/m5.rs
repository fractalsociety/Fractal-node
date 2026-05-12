//! PRD §18 **M5** helpers: build `SETTLE_BATCH` + Merkle `CLAIM_PAYOUT` transactions (native VM).

use fractal_core::{
    merkle_proof, Address, NativeCall, OnChainTaskReceipt, PayoutEntry, SettleBatchPayload, Transaction,
    TxBody, VmKind, HARDHAT_DEFAULT_SIGNER_0, HARDHAT_DEFAULT_SIGNER_1,
};
use fractal_crypto::hash::keccak256;
use fractal_crypto::Hash256;

/// Default devnet operator (Hardhat #0) and claim agent (Hardhat #1), matching `NodeInner::devnet()`.
pub fn default_devnet_operator() -> Address {
    HARDHAT_DEFAULT_SIGNER_0
}

pub fn default_devnet_claim_agent() -> Address {
    HARDHAT_DEFAULT_SIGNER_1
}

/// Build `count` synthetic receipts + one-per-receipt payout entries (amount `payout_each` to `agent`).
///
/// `operator_sig` may be zeroed when the operator has no registered agent pubkey (devnet path in `apply_settle_batch`).
pub fn build_settle_batch_payload(
    operator: Address,
    agent: Address,
    batch_id: Hash256,
    count: u32,
    payout_each: u128,
    submitted_at: u64,
) -> SettleBatchPayload {
    let mut receipts = Vec::with_capacity(count as usize);
    let mut payout_entries = Vec::with_capacity(count as usize);
    for i in 0..count {
        let mut rid = [0u8; 32];
        rid[28..].copy_from_slice(&i.to_be_bytes());
        receipts.push(OnChainTaskReceipt {
            receipt_id: rid,
            job_id: rid,
            requester: operator,
            worker: 1,
            verifier: 2,
            artifact_root: rid,
            output_hash: rid,
            score: 90,
            payout_amount: payout_each,
            verifier_fee: 0,
            protocol_fee: 0,
            final_status: 1,
            finalized_at: 1,
            schema_version: 1,
        });
        payout_entries.push(PayoutEntry {
            index: i,
            account: agent,
            amount: payout_each,
        });
    }
    SettleBatchPayload {
        batch_id,
        operator,
        receipts,
        payout_entries,
        submitted_at,
        operator_sig: [0u8; 64],
    }
}

/// One settle transaction + `count` claim transactions (agent nonces `agent_start_nonce` ..).
pub fn build_settle_then_claim_txs(
    operator: Address,
    operator_nonce: u64,
    agent: Address,
    agent_start_nonce: u64,
    batch_id: Hash256,
    count: u32,
    payout_each: u128,
    submitted_at: u64,
) -> (Transaction, Vec<Transaction>) {
    let payload = build_settle_batch_payload(
        operator,
        agent,
        batch_id,
        count,
        payout_each,
        submitted_at,
    );
    let leaves: Vec<Hash256> = payload
        .payout_entries
        .iter()
        .map(|e| keccak256(&borsh::to_vec(e).expect("payout borsh")))
        .collect();

    let settle = Transaction {
        signer: operator,
        nonce: operator_nonce,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::SettleBatch(payload.clone())),
    };

    let mut claims = Vec::with_capacity(count as usize);
    for i in 0..count {
        let proof = merkle_proof(&leaves, i as usize).expect("proof");
        claims.push(Transaction {
            signer: agent,
            nonce: agent_start_nonce.saturating_add(i as u64),
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::ClaimPayout {
                batch_id,
                account: agent,
                amount: payout_each,
                leaf_index: i,
                proof,
            }),
        });
    }
    (settle, claims)
}
