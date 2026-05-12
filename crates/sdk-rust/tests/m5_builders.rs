use fractal_sdk::m5::{build_settle_then_claim_txs, default_devnet_claim_agent, default_devnet_operator};

#[test]
fn m5_build_settle_and_claim_txs_count_matches() {
    let op = default_devnet_operator();
    let ag = default_devnet_claim_agent();
    let batch_id = [7u8; 32];
    let (settle, claims) = build_settle_then_claim_txs(op, 0, ag, 0, batch_id, 5, 1, 0);
    assert!(matches!(
        settle.body,
        fractal_sdk::TxBody::Native(fractal_sdk::NativeCall::SettleBatch(_))
    ));
    assert_eq!(claims.len(), 5);
    for (i, c) in claims.iter().enumerate() {
        assert_eq!(c.signer, ag);
        assert_eq!(c.nonce, i as u64);
    }
}
