//! Public proof-card package.
//!
//! Build a public-facing `ProofCard` (claim, level, tier, key metrics,
//! disclaimer, proof hash) from a signed `ProofManifest` and `Scorecard`.

use crate::protocol::{Hash, ProofManifest};
use crate::verifier::{Scorecard, SimulationTier};

/// Public-facing proof summary.
#[derive(Debug, Clone, PartialEq)]
pub struct ProofCard {
    /// Human-readable claim identifier.
    pub claim: String,
    /// Proof level label.
    pub proof_level: String,
    /// Simulation tier from the scorecard.
    pub simulation_tier: SimulationTier,
    /// Net return from the scorecard primary metrics.
    pub net_return: f64,
    /// Maximum drawdown from the scorecard risk metrics.
    pub max_drawdown: f64,
    /// Public simulation disclaimer.
    pub disclaimer: String,
    /// Hash reference for the proof evidence.
    pub proof_hash: Hash,
}

/// Build a public proof card from a signed manifest and scorecard.
pub fn build(manifest: &ProofManifest, scorecard: &Scorecard) -> ProofCard {
    ProofCard {
        claim: manifest.claim_id.clone(),
        proof_level: format!("{:?}", scorecard.proof_level),
        simulation_tier: scorecard.simulation_tier,
        net_return: scorecard
            .primary_metrics
            .get("net_return")
            .map(|metric| metric.value)
            .unwrap_or(0.0),
        max_drawdown: scorecard.risk_metrics.max_drawdown,
        disclaimer: scorecard.disclaimer.clone(),
        proof_hash: manifest.trace_merkle_root.clone(),
    }
}
