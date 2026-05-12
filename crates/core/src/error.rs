use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ExecError {
    #[error("unknown signer account")]
    UnknownSigner,
    #[error("bad nonce: expected {expected}, got {actual}")]
    BadNonce { expected: u64, actual: u64 },
    #[error("insufficient balance")]
    InsufficientBalance,
    #[error("transaction vm and payload shape mismatch")]
    InvalidShape,
}
