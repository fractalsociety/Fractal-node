//! Parse off-chain receipt JSON for M5 bridge.

use std::path::PathBuf;

use fractal_core::HARDHAT_DEFAULT_SIGNER_0;
use fractal_mvp_backend::receipt_json::load_settle_payload_from_json;
use fractal_sdk::m5::build_settle_then_claim_txs_from_payload;

#[test]
fn load_sample_receipts_builds_three_claims() {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/mvp_receipts_sample.json");
    let (payload, claim_agent) = load_settle_payload_from_json(p.to_str().unwrap()).unwrap();
    assert_eq!(payload.receipts.len(), 3);
    assert_eq!(payload.payout_entries.len(), 3);
    assert_eq!(payload.operator, HARDHAT_DEFAULT_SIGNER_0);
    let total: u128 = payload.payout_entries.iter().map(|e| e.amount).sum();
    assert_eq!(total, 5 + 7 + 11);
    let (settle, claims) =
        build_settle_then_claim_txs_from_payload(payload, 0, claim_agent, 0).unwrap();
    assert!(matches!(
        settle.body,
        fractal_sdk::TxBody::Native(fractal_sdk::NativeCall::SettleBatch(_))
    ));
    assert_eq!(claims.len(), 3);
}
