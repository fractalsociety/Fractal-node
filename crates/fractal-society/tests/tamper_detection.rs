//! Gate P01-N03: tampering any committed field changes the payload hash and
//! breaks the author signature.

use chrono::DateTime;
use fractal_society::protocol::{Hash, ProofManifest, Visibility};
use fractal_society::signing::AuthorSigner;

fn sample_manifest() -> ProofManifest {
    ProofManifest {
        manifest_version: "1.0.0".to_string(),
        claim_id: "claim-1".to_string(),
        protocol_hash: Hash::new(b"protocol"),
        agent_hash: Hash::new(b"agent"),
        dataset_hash: Hash::new(b"dataset"),
        environment_hash: Hash::new(b"env"),
        trace_merkle_root: Hash::new(b"trace"),
        verifier_set_hash: Hash::new(b"verifiers"),
        scorecard_hash: Hash::new(b"scorecard"),
        disclosure: Visibility::CommittedPrivate,
        author_signature: String::new(),
        platform_attestation: None,
        chain_reference: None,
        timestamp: DateTime::from_timestamp(0, 0).unwrap(),
    }
}

#[test]
fn signed_manifest_verifies_before_tamper() {
    let signer = AuthorSigner::from_seed(&[7u8; 32]);
    let pk = signer.public_key();
    let mut m = sample_manifest();
    m.author_signature = m.author_signature_hex(&signer).unwrap();
    m.verify_author(&pk).unwrap();
}

#[test]
fn tampering_a_field_changes_hash_and_breaks_signature() {
    let signer = AuthorSigner::from_seed(&[7u8; 32]);
    let pk = signer.public_key();
    let mut m = sample_manifest();
    m.author_signature = m.author_signature_hex(&signer).unwrap();
    let original_payload_hash = Hash::of(&m.signable_bytes().unwrap()).unwrap();
    assert!(m.verify_author(&pk).is_ok());

    // Mutate one committed field.
    m.claim_id = "claim-2".to_string();
    let tampered_payload_hash = Hash::of(&m.signable_bytes().unwrap()).unwrap();
    assert_ne!(original_payload_hash, tampered_payload_hash);
    assert!(m.verify_author(&pk).is_err());
}

#[test]
fn wrong_author_key_fails() {
    let signer = AuthorSigner::from_seed(&[7u8; 32]);
    let other = AuthorSigner::from_seed(&[8u8; 32]);
    let mut m = sample_manifest();
    m.author_signature = m.author_signature_hex(&signer).unwrap();
    assert!(m.verify_author(&other.public_key()).is_err());
}
