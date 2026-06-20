//! Forecasting domain adapter (AR-09).
//!
//! A deterministic probabilistic-forecasting environment. The adapter generates
//! a fixed `(feature, binary_outcome)` sequence from a seed; each step the agent
//! predicts the probability of outcome 1 and is scored by the Brier score. All
//! randomness derives from the constructor seed, so the same seed yields the
//! same run. There is no trading, order, position, or market code here.

use async_trait::async_trait;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::protocol::{DatasetManifest, EnvironmentManifest, Protocol};
use crate::simulation::{
    Agent, CapabilityManifest, DatasetHandle, DomainAdapter, Environment, EnvironmentStep, Episode,
    EpisodeStep, MetricSet, PolicyDecision, PublicEvidenceBundle, PublicEvidenceStep, RunTrace,
    RuntimeState, StepResult, TerminalCondition, TerminalConditionType, ValidationReport,
};

use super::types::{ForecastingAction, ForecastingObservation, ForecastingOutcome};

/// Adapter and version identifier for the forecasting domain.
pub const FORECASTING_ADAPTER_ID: &str = "forecasting-binary";
/// Semantic version of the forecasting adapter.
pub const FORECASTING_ADAPTER_VERSION: &str = "0.1.0";

/// A single deterministic `(feature, outcome)` sample in the sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastSample {
    /// Real-valued feature.
    pub feature: f64,
    /// Realized binary outcome (true = 1).
    pub outcome: bool,
}

/// Deterministic binary-forecasting environment.
pub struct ForecastingAdapter {
    max_steps: u64,
    seed: u64,
    samples: Vec<ForecastSample>,
    step: u64,
    last_outcome: f64,
}

impl ForecastingAdapter {
    /// Create a forecasting environment that runs `max_steps` steps, with the
    /// `(feature, outcome)` sequence generated deterministically from `seed`.
    pub fn new(max_steps: u64, seed: u64) -> Self {
        Self {
            max_steps,
            seed,
            samples: generate_samples(seed, max_steps),
            step: 0,
            last_outcome: 0.0,
        }
    }

    fn reset_state(&mut self) {
        self.samples = generate_samples(self.seed, self.max_steps);
        self.step = 0;
        self.last_outcome = 0.0;
    }

    fn forecast(&mut self, probability: f64) -> ForecastingOutcome {
        let sample = self.samples.get(self.step as usize).cloned().unwrap_or(
            // Past the generated sequence: deterministic fallback.
            ForecastSample {
                feature: 0.0,
                outcome: false,
            },
        );
        let actual = if sample.outcome { 1.0 } else { 0.0 };
        let brier = (probability - actual).powi(2);
        self.last_outcome = actual;
        self.step += 1;
        let terminal = self.step >= self.max_steps;
        ForecastingOutcome {
            step: self.step,
            actual,
            predicted: probability,
            brier,
            terminal,
        }
    }
}

/// Generate a deterministic `(feature, outcome)` sequence from `seed`.
fn generate_samples(seed: u64, count: u64) -> Vec<ForecastSample> {
    let mut rng = StdRng::seed_from_u64(seed);
    (0..count)
        .map(|_| {
            // Feature in [0, 1); outcome is a coin biased by the feature so a
            // perfect forecaster could in principle learn the relationship.
            let feature: f64 = rng.gen();
            let threshold: f64 = rng.gen();
            ForecastSample {
                feature,
                outcome: threshold < feature,
            }
        })
        .collect()
}

#[async_trait]
impl DomainAdapter for ForecastingAdapter {
    type Obs = ForecastingObservation;
    type Act = ForecastingAction;
    type Out = ForecastingOutcome;

    fn id(&self) -> (String, String) {
        (
            FORECASTING_ADAPTER_ID.to_string(),
            FORECASTING_ADAPTER_VERSION.to_string(),
        )
    }

