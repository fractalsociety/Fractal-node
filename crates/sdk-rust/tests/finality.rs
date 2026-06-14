use fractal_sdk::{BlockFinalityStatus, FinalityRequirement, FinalityStatus};
use std::str::FromStr;

#[test]
fn parses_rpc_finality_status_labels() {
    assert_eq!(
        FinalityStatus::from_str(FinalityStatus::SOFT_WIRE).unwrap(),
        FinalityStatus::Soft
    );
    assert_eq!(
        FinalityStatus::from_str(FinalityStatus::PROOF_WIRE).unwrap(),
        FinalityStatus::Proof
    );
    assert!(FinalityStatus::from_str("committee").is_err());
}

#[test]
fn proof_requirement_is_only_satisfied_by_proof_finality() {
    assert!(FinalityStatus::Soft.satisfies(FinalityRequirement::SoftAllowed));
    assert!(!FinalityStatus::Soft.satisfies(FinalityRequirement::ProofRequired));
    assert!(FinalityStatus::Proof.satisfies(FinalityRequirement::ProofRequired));
}

#[test]
fn block_finality_status_exposes_requirement_checks() {
    let status = BlockFinalityStatus {
        block_hash: [7u8; 32],
        block_number: 42,
        status: FinalityStatus::Proof,
        proof_circuit_version: Some("mixed_state_transition_v1".into()),
        proof_coverage_manifest_digest: Some("0xabc".into()),
        proof_covered_features: Some("0x3f".into()),
    };

    assert!(status.is_proof_final());
    assert!(status.satisfies(FinalityRequirement::ProofRequired));
    assert_eq!(
        status.proof_coverage(),
        Some(("mixed_state_transition_v1", "0xabc", "0x3f"))
    );
}
