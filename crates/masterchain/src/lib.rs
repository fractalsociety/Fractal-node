//! Dedicated masterchain BFT coordinator (`docs/prd.md` §7.10, M11).
//!
//! Run via `fractal-masterchain` binary. Shard validators set `FRACTAL_MASTERCHAIN_RPC`
//! to post anchors instead of sealing masterchain blocks locally.

#[cfg(feature = "runtime")]
pub mod bft;
pub mod client;
pub mod ledger;
#[cfg(feature = "runtime")]
pub mod network;
#[cfg(feature = "runtime")]
pub mod node;
#[cfg(feature = "runtime")]
mod rpc;

#[cfg(feature = "runtime")]
pub use bft::{MasterchainTimeoutGossipV1, MasterchainVoteGossipV1};
pub use ledger::{
    anchor_from_block_header, forced_inclusion_request_id, invalid_proof_evidence_hash,
    submission_reason_code, AsyncCrossZoneMessageV1, ExecutionZoneMetadataV1,
    ExecutionZoneRecordV1, ForcedInclusionEventV1, ForcedInclusionRequestV1,
    InvalidProofSlashEventV1, MasterchainError, MasterchainLedger, ProofSlashingPolicyV1,
    ProverIdentityV1, ProverMarketParamsV1, ZoneId, ZoneProofFinalUpdateV1,
    INVALID_PROOF_AGGREGATOR_REJECTED, INVALID_PROOF_BAD_RANGE, INVALID_PROOF_DUPLICATE,
    INVALID_PROOF_EMPTY_DIGEST, INVALID_PROOF_MISSING_VERIFIED_STWO,
    INVALID_PROOF_RANGE_EXCEEDS_ANCHOR, INVALID_PROOF_UNKNOWN_SHARD,
};
#[cfg(feature = "runtime")]
pub use network::{masterchain_gossip_task, parse_masterchain_bootstraps};
#[cfg(feature = "runtime")]
pub use node::{
    configure_proof_slashing_from_env, masterchain_bft_producer_loop,
    masterchain_block_time_ms_from_env, masterchain_ledger_from_env,
    masterchain_validator_index_from_env, masterchain_validator_set_from_env,
    prover_address_from_env, prover_reward_treasury_from_env, MasterchainBftNode,
    MasterchainHandle,
};

#[cfg(feature = "runtime")]
pub async fn run_masterchain_bft() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let shard_count = std::env::var(fractal_shard::ENV_SHARD_COUNT)
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .filter(|&n| n >= 1)
        .unwrap_or(2);
    let mut inner = MasterchainBftNode::devnet_from_env();
    inner.shard_count = shard_count;

    if let Some(path) = std::env::var("FRACTAL_MASTERCHAIN_ROCKSDB_PATH")
        .ok()
        .filter(|s| !s.trim().is_empty())
    {
        let db = fractal_storage::FractalRocksDb::open(std::path::Path::new(path.trim()))?;
        eprintln!("fractal-masterchain: RocksDB open {:?}", path.trim());
        inner.chain_store = Some(db);
    }

    let validator_count = inner.validators.len();
    let validator_index = inner.validator_index;
    let quorum = inner.validators.quorum_threshold();
    let prover_rewards = inner.ledger.prover_economics.clone();
    let prover_treasury_balance = inner.ledger.treasury_balance_wei;
    let proof_slashing = inner.ledger.proof_slashing_policy.clone();
    let (vote_tx, vote_rx) = tokio::sync::mpsc::unbounded_channel();
    let (timeout_tx, timeout_rx) = tokio::sync::mpsc::unbounded_channel();
    inner.set_vote_sink(Some(vote_tx));
    inner.set_timeout_sink(Some(timeout_tx));
    let node: MasterchainHandle = std::sync::Arc::new(tokio::sync::Mutex::new(inner));
    let addr: std::net::SocketAddr = std::env::var("FRACTAL_MASTERCHAIN_RPC_ADDR")
        .or_else(|_| std::env::var("FRACTAL_RPC_ADDR"))
        .unwrap_or_else(|_| "127.0.0.1:8550".into())
        .parse()?;
    let rpc_stats = fractal_rpc::RpcCallStats::default();
    let (handle, bound) =
        fractal_rpc::serve_masterchain_http(addr, node.clone(), rpc_stats).await?;
    eprintln!(
        "fractal-masterchain: BFT coordinator json-rpc at http://{bound} (shard_count={shard_count}, validators={validator_count}, index={validator_index}, quorum={quorum}, block_ms={}, prover_rewards={}, reward_per_block={}, lag_half_life={}, reward_treasury_wei={}, proof_slashing={}, require_verified_stwo={}, slash_amount_wei={})",
        masterchain_block_time_ms_from_env(),
        prover_rewards.enabled,
        prover_rewards.base_reward_per_block_wei,
        prover_rewards.lag_half_life_seconds,
        prover_treasury_balance,
        proof_slashing.enabled,
        proof_slashing.require_verified_stwo,
        proof_slashing.slash_amount_wei
    );

    let p2p_listen: libp2p::Multiaddr = std::env::var("FRACTAL_MASTERCHAIN_P2P_LISTEN")
        .unwrap_or_else(|_| "/ip4/0.0.0.0/udp/0/quic-v1".into())
        .parse()?;
    let bootstraps = std::env::var("FRACTAL_MASTERCHAIN_BOOTSTRAP")
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(|s| parse_masterchain_bootstraps(&s))
        .transpose()?
        .unwrap_or_default();
    tokio::spawn(masterchain_gossip_task(
        node.clone(),
        p2p_listen,
        bootstraps,
        None,
        Some(vote_rx),
        Some(timeout_rx),
    ));

    tokio::spawn(masterchain_bft_producer_loop(node.clone()));
    tokio::signal::ctrl_c().await?;
    handle.stop()?;
    Ok(())
}

#[cfg(not(feature = "runtime"))]
pub async fn run_masterchain_bft() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    Err("fractal-masterchain built without runtime feature".into())
}
