//! Verifier system and proof levels
//!
//! Provides types for:
//! - Verifier packages and execution
//! - Proof levels (P0-P6)
//! - Verification CLI
//! - Review and replication

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::protocol::{Hash, Version};

/// Unique verifier identifier
pub type VerifierId = String;

/// Proof level - indicates how verified a claim is
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ProofLevel {
    /// P0: Private draft - no public commitment
    PrivateDraft = 0,
    /// P1: Committed - signed manifest and timestamped hash
    Committed = 1,
    /// P2: Auditable - approved reviewers can inspect
    Auditable = 2,
    /// P3: Reproducible - independent rerun meets tolerance
    Reproducible = 3,
    /// P4: Replicated - independent implementation confirms
    Replicated = 4,
    /// P5: Operational - live shadow/testnet history
    Operational = 5,
    /// P6: Live verified - bounded real deployment
    LiveVerified = 6,
}

impl ProofLevel {
    /// Get level number
    pub fn level(&self) -> u8 {
        match self {
            Self::PrivateDraft => 0,
            Self::Committed => 1,
            Self::Auditable => 2,
            Self::Reproducible => 3,
            Self::Replicated => 4,
            Self::Operational => 5,
            Self::LiveVerified => 6,
        }
    }

    /// Get display name
    pub fn name(&self) -> &str {
        match self {
            Self::PrivateDraft => "Private Draft",
            Self::Committed => "Committed",
            Self::Auditable => "Auditable",
            Self::Reproducible => "Reproducible",
            Self::Replicated => "Replicated",
            Self::Operational => "Operational",
            Self::LiveVerified => "Live Verified",
        }
    }

    /// Check if this level is publicly visible
    pub fn is_public(&self) -> bool {
        *self >= Self::Committed
    }
}

/// Verifier package
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifierPackage {
    /// Verifier ID
    pub id: VerifierId,
    /// Verifier version
    pub version: Version,
    /// Verifier name
    pub name: String,
    /// Description
    pub description: String,
    /// Author
    pub author: String,
    /// Input schema
    pub input_schema: serde_json::Value,
    /// Output schema
    pub output_schema: serde_json::Value,
    /// Verification logic reference
    pub verification_logic: VerificationLogic,
    /// Calibration fixtures
    pub calibration_fixtures: Vec<CalibrationFixture>,
    /// Known false positive risks
    pub known_false_positives: Vec<String>,
    /// Known false negative risks
    pub known_false_negatives: Vec<String>,
    /// Required evidence
    pub required_evidence: Vec<String>,
    /// Resource budget
    pub resource_budget: ResourceBudget,
    /// Safety policy
    pub safety_policy: SafetyPolicy,
    /// License
    pub license: String,
}

/// Verification logic implementation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VerificationLogic {
    /// Inline code (for simple verifiers)
    Inline { code: String, language: String },
    /// Reference to external module
    Module { module_path: String, function: String },
    /// WASM blob
    Wasm { blob_hash: Hash },
}

/// Calibration fixture for testing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalibrationFixture {
    /// Fixture name
    pub name: String,
    /// Expected result
    pub expected_result: FixtureResult,
    /// Input data
    pub input_data: serde_json::Value,
    /// Description
    pub description: String,
}

/// Expected fixture result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FixtureResult {
    /// Should pass
    Pass,
    /// Should fail
    Fail,
    /// Should return specific value
    Value(serde_json::Value),
}

/// Resource budget for verifier execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceBudget {
    /// Max runtime in seconds
    pub max_runtime_seconds: u64,
    /// Max memory in MB
    pub max_memory_mb: u64,
    /// Max CPU usage
    pub max_cpu_cores: u64,
}

/// Verifier safety policy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyPolicy {
    /// Allow network access
    pub allow_network: bool,
    /// Allow file system access
    pub allow_fs: bool,
    /// Allow subprocess execution
    pub allow_subprocess: bool,
}

