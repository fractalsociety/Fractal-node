//! HotStuff-2 wire types + misbehavior evidence verification (no `fractal-core` dependency).
//!
//! Used by `fractal-consensus` for networking/block production and by `fractal-core` for
//! permissionless [`fractal_core::NativeCall::SlashConsensusStakeVerified`].

pub mod misbehavior;
pub mod qc;
pub mod quorum;
pub mod timeout;
pub mod validators;
pub mod vote;

pub use misbehavior::{
    validator_set_from_registry, verify_consensus_misbehavior_evidence,
    ConsensusMisbehaviorEvidenceV1, MisbehaviorError, MisbehaviorKind,
};
pub use qc::{
    genesis_parent_qc, hash_qc, is_genesis_parent_qc, singleton_qc_certifying, QuorumCertificate,
};
pub use quorum::quorum_stake_threshold;
pub use timeout::{
    high_qc_rank, verify_formed_timeout_cert, FormedTimeoutCert, Timeout, TimeoutError,
    TimeoutSignBody,
};
pub use validators::{ValidatorEntry, ValidatorId, ValidatorSet};
pub use vote::{verify_formed_qc, FormedQc, Vote, VoteError, VotePool, VoteSignBody};
