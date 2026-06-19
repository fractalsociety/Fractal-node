//! Generic simulation kernel and domain adapter contract
//!
//! This module provides the domain-neutral simulation infrastructure.
//! Domain-specific logic (trading, software, etc.) is implemented via adapters.
//!
//! # Architecture
//!
//! The kernel works through the `DomainAdapter` trait, which defines:
//! - How observations are structured
//! - How actions are validated and executed
//! - How outcomes are scored
//! - What verifiers are required

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Debug;

use crate::error::Result;
use crate::protocol::{
    AgentManifest, DatasetManifest, EnvironmentManifest, Protocol, VerifierReport,
};

/// Capability manifest - what a domain adapter provides
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityManifest {
    /// Supported observation types
    pub observation_types: Vec<String>,
    /// Supported action types
    pub action_types: Vec<String>,
    /// Required resources
    pub required_resources: Vec<ResourceRequirement>,
    /// Maximum concurrent episodes
    pub max_concurrent_episodes: usize,
}

/// Resource requirement
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceRequirement {
    /// Resource type
    pub resource_type: String,
    /// Minimum amount
    pub minimum: u64,
    /// Maximum amount
    pub maximum: Option<u64>,
}

/// Generic observation from the environment
pub trait Observation: Send + Sync + Debug {
    /// Convert to JSON for logging
    fn to_json(&self) -> Result<serde_json::Value>;
}

/// Generic action proposed by an agent
pub trait Action: Send + Sync + Debug {
    /// Validate the action schema
    fn validate(&self) -> Result<()>;
    /// Convert to JSON for logging
    fn to_json(&self) -> Result<serde_json::Value>;
}

/// Generic outcome from an action
pub trait Outcome: Send + Sync + Debug {
    /// Get primary score
    fn primary_score(&self) -> f64;
    /// Check if terminal state reached
    fn is_terminal(&self) -> bool;
    /// Convert to JSON for logging
    fn to_json(&self) -> Result<serde_json::Value>;
}

/// Domain adapter contract
///
/// All domain-specific implementations must implement this trait.
/// The generic parameters allow for domain-specific observation/action/outcome types
/// while maintaining a common interface.
#[async_trait]
pub trait DomainAdapter: Send + Sync {
    /// Observation type for this domain
    type Obs: Observation;
    /// Action type for this domain
    type Act: Action;
    /// Outcome type for this domain
    type Out: Outcome;

    /// Get adapter ID and version
    fn id(&self) -> (String, String);

    /// Get capability manifest
    fn capability_manifest(&self) -> CapabilityManifest;

    /// Validate a protocol for this domain
    fn validate_protocol(&self, protocol: &Protocol) -> Result<ValidationReport>;

    /// Resolve a dataset manifest to a dataset handle
    async fn resolve_dataset(&self, manifest: &DatasetManifest) -> Result<Box<dyn DatasetHandle>>;

    /// Create an environment from configuration
    async fn create_environment(
        &self,
        config: &EnvironmentManifest,
    ) -> Result<Box<dyn Environment>>;

    /// Reset the environment to its initial state and return the initial
    /// observation. Called by the kernel at the start of each episode.
    async fn reset(&mut self) -> Result<Self::Obs>;

    /// Normalize raw observation data
    fn normalize_observation(&self, raw: serde_json::Value) -> Result<Self::Obs>;

    /// Validate an action against current state and policy
    fn validate_action(&self, action: &Self::Act, state: &RuntimeState) -> Result<PolicyDecision>;

    /// Execute a single step in the environment
    async fn step(&mut self, action: Self::Act) -> Result<StepResult<Self::Obs, Self::Out>>;

    /// Score a run trace.
    async fn score(&self, trace: &RunTrace) -> Result<MetricSet>;

    /// Build public (redacted) evidence from a full run trace.
    fn build_public_evidence(&self, trace: &RunTrace) -> Result<PublicEvidenceBundle>;

    /// Get terminal conditions for this domain
    fn terminal_conditions(&self) -> Vec<TerminalCondition>;
}

/// A policy that chooses actions for a [`DomainAdapter`] (PHASE-02).
///
/// Implementations must be deterministic given their construction (including
/// any seed). The kernel never passes wall-clock time or OS randomness into
/// [`Agent::act`], so a run is fully determined by the adapter, the agent, the
/// seed, and the kernel config.
#[async_trait]
pub trait Agent<A: DomainAdapter>: Send + Sync {
    /// Stable agent identifier.
    fn id(&self) -> &str;