/// Verifier execution report
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifierReport {
    /// Report ID
    pub id: String,
    /// Verifier ID
    pub verifier_id: VerifierId,
    /// Verifier version
    pub verifier_version: Version,
    /// Pass/fail
    pub passed: bool,
    /// Score (if applicable)
    pub score: Option<f64>,
    /// Details
    pub details: serde_json::Value,
    /// Warnings
    pub warnings: Vec<String>,
    /// Errors
    pub errors: Vec<String>,
    /// Execution time in seconds
    pub execution_time_seconds: f64,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Review record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Review {
    /// Review ID
    pub id: String,
    /// Proof ID being reviewed
    pub proof_id: String,
    /// Reviewer identity
    pub reviewer: String,
    /// Review decision
    pub decision: ReviewDecision,
    /// Comments
    pub comments: String,
    /// Confidence level
    pub confidence: ReviewConfidence,
    /// Conflict of interest declarations
    pub coi_declarations: Vec<String>,
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
    /// Abstain
    Abstain,
}

/// Confidence level for review
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReviewConfidence {
    /// Low confidence
    Low,
    /// Medium confidence
    Medium,
    /// High confidence
    High,
    /// Expert (reviewer is domain expert)
    Expert,
}

/// Replication record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Replication {
    /// Replication ID
    pub id: String,
    /// Original proof ID
    pub original_proof_id: String,
    /// Replicator identity
    pub replicator: String,
    /// Success
    pub success: bool,
    /// Differences found
    pub differences: Vec<ReplicationDifference>,
    /// Reproduction tolerance
    pub tolerance: f64,
    /// Actual difference
    pub actual_difference: Option<f64>,
    /// Environment used
    pub environment: String,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Replication difference
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationDifference {
    /// Difference type
    pub difference_type: String,
    /// Description
    pub description: String,
    /// Magnitude
    pub magnitude: Option<f64>,
    /// Is material (affects conclusions)
    pub is_material: bool,
}

/// Challenge record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Challenge {
    /// Challenge ID
    pub id: String,
    /// Proof ID being challenged
    pub proof_id: String,
    /// Challenger identity
    pub challenger: String,
    /// Challenge type
    pub challenge_type: ChallengeType,
    /// Grounds for challenge
    pub grounds: String,
    /// Challenge bond (if applicable)
    pub bond: Option<u64>,
    /// Challenge status
    pub status: ChallengeStatus,
    /// Resolution
    pub resolution: Option<ChallengeResolution>,
    /// Timestamp created
    pub created_at: chrono::DateTime<chrono::Utc>,
    /// Timestamp resolved
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Challenge type
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChallengeType {
    /// Methodology error
    MethodologyError,
    /// Data issues
    DataIssue,
    /// Calculation error
    CalculationError,
    /// Reproducibility failure
    ReproducibilityFailure,
    /// Other
    Other { description: String },
}

/// Challenge status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChallengeStatus {
    /// Open
    Open,
    /// Under review
    UnderReview,
    /// Resolved
    Resolved,
    /// Dismissed
    Dismissed,
    /// Withdrawn
    Withdrawn,
}

/// Challenge resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChallengeResolution {
    /// Resolution outcome
    pub outcome: ChallengeOutcome,
    /// Reviewer panel
    pub reviewers: Vec<String>,
    /// Explanation
    pub explanation: String,
    /// Reputation adjustments
    pub reputation_adjustments: HashMap<String, i64>,
}

/// Challenge outcome
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChallengeOutcome {
    /// Challenge upheld (original claim rejected)
    Upheld,
    /// Challenge rejected (original claim stands)
    Rejected,
    /// Partial (original claim modified)
    Partial,
}

