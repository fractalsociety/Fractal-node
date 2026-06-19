//! Gates P02-N02 / P02-N05: the reference adapter satisfies the full
//! DomainAdapter contract through the kernel, and invalid actions are rejected.

use fractal_society::adapters::{BanditAction, ReferenceAdapter, ReferenceAgent};
use fractal_society::kernel::{self, KernelConfig};
use fractal_society::simulation::{DomainAdapter, PolicyDecision, RuntimeState};
use serde_json::Value;

/// P02-N02: the reference adapter runs end-to-end through the generic kernel
/// (the same public API a trading adapter will use).
#[tokio::test]
async fn reference_adapter_runs_full_kernel_path() {
    let cfg = KernelConfig {
        episodes: 1,
        max_steps_per_episode: 10,
    };
    let out = kernel::run(
        ReferenceAdapter::new(4, 10, 99),
        ReferenceAgent::new(4, 99),
        99,
        &cfg,
    )
    .await
    .unwrap();

    assert_eq!(out.evidence.decision_traces.len(), 10);
    assert!(out.metrics.primary_metric >= 0.0);
    assert!(out.metrics.metrics.contains_key("total_reward"));
    assert_eq!(out.manifest.adapter_id, "reference-bandit");
    assert_eq!(out.manifest.agent_id, "reference-random-agent");
}

/// P02-N05: invalid actions are rejected by the policy before stepping.
#[test]
fn invalid_arm_is_rejected() {
    let adapter = ReferenceAdapter::new(4, 10, 1);
    let state = RuntimeState {
        episode: 0,
        step: 0,
        reward: 0.0,
        state_data: Value::Null,
    };
    let good = adapter
        .validate_action(&BanditAction { arm: 0 }, &state)
        .unwrap();
    assert!(matches!(good, PolicyDecision::Approved));

    let bad = adapter
        .validate_action(&BanditAction { arm: 99 }, &state)
        .unwrap();
    assert!(matches!(bad, PolicyDecision::Rejected { .. }));
}
