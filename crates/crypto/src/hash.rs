use borsh::BorshSerialize;
use sha2::{Digest, Sha256};
use sha3::Keccak256;

pub type Hash256 = [u8; 32];

pub fn sha256(bytes: &[u8]) -> Hash256 {
    let mut h = Sha256::new();
    Digest::update(&mut h, bytes);
    h.finalize().into()
}

pub fn keccak256(bytes: &[u8]) -> Hash256 {
    let mut h = Keccak256::new();
    sha3::Digest::update(&mut h, bytes);
    h.finalize().into()
}

/// Canonical commitment for any borsh-serializable value (deterministic encoding).
pub fn commit_borsh<T: BorshSerialize>(value: &T) -> Result<Hash256, std::io::Error> {
    let bytes = borsh::to_vec(value)?;
    Ok(keccak256(&bytes))
}
