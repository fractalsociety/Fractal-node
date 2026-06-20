//! Artifact directory reader (AR-06).

use std::fs;
use std::path::Path;

use crate::artifact_format::{DirectoryManifest, EXPLORATION_JSON};
use crate::error::Result;
use crate::pkgs::run_bundle::RunBundle;
use crate::protocol::{Hash, ProofManifest};
use crate::verifier::Scorecard;

use super::LoadedArtifact;

/// Read a navigable artifact directory written by
/// [`write_artifact_dir`](super::write_artifact_dir).
///
/// Deserializes the manifest, scorecard, bundle, and (if present) the
/// exploration graph, and recomputes the directory root hash from every file.
pub fn read_artifact_dir(root: &Path) -> Result<LoadedArtifact> {
    let manifest: ProofManifest = read_json(root, "evidence/manifest.json")?;
    let scorecard: Scorecard = read_json(root, "evidence/scorecard.json")?;
    let bundle: RunBundle = read_json(root, "evidence/bundle.json")?;

    let graph_path = root.join(EXPLORATION_JSON);
    let graph = if graph_path.exists() {
        let bytes = fs::read(&graph_path)?;
        Some(serde_json::from_slice(&bytes).map_err(crate::error::Error::Json)?)
    } else {
        None
    };

    let root_hash = recompute_root_hash(root, graph.is_some())?;

    Ok(LoadedArtifact {
        manifest,
        scorecard,
        bundle,
        graph,
        root_hash,
    })
}

/// Recompute the directory root hash by re-hashing every known file, exactly as
/// the writer did.
fn recompute_root_hash(root: &Path, has_graph: bool) -> Result<Hash> {
    let paths = known_paths(has_graph);
    let mut dir = DirectoryManifest {
        files: std::collections::BTreeMap::new(),
    };
    for rel in paths {
        let bytes = fs::read(root.join(rel)).map_err(crate::error::Error::Io)?;
        dir.insert(rel, &bytes);
    }
    dir.root_hash()
}

/// The exact set of files hashed by the writer.
fn known_paths(has_graph: bool) -> Vec<&'static str> {
    let mut paths: Vec<&'static str> = vec![
        "PAPER.md",
        "logic/claims.md",
        "logic/experiments.md",
        "logic/architecture.md",
        "src/configs.md",
        "src/environment.md",
        "evidence/manifest.json",
        "evidence/scorecard.json",
        "evidence/bundle.json",
        "evidence/proof_card.md",
    ];
    if has_graph {
        paths.push(EXPLORATION_JSON);
    }
    paths
}

fn read_json<T: serde::de::DeserializeOwned>(root: &Path, rel: &str) -> Result<T> {
    let bytes = fs::read(root.join(rel)).map_err(crate::error::Error::Io)?;
    serde_json::from_slice(&bytes).map_err(crate::error::Error::Json)
}
