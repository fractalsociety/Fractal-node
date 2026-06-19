//! Deterministic reference adapter: a seeded k-armed Bernoulli bandit
//! (PHASE-02, gates P02-N02 / P02-N03 / P02-N04 / P02-N05).
//!
//! This is deliberately non-trading — it exists to prove the kernel is generic.
//! All randomness is derived from a constructor seed via `StdRng`, so the same
//! seed yields the same run. There is no Hyperliquid, order, position, or
//! market code here.

use async_trait::async_trait;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::error::Result;
use crate::protocol::{DatasetManifest, EnvironmentManifest, Protocol};
use crate::simulation::{
    Action, Agent, CapabilityManifest, DatasetHandle, DomainAdapter, Environment, EnvironmentStep,
    Episode, EpisodeStep, MetricSet, Observation, Outcome, PolicyDecision, PublicEvidenceBundle,
    PublicEvidenceStep, RunTrace, RuntimeState, StepResult, TerminalCondition,
    TerminalConditionType, ValidationReport,
};

/// Adapter and version identifier for the reference bandit.
pub const REFERENCE_ADAPTER_ID: &str = "reference-bandit";
/// Semantic version of the reference bandit adapter.
pub const REFERENCE_ADAPTER_VERSION: &str = "0.1.0";

/// Observation emitted by the bandit after each pull.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BanditObservation {
    /// Step index within the current episode.
    pub step: u64,
    /// Number of arms available.
    pub arm_count: u64,
    /// Reward received on the most recent pull.
    pub last_reward: f64,
}

impl Observation for BanditObservation {
    fn to_json(&self) -> Result<serde_json::Value> {
        Ok(serde_json::to_value(self)?)
    }
}

/// Action: pull a specific arm.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BanditAction {
    /// Arm index to pull (0-based).
    pub arm: u64,
}

impl Action for BanditAction {
    fn validate(&self) -> Result<()> {
        Ok(())
    }
    fn to_json(&self) -> Result<serde_json::Value> {
        Ok(serde_json::to_value(self)?)
    }
}

/// Outcome of a single pull.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BanditOutcome {
    /// Reward from this pull (0.0 or 1.0).
    pub reward: f64,
    /// Step index after this pull.
    pub step: u64,
    /// Whether the episode terminated after this pull.
    pub terminal: bool,
}

impl Outcome for BanditOutcome {
    fn primary_score(&self) -> f64 {
        self.reward
    }
    fn is_terminal(&self) -> bool {
        self.terminal
    }
    fn to_json(&self) -> Result<serde_json::Value> {
        Ok(serde_json::to_value(self)?)
    }
}

/// A seeded k-armed Bernoulli bandit environment.
pub struct ReferenceAdapter {
    arms: u64,
    max_steps: u64,
    seed: u64,
    arm_probs: Vec<f64>,
    rng: StdRng,
    step: u64,
    last_reward: f64,
}

impl ReferenceAdapter {
    /// Create a new bandit with `arms` arms, terminating after `max_steps`
    /// pulls per episode. All randomness derives from `seed`.
    pub fn new(arms: u64, max_steps: u64, seed: u64) -> Self {
        // Deterministic, evenly-spread per-arm win probabilities in (0, 1).
        let arm_probs = (0..arms)
            .map(|i| {
                let p = ((i as f64 + 1.0) / (arms as f64 + 1.0) * 10.0).fract();
                if p <= 0.0 {
                    0.05
                } else {
                    p
                }
            })
            .collect();
        Self {
            arms,
            max_steps,
            seed,
            arm_probs,
            rng: StdRng::seed_from_u64(seed),
            step: 0,
            last_reward: 0.0,
        }
    }

    fn reset_state(&mut self) {
        self.rng = StdRng::seed_from_u64(self.seed);
        self.step = 0;
        self.last_reward = 0.0;
    }

    fn pull(&mut self, arm: u64) -> BanditOutcome {
        let win = if arm < self.arms {
            let u: f64 = self.rng.gen();
            u < self.arm_probs[arm as usize]
        } else {
            false
        };
        let reward = if win { 1.0 } else { 0.0 };
        self.last_reward = reward;
        self.step += 1;
        let terminal = self.step >= self.max_steps;
        BanditOutcome {
            reward,
            step: self.step,
            terminal,
        }
    }
}

