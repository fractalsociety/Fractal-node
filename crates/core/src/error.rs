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
    #[error("block gas limit exceeded")]
    GasLimitExceeded,
    #[error("not authorized")]
    NotAuthorized,
    #[error("entity not found")]
    NotFound,
    #[error("merkle proof invalid")]
    InvalidProof,
    #[error("agent id already bound")]
    AgentIdCollision,
    #[error("duplicate receipt id")]
    DuplicateReceipt,
    #[error("invalid payout entry ordering")]
    BadPayoutOrdering,
    #[error("payout already claimed")]
    AlreadyClaimed,
    #[error("batch not found")]
    BatchNotFound,
    #[error("gas arithmetic overflow")]
    GasOverflow,
    #[error("invalid signature")]
    BadSignature,
}
