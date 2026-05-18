use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Ed25519Error {
    #[error("invalid signature length")]
    BadSignature,
    #[error("signature verification failed")]
    VerificationFailed,
}

pub fn verify_message(public_key: &[u8; 32], message: &[u8], sig_bytes: &[u8; 64]) -> Result<(), Ed25519Error> {
    let vk = VerifyingKey::from_bytes(public_key).map_err(|_| Ed25519Error::BadSignature)?;
    let sig = Signature::from_bytes(sig_bytes);
    vk.verify(message, &sig).map_err(|_| Ed25519Error::VerificationFailed)
}

pub fn sign_message(signing_key: &SigningKey, message: &[u8]) -> [u8; 64] {
    signing_key.sign(message).to_bytes()
}
