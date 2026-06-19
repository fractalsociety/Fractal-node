//! Gates P02-N03 / P02-N04 / P02-N07: the generic kernel is deterministic for a
//! given seed, varies across seeds, and would catch injected nondeterminism.

use fractal_society::adapters::{ReferenceAdapter, ReferenceAgent};
use fractal_society::kernel::{self, KernelConfig};

fn make(seed: u64) -> (ReferenceAdapter, ReferenceAgent) {
    (
        ReferenceAdapter::new(4, 20, seed),
        ReferenceAgent::new(4, seed),
    )
}

/// P02-N03: same seed -> byte-identical evidence hash across 100 independent
/// runs (fresh adapter + agent each time).
#[tokio::test]
async fn same_seed_100_runs_byte_identical_evidence_hash() {
    let cfg = KernelConfig::default();
    let (a0, g0) = make(42);
    let first = kernel::run(a0, g0, 42, &cfg).await.unwrap();
    for i in 1..=100 {
        let (a, g) = make(42);
        let r = kernel::run(a, g, 42, &cfg).await.unwrap();
        assert_eq!(
            r.evidence_hash, first.evidence_hash,
            "evidence hash diverged on run {i}"
        );
        // The decision trace count must also be identical.
        assert_eq!(
            r.evidence.decision_traces.len(),
            first.evidence.decision_traces.len()
        );
    }
}

/// P02-N04: different seeds produce controlled variation (different hashes).
#[tokio::test]
async fn different_seeds_produce_different_hashes() {
    let cfg = KernelConfig::default();
    let (a1, g1) = make(1);
    let (a2, g2) = make(2);
    let run1 = kernel::run(a1, g1, 1, &cfg).await.unwrap();
    let run2 = kernel::run(a2, g2, 2, &cfg).await.unwrap();
    assert_ne!(run1.evidence_hash, run2.evidence_hash);
}

/// P02-N07 guard: the kernel records the same number of steps every run for the
/// same seed (a flaky/nondeterministic adapter would shift this).
#[tokio::test]
async fn step_count_is_stable_across_runs() {
    let cfg = KernelConfig::default();
    let (a, g) = make(123);
    let r = kernel::run(a, g, 123, &cfg).await.unwrap();
    let expected = r.evidence.decision_traces.len();
    for _ in 0..20 {
        let (a, g) = make(123);
        let r = kernel::run(a, g, 123, &cfg).await.unwrap();
        assert_eq!(r.evidence.decision_traces.len(), expected);
    }
}
