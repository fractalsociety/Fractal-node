//! `FRACTAL_VALIDATOR_SECRET_HEX` + dev fallback (`docs/prd.md` §7.3 / M7-d-2).
//!
//! End-to-end checks that `NodeInner::devnet_with_validator_secret` plumbs the
//! supplied (or fallback) BLS key through, and that the `ValidatorSet` exposes
//! matching public keys + the M7-d quorum threshold.

use fractal_consensus::ValidatorSet;
use fractal_crypto::BlsSecretKey;
use fractal_node::NodeInner;

#[test]
fn devnet_with_validator_index_populates_dev_secret_for_singleton() {
    let n = NodeInner::devnet_with_validator_index(ValidatorSet::phase1_singleton(), 0);
    let sk = n.validator_secret.expect("singleton dev secret");
    let expected_pk = n.validators.bls_pubkey(0).expect("singleton pubkey");
    assert_eq!(&sk.public_key(), expected_pk);
}

#[test]
fn devnet_with_validator_index_populates_dev_secret_for_bft7() {
    for idx in 0..7 {
        let n = NodeInner::devnet_with_validator_index(ValidatorSet::phase2_bft7_fixture(), idx);
        let sk = n.validator_secret.expect("bft7 dev secret");
        let expected_pk = n.validators.bls_pubkey(idx).expect("bft7 pubkey");
        assert_eq!(&sk.public_key(), expected_pk, "idx={idx}");
    }
}

#[test]
fn devnet_with_validator_secret_accepts_none() {
    let n = NodeInner::devnet_with_validator_secret(
        ValidatorSet::phase2_bft7_fixture(),
        0,
        None,
    );
    assert!(n.validator_secret.is_none());
    // Validator set still exposes the pubkey so peers can verify others' votes.
    assert!(n.validators.bls_pubkey(0).is_some());
}

#[test]
fn validator_set_quorum_threshold_for_fixtures() {
    assert_eq!(ValidatorSet::phase1_singleton().quorum_threshold(), 1);
    assert_eq!(ValidatorSet::phase2_bft7_fixture().quorum_threshold(), 5);
}

#[test]
fn devnet_with_validator_secret_accepts_operator_supplied_key() {
    // Sanity: an operator-supplied BlsSecretKey is plumbed through unchanged.
    let custom = BlsSecretKey::from_ikm(&[7u8; 32]).expect("ikm");
    let pk_before = custom.public_key();
    let n = NodeInner::devnet_with_validator_secret(
        ValidatorSet::phase2_bft7_fixture(),
        3,
        Some(custom),
    );
    assert_eq!(
        n.validator_secret.as_ref().unwrap().public_key(),
        pk_before
    );
}
