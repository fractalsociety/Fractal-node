use std::sync::Arc;

use fractal_core::{NativeCall, TxBody};
use fractal_node::{try_produce_one_tick, NodeInner, ProduceTickOutcome};
use fractal_rpc::{
    build_module, ChainInteraction, RlmfAttestationRecord, RlmfAttestationResponse,
    RlmfAttestationStored, SharedChain,
};
use tokio::sync::Mutex;

fn hash_hex(byte: u8) -> String {
    format!("0x{}", hex::encode([byte; 32]))
}

fn record_fixture(subject: &str, source: &str) -> RlmfAttestationRecord {
    let mut record = RlmfAttestationRecord {
        commitment_hash: hash_hex(0),
        subject_id: subject.to_string(),
        source_system: source.to_string(),
        dataset_hash: hash_hex(0x11),
        job_hash: hash_hex(0x22),
        judge_report_hash: hash_hex(0x33),
        benchmark_report_hash: hash_hex(0x44),
        model_artifact_hash: hash_hex(0x55),
        promotion_decision: "promote".to_string(),
        evidence_hashes: vec![hash_hex(0x66), hash_hex(0x77)],
        lineage_hashes: vec![hash_hex(0x88)],
    };
    let commitment = record
        .canonical_commitment()
        .expect("fixture must be canonicalizable");
    record.commitment_hash = format!("0x{}", hex::encode(commitment));
    record
}

fn decode_hash32(raw: &str) -> [u8; 32] {
    hex::decode(raw.trim_start_matches("0x"))
        .unwrap()
        .try_into()
        .unwrap()
}

#[tokio::test]
async fn submit_rlmf_attestation_mines_commitment_and_indexes_record() {
    let node = Arc::new(Mutex::new(NodeInner::devnet()));
    let record = record_fixture("adapter.invoice_verifier.v7", "fractalwork");
    let commitment = decode_hash32(&record.commitment_hash);

    let response = {
        let mut n = node.lock().await;
        n.submit_rlmf_attestation(record.clone()).unwrap()
    };
    assert!(response.transaction_hash.starts_with("0x"));
    let tx_hash = decode_hash32(&response.transaction_hash);

    assert!(matches!(
        try_produce_one_tick(&node).await,
        ProduceTickOutcome::Produced(1)
    ));

    let n = node.lock().await;
    // The commitment landed as a real mined native transaction.
    let (block_number, _block_hash, tx_index) = n.mined_tx_info(&tx_hash).unwrap();
    assert_eq!(block_number, 1);
    assert_eq!(tx_index, 0);
    let mined = n.tx_by_hash(&tx_hash).unwrap();
    assert!(matches!(
        &mined.body,
        TxBody::Native(NativeCall::ProofCommitmentV1 { proof_hash: got }) if got == &commitment
    ));

    // Indexed record is queryable by commitment hash.
    let stored = n.rlmf_attestation_by_commitment(commitment).unwrap();
    assert_eq!(stored.record, record);
    assert_eq!(stored.transaction_hash, response.transaction_hash);

    // And by subjectId / sourceSystem / transaction hash filters.
    let by_subject =
        n.list_rlmf_attestations(Some("adapter.invoice_verifier.v7"), None, None, None, 10);
    assert_eq!(by_subject.len(), 1);
    let by_source = n.list_rlmf_attestations(None, Some("fractalwork"), None, None, 10);
    assert_eq!(by_source.len(), 1);
    let by_tx = n.list_rlmf_attestations(None, None, None, Some(&response.transaction_hash), 10);
    assert_eq!(by_tx.len(), 1);
    let miss = n.list_rlmf_attestations(Some("someone-else"), None, None, None, 10);
    assert!(miss.is_empty());
}

