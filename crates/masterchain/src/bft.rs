//! HotStuff-style BFT over masterchain blocks (coordination only, PRD §7.10).

use fractal_consensus::{
    FormedQc, FormedTimeoutCert, QuorumCertificate, RecordTimeoutOutcome, Timeout, TimeoutPool,
    TimeoutSignBody, Vote, VotePool, VoteSignBody, is_genesis_parent_qc, verify_formed_qc,
    verify_formed_timeout_cert,
};
use fractal_crypto::Hash256;
use fractal_crypto::hash::keccak256;
use fractal_shard::MasterchainBlockV1;

/// Gossiped masterchain vote envelope (topic payload is `borsh(Self)`).
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct MasterchainVoteGossipV1 {
    pub block: MasterchainBlockV1,
    pub vote: Vote,
}

/// Gossiped masterchain timeout envelope (topic payload is `borsh(Self)`).
#[derive(Debug, Clone, PartialEq, Eq, borsh::BorshSerialize, borsh::BorshDeserialize)]
pub struct MasterchainTimeoutGossipV1 {
    pub timeout: Timeout,
}

#[must_use]
pub fn masterchain_block_hash(block: &MasterchainBlockV1) -> Hash256 {
    let bytes = borsh::to_vec(block).expect("masterchain block borsh");
    keccak256(&bytes)
}

/// Sign a masterchain block vote for gossip.
#[must_use]
pub fn sign_masterchain_vote(
    view: u64,
    block: &MasterchainBlockV1,
    validator_index: u32,
    secret: &fractal_crypto::BlsSecretKey,
) -> Vote {
    Vote::sign(
        VoteSignBody {
            view,
            height: block.height,
            header_hash: masterchain_block_hash(block),
        },
        validator_index,
        secret,
    )
}

/// Record a vote for a sealed masterchain block and try to form a QC (singleton forms immediately).
pub fn record_masterchain_vote(
    pool: &mut VotePool,
    validators: &fractal_consensus::ValidatorSet,
    stake_weights: Option<&[u128]>,
    view: u64,
    block: &MasterchainBlockV1,
    validator_index: u32,
    secret: &fractal_crypto::BlsSecretKey,
) -> Option<FormedQc> {
    let vote = sign_masterchain_vote(view, block, validator_index, secret);
    record_masterchain_vote_message(pool, validators, stake_weights, block, vote)
}

/// Record a gossiped masterchain vote and try to form a QC.
pub fn record_masterchain_vote_message(
    pool: &mut VotePool,
    validators: &fractal_consensus::ValidatorSet,
    stake_weights: Option<&[u128]>,
    block: &MasterchainBlockV1,
    vote: Vote,
) -> Option<FormedQc> {
    let hh = masterchain_block_hash(block);
    if vote.height != block.height || vote.header_hash != hh {
        return None;
    }
    let view = vote.view;
    let _outcome = pool.record(vote, validators, stake_weights);
    pool.try_form_qc(view, block.height, hh, validators, stake_weights)
}

/// Sign a view timeout for masterchain pacemaker gossip.
#[must_use]
pub fn sign_masterchain_timeout(
    view: u64,
    high_qc: QuorumCertificate,
    validator_index: u32,
    secret: &fractal_crypto::BlsSecretKey,
) -> Timeout {
    Timeout::sign(TimeoutSignBody { view, high_qc }, validator_index, secret)
}

/// Record a gossiped timeout and try to form the best timeout certificate for its view.
pub fn record_masterchain_timeout(
    pool: &mut TimeoutPool,
    validators: &fractal_consensus::ValidatorSet,
    timeout: Timeout,
) -> (RecordTimeoutOutcome, Option<FormedTimeoutCert>) {
    let view = timeout.view;
    let outcome = pool.record(timeout, validators);
    let cert = pool.try_form_best_timeout_cert_for_view(view, validators);
    (outcome, cert)
}

/// Verify QC binds to a known masterchain block hash.
pub fn verify_masterchain_qc(
    formed: &FormedQc,
    block: &MasterchainBlockV1,
    validators: &fractal_consensus::ValidatorSet,
    stake_weights: Option<&[u128]>,
) -> bool {
    let hh = masterchain_block_hash(block);
    if is_genesis_parent_qc(&formed.qc) {
        return block.height == 0;
    }
    if formed.qc.block_header_hash != hh {
        return false;
    }
    verify_formed_qc(formed, validators, stake_weights.as_deref()).is_ok()
}

/// Verify a masterchain timeout certificate against the active validator set.
pub fn verify_masterchain_timeout_cert(
    formed: &FormedTimeoutCert,
    validators: &fractal_consensus::ValidatorSet,
) -> bool {
    verify_formed_timeout_cert(formed, validators).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use fractal_shard::ShardAnchor;

    #[test]
    fn masterchain_block_hash_is_stable() {
        let b = MasterchainBlockV1 {
            height: 1,
            shard_anchors: vec![ShardAnchor {
                shard_id: 0,
                block_height: 4,
                state_root: [1u8; 32],
                witness_commitment: [2u8; 32],
            }],
            validity_proofs: vec![],
            global_state_root: [3u8; 32],
            global_zk_root: [4u8; 32],
            cross_shard_messages: vec![],
        };
        assert_eq!(masterchain_block_hash(&b), masterchain_block_hash(&b));
    }
}