    /// Choose an action for the current observation.
    async fn act(&mut self, observation: &A::Obs) -> Result<A::Act>;

    /// The agent's manifest, if it exposes one. Defaults to `None`.
    fn manifest(&self) -> Option<AgentManifest> {
        None
    }
}

/// Validation report for protocol validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationReport {
    /// Is valid
    pub is_valid: bool,
    /// Validation errors
    pub errors: Vec<String>,
    /// Validation warnings
    pub warnings: Vec<String>,
}

/// Dataset handle - abstract interface to dataset data
#[async_trait]
pub trait DatasetHandle: Send + Sync {
    /// Get total number of episodes
    fn episode_count(&self) -> usize;

    /// Get a specific episode by index
    async fn get_episode(&self, index: usize) -> Result<Box<dyn Episode>>;
}

/// Episode - a single run through an environment
#[async_trait]
pub trait Episode: Send + Sync {
    /// Reset episode to initial state
    async fn reset(&mut self) -> Result<()>;

    /// Get current observation
    fn current_observation(&self) -> Result<serde_json::Value>;

    /// Step with given action
    async fn step(&mut self, action: serde_json::Value) -> Result<EpisodeStep>;
}

/// Episode step result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpisodeStep {
    /// Observation
    pub observation: serde_json::Value,
    /// Reward
    pub reward: f64,
    /// Is done
    pub done: bool,
    /// Info
    pub info: serde_json::Value,
}

/// Environment - where agents interact
#[async_trait]
pub trait Environment: Send + Sync {
    /// Reset environment
    async fn reset(&mut self) -> Result<()>;

    /// Get current observation
    fn observation(&self) -> Result<serde_json::Value>;

    /// Execute action
    async fn execute(&mut self, action: serde_json::Value) -> Result<EnvironmentStep>;

    /// Close environment
    async fn close(&mut self) -> Result<()>;
}

/// Environment step result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentStep {
    /// Observation
    pub observation: serde_json::Value,
    /// Reward
    pub reward: f64,
    /// Is terminal
    pub terminal: bool,
    /// Whether action was truncated
    pub truncated: bool,
    /// Additional info
    pub info: serde_json::Value,
}

/// Runtime state during execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeState {
    /// Current episode number
    pub episode: u64,
    /// Current step within episode
    pub step: u64,
    /// Current reward
    pub reward: f64,
    /// State data
    pub state_data: serde_json::Value,
}

/// Policy decision for action validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PolicyDecision {
    /// Approve action
    Approved,
    /// Reject action
    Rejected {
        /// Reason the action was rejected.
        reason: String,
    },
    /// Modify action
    Modified {
        /// Original proposed action.
        original_action: serde_json::Value,
        /// Modified action applied instead.
        modified_action: serde_json::Value,
    },
}

/// Step result from domain adapter
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult<O, U> {
    /// New observation
    pub observation: O,
    /// Outcome/reward
    pub outcome: U,
    /// Whether episode is done
    pub done: bool,
    /// Additional info
    pub info: serde_json::Value,
}

/// Runtime trace accumulated during a run, before canonicalization into a
/// [`protocol::EvidenceBundle`](crate::protocol::EvidenceBundle).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunTrace {
    /// Run ID
    pub run_id: String,
    /// Agent manifest, if the agent exposes one.
    pub agent: Option<AgentManifest>,
    /// Steps taken
    pub steps: Vec<EvidenceStep>,
    /// Final outcome
    pub final_outcome: Option<serde_json::Value>,
    /// Verifier reports
    pub verifier_reports: Vec<VerifierReport>,
}

impl RunTrace {
    /// Create an empty trace for `run_id`.
    pub fn new(run_id: impl Into<String>) -> Self {
        Self {
            run_id: run_id.into(),
            agent: None,
            steps: Vec::new(),
            final_outcome: None,
            verifier_reports: Vec::new(),
        }
    }

    /// Record a step. `timestamp` must be a logical run clock supplied by the
    /// kernel (not a wall clock) to preserve determinism.
    pub fn record_step(
        &mut self,
        step: u64,
        observation: serde_json::Value,
        action: serde_json::Value,
        outcome: serde_json::Value,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) {
        self.steps.push(EvidenceStep {
            step,
            observation,
            action,
            outcome,
            timestamp,
        });
    }

