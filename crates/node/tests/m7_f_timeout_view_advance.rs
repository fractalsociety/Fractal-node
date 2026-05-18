//! View liveness via quorum timeouts (`docs/prd.md` §7.4 / §18 M7-f).

use fractal_consensus::{
    genesis_parent_qc, RecordTimeoutOutcome, Timeout, TimeoutSignBody, ValidatorSet,
};
use fractal_node::NodeInner;

#[test]
fn singleton_skips_timeout_view_advance() {
    let mut n = NodeInner::devnet();
    n.view = 9;
    n.try_advance_view_on_timeout_quorum();
    assert_eq!(n.view, 9);
}

#[test]
fn bft7_quorum_timeouts_advance_view() {
    let validators = ValidatorSet::phase2_bft7_fixture();
    let mut n = NodeInner::devnet_with_validator_index(validators.clone(), 0);
    n.view = 2;
    let hq = genesis_parent_qc();
    for i in 0u32..5 {
        let sk = validators.dev_bls_secret(i as usize).expect("fixture secret");
        let t = Timeout::sign(
            TimeoutSignBody {
                view: 2,
                high_qc: hq.clone(),
            },
            i,
            &sk,
        );
        let out = n.record_timeout(t);
        if i < 4 {
            assert!(matches!(out, RecordTimeoutOutcome::Accepted), "i={i} {out:?}");
        } else {
            assert_eq!(out, RecordTimeoutOutcome::ReachedQuorum);
        }
    }
    n.try_advance_view_on_timeout_quorum();
    assert_eq!(n.view, 3);
}
