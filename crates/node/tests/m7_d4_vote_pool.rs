//! `NodeInner` vote-pool wrappers (`docs/prd.md` §7.3 / M7-d-4).
//!
//! These checks exercise the node's `build_self_vote` / `record_vote` /
//! `try_form_qc` surface against the real `VotePool` from `fractal-consensus`,
//! ensuring the `validators` + `validator_secret` plumbing established in
//! M7-d-2 is wired correctly. Gossipsub propagation is M7-d-5.

use fractal_consensus::{verify_formed_qc, RecordVoteOutcome, ValidatorSet, Vote, VoteSignBody};
use fractal_node::NodeInner;

fn sign_for(set: &ValidatorSet, idx: u32, view: u64, height: u64, hh_byte: u8) -> Vote {
    let sk = set.dev_bls_secret(idx as usize).unwrap();
    Vote::sign(
        VoteSignBody {
            view,
            height,
            header_hash: [hh_byte; 32],
        },
        idx,
        &sk,
    )
}

#[test]
fn singleton_node_self_vote_reaches_quorum_and_forms_qc() {
    let mut n = NodeInner::devnet_with_validator_index(ValidatorSet::phase1_singleton(), 0);
    let v = n
        .build_self_vote(0, 1, [0x42; 32])
        .expect("singleton has dev secret");
    assert_eq!(n.record_vote(v), RecordVoteOutcome::ReachedQuorum);

    let formed = n
        .try_form_qc(0, 1, [0x42; 32])
        .expect("singleton forms QC immediately");
    assert_eq!(formed.signer_indices, vec![0u32]);
    verify_formed_qc(&formed, &n.validators).expect("QC verifies via fast_aggregate_verify");
}

#[test]
fn node_without_signing_key_cannot_build_self_vote_but_can_record_peers() {
    let mut n =
        NodeInner::devnet_with_validator_secret(ValidatorSet::phase2_bft7_fixture(), 0, None);
    assert!(n.build_self_vote(1, 1, [0xaa; 32]).is_none());
    // But it can still record peer votes.
    let set = n.validators.clone();
    for idx in 0u32..5 {
        let outcome = n.record_vote(sign_for(&set, idx, 1, 1, 0xaa));
        if idx < 4 {
            assert_eq!(outcome, RecordVoteOutcome::Accepted, "idx={idx}");
        } else {
            assert_eq!(outcome, RecordVoteOutcome::ReachedQuorum, "idx={idx}");
        }
    }
    let formed = n.try_form_qc(1, 1, [0xaa; 32]).expect("five signers → QC");
    verify_formed_qc(&formed, &n.validators).expect("aggregate verifies");
}

#[test]
fn node_record_vote_rejects_tampered_signature() {
    let mut n = NodeInner::devnet_with_validator_index(ValidatorSet::phase2_bft7_fixture(), 0);
    let mut v = sign_for(&n.validators.clone(), 1, 2, 2, 0xbb);
    v.signature.0[3] ^= 0x55;
    assert_eq!(n.record_vote(v), RecordVoteOutcome::BadSignature);
    assert_eq!(n.vote_pool.slots_len(), 0);
}

#[test]
fn node_record_vote_rejects_duplicate_validator_index_at_same_block() {
    let mut n = NodeInner::devnet_with_validator_index(ValidatorSet::phase2_bft7_fixture(), 0);
    let set = n.validators.clone();
    let v = sign_for(&set, 4, 7, 3, 0xcc);
    assert_eq!(n.record_vote(v.clone()), RecordVoteOutcome::Accepted);
    assert_eq!(n.record_vote(v), RecordVoteOutcome::DuplicateValidator);
}

#[test]
fn try_form_qc_returns_none_until_threshold() {
    let mut n = NodeInner::devnet_with_validator_index(ValidatorSet::phase2_bft7_fixture(), 0);
    let set = n.validators.clone();
    for idx in 0u32..3 {
        let _ = n.record_vote(sign_for(&set, idx, 1, 1, 0xdd));
    }
    assert!(n.try_form_qc(1, 1, [0xdd; 32]).is_none());
    // Cross the threshold (5-of-7).
    for idx in 3u32..5 {
        let _ = n.record_vote(sign_for(&set, idx, 1, 1, 0xdd));
    }
    let formed = n.try_form_qc(1, 1, [0xdd; 32]).expect("threshold met");
    assert_eq!(formed.signer_indices, vec![0, 1, 2, 3, 4]);
    verify_formed_qc(&formed, &n.validators).expect("verify");
}
