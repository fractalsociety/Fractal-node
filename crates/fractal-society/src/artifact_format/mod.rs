//! Navigable artifact directory format (AR-06).
//!
//! Packs a completed [`PipelineResult`] (and an optional exploration graph) into
//! an agent-browsable directory, in the spirit of ARA's layered layout:
//!
//! ```text
//! <artifact>/
//!   PAPER.md                 # root manifest + layer index
//!   logic/
//!     claims.md              # the claim under test
//!     experiments.md         # protocol / experiment description
//!     architecture.md        # adapter / agent identity
//!   src/
//!     configs.md             # metric + verifier configuration
//!     environment.md         # environment / dataset hashes
//!   trace/
//!     exploration.json       # AR-05 exploration graph (dead ends)
//!   evidence/
//!     manifest.json          # ProofManifest
//!     scorecard.json         # Scorecard
//!     bundle.json            # RunBundle
//!     proof_card.md          # human-readable proof card
//! ```
//!
//! The artifact's **root hash** is the canonical hash of a
//! [`DirectoryManifest`] mapping every file path to its content hash — a tiny
//! directory Merkle root. Tampering any file changes the root hash, and writing
//! then reading round-trips the scorecard, bundle, manifest, and graph.

pub mod reader;
pub mod writer;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::exploration::ExplorationGraph;
use crate::pkgs::run_bundle::RunBundle;
use crate::protocol::{Hash, ProofManifest};
use crate::verifier::Scorecard;

pub use reader::read_artifact_dir;
pub use writer::write_artifact_dir;

/// Relative path of the root index file.
pub const PAPER_MD: &str = "PAPER.md";
/// Relative path of the exploration graph file (present only when a graph exists).
pub const EXPLORATION_JSON: &str = "trace/exploration.json";
/// Relative path of the proof manifest file.
pub const MANIFEST_JSON: &str = "evidence/manifest.json";
/// Relative path of the scorecard file.
pub const SCORECARD_JSON: &str = "evidence/scorecard.json";
/// Relative path of the run bundle file.
pub const BUNDLE_JSON: &str = "evidence/bundle.json";
/// Relative path of the proof card file.
pub const PROOF_CARD_MD: &str = "evidence/proof_card.md";

/// The fixed set of layer files (always present) in canonical order.
pub const FIXED_FILES: &[&str] = &[
    PAPER_MD,
    "logic/claims.md",
    "logic/experiments.md",
    "logic/architecture.md",
    "src/configs.md",
    "src/environment.md",
    MANIFEST_JSON,
    SCORECARD_JSON,
    BUNDLE_JSON,
    PROOF_CARD_MD,
];

/// Directory manifest: every file path → its content hash. Sorted by path, so
/// [`DirectoryManifest::root_hash`] is independent of write order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryManifest {
    /// File path (relative, `/`-separated) → SHA-256 content hash.
    pub files: BTreeMap<String, Hash>,
}

impl DirectoryManifest {
    /// Canonical root hash of the artifact directory.
    pub fn root_hash(&self) -> Result<Hash> {
        Hash::of(self)
    }

    /// Insert a file's content hash under `path`.
    pub fn insert(&mut self, path: impl Into<String>, bytes: &[u8]) {
        self.files.insert(path.into(), Hash::new(bytes));
    }
}

/// An artifact read back from a directory.
#[derive(Debug, Clone)]
pub struct LoadedArtifact {
    /// The signed proof manifest.
    pub manifest: ProofManifest,
    /// The scorecard.
    pub scorecard: Scorecard,
    /// The tamper-evident run bundle.
    pub bundle: RunBundle,
    /// The exploration graph, if the artifact carried one.
    pub graph: Option<ExplorationGraph>,
    /// Recomputed root hash of the directory.
    pub root_hash: Hash,
}
