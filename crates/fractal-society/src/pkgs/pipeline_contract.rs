//! Pipeline-contract package.
//!
//! Defines the interface-first data model future orchestration code can compose.

use crate::protocol::Hash;
use crate::verifier::VerifierReport;

/// Highest pipeline stage reached by an outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStage {
    /// Run evidence has not yet been produced.
    Run,
    /// Evidence exists and can be scored.
    Score,
    /// One or more verifier reports exist.
    Verify,
    /// Proof/evidence has been committed.
    Commit,
    /// Reward release has occurred.
    Reward,
}

/// Minimal pipeline outcome record for future orchestration.
#[derive(Debug, Clone)]
pub struct PipelineOutcome {
    /// Evidence bundle hash.
    pub evidence_hash: Hash,
    /// Scorecard hash.
    pub scorecard_hash: Hash,
    /// Verifier reports emitted for this outcome.
    pub verifier_reports: Vec<VerifierReport>,
    /// Whether the proof was committed.
    pub committed: bool,
    /// Whether reward was released.
    pub reward_released: bool,
}

impl PipelineOutcome {
    /// Return the highest pipeline stage reached.
    pub fn stage(&self) -> PipelineStage {
        if self.reward_released {
            PipelineStage::Reward
        } else if self.committed {
            PipelineStage::Commit
        } else if !self.verifier_reports.is_empty() {
            PipelineStage::Verify
        } else if !is_empty_hash(&self.evidence_hash) {
            PipelineStage::Score
        } else {
            PipelineStage::Run
        }
    }

    /// Return true when every verifier report passed.
    pub fn all_verifiers_passed(&self) -> bool {
        self.verifier_reports.iter().all(|report| report.passed)
    }

    /// Return true when rewards are released and all verifiers passed.
    pub fn is_complete(&self) -> bool {
        self.reward_released && self.all_verifiers_passed()
    }
}

/// Validate basic pipeline well-formedness.
pub fn validate(outcome: &PipelineOutcome) -> std::result::Result<(), String> {
    if is_empty_hash(&outcome.evidence_hash) {
        return Err("evidence_hash must be non-empty".to_string());
    }
    if outcome.reward_released && !outcome.committed {
        return Err("reward cannot release before commitment".to_string());
    }
    if outcome.reward_released && !outcome.all_verifiers_passed() {
        return Err("reward cannot release unless all verifiers passed".to_string());
    }
    Ok(())
}

fn is_empty_hash(hash: &Hash) -> bool {
    hash.0.is_empty() || hash.0 == "0".repeat(64)
}