#[tokio::test]
async fn resubmitting_identical_record_is_idempotent_and_conflict_is_rejected() {
    let node = Arc::new(Mutex::new(NodeInner::devnet()));
    let record = record_fixture("dataset.sft.q3", "dataevol");

    let (first, second) = {
        let mut n = node.lock().await;
        let first = n.submit_rlmf_attestation(record.clone()).unwrap();
        let second = n.submit_rlmf_attestation(record.clone()).unwrap();
        (first, second)
    };
    assert_eq!(first.transaction_hash, second.transaction_hash);

    // Exactly one commitment transaction exists in the pool/chain.
    {
        let n = node.lock().await;
        assert_eq!(n.rlmf_attestations.len(), 1);
    }

    // Same commitment, tampered contents -> rejected. A tampered field changes
    // the canonical commitment, so validation fails on the mismatch.
    let mut tampered = record.clone();
    tampered.promotion_decision = "reject".to_string();
    let err = {
        let mut n = node.lock().await;
        n.submit_rlmf_attestation(tampered).unwrap_err()
    };
    assert!(err.contains("invalid RLMF attestation"), "got: {err}");
}

#[tokio::test]
async fn rlmf_attestation_rpc_round_trip_and_invalid_inputs() {
    let ctx: SharedChain = Arc::new(Mutex::new(NodeInner::devnet()));
    let module = build_module(ctx);

    let record = record_fixture("prompt.pack.worker.v12", "dataevol");
    let submitted: RlmfAttestationResponse = module
        .call("fractal_submitRlmfAttestation", [record.clone()])
        .await
        .expect("submit attestation");
    assert_eq!(submitted.attestation, record);
    assert!(submitted.finalized);

    let fetched: Option<RlmfAttestationStored> = module
        .call(
            "fractal_getRlmfAttestation",
            [serde_json::json!({ "commitmentHash": record.commitment_hash })],
        )
        .await
        .expect("get attestation");
    let fetched = fetched.expect("attestation should be indexed");
    assert_eq!(fetched.record, record);

    let listed: Vec<RlmfAttestationStored> = module
        .call(
            "fractal_listRlmfAttestations",
            [serde_json::json!({ "sourceSystem": "dataevol", "limit": 5 })],
        )
        .await
        .expect("list attestations");
    assert_eq!(listed.len(), 1);

    // Invalid: commitment hash does not match record contents.
    let mut wrong_commitment = record_fixture("x", "y");
    wrong_commitment.commitment_hash = hash_hex(0xEE);
    let err = module
        .call::<_, RlmfAttestationResponse>("fractal_submitRlmfAttestation", [wrong_commitment])
        .await
        .unwrap_err();
    assert!(err.to_string().contains("canonical commitment"), "{err}");

    // Invalid: bad promotion decision.
    let mut bad_decision = record_fixture("x", "y");
    bad_decision.promotion_decision = "maybe".to_string();
    let err = module
        .call::<_, RlmfAttestationResponse>("fractal_submitRlmfAttestation", [bad_decision])
        .await
        .unwrap_err();
    assert!(err.to_string().contains("promotionDecision"), "{err}");

    // Invalid: malformed hash hex.
    let mut bad_hex = record_fixture("x", "y");
    bad_hex.dataset_hash = "0xnothex".to_string();
    let err = module
        .call::<_, RlmfAttestationResponse>("fractal_submitRlmfAttestation", [bad_hex])
        .await
        .unwrap_err();
    assert!(err.to_string().contains("invalid"), "{err}");

    // Invalid: oversized evidence list.
    let mut too_many = record_fixture("x", "y");
    too_many.evidence_hashes = (0..65).map(|i| hash_hex(i as u8)).collect();
    let err = module
        .call::<_, RlmfAttestationResponse>("fractal_submitRlmfAttestation", [too_many])
        .await
        .unwrap_err();
    assert!(err.to_string().contains("evidenceHashes"), "{err}");

    // Unknown fields are rejected (deny_unknown_fields).
    let err = module
        .call::<_, RlmfAttestationResponse>(
            "fractal_submitRlmfAttestation",
            [serde_json::json!({ "surprise": true })],
        )
        .await
        .unwrap_err();
    assert!(err.to_string().contains("malformed"), "{err}");
}

#[tokio::test]
async fn canonical_commitment_is_deterministic_and_field_sensitive() {
    let a = record_fixture("subject", "source");
    let b = record_fixture("subject", "source");
    assert_eq!(
        a.canonical_commitment().unwrap(),
        b.canonical_commitment().unwrap()
    );
    let mut c = record_fixture("subject", "source");
    c.lineage_hashes.push(hash_hex(0x99));
    assert_ne!(
        a.canonical_commitment().unwrap(),
        c.canonical_commitment().unwrap()
    );
}
