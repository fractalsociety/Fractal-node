//! Core research protocol types
//!
//! Defines the canonical schemas for the research pipeline:
//! - ResearchProject
//! - Protocol
//! - DatasetSnapshot
//! - Environment
//! - AgentPackage
//! - ExperimentRun
//! - EvidenceBundle
//! - VerifierRun
//! - Review
//! - Replication
//! - ProofManifest

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique identifier for a research object
pub type ObjectId = String;

/// Version string (semver)
pub type Version = String;

/// Content hash (Blake3)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Hash(pub String);

impl Hash {
    /// Create a new hash from bytes
    pub fn new(data: &[u8]) -> Self {
        let hash = blake3::hash(data);
        Self(hex::encode(hash.as_bytes()))
    }

    /// Parse a hash from hex string
    pub fn from_hex(hex: &str) -> Result<Self, crate::error::Error> {
        // Validate hex format
        if hex.len() != 64 {
            return Err(crate::error::Error::InvalidArtifact(
                "Hash must be 64 hex characters".to_string(),
            ));
        }
        Ok(Self(hex.to_string()))
    }
}

/// Research project - the top-level container for a research effort
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchProject {
    /// Unique project identifier
    pub id: ObjectId,
    /// Project version
    pub version: Version,
    /// Research question
    pub question: String,
    /// Falsifiable claim
    pub claim: String,
    /// Domain adapter reference
    pub domain_adapter: DomainAdapterRef,
    /// Protocol definition
    pub protocol: Protocol,
    /// Dataset manifests
    pub datasets: HashMap<String, DatasetManifest>,
    /// Environment manifest
    pub environment: EnvironmentManifest,
    /// Created at timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Updated at timestamp
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Domain adapter reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DomainAdapterRef {
    /// Adapter ID
    pub id: String,
    /// Adapter version
    pub version: Version,
}

/// Research protocol definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Protocol {
    /// Protocol identifier
    pub id: ObjectId,
    /// Protocol version
    pub version: Version,
    /// Agent and dependency versions
    pub agent_versions: HashMap<String, Version>,
    /// Allowed observations and tools
    pub allowed_tools: Vec<String>,
    /// Dataset boundaries
    pub dataset_boundaries: DatasetBoundaries,
    /// Primary metrics
    pub primary_metrics: Vec<MetricDefinition>,
    /// Cost model
    pub cost_model: CostModel,
    /// Safety and permission policy
    pub safety_policy: SafetyPolicy,
    /// Required verifiers
    pub required_verifiers: Vec<VerifierRef>,
}

/// Dataset boundaries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetBoundaries {
    /// Development window
    pub development: WindowSpec,
    /// Validation window
    pub validation: WindowSpec,
    /// Evaluation window (may be private/hidden)
    pub evaluation: WindowSpec,
}

/// Time window specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowSpec {
    /// Start timestamp
    pub start: chrono::DateTime<chrono::Utc>,
    /// End timestamp
    pub end: chrono::DateTime<chrono::Utc>,
    /// Random seed
    pub seed: u64,
}

/// Metric definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricDefinition {
    /// Metric name
    pub name: String,
    /// Higher is better
    pub higher_is_better: bool,
    /// Metric type
    pub metric_type: MetricType,
}

/// Metric type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MetricType {
    /// Scalar value
    Scalar,
    /// Percentage
    Percentage,
    /// Currency
    Currency,
    /// Count
    Count,
}

/// Cost model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostModel {
    /// Fee schedule reference
    pub fee_schedule: String,
    /// Latency in milliseconds
    pub latency_ms: u32,
    /// Slippage model reference
    pub slippage_model: String,
}

/// Safety policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyPolicy {
    /// Maximum drawdown allowed
    pub max_drawdown: f64,
    /// Maximum leverage
    pub max_leverage: f64,
    /// Policy violations equal to zero
    pub policy_violations_eq_zero: bool,
}

/// Verifier reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifierRef {
    /// Verifier ID
    pub id: String,
    /// Verifier version
    pub version: Version,
}

/// Dataset manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetManifest {
    /// Dataset ID
    pub id: ObjectId,
    /// Source
    pub source: DataSource,
    /// Time range
    pub time_range: (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>),
    /// Schema version
    pub schema_version: Version,
    /// Missingness indicators
    pub missingness: HashMap<String, f64>,
    /// Transformations applied
    pub transformations: Vec<String>,
    /// Content hash
    pub content_hash: Hash,
    /// Visibility level
    pub visibility: Visibility,
}

/// Data source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DataSource {
    /// Historical data
    Historical { venue: String, market: String },
    /// Synthetic data
    Synthetic { generator: String },
    /// Live data
    Live { venue: String },
}

/// Visibility level for artifacts
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Visibility {
    /// Private - no public access
    Private,
    /// Committed - hash is public, artifacts are private
    CommittedPrivate,
    /// Reviewer access - encrypted access for approved reviewers
    ReviewerAccess,
    /// Partial public - selected fields are public
    PartialPublic,
    /// Open - full public access
    Open,
}

/// Environment manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentManifest {
    /// Environment ID
    pub id: ObjectId,
    /// Domain adapter
    pub domain_adapter: DomainAdapterRef,
    /// Configuration
    pub config: serde_json::Value,
    /// Version hash
    pub version_hash: Hash,
}

