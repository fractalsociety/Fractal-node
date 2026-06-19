// Fractal Society: Train in Simulation. Prove in Public. Deploy with Confidence.
//
// A domain-neutral research protocol for AI agents with trading as the first domain adapter.
//
// # Architecture Principles
//
// 1. **Generic Core, Domain Adapters**: The kernel is domain-neutral. Trading-specific logic lives in adapters.
// 2. **Proof Over Screenshots**: Every result is backed by cryptographic commitments and verifiable evidence.
// 3. **Simulation First**: All agents begin with deterministic simulations before real-world deployment.
// 4. **Public Proof, Private IP**: Users can prove capabilities without revealing proprietary strategies.

#![warn(missing_docs)]
#![deny(unsafe_code)]

pub mod error;
pub mod protocol;
pub mod artifact;
pub mod simulation;
pub mod verifier;

/// Fractal Society version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Core research pipeline result
pub type Result<T> = std::result::Result<T, error::Error>;

/// Re-exports commonly used types
pub mod prelude {
    pub use crate::error::{Error, Result};
    pub use crate::protocol::{
        ResearchProject, Protocol, DatasetManifest, EnvironmentManifest,
        AgentManifest, ExperimentRun, EvidenceBundle, Visibility,
    };
    pub use crate::artifact::{
        ArtifactId, ArtifactHash, ArtifactManifest,
    };
    pub use crate::simulation::{DomainAdapter, Observation, Action, Outcome};
    pub use crate::verifier::{VerifierPackage, VerifierReport, ProofLevel};
}

/// Fractal Society configuration
#[derive(Debug, Clone)]
pub struct Config {
    /// Maximum simulated capital for new agents
    pub max_initial_capital: u64,
    /// Default leverage cap
    pub default_leverage_cap: f64,
    /// Enable trading domain
    pub trading_enabled: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_initial_capital: 100_000, // $100,000 USDC
            default_leverage_cap: 2.0,     // 2x
            trading_enabled: false,         // Disabled until explicitly enabled
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version() {
        assert!(!VERSION.is_empty());
    }

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.max_initial_capital, 100_000);
        assert_eq!(config.default_leverage_cap, 2.0);
        assert!(!config.trading_enabled);
    }
}
