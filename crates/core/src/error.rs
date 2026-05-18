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
    #[error("slashing evidence hash was not committed by governance")]
    MissingSlashingEvidence,
    #[error("consensus misbehavior evidence is invalid or failed verification")]
    InvalidMisbehaviorEvidence,
    #[error("misbehavior evidence does not match the slash target fingerprint")]
    MisbehaviorFingerprintMismatch,
    #[error("misbehavior evidence was already applied on-chain")]
    DuplicateMisbehaviorEvidence,
    #[error("permissionless validator entry is disabled")]
    PermissionlessEntryDisabled,
    #[error("validator fingerprint already registered")]
    ValidatorAlreadyRegistered,
    #[error("validator fingerprint is not registered")]
    ValidatorNotRegistered,
    #[error("bonded stake below minimum validator stake for registration")]
    BelowMinValidatorStake,
    #[error("wallet capability already registered on-chain")]
    DuplicateWalletCapability,
    #[error("wallet parent capability not found on-chain")]
    WalletCapabilityNotFound,
    #[error("wallet capability is revoked")]
    WalletCapabilityRevoked,
    #[error("wallet capability failed signature, time, or chain_id checks")]
    WalletCapabilityInvalid,
    #[error("wallet child capability is not strictly attenuated from parent")]
    WalletAttenuationFailed,
    #[error("wallet budget account not found")]
    WalletBudgetNotFound,
    #[error("wallet budget account not owned by signer")]
    WalletBudgetNotOwned,
    #[error("wallet budget parent/child link invalid")]
    WalletBudgetLinkInvalid,
    #[error("wallet budget still has reserved funds")]
    WalletBudgetNotEmpty,
    #[error("wallet capability already revoked on-chain")]
    WalletCapabilityAlreadyRevoked,
    #[error("wallet revoke issuer signature invalid")]
    WalletRevokeSignatureInvalid,
    #[error("wallet mint requires non-revocation proof borsh")]
    WalletRevocationProofRequired,
    #[error("wallet mint revocation proof invalid or root mismatch")]
    WalletRevocationProofInvalid,
    #[error("wallet task not found")]
    WalletTaskNotFound,
    #[error("wallet task lifecycle state invalid for this operation")]
    WalletTaskState,
    #[error("wallet emergency stop is active")]
    WalletEmergencyStopActive,
    #[error("wallet tool receipt already settled on-chain")]
    WalletToolReceiptAlreadySettled,
    #[error("duplicate wallet tool batch id")]
    WalletToolBatchDuplicate,
    #[error("wallet tool batch payload invalid")]
    WalletToolBatchInvalid,
    #[error("wallet provider already registered")]
    WalletProviderAlreadyRegistered,
    #[error("wallet provider not found")]
    WalletProviderNotFound,
    #[error("wallet provider not owned by signer")]
    WalletProviderNotOwned,
    #[error("wallet provider stake insufficient")]
    WalletProviderStakeInsufficient,
    #[error("wallet provider unstake request is not mature")]
    WalletProviderUnstakeNotMature,
    #[error("wallet provider still has bonded or pending stake")]
    WalletProviderStakeNotEmpty,
}
