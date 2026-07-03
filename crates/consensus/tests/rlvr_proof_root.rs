//! RLVR-048: bind the deterministic root over included RLVR proofs into the
//! versioned block-header extension, and prove changing any included proof
//! changes the block commitment (header hash).

use fractal_consensus::{
    header_hash, proof_ingestion_header_extra, proof_ingestion_header_extra_with_rlvr,
    rlvr_proof_root, BlockHeader, DaSamplingParamsV1, ExecutionFeatureSetV1,
    ZoneBlobDaCommitmentV1, DEFAULT_DA_NAMESPACE,
};

fn h(byte: u8) -> [u8; 32] {
    [byte; 32]
}

fn fixed_commitment() -> ZoneBlobDaCommitmentV1 {
    ZoneBlobDaCommitmentV1 {
        namespace: DEFAULT_DA_NAMESPACE,
        da_root: h(7),
        byte_count: 128,
        share_count: 4,
        share_size: 32,
        sampling: DaSamplingParamsV1 {
            seed: 41,
            sample_count: 8,
            min_samples: 4,
        },
    }
}

fn header_with_extra(extra: [u8; 32]) -> BlockHeader {
    BlockHeader {
        version: 1,
        chain_id: 41,
        height: 1,
        view: 0,
        parent_hash: [0u8; 32],
        parent_qc_hash: [0u8; 32],
        proposer: [0u8; 32],
        timestamp_ms: 1_000,
        parent_state_root: [0u8; 32],
        state_root: [0u8; 32],
        tx_root: [0u8; 32],
        receipt_root: [0u8; 32],
        native_event_root: [0u8; 32],
        evm_log_root: [0u8; 32],
        zone_namespace: DEFAULT_DA_NAMESPACE,
        da_root: [0u8; 32],
        da_bytes: 0,
        da_share_count: 0,
        da_gas_used: 0,
        da_fee_paid: 0,
        gas_used: 0,
        gas_limit: 60_000_000,
        feature_set: ExecutionFeatureSetV1::empty(),
        extra,
    }
}

#[test]
fn rlvr_proof_root_is_deterministic() {
    let proofs = [h(1), h(2), h(3)];
    assert_eq!(rlvr_proof_root(&proofs), rlvr_proof_root(&proofs));
}

#[test]
fn rlvr_proof_root_empty_is_zero() {
    assert_eq!(rlvr_proof_root(&[]), [0u8; 32]);
}

#[test]
fn rlvr_proof_root_changes_when_any_included_proof_changes() {
    let a = h(1);
    let b = h(2);
    let c = h(3);

    // Changing a single included proof changes the root.
    assert_ne!(rlvr_proof_root(&[a]), rlvr_proof_root(&[b]));
    assert_ne!(rlvr_proof_root(&[a, b]), rlvr_proof_root(&[a, c]));

    // Adding a proof changes the root.
    assert_ne!(rlvr_proof_root(&[a, b]), rlvr_proof_root(&[a, b, c]));
    // Removing a proof changes the root.
    assert_ne!(rlvr_proof_root(&[a, b, c]), rlvr_proof_root(&[a, b]));

    // Reordering changes the root (inclusion order is bound).
    assert_ne!(rlvr_proof_root(&[a, b]), rlvr_proof_root(&[b, a]));

    // A duplicate included proof changes the root (binds multiplicity).
    assert_ne!(rlvr_proof_root(&[a, b]), rlvr_proof_root(&[a, a, b]));
}

#[test]
fn rlvr_proof_root_leaf_is_domain_separated() {
    // The root over a single proof is `keccak256("fractal:rlvr-proof-leaf:v1" || proof)`.
    // It must differ from both the raw proof hash and a tag-less `keccak256(proof)`,
    // proving the domain tag is applied (so an RLVR proof hash cannot masquerade as
    // another commitment type).
    let proof = h(5);
    assert_ne!(rlvr_proof_root(&[proof]), proof);
    assert_ne!(
        rlvr_proof_root(&[proof]),
        fractal_crypto::hash::keccak256(&proof)
    );
}

#[test]
fn v2_header_extra_binds_rlvr_root() {
    let commitment = fixed_commitment();
    let payload_root = h(9);
    let root_a = rlvr_proof_root(&[h(1), h(2)]);
    let root_b = rlvr_proof_root(&[h(1), h(3)]);

    let extra_a =
        proof_ingestion_header_extra_with_rlvr(payload_root, &commitment, root_a).unwrap();
    let extra_b =
        proof_ingestion_header_extra_with_rlvr(payload_root, &commitment, root_b).unwrap();
    assert_ne!(
        extra_a, extra_b,
        "v2 extra must change when rlvr_proof_root changes"
    );

    // Changing payload_root changes the v2 extra too (full binding retained).
    let extra_other_payload =
        proof_ingestion_header_extra_with_rlvr(h(99), &commitment, root_a).unwrap();
    assert_ne!(extra_a, extra_other_payload);

    // Zero root (no RLVR proofs) is a valid, distinct commitment.
    let extra_empty =
        proof_ingestion_header_extra_with_rlvr(payload_root, &commitment, [0u8; 32]).unwrap();
    assert_ne!(extra_empty, extra_a);
}

#[test]
fn v2_header_extra_is_versioned_apart_from_v1() {
    let commitment = fixed_commitment();
    let payload_root = h(9);
    let v1 = proof_ingestion_header_extra(payload_root, &commitment).unwrap();
    // v2 with zero rlvr root must NOT collide with v1 (distinct domain + field).
    let v2_zero =
        proof_ingestion_header_extra_with_rlvr(payload_root, &commitment, [0u8; 32]).unwrap();
    assert_ne!(v1, v2_zero, "v1 and v2 commitments must not collide");
}

#[test]
fn changing_any_included_rlvr_proof_changes_the_block_commitment() {
    // "Done when": changing any included RLVR proof changes the block
    // commitment (header hash). We compose proof set -> rlvr_proof_root ->
    // versioned header extra -> header_hash, and prove each mutation flows
    // through to a different header hash.
    let commitment = fixed_commitment();
    let payload_root = h(9);

    let build_header = |proofs: &[[u8; 32]]| {
        let root = rlvr_proof_root(proofs);
        let extra =
            proof_ingestion_header_extra_with_rlvr(payload_root, &commitment, root).unwrap();
        header_hash(&header_with_extra(extra)).unwrap()
    };

    let base = build_header(&[h(1), h(2)]);
    // Change one included proof.
    assert_ne!(
        base,
        build_header(&[h(1), h(9)]),
        "changing a proof must change header hash"
    );
    // Add a proof.
    assert_ne!(
        base,
        build_header(&[h(1), h(2), h(3)]),
        "adding a proof must change header hash"
    );
    // Remove a proof.
    assert_ne!(
        base,
        build_header(&[h(1)]),
        "removing a proof must change header hash"
    );
    // Reorder.
    assert_ne!(
        base,
        build_header(&[h(2), h(1)]),
        "reordering proofs must change header hash"
    );
    // Empty vs non-empty.
    assert_ne!(
        base,
        build_header(&[]),
        "empty vs non-empty proof set must differ"
    );
}

#[test]
fn header_hash_binds_extra_field_directly() {
    // Independent of RLVR: confirms the `extra` extension field feeds
    // `header_hash`, which is the mechanism RLVR-048 relies on.
    let h1 = header_hash(&header_with_extra(h(1))).unwrap();
    let h2 = header_hash(&header_with_extra(h(2))).unwrap();
    assert_ne!(h1, h2, "header hash must change when extra changes");
}