/// Agent package manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentManifest {
    /// Agent ID
    pub id: ObjectId,
    /// Agent version
    pub version: Version,
    /// Author identity
    pub author: String,
    /// Model reference
    pub model_ref: Option<ModelRef>,
    /// System prompt
    pub system_prompt: Option<String>,
    /// Code package hash
    pub code_hash: Hash,
    /// Tool allowlist
    pub tool_allowlist: Vec<String>,
    /// Skill dependencies
    pub skill_dependencies: Vec<SkillRef>,
    /// Resource limits
    pub resource_limits: ResourceLimits,
    /// Network policy
    pub network_policy: NetworkPolicy,
    /// License
    pub license: String,
}

/// Model reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelRef {
    /// Model provider
    pub provider: String,
    /// Model name
    pub model: String,
    /// Model version
    pub version: Version,
}

/// Skill reference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRef {
    /// Skill ID
    pub id: String,
    /// Skill version
    pub version: Version,
}

/// Resource limits
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceLimits {
    /// Max memory in GB
    pub max_memory_gb: u32,
    /// Max runtime in minutes
    pub max_runtime_minutes: u32,
    /// Max CPU cores
    pub max_cpu_cores: u32,
}

/// Network policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkPolicy {
    /// Allow network access
    pub allow_network: bool,
    /// Allowed domains (if network allowed)
    pub allowed_domains: Vec<String>,
}

/// Experiment run
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentRun {
    /// Run ID
    pub id: ObjectId,
    /// Project ID
    pub project_id: ObjectId,
    /// Agent manifest
    pub agent: AgentManifest,
    /// Environment
    pub environment: EnvironmentManifest,
    /// Protocol
    pub protocol: Protocol,
    /// Start time
    pub start_time: chrono::DateTime<chrono::Utc>,
    /// End time
    pub end_time: Option<chrono::DateTime<chrono::Utc>>,
    /// Run status
    pub status: RunStatus,
    /// Seed used
    pub seed: u64,
}

/// Run status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RunStatus {
    /// Queued
    Queued,
    /// Running
    Running,
    /// Completed
    Completed,
    /// Failed
    Failed { error: String },
    /// Cancelled
    Cancelled,
}

/// Evidence bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceBundle {
    /// Bundle ID
    pub id: ObjectId,
    /// Run ID
    pub run_id: ObjectId,
    /// Decision traces
    pub decision_traces: Vec<DecisionTrace>,
    /// Metrics
    pub metrics: HashMap<String, f64>,
    /// Verifier reports
    pub verifier_reports: Vec<VerifierReport>,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Decision trace
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionTrace {
    /// Step number
    pub step: u64,
    /// Observation hash
    pub observation_hash: Hash,
    /// Proposed action
    pub action: serde_json::Value,
    /// Risk policy decision
    pub risk_decision: RiskDecision,
    /// Outcome
    pub outcome: serde_json::Value,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Risk decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RiskDecision {
    /// Approved
    Approved,
    /// Rejected
    Rejected { reason: String },
    /// Modified
    Modified { original: serde_json::Value, modified: serde_json::Value },
}

/// Verifier report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifierReport {
    /// Verifier ID
    pub verifier_id: String,
    /// Verifier version
    pub verifier_version: Version,
    /// Pass/fail
    pub passed: bool,
    /// Score
    pub score: Option<f64>,
    /// Details
    pub details: serde_json::Value,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Review record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    /// Review ID
    pub id: ObjectId,
    /// Proof ID being reviewed
    pub proof_id: ObjectId,
    /// Reviewer
    pub reviewer: String,
    /// Review decision
    pub decision: ReviewDecision,
    /// Comments
    pub comments: String,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Review decision
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReviewDecision {
    /// Approve
    Approve,
    /// Request changes
    RequestChanges,
    /// Reject
    Reject,
}

/// Replication record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replication {
    /// Replication ID
    pub id: ObjectId,
    /// Original proof ID
    pub original_proof_id: ObjectId,
    /// Replicator
    pub replicator: String,
    /// Success
    pub success: bool,
    /// Differences
    pub differences: Vec<String>,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Proof manifest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProofManifest {
    /// Manifest version
    pub manifest_version: Version,
    /// Claim ID
    pub claim_id: ObjectId,
    /// Protocol hash
    pub protocol_hash: Hash,
    /// Agent hash
    pub agent_hash: Hash,
    /// Dataset manifest hash
    pub dataset_hash: Hash,
    /// Environment hash
    pub environment_hash: Hash,
    /// Run trace Merkle root
    pub trace_merkle_root: Hash,
    /// Verifier set hash
    pub verifier_set_hash: Hash,
    /// Scorecard hash
    pub scorecard_hash: Hash,
    /// Disclosure policy
    pub disclosure: Visibility,
    /// Author signature
    pub author_signature: String,
    /// Platform attestation (if applicable)
    pub platform_attestation: Option<String>,
    /// Chain/network reference
    pub chain_reference: Option<ChainReference>,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Chain reference for on-chain commitments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChainReference {
    /// Network name
    pub network: String,
    /// Transaction hash
    pub transaction_hash: String,
    /// Block number
    pub block_number: u64,
    /// Finality status
    pub finalized: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_creation() {
        let data = b"test data";
        let hash = Hash::new(data);
        assert_eq!(hash.0.len(), 64); // Blake3 hex output
    }

    #[test]
    fn test_visibility_levels() {
        assert_eq!(Visibility::Private, Visibility::Private);
        assert_ne!(Visibility::Private, Visibility::Open);
    }
}
