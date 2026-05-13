//! Placeholder BLS12-381 aggregate types for HotStuff-2 QCs (real crypto in M7+).
//!
//! M1 keeps deterministic builds without linking `blst`.

use borsh::{BorshDeserialize, BorshSerialize};
use thiserror::Error;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize,
)]
pub struct BlsPublicKey(pub [u8; 48]);

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize,
)]
pub struct BlsSignature(pub [u8; 96]);

#[derive(Debug, Clone, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct AggregateSignature {
    pub bytes: [u8; 96],
}

#[derive(Debug, Error)]
pub enum BlsError {
    #[error("BLS aggregate verification not implemented (M1 placeholder)")]
    NotImplemented,
}

impl AggregateSignature {
    pub fn verify_placeholder(&self) -> Result<(), BlsError> {
        Err(BlsError::NotImplemented)
    }
}
