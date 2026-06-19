//! Generic simulation kernel (PHASE-02, gates P02-N01 / P02-N10).
//!
//! Drives an [`Agent`](crate::simulation::Agent) against a
//! [`DomainAdapter`](crate::simulation::DomainAdapter) for a fixed number of
//! episodes and emits a canonical
//! [`EvidenceBundle`](crate::protocol::EvidenceBundle) plus a
//! [`MetricSet`](crate::simulation::MetricSet).
//!
//! # Determinism contract
//!
//! A run is a pure function of `(adapter, agent, seed, config)`. The kernel:
//! - never reads a wall clock inside the run (step timestamps come from a
//!   logical clock derived from the step index),
//! - never draws OS randomness,
//! - derives the run id solely from the seed.
//!
//! Therefore the same seed (with adapters/agents constructed identically)
//! produces byte-identical evidence hashes, which is what gate P02-N03 tests.

use crate::error::Result;
use crate::protocol::{self, Hash};
use crate::simulation::{
    Action, Agent, DomainAdapter, MetricSet, Observation, Outcome, PolicyDecision, RunTrace,
    RuntimeState,
};
use serde::{Deserialize, Serialize};

/// Kernel configuration for a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelConfig {
    /// Number of episodes to run.
    pub episodes: u64,
    /// Maximum steps per episode before forcing termination.
    pub max_steps_per_episode: u64,
}

impl Default for KernelConfig {
    fn default() -> Self {
        Self {
            episodes: 1,
            max_steps_per_episode: 100,
        }
    }
}

/// Frozen description of a run, sufficient to replay it deterministically.
///
/// Contains no wall-clock field; the run id is derived from the seed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunManifest {
    /// Run identifier (derived from the seed).
    pub run_id: String,
    /// Seed the run was started from.
    pub seed: u64,
    /// Adapter identifier.
    pub adapter_id: String,
    /// Adapter version.
    pub adapter_version: String,
    /// Agent identifier.
    pub agent_id: String,
    /// Number of episodes configured.
    pub episodes: u64,
    /// Max steps per episode configured.
    pub max_steps_per_episode: u64,
}

impl RunManifest {
    /// Canonical content hash of this manifest (freezes the run description).
    pub fn content_hash(&self) -> Result<Hash> {
        Hash::of(self)
    }
}

/// Outcome of a kernel run.
#[derive(Debug, Clone)]
pub struct RunOutcome {
    /// Manifest describing the run.
    pub manifest: RunManifest,
    /// Canonical evidence bundle emitted by the run.
    pub evidence: protocol::EvidenceBundle,
    /// Metrics computed by the adapter's scorer.
    pub metrics: MetricSet,
    /// Canonical hash of `evidence` — the value compared for determinism.
    pub evidence_hash: Hash,
}

/// Deterministic pseudo-timestamp from a step index. Never a wall clock.
fn logical_timestamp(step: u64) -> chrono::DateTime<chrono::Utc> {
    chrono::DateTime::<chrono::Utc>::from_timestamp(step as i64, 0)
        .unwrap_or_else(|| chrono::DateTime::<chrono::Utc>::from_timestamp(0, 0).expect("epoch"))
}

/// Run `agent` against `adapter` deterministically from `seed`.
///
/// The caller must pass freshly-constructed (or freshly-reset) adapter and
/// agent instances; the kernel itself holds no global or wall-clock state. To
/// replay a run, construct fresh instances the same way and call [`replay`]
/// (or [`run`] again) with the same seed and config.
pub async fn run<A, Ag>(
    mut adapter: A,
    mut agent: Ag,
    seed: u64,
    config: &KernelConfig,
) -> Result<RunOutcome>
where
    A: DomainAdapter,
    Ag: Agent<A>,
{
    let (adapter_id, adapter_version) = adapter.id();
    let agent_id = agent.id().to_string();
    let run_id = format!("run-{seed}");

    let mut trace = RunTrace::new(run_id.clone());
    if let Some(manifest) = agent.manifest() {
        trace.set_agent(manifest);
    }

    let mut total_reward = 0.0_f64;
    let mut global_step = 0u64;

    for episode in 0..config.episodes {
        let mut obs = adapter.reset().await?;
        for _ in 0..config.max_steps_per_episode {
            let state = RuntimeState {
                episode,
                step: global_step,
                reward: total_reward,
                state_data: serde_json::Value::Null,
            };
            let action = agent.act(&obs).await?;
            let policy = adapter.validate_action(&action, &state)?;
            let ts = logical_timestamp(global_step);
            let obs_json = obs.to_json()?;
            let action_json = action.to_json()?;
            match policy {
                PolicyDecision::Rejected { reason } => {
                    // Record the rejection and let the agent retry with the same
                    // observation. Bounded by max_steps_per_episode.
                    trace.record_step(
                        global_step,
                        obs_json,
                        action_json,
                        serde_json::json!({ "rejected": reason }),
                        ts,
                    );
                    global_step += 1;
                    continue;
                }
                PolicyDecision::Modified { .. } | PolicyDecision::Approved => {
                    // `Modified` carries a JSON value (not a typed Act); the
                    // adapter applies any modification internally during step.
                }
            }
            let step_result = adapter.step(action).await?;
            let outcome_json = step_result.outcome.to_json()?;
            trace.record_step(global_step, obs_json, action_json, outcome_json, ts);
            total_reward += step_result.outcome.primary_score();
            global_step += 1;
            if step_result.done {
                break;
            }
            obs = step_result.observation;
        }
    }

    trace.set_final_outcome(serde_json::json!({
        "total_reward": total_reward,
        "steps": global_step,
    }));

    let metrics = adapter.score(&trace).await?;
    let mut metrics_map = metrics.metrics.clone();
    metrics_map.insert("total_reward".to_string(), total_reward);
    metrics_map.insert("steps".to_string(), global_step as f64);

    let evidence = trace.into_evidence(run_id.clone(), metrics_map, logical_timestamp(0))?;
    let evidence_hash = Hash::of(&evidence)?;

    let manifest = RunManifest {
        run_id,
        seed,
        adapter_id,
        adapter_version,
        agent_id,
        episodes: config.episodes,
        max_steps_per_episode: config.max_steps_per_episode,
    };

    Ok(RunOutcome {
        manifest,
        evidence,
        metrics,
        evidence_hash,
    })
}

/// Re-run a previously-recorded [`RunManifest`] with fresh adapter and agent
/// instances constructed identically to the original run (gate P02-N10).
///
/// The returned evidence hash matches the original run's when the adapter and
/// agent are deterministic functions of the seed.
pub async fn replay<A, Ag>(adapter: A, agent: Ag, manifest: &RunManifest) -> Result<RunOutcome>
where
    A: DomainAdapter,
    Ag: Agent<A>,
{
    let config = KernelConfig {
        episodes: manifest.episodes,
        max_steps_per_episode: manifest.max_steps_per_episode,
    };
    run(adapter, agent, manifest.seed, &config).await
}