    /// Set the agent manifest for this trace.
    pub fn set_agent(&mut self, agent: AgentManifest) {
        self.agent = Some(agent);
    }

    /// Set the final outcome value of the run.
    pub fn set_final_outcome(&mut self, outcome: serde_json::Value) {
        self.final_outcome = Some(outcome);
    }

    /// Attach a verifier report produced for this run.
    pub fn add_verifier_report(&mut self, report: VerifierReport) {
        self.verifier_reports.push(report);
    }

    /// Number of steps recorded.
    pub fn step_count(&self) -> u64 {
        self.steps.len() as u64
    }

    /// Canonicalize into a [`protocol::EvidenceBundle`]. Each step becomes a
    /// [`DecisionTrace`](crate::protocol::DecisionTrace) with its observation
    /// hashed and a policy decision of `Approved`: the kernel validates actions
    /// before stepping, so rejected actions never enter the trace.
    pub fn into_evidence(
        self,
        id: crate::protocol::ObjectId,
        metrics: HashMap<String, f64>,
        timestamp: chrono::DateTime<chrono::Utc>,
    ) -> crate::error::Result<crate::protocol::EvidenceBundle> {
        let mut decision_traces = Vec::with_capacity(self.steps.len());
        for s in self.steps {
            decision_traces.push(crate::protocol::DecisionTrace {
                step: s.step,
                observation_hash: crate::protocol::Hash::of(&s.observation)?,
                action: s.action,
                risk_decision: crate::protocol::RiskDecision::Approved,
                outcome: s.outcome,
                timestamp: s.timestamp,
            });
        }
        Ok(crate::protocol::EvidenceBundle {
            id,
            run_id: self.run_id,
            decision_traces,
            metrics,
            verifier_reports: self.verifier_reports,
            timestamp,
        })
    }
}

/// Single step evidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceStep {
    /// Step number
    pub step: u64,
    /// Observation (may be hashed for privacy)
    pub observation: serde_json::Value,
    /// Action taken
    pub action: serde_json::Value,
    /// Outcome
    pub outcome: serde_json::Value,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Metric set from scoring
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricSet {
    /// Primary metric value
    pub primary_metric: f64,
    /// Additional metrics
    pub metrics: HashMap<String, f64>,
    /// Confidence intervals (if applicable)
    pub confidence_intervals: HashMap<String, (f64, f64)>,
}

/// Public (redacted) evidence bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicEvidenceBundle {
    /// Original evidence bundle ID
    pub evidence_id: String,
    /// Redacted steps
    pub steps: Vec<PublicEvidenceStep>,
    /// Public metrics
    pub metrics: MetricSet,
    /// Verifier summary
    pub verifier_summary: Vec<String>,
}

/// Public evidence step (redacted)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicEvidenceStep {
    /// Step number
    pub step: u64,
    /// Action type (not full action)
    pub action_type: String,
    /// Outcome type
    pub outcome_type: String,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Terminal condition for episodes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalCondition {
    /// Condition type
    pub condition_type: TerminalConditionType,
    /// Threshold value
    pub threshold: f64,
    /// Is strict inequality
    pub strict: bool,
}

/// Terminal condition type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TerminalConditionType {
    /// Maximum steps reached
    MaxSteps,
    /// Minimum reward threshold
    MinReward,
    /// Maximum reward threshold
    MaxReward,
    /// Custom condition
    Custom {
        /// Name of the custom condition.
        name: String,
    },
}

/// Generic environment configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    /// Environment ID
    pub id: String,
    /// Domain adapter ID
    pub domain_adapter: String,
    /// Configuration parameters
    pub parameters: HashMap<String, serde_json::Value>,
    /// Resource limits (canonical [`protocol::ResourceLimits`](crate::protocol::ResourceLimits))
    pub resource_limits: crate::protocol::ResourceLimits,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_manifest() {
        let manifest = CapabilityManifest {
            observation_types: vec!["price".to_string()],
            action_types: vec!["buy".to_string(), "sell".to_string()],
            required_resources: vec![],
            max_concurrent_episodes: 10,
        };

        assert_eq!(manifest.observation_types.len(), 1);
        assert_eq!(manifest.max_concurrent_episodes, 10);
    }
}
