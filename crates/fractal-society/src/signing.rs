//! Author signatures over canonical payloads (PHASE-01, gate P01-N03).
//!
//! Artifact and proof manifests are signed by an author's Ed25519 key over the
//! canonical bytes of their *signable payload* — every field except the
//! signature itself, so a signature never covers itself. This reuses
//! `fractal-crypto`'s Ed25519 helpers, matching the TypeScript app's
//! `@noble/ed25519` convention so a signature made in Rust verifies in TS and
//! vice versa.

use crate::canonical;
use crate::error::{Error, Result};
use ed25519_dalek::SigningKey;
use serde::Serialize;

/// An author's Ed25519 signing key.
#[derive(Debug, Clone)]
pub struct AuthorSigner {
    key: SigningKey,
}

impl AuthorSigner {
    /// Create a signer from a 32-byte seed.
    pub fn from_seed(seed: &[u8; 32]) -> Self {
        Self {
            key: SigningKey::from_bytes(seed),
        }
    }

    /// Generate a random signer. Intended for tests and local dev; production
    /// keys must come from a protected secret store.
    pub fn generate<R: rand::CryptoRng + rand::RngCore>(rng: &mut R) -> Self {
        Self {
            key: SigningKey::generate(rng),
        }
    }

    /// The 32-byte public verification key corresponding to this signer.
    pub fn public_key(&self) -> [u8; 32] {
        self.key.verifying_key().to_bytes()
    }

    /// Sign a raw message.
    pub fn sign_bytes(&self, message: &[u8]) -> [u8; 64] {
        fractal_crypto::sign_message(&self.key, message)
    }

    /// Sign the canonical bytes of a serializable payload, returning hex.
    pub fn sign_canonical<T: Serialize + ?Sized>(&self, payload: &T) -> Result<String> {
        let bytes = canonical::signable_bytes(payload)?;
        Ok(hex::encode(self.sign_bytes(&bytes)))
    }
}

/// Verify an Ed25519 signature over `message` with a 32-byte public key.
pub fn verify_signature(public_key: &[u8; 32], message: &[u8], signature: &[u8; 64]) -> Result<()> {
    fractal_crypto::verify_message(public_key, message, signature)
        .map_err(|e| Error::Signature(e.to_string()))
}

/// Verify a hex-encoded Ed25519 signature over the canonical bytes of `payload`.
pub fn verify_canonical<T: Serialize + ?Sized>(
    public_key: &[u8; 32],
    payload: &T,
    signature_hex: &str,
) -> Result<()> {
    let signature = decode_signature_hex(signature_hex)?;
    let message = canonical::signable_bytes(payload)?;
    verify_signature(public_key, &message, &signature)
}

/// Decode a hex-encoded 64-byte Ed25519 signature.
pub fn decode_signature_hex(hex_sig: &str) -> Result<[u8; 64]> {
    let bytes = hex::decode(hex_sig)
        .map_err(|e| Error::Deserialization(format!("invalid signature hex: {e}")))?;
    if bytes.len() != 64 {
        return Err(Error::Signature(format!(
            "Ed25519 signature must be 64 bytes, got {}",
            bytes.len()
        )));
    }
    let mut out = [0u8; 64];
    out.copy_from_slice(&bytes);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn alice() -> (AuthorSigner, [u8; 32]) {
        let signer = AuthorSigner::from_seed(&[0x42; 32]);
        let pk = signer.public_key();
        (signer, pk)
    }

    #[test]
    fn sign_then_verify_roundtrip() {
        let (signer, pk) = alice();
        let payload = json!({"claim": "alpha", "score": 0.42, "ok": true});
        let sig_hex = signer.sign_canonical(&payload).unwrap();
        verify_canonical(&pk, &payload, &sig_hex).unwrap();
    }

    #[test]
    fn tampered_payload_fails_verification() {
        let (signer, pk) = alice();
        let original = json!({"claim": "alpha", "score": 0.42});
        let sig_hex = signer.sign_canonical(&original).unwrap();
        let tampered = json!({"claim": "alpha", "score": 0.43});
        assert!(verify_canonical(&pk, &tampered, &sig_hex).is_err());
    }

    #[test]
    fn wrong_key_fails_verification() {
        let (signer, _) = alice();
        let payload = json!({"claim": "alpha"});
        let sig_hex = signer.sign_canonical(&payload).unwrap();
        let other = AuthorSigner::from_seed(&[0x99; 32]);
        assert!(verify_canonical(&other.public_key(), &payload, &sig_hex).is_err());
    }

    #[test]
    fn rejects_short_signature_hex() {
        assert!(decode_signature_hex("deadbeef").is_err());
    }
}
