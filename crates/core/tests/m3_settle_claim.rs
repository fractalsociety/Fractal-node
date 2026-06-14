//! PRD §18 M3 exit path: `SETTLE_BATCH` with 100 receipts + Merkle `CLAIM_PAYOUT` for each leaf.

use fractal_core::{
    apply_block, merkle_proof, merkle_root, Account, NativeCall, OnChainTaskReceipt, PayoutEntry,
    SettleBatchPayload, State, Transaction, TxBody, VmKind,
};
use fractal_crypto::hash::keccak256;

type Address = fractal_core::Address;

fn addr_tag(b: u8) -> Address {
    let mut a = [0u8; 20];
    a[19] = b;
    a
}

#[test]
fn settle_batch_hundred_receipts_then_hundred_claims() {
    let operator = addr_tag(1);
    let agent = addr_tag(2);
    let mut st = State::default();
    st.accounts.insert(
        operator,
        Account {
            nonce: 0,
            balance: 10_000_000,
        },
    );
    st.accounts.insert(
        agent,
        Account {
            nonce: 0,
            balance: 0,
        },
    );

    let mut receipts = Vec::new();
    let mut payouts = Vec::new();
    for i in 0u32..100 {
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
            payout_amount: 1,
            verifier_fee: 0,
            protocol_fee: 0,
            final_status: 1,
            finalized_at: 1,
            schema_version: 1,
        });
        payouts.push(PayoutEntry {
            index: i,
            account: agent,
            amount: 1,
        });
    }
    let batch_id = [9u8; 32];
    let leaves: Vec<_> = payouts
        .iter()
        .map(|e| keccak256(&borsh::to_vec(e).unwrap()))
        .collect();
    let expected_payout_root = merkle_root(&leaves);

    let settle = Transaction {
        signer: operator,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::SettleBatch(SettleBatchPayload {
            batch_id,
            operator,
            receipts,
            payout_entries: payouts,
            submitted_at: 0,
            operator_sig: [0u8; 64],
        })),
    };

    let mut txs = vec![settle];
    for i in 0u32..100 {
        let proof = merkle_proof(&leaves, i as usize).expect("proof");
        txs.push(Transaction {
            signer: agent,
            nonce: i as u64,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::ClaimPayout {
                batch_id,
                account: agent,
                amount: 1,
                leaf_index: i,
                proof,
            }),
        });
    }

    apply_block(&mut st, &txs).expect("m3 settle + claims");
    assert_eq!(
        st.batches.get(&batch_id).unwrap().payout_root,
        expected_payout_root
    );
    assert_eq!(st.accounts.get(&agent).unwrap().balance, 100);
}
