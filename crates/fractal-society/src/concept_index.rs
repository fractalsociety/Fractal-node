//! Append-only cross-package concept index — the "don't re-explore" registry.
//!
//! Each packaged paper appends its concept nodes (topics, contributions,
//! systems, dead-ends) to `concept-index.jsonl` in the packages repo. A new
//! agent queries the index before exploring to see whether a concept was
//! already covered, and — crucially — whether it was a dead-end (so the same
//! failed path isn't retried).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::protocol::Hash;
use crate::research_package::{ConceptKind, ConceptStatus, PaperPackage};

/// The index file name within the packages repo.
pub const CONCEPT_INDEX: &str = "concept-index.jsonl";

/// One row of the append-only concept index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptIndexEntry {
    /// Stable concept id (from the digest).
    pub concept_id: String,
    /// Human label.
    pub label: String,
    /// Node kind.
    pub kind: ConceptKind,
    /// Lifecycle status.
    pub status: ConceptStatus,
    /// Root hash of the package that recorded this concept.
    pub package_hash: Hash,
    /// Path to the package within the repo (e.g. `packages/<id>`).
    pub package_path: String,
    /// For dead-ends: why it was rejected.
    pub dead_end_reason: Option<String>,
}

/// Append a package's concept nodes to the index.
///
/// Each concept becomes one JSONL row (canonical JSON, one line). Idempotent at
/// the caller's discretion — duplicate appends are allowed (the index is an
/// append-only log, not a set).
pub fn append(repo_path: &Path, package: &PaperPackage) -> Result<()> {
    let package_path = package
        .dir
        .strip_prefix(repo_path)
        .unwrap_or(&package.dir)
        .to_string_lossy()
        .to_string();
    let entries = entries_for(package, &package_path);

    let index_path = repo_path.join(CONCEPT_INDEX);
    if let Some(parent) = index_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&index_path)?;
    for entry in &entries {
        // canonical JSON is always valid UTF-8; write the bytes + a newline.
        let bytes = crate::canonical::canonical_json(entry)?;
        file.write_all(&bytes)?;
        writeln!(file)?;
    }
    Ok(())
}

/// Build index entries for a package's concept nodes (mapped from the graph).
fn entries_for(package: &PaperPackage, package_path: &str) -> Vec<ConceptIndexEntry> {
    // Reconstruct ConceptKind/Status from the exploration node kind/status.
    package
        .graph
        .nodes
        .iter()
        .map(|n| ConceptIndexEntry {
            concept_id: n.id.clone(),
            label: n.description.clone(),
            kind: kind_from_node(&n.kind),
            status: status_from_node(&n.status),
            package_hash: package.root_hash.clone(),
            package_path: package_path.to_string(),
            dead_end_reason: n.dead_end_reason.clone(),
        })
        .collect()
}

fn kind_from_node(k: &crate::exploration::NodeKind) -> ConceptKind {
    use crate::exploration::NodeKind as N;
    match k {
        N::Hypothesis => ConceptKind::Claim,
        N::Strategy => ConceptKind::System,
        N::DeadEnd | N::Abandoned => ConceptKind::DeadEnd,
        N::Approach | N::Config => ConceptKind::Contribution,
    }
}

fn status_from_node(s: &crate::exploration::NodeStatus) -> ConceptStatus {
    use crate::exploration::NodeStatus as S;
    match s {
        S::Active => ConceptStatus::Active,
        S::Proven => ConceptStatus::Supported,
        S::Disproven => ConceptStatus::Refuted,
        S::Superseded => ConceptStatus::Superseded,
        S::Abandoned => ConceptStatus::Abandoned,
    }
}

/// Read the entire index.
pub fn read_all(repo_path: &Path) -> Result<Vec<ConceptIndexEntry>> {
    let index_path = repo_path.join(CONCEPT_INDEX);
    if !index_path.exists() {
        return Ok(Vec::new());
    }
    let contents = fs::read_to_string(&index_path)?;
    let mut entries = Vec::new();
    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let entry: ConceptIndexEntry =
            serde_json::from_str(line).map_err(crate::error::Error::Json)?;
        entries.push(entry);
    }
    Ok(entries)
}

/// Query the index for a concept (case-insensitive substring match on id or
/// label). Returns matching entries, so an agent can see prior coverage and
/// any dead-end reasons before re-exploring.
pub fn query(repo_path: &Path, needle: &str) -> Result<Vec<ConceptIndexEntry>> {
    let needle = needle.to_ascii_lowercase();
    Ok(read_all(repo_path)?
        .into_iter()
        .filter(|e| {
            e.concept_id.to_ascii_lowercase().contains(&needle)
                || e.label.to_ascii_lowercase().contains(&needle)
        })
        .collect())
}

/// Count dead-end entries (the "already-tried-and-failed" set).
pub fn dead_end_count(repo_path: &Path) -> Result<usize> {
    Ok(read_all(repo_path)?
        .into_iter()
        .filter(|e| e.kind == ConceptKind::DeadEnd)
        .count())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::research_package::{assemble_package, ConceptNode};
    use crate::signing::AuthorSigner;
    use chrono::DateTime;

    fn repo_root() -> std::path::PathBuf {
        let p = std::env::temp_dir().join(format!("fractal-concept-idx-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn package_at(repo: &Path) -> PaperPackage {
        let digest = crate::research_package::PaperDigest {
            source: crate::research_package::PaperSource {
                title: "Idx Paper".to_string(),
                authors: vec!["X".to_string()],
                venue: None,
                year: None,
                url: None,
                source_hash: crate::protocol::Hash::new(b"idx-src"),
            },
            summary: "s".to_string(),
            claims: vec![],
            method: None,
            concepts: vec![
                ConceptNode {
                    id: "topic".to_string(),
                    label: "autonomy taxonomy".to_string(),
                    kind: ConceptKind::Topic,
                    parent: None,
                    status: ConceptStatus::Active,
                    dead_end_reason: None,
                    description: None,
                },
                ConceptNode {
                    id: "de".to_string(),
                    label: "general-purpose autonomy".to_string(),
                    kind: ConceptKind::DeadEnd,
                    parent: Some("topic".to_string()),
                    status: ConceptStatus::Refuted,
                    dead_end_reason: Some("goal drift".to_string()),
                    description: None,
                },
            ],
            limitations: vec![],
            reviewer: "test".to_string(),
        };
        let pkg_dir = repo.join("packages/idx-paper-deadbeef");
        let signer = AuthorSigner::from_seed(&[1u8; 32]);
        assemble_package(
            &digest,
            &signer,
            DateTime::from_timestamp(1, 0).unwrap(),
            &pkg_dir,
            None,
        )
        .unwrap()
    }

    #[test]
    fn append_then_query_finds_concepts_and_dead_ends() {
        let repo = repo_root();
        let pkg = package_at(&repo);

        append(&repo, &pkg).unwrap();

        let hits = query(&repo, "autonomy").unwrap();
        assert_eq!(
            hits.len(),
            2,
            "both 'autonomy taxonomy' and 'general-purpose autonomy' match"
        );
        assert_eq!(dead_end_count(&repo).unwrap(), 1);

        let de = query(&repo, "general-purpose").unwrap();
        assert_eq!(de.len(), 1);
        assert_eq!(de[0].kind, ConceptKind::DeadEnd);
        assert_eq!(de[0].dead_end_reason.as_deref(), Some("goal drift"));

        let _ = std::fs::remove_dir_all(&repo);
    }
}