    fn capability_manifest(&self) -> CapabilityManifest {
        CapabilityManifest {
            observation_types: vec!["feature".to_string(), "last_outcome".to_string()],
            action_types: vec!["predict_probability".to_string()],
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
        Ok(Box::new(ForecastDataset {
            episodes: self.max_steps as usize,
        }))
    }

    async fn create_environment(
        &self,
        _config: &EnvironmentManifest,
    ) -> Result<Box<dyn Environment>> {
        Ok(Box::new(ForecastEnvironment {
            max_steps: self.max_steps,
            samples: self.samples.clone(),
            step: 0,
            last_outcome: 0.0,
        }))
    }

    async fn reset(&mut self) -> Result<Self::Obs> {
        self.reset_state();
        Ok(ForecastingObservation {
            step: 0,
            feature: self.samples.first().map(|s| s.feature).unwrap_or(0.0),
            last_outcome: 0.0,
        })
    }

    fn normalize_observation(&self, raw: serde_json::Value) -> Result<Self::Obs> {
        Ok(serde_json::from_value(raw)?)
    }

    fn validate_action(&self, action: &Self::Act, _state: &RuntimeState) -> Result<PolicyDecision> {
        if !(0.0..=1.0).contains(&action.probability) {
            Ok(PolicyDecision::Rejected {
                reason: format!("probability {} out of range [0, 1]", action.probability),
            })
        } else {
            Ok(PolicyDecision::Approved)
        }
    }

    async fn step(&mut self, action: Self::Act) -> Result<StepResult<Self::Obs, Self::Out>> {
        let outcome = self.forecast(action.probability);
        let next_feature = self
            .samples
            .get(outcome.step as usize)
            .map(|s| s.feature)
            .unwrap_or(0.0);
        let observation = ForecastingObservation {
            step: outcome.step,
            feature: next_feature,
            last_outcome: outcome.actual,
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
        let mut brier_sum = 0.0_f64;
        let mut count = 0u64;
        for s in &trace.steps {
            if let Ok(out) = serde_json::from_value::<ForecastingOutcome>(s.outcome.clone()) {
                brier_sum += out.brier;
                count += 1;
            }
        }
        let mean_brier = if count == 0 {
            0.0
        } else {
            brier_sum / count as f64
        };
        let mut metrics = std::collections::HashMap::new();
        metrics.insert("mean_brier".to_string(), mean_brier);
        metrics.insert("steps".to_string(), count as f64);
        Ok(MetricSet {
            // Higher is better: skill = 1 - mean_brier (in [0, 1]).
            primary_metric: 1.0 - mean_brier,
            metrics,
            confidence_intervals: std::collections::HashMap::new(),
        })
    }

    fn build_public_evidence(&self, trace: &RunTrace) -> Result<PublicEvidenceBundle> {
        let steps = trace
            .steps
            .iter()
            .map(|s| PublicEvidenceStep {
                step: s.step,
                action_type: "predict_probability".to_string(),
                outcome_type: "brier".to_string(),
                timestamp: s.timestamp,
            })
            .collect();
        Ok(PublicEvidenceBundle {
            evidence_id: trace.run_id.clone(),
            steps,
            metrics: MetricSet {
                primary_metric: 0.0,
                metrics: std::collections::HashMap::new(),
                confidence_intervals: std::collections::HashMap::new(),
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

/// A deterministic feature-based forecaster. Predicts `P(outcome=1)` from the
/// feature via a fixed logistic map, fully determined by its seed.
pub struct ForecastingAgent {
    seed: u64,
    rng: StdRng,
}

impl ForecastingAgent {
    /// Create a forecaster seeded by `seed`.
    pub fn new(seed: u64) -> Self {
        Self {
            seed,
            rng: StdRng::seed_from_u64(seed.wrapping_mul(7919)),
        }
    }
}

/// Stable agent identifier for [`ForecastingAgent`].
pub const FORECASTING_AGENT_ID: &str = "forecasting-logistic-agent";

#[async_trait]
impl Agent<ForecastingAdapter> for ForecastingAgent {
    fn id(&self) -> &str {
        FORECASTING_AGENT_ID
    }

    async fn act(&mut self, observation: &ForecastingObservation) -> Result<ForecastingAction> {
        // Logistic map on the feature, jittered deterministically by the RNG.
        let jitter: f64 = self.rng.gen_range(-0.05..0.05);
        let logit = (observation.feature - 0.5) * 4.0 + jitter;
        let prob = (1.0 / (1.0 + (-logit).exp())).clamp(0.0, 1.0);
        let _ = self.seed; // seed retained for determinism documentation
        Ok(ForecastingAction { probability: prob })
    }
}

/// Minimal untyped environment mirroring the forecasting adapter.
pub struct ForecastEnvironment {
    max_steps: u64,
    samples: Vec<ForecastSample>,
    step: u64,
    last_outcome: f64,
}

#[async_trait]
impl Environment for ForecastEnvironment {
    async fn reset(&mut self) -> Result<()> {
        self.step = 0;
        self.last_outcome = 0.0;
        Ok(())
    }

    fn observation(&self) -> Result<serde_json::Value> {
        Ok(serde_json::json!({
            "step": self.step,
            "feature": self.samples.first().map(|s| s.feature).unwrap_or(0.0),
            "last_outcome": self.last_outcome,
        }))
    }

    async fn execute(&mut self, action: serde_json::Value) -> Result<EnvironmentStep> {
        let probability = action
            .get("probability")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.5)
            .clamp(0.0, 1.0);
        let sample = self
            .samples
            .get(self.step as usize)
            .cloned()
            .unwrap_or(ForecastSample {
                feature: 0.0,
                outcome: false,
            });
        let actual = if sample.outcome { 1.0 } else { 0.0 };
        let brier = (probability - actual).powi(2);
        self.last_outcome = actual;
        self.step += 1;
        let terminal = self.step >= self.max_steps;
        Ok(EnvironmentStep {
            observation: serde_json::json!({
                "step": self.step,
                "feature": sample.feature,
                "last_outcome": actual,
            }),
            reward: 1.0 - brier,
            terminal,
            truncated: false,
            info: serde_json::json!({}),
        })
    }

    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

/// Minimal synthetic dataset handle.
pub struct ForecastDataset {
    episodes: usize,
}

#[async_trait]
impl DatasetHandle for ForecastDataset {
    fn episode_count(&self) -> usize {
        self.episodes
    }

    async fn get_episode(&self, _index: usize) -> Result<Box<dyn Episode>> {
        Ok(Box::new(ForecastEpisode { step: 0 }))
    }
}

/// Minimal episode stub.
pub struct ForecastEpisode {
    step: u64,
}

#[async_trait]
impl Episode for ForecastEpisode {
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
            done: false,
            info: serde_json::json!({}),
        })
    }
}
