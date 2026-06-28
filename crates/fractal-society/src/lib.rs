//! Fractal Society: Train in Simulation. Prove in Public. Deploy with Confidence.
//!
//! A domain-neutral research protocol for AI agents with trading as the first
//! domain adapter. This crate is the canonical, tested protocol spec (PHASE-01
//! schemas + PHASE-02 generic kernel); the TypeScript app in `fractalwork`
//! mirrors these schemas at runtime.
//!
//! # Architecture Principles
//!
//! 1. **Generic Core, Domain Adapters**: the kernel is domain-neutral.
//!    Trading-specific logic lives in adapters, never in the kernel or schema.
//! 2. **Proof Over Screenshots**: every result is backed by cryptographic
//!    commitments and verifiable evidence.
//! 3. **Simulation First**: all agents begin with deterministic simulations
//!    before real-world deployment.
//! 4. **Public Proof, Private IP**: users can prove capabilities without
//!    revealing proprietary strategies.

#![deny(missing_docs)]
#![deny(unsafe_code)]

pub mod adapters;
pub mod artifact;
pub mod artifact_format;
pub mod canonical;
#[cfg(feature = "live-chain")]
pub mod chain;
pub mod commit_service;
pub mod concept_index;
pub mod error;
pub mod exploration;
pub mod git_output;
pub mod kernel;
pub mod market_data;
pub mod offline_verify;
pub mod persistence;
pub mod pipeline;
pub mod pkgs;
pub mod protocol;
pub mod research_package;
pub mod rigor;
pub mod signing;
pub mod simulation;
pub mod verifier;

/// Fractal Society version
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Core research pipeline result
pub type Result<T> = std::result::Result<T, error::Error>;

/// Re-exports commonly used types
pub mod prelude {
    pub use crate::adapters::{ReferenceAdapter, ReferenceAgent};
    pub use crate::artifact::{ArtifactHash, ArtifactId, ArtifactManifest};
    pub use crate::canonical::content_hash;
    pub use crate::commit_service::{
        commit_research_package, retrieve_payload, retrieve_research_package, PackageKind,
        PackageMetadata, PublishedPackage, RetrievedPackage,
    };
    pub use crate::error::{Error, Result};
    pub use crate::kernel::{run, KernelConfig, RunManifest};
    pub use crate::protocol::{
        AgentManifest, DatasetManifest, EnvironmentManifest, EvidenceBundle, Hash, Protocol,
        ResearchProject, Visibility,
    };
    pub use crate::signing::AuthorSigner;
    pub use crate::simulation::{Action, DomainAdapter, Observation, Outcome};
    pub use crate::verifier::{ProofLevel, VerifierPackage, VerifierReport};
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
            default_leverage_cap: 2.0,    // 2x
            trading_enabled: false,       // Disabled until explicitly enabled
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.max_initial_capital, 100_000);
        assert_eq!(config.default_leverage_cap, 2.0);
        assert!(!config.trading_enabled);
    }
}
