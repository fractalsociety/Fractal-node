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
    #[error("EVM execution reverted or halted")]
    EvmFailed,
    /// `REVERT` return data (often ABI-encoded `Error(string)`); empty when revert has no payload.
    #[error("execution reverted")]
    EvmRevert { return_data: Vec<u8> },
    #[error("wallet TaskReceipt witness requires fractal-core built with --features wallet")]
    WalletFeatureDisabled,
    #[error("wallet receipt witness does not match commitment")]
    WalletCommitmentMismatch,
    #[error("duplicate wallet task-receipt anchor")]
    DuplicateWalletAnchor,
    #[error("duplicate proof commitment")]
    DuplicateProofCommitment,
    #[error("permissionless validator entry is disabled")]
    PermissionlessEntryDisabled,
    #[error("validator fingerprint is already registered")]
    ValidatorAlreadyRegistered,
    #[error("bonded stake is below minimum validator stake")]
    BelowMinValidatorStake,
}
