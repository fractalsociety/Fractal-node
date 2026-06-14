use fractal_sdk::m5::{
    build_settle_batch_payload, build_settle_then_claim_txs,
    build_settle_then_claim_txs_from_payload, default_devnet_claim_agent, default_devnet_operator,
    M5FromPayloadError,
};

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

#[test]
fn m5_from_payload_matches_synthetic_shape() {
    let op = default_devnet_operator();
    let ag = default_devnet_claim_agent();
    let batch_id = [7u8; 32];
    let payload = build_settle_batch_payload(op, ag, batch_id, 4, 2, 99);
    let (settle, claims) =
        build_settle_then_claim_txs_from_payload(payload.clone(), 3, ag, 10).unwrap();
    assert_eq!(settle.signer, op);
    assert_eq!(settle.nonce, 3);
    let (settle2, claims2) = build_settle_then_claim_txs(op, 3, ag, 10, batch_id, 4, 2, 99);
    assert_eq!(
        borsh::to_vec(&settle).unwrap(),
        borsh::to_vec(&settle2).unwrap()
    );
    assert_eq!(claims.len(), claims2.len());
    for (a, b) in claims.iter().zip(claims2.iter()) {
        assert_eq!(borsh::to_vec(a).unwrap(), borsh::to_vec(b).unwrap());
    }
}

#[test]
fn m5_from_payload_rejects_wrong_claim_agent() {
    let op = default_devnet_operator();
    let ag = default_devnet_claim_agent();
    let other = [0x11u8; 20];
    let mut payload = build_settle_batch_payload(op, ag, [1u8; 32], 2, 1, 0);
    payload.payout_entries[0].account = other;
    let err = build_settle_then_claim_txs_from_payload(payload, 0, ag, 0).unwrap_err();
    assert_eq!(err, M5FromPayloadError::ClaimAgentMismatch { leaf: 0 });
}