/// Scorecard - public summary of performance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Scorecard {
    /// Scorecard ID
    pub id: String,
    /// Agent reference
    pub agent_id: String,
    /// Agent version
    pub agent_version: Version,
    /// Protocol reference
    pub protocol_id: String,
    /// Primary metrics
    pub primary_metrics: HashMap<String, MetricValue>,
    /// Baseline comparisons
    pub baselines: HashMap<String, BaselineResult>,
    /// Risk metrics
    pub risk_metrics: RiskMetrics,
    /// Verifier summary
    pub verifier_summary: VerifierSummary,
    /// Simulation tier
    pub simulation_tier: SimulationTier,
    /// Cost assumptions
    pub cost_assumptions: CostAssumptions,
    /// Confidence intervals
    pub confidence_intervals: HashMap<String, (f64, f64)>,
    /// Proof level
    pub proof_level: ProofLevel,
    /// Limitations and assumptions
    pub limitations: Vec<String>,
    /// Disclaimer
    pub disclaimer: String,
    /// Timestamp
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

/// Metric value with context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricValue {
    /// Value
    pub value: f64,
    /// Higher is better
    pub higher_is_better: bool,
    /// Unit
    pub unit: String,
}

/// Baseline comparison result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BaselineResult {
    /// Baseline name
    pub baseline_name: String,
    /// Baseline value
    pub baseline_value: f64,
    /// Candidate value
    pub candidate_value: f64,
    /// Difference (candidate - baseline)
    pub difference: f64,
    /// Percent difference
    pub percent_difference: f64,
    /// Is better
    pub is_better: bool,
}

/// Risk metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RiskMetrics {
    /// Maximum drawdown
    pub max_drawdown: f64,
    /// Volatility
    pub volatility: f64,
    /// CVaR at 95%
    pub cvar_95: f64,
    /// Worst day
    pub worst_day: f64,
    /// Policy violations
    pub policy_violations: u64,
}

/// Verifier summary
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifierSummary {
    /// Total verifiers run
    pub total_verifiers: u64,
    /// Verifiers passed
    pub verifiers_passed: u64,
    /// Verifiers failed
    pub verifiers_failed: u64,
    /// Required verifiers passed
    pub required_passed: u64,
    /// Required verifiers total
    pub required_total: u64,
}

/// Simulation tier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SimulationTier {
    /// S0: Deterministic synthetic fixtures
    S0 = 0,
    /// S1: Candle/bar replay
    S1 = 1,
    /// S2: Top-of-book replay
    S2 = 2,
    /// S3: L2 event replay
    S3 = 3,
    /// S4: Walk-forward randomized holdouts
    S4 = 4,
    /// S5: Live shadow market
    S5 = 5,
    /// S6: Venue testnet
    S6 = 6,
    /// S7: Bounded live canary
    S7 = 7,
}

impl SimulationTier {
    /// Get tier number
    pub fn tier(&self) -> u8 {
        *self as u8
    }

    /// Get proof ceiling for this tier
    pub fn proof_ceiling(&self) -> ProofLevel {
        match self {
            Self::S0 | Self::S1 => ProofLevel::Committed,
            Self::S2 => ProofLevel::Auditable,
            Self::S3 | Self::S4 => ProofLevel::Reproducible,
            Self::S5 => ProofLevel::Operational,
            Self::S6 => ProofLevel::Operational,
            Self::S7 => ProofLevel::LiveVerified,
        }
    }
}

/// Cost assumptions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostAssumptions {
    /// Fee model
    pub fee_model: String,
    /// Latency in milliseconds
    pub latency_ms: u32,
    /// Slippage model
    pub slippage_model: String,
    /// Starting capital
    pub starting_capital: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_proof_levels() {
        assert!(ProofLevel::Committed > ProofLevel::PrivateDraft);
        assert!(ProofLevel::Reproducible > ProofLevel::Auditable);
        assert_eq!(ProofLevel::Committed.level(), 1);
        assert!(ProofLevel::Committed.is_public());
        assert!(!ProofLevel::PrivateDraft.is_public());
    }

    #[test]
    fn test_simulation_tiers() {
        assert_eq!(SimulationTier::S0.tier(), 0);
        assert!(SimulationTier::S3.proof_ceiling() >= ProofLevel::Reproducible);
    }
}
