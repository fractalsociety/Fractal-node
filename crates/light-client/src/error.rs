//! Light-client verification errors.

use fractal_proof_aggregator::{AggregatorError, SubmissionError};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum LightClientError {
    #[error("global state root mismatch (recomputed from shard anchors)")]
    GlobalStateRootMismatch,
    #[error("masterchain height mismatch: block={block} bundle={bundle}")]
    MasterchainHeightMismatch { block: u64, bundle: u64 },
    #[error("plonky2 statement does not match masterchain block")]
    Plonky2StatementMismatch,
    #[error("missing plonky2 bundle but validity proofs are non-empty")]
    MissingPlonky2Bundle,
    #[error("proof submission validation failed: {0}")]
    Submission(#[from] SubmissionError),
    #[error("aggregator verification failed: {0}")]
    Aggregator(#[from] AggregatorError),
    #[error("rpc error: {0}")]
    Rpc(String),
    #[error("json parse error: {0}")]
    Json(String),
    #[error("hex decode error: {0}")]
    Hex(String),
    #[error("unknown shard {0} in anchor query")]
    UnknownShard(u32),
}
