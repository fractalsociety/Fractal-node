//! Client-facing finality status helpers.

use std::{fmt, str::FromStr};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinalityStatus {
    /// Committee or sequencer accepted the block, but settlement proof is not final yet.
    Soft,
    /// A validity proof has been accepted for settlement/bridge use.
    Proof,
}

impl FinalityStatus {
    pub const SOFT_WIRE: &'static str = "soft";
    pub const PROOF_WIRE: &'static str = "proof";

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Soft => Self::SOFT_WIRE,
            Self::Proof => Self::PROOF_WIRE,
        }
    }

    pub fn is_proof_final(self) -> bool {
        matches!(self, Self::Proof)
    }

    pub fn satisfies(self, required: FinalityRequirement) -> bool {
        match required {
            FinalityRequirement::SoftAllowed => true,
            FinalityRequirement::ProofRequired => self.is_proof_final(),
        }
    }
}

impl fmt::Display for FinalityStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for FinalityStatus {
    type Err = FinalityStatusParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            Self::SOFT_WIRE => Ok(Self::Soft),
            Self::PROOF_WIRE => Ok(Self::Proof),
            _ => Err(FinalityStatusParseError),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinalityRequirement {
    SoftAllowed,
    ProofRequired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FinalityStatusParseError;

impl fmt::Display for FinalityStatusParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unknown finality status")
    }
}

impl std::error::Error for FinalityStatusParseError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BlockFinalityStatus {
    pub block_hash: [u8; 32],
    pub block_number: u64,
    pub status: FinalityStatus,
    pub proof_circuit_version: Option<String>,
    pub proof_coverage_manifest_digest: Option<String>,
    pub proof_covered_features: Option<String>,
}

impl BlockFinalityStatus {
    pub fn is_proof_final(&self) -> bool {
        self.status.is_proof_final()
    }

    pub fn satisfies(&self, required: FinalityRequirement) -> bool {
        self.status.satisfies(required)
    }

    pub fn proof_coverage(&self) -> Option<(&str, &str, &str)> {
        Some((
            self.proof_circuit_version.as_deref()?,
            self.proof_coverage_manifest_digest.as_deref()?,
            self.proof_covered_features.as_deref()?,
        ))
    }
}
