use fractal_society::artifact::{PackageDigest, Signature};
use fractal_society::pkgs::signature_verification::{all_valid, verify_all};
use fractal_society::protocol::Hash;
use fractal_society::signing::AuthorSigner;

fn unsigned_digest() -> PackageDigest {
    PackageDigest {
        package_id: "pkg-1".to_string(),
        version: "0.1.0".to_string(),
        content_hash: Hash::new(b"content"),
        manifest_hash: Hash::new(b"manifest"),
        signatures: Vec::new(),
    }
}

fn sign_digest(digest: &mut PackageDigest, signer: &AuthorSigner, signer_id: &str) {
    let bytes = digest.signable_bytes().unwrap();
    digest.signatures.push(Signature {
        signer: signer_id.to_string(),
        signature: hex::encode(signer.sign_bytes(&bytes)),
        algorithm: "ed25519".to_string(),
        timestamp: chrono::DateTime::from_timestamp(0, 0).unwrap(),
    });
}

#[test]
fn signed_digest_is_valid_with_known_key() {
    let signer = AuthorSigner::from_seed(&[1u8; 32]);
    let public_key = signer.public_key();
    let mut digest = unsigned_digest();
    sign_digest(&mut digest, &signer, "alice");

    assert!(all_valid(&digest, &[&public_key]));
    assert_eq!(verify_all(&digest, &[&public_key]), 1);
}

#[test]
fn wrong_key_set_is_not_valid() {
    let signer = AuthorSigner::from_seed(&[1u8; 32]);
    let wrong = AuthorSigner::from_seed(&[2u8; 32]).public_key();
    let mut digest = unsigned_digest();
    sign_digest(&mut digest, &signer, "alice");

    assert!(!all_valid(&digest, &[&wrong]));
    assert_eq!(verify_all(&digest, &[&wrong]), 0);
}

#[test]
fn counts_multiple_valid_signatures() {
    let alice = AuthorSigner::from_seed(&[1u8; 32]);
    let bob = AuthorSigner::from_seed(&[2u8; 32]);
    let alice_pk = alice.public_key();
    let bob_pk = bob.public_key();
    let mut digest = unsigned_digest();
    sign_digest(&mut digest, &alice, "alice");
    sign_digest(&mut digest, &bob, "bob");

    assert_eq!(verify_all(&digest, &[&alice_pk, &bob_pk]), 2);
    assert!(all_valid(&digest, &[&alice_pk, &bob_pk]));
}

#[test]
fn unsigned_digest_is_not_all_valid() {
    let signer = AuthorSigner::from_seed(&[1u8; 32]);
    let public_key = signer.public_key();
    let digest = unsigned_digest();

    assert_eq!(verify_all(&digest, &[&public_key]), 0);
    assert!(!all_valid(&digest, &[&public_key]));
}
