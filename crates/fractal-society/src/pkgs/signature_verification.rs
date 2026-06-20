//! Signature-verification package.
//!
//! Verifies signatures attached to a package digest against a set of public keys.

use crate::artifact::PackageDigest;

/// Count attached digest signatures that verify against at least one public key.
pub fn verify_all(digest: &PackageDigest, public_keys: &[&[u8; 32]]) -> usize {
    digest
        .signatures
        .iter()
        .filter(|signature| {
            public_keys.iter().any(|public_key| {
                digest
                    .verify_signature(&signature.signer, public_key)
                    .is_ok()
            })
        })
        .count()
}

/// Return true iff every attached signature verifies against the provided public keys.
pub fn all_valid(digest: &PackageDigest, public_keys: &[&[u8; 32]]) -> bool {
    !digest.signatures.is_empty() && verify_all(digest, public_keys) == digest.signatures.len()
}