#[async_trait]
impl DomainAdapter for ReferenceAdapter {
    type Obs = BanditObservation;
    type Act = BanditAction;
    type Out = BanditOutcome;

    fn id(&self) -> (String, String) {
        (
            REFERENCE_ADAPTER_ID.to_string(),
            REFERENCE_ADAPTER_VERSION.to_string(),
        )
    }

    fn capability_manifest(&self) -> CapabilityManifest {
        CapabilityManifest {
            observation_types: vec!["step".to_string(), "last_reward".to_string()],
            action_types: vec!["pull_arm".to_string()],
            required_resources: Vec::new(),
            max_concurrent_episodes: 1,
        }
    }

    fn validate_protocol(&self, protocol: &Protocol) -> Result<ValidationReport> {
        let mut errors = Vec::new();
        if protocol.primary_metrics.is_empty() {
            errors.push("protocol must define at least one primary metric".to_string());
        }
        Ok(ValidationReport {
            is_valid: errors.is_empty(),
            errors,
            warnings: Vec::new(),
        })
    }

    async fn resolve_dataset(&self, _manifest: &DatasetManifest) -> Result<Box<dyn DatasetHandle>> {
        Ok(Box::new(StubDataset {
            episodes: self.max_steps as usize,
        }))
    }

    async fn create_environment(
        &self,
        _config: &EnvironmentManifest,
    ) -> Result<Box<dyn Environment>> {
        Ok(Box::new(BanditEnvironment {
            arms: self.arms,
            max_steps: self.max_steps,
            arm_probs: self.arm_probs.clone(),
            rng: StdRng::seed_from_u64(self.seed),
            step: 0,
            last_reward: 0.0,
        }))
    }

    async fn reset(&mut self) -> Result<Self::Obs> {
        self.reset_state();
        Ok(BanditObservation {
            step: 0,
            arm_count: self.arms,
            last_reward: 0.0,
        })
    }

    fn normalize_observation(&self, raw: serde_json::Value) -> Result<Self::Obs> {
        Ok(serde_json::from_value(raw)?)
    }

    fn validate_action(&self, action: &Self::Act, _state: &RuntimeState) -> Result<PolicyDecision> {
        if action.arm >= self.arms {
            Ok(PolicyDecision::Rejected {
                reason: format!("arm {} out of range (0..{})", action.arm, self.arms),
            })
        } else {
            Ok(PolicyDecision::Approved)
        }
    }

    async fn step(&mut self, action: Self::Act) -> Result<StepResult<Self::Obs, Self::Out>> {
        let outcome = self.pull(action.arm);
        let observation = BanditObservation {
            step: outcome.step,
            arm_count: self.arms,
            last_reward: outcome.reward,
        };
        let done = outcome.terminal;
        Ok(StepResult {
            observation,
            outcome,
            done,
            info: serde_json::json!({}),
        })
    }

    async fn score(&self, trace: &RunTrace) -> Result<MetricSet> {
        let mut total = 0.0_f64;
        let mut count = 0u64;
        for s in &trace.steps {
            if let Ok(out) = serde_json::from_value::<BanditOutcome>(s.outcome.clone()) {
                total += out.reward;
                count += 1;
            }
        }
        let avg = if count == 0 {
            0.0
        } else {
            total / count as f64
        };
        let mut metrics = HashMap::new();
        metrics.insert("total_reward".to_string(), total);
        metrics.insert("steps".to_string(), count as f64);
        Ok(MetricSet {
            primary_metric: avg,
            metrics,
            confidence_intervals: HashMap::new(),
        })
    }

    fn build_public_evidence(&self, trace: &RunTrace) -> Result<PublicEvidenceBundle> {
        let steps = trace
            .steps
            .iter()
            .map(|s| PublicEvidenceStep {
                step: s.step,
                action_type: "pull_arm".to_string(),
                outcome_type: "reward".to_string(),
                timestamp: s.timestamp,
            })
            .collect();
        Ok(PublicEvidenceBundle {
            evidence_id: trace.run_id.clone(),
            steps,
            metrics: MetricSet {
                primary_metric: 0.0,
                metrics: HashMap::new(),
                confidence_intervals: HashMap::new(),
            },
            verifier_summary: Vec::new(),
        })
    }

    fn terminal_conditions(&self) -> Vec<TerminalCondition> {
        vec![TerminalCondition {
            condition_type: TerminalConditionType::MaxSteps,
            threshold: self.max_steps as f64,
            strict: true,
        }]
    }
}

/// A deterministic random agent: picks arms from a seeded RNG, ignoring the
/// observation. Fully determined by its constructor seed.
pub struct ReferenceAgent {
    arms: u64,
    rng: StdRng,
}

impl ReferenceAgent {
    /// Create an agent that chooses among `arms` arms, seeded by `seed`.
    pub fn new(arms: u64, seed: u64) -> Self {
        Self {
            arms,
            rng: StdRng::seed_from_u64(seed),
        }
    }
}

/// Stable agent identifier for [`ReferenceAgent`].
pub const REFERENCE_AGENT_ID: &str = "reference-random-agent";

#[async_trait]
impl Agent<ReferenceAdapter> for ReferenceAgent {
    fn id(&self) -> &str {
        REFERENCE_AGENT_ID
    }

    async fn act(&mut self, _observation: &BanditObservation) -> Result<BanditAction> {
        let arm = self.rng.gen_range(0..self.arms);
        Ok(BanditAction { arm })
    }
}

/// Minimal untyped environment mirroring the bandit (used by
/// [`DomainAdapter::create_environment`]).
pub struct BanditEnvironment {
    arms: u64,
    max_steps: u64,
    arm_probs: Vec<f64>,
    rng: StdRng,
    step: u64,
    last_reward: f64,
}

#[async_trait]
impl Environment for BanditEnvironment {
    async fn reset(&mut self) -> Result<()> {
        self.step = 0;
        self.last_reward = 0.0;
        Ok(())
    }

    fn observation(&self) -> Result<serde_json::Value> {
        Ok(serde_json::json!({
            "step": self.step,
            "arm_count": self.arms,
            "last_reward": self.last_reward,
        }))
    }

    async fn execute(&mut self, action: serde_json::Value) -> Result<EnvironmentStep> {
        let arm = action.get("arm").and_then(|v| v.as_u64()).unwrap_or(0);
        let win = if arm < self.arms {
            let u: f64 = self.rng.gen();
            u < self.arm_probs[arm as usize]
        } else {
            false
        };
        let reward = if win { 1.0 } else { 0.0 };
        self.last_reward = reward;
        self.step += 1;
        let terminal = self.step >= self.max_steps;
        Ok(EnvironmentStep {
            observation: serde_json::json!({
                "step": self.step,
                "arm_count": self.arms,
                "last_reward": reward,
            }),
            reward,
            terminal,
            truncated: false,
            info: serde_json::json!({}),
        })
    }

    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Minimal synthetic dataset handle (used by
/// [`DomainAdapter::resolve_dataset`]).
pub struct StubDataset {
    episodes: usize,
}

#[async_trait]
impl DatasetHandle for StubDataset {
    fn episode_count(&self) -> usize {
        self.episodes
    }

    async fn get_episode(&self, _index: usize) -> Result<Box<dyn Episode>> {
        Ok(Box::new(StubEpisode { step: 0 }))
    }
}

/// Minimal single-step episode for the stub dataset.
pub struct StubEpisode {
    step: u64,
}

#[async_trait]
impl Episode for StubEpisode {
    async fn reset(&mut self) -> Result<()> {
        self.step = 0;
        Ok(())
    }

    fn current_observation(&self) -> Result<serde_json::Value> {
        Ok(serde_json::json!({ "step": self.step }))
    }

    async fn step(&mut self, _action: serde_json::Value) -> Result<EpisodeStep> {
        self.step += 1;
        Ok(EpisodeStep {
            observation: serde_json::json!({ "step": self.step }),
            reward: 0.0,
            done: true,
            info: serde_json::json!({}),
        })
    }
}
