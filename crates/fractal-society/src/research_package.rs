//! Reusable paper → verified research-package pipeline.
//!
//! The "upload any paper, have an agent review it, package it, commit it, and
//! hash that it was checked" flow. The deterministic core lives here; the LLM
//! extraction (PDF → [`PaperDigest`]) is agent work done *outside* the crate, so
//! any agent (Claude, the TS app, a future skill) can drive the same pipeline.
//!
//! Flow:
//! ```text
//! PaperDigest (extracted by an agent)
//!   -> assemble_package(...)
//!      -> ARA-style artifact directory (PAPER.md + logic/ + src/ + trace/ + evidence/)
//!      -> ExplorationGraph of the paper's concepts (incl. dead-ends)
//!      -> signed PaperManifest (review verdict + root hash = the proof)
//!      -> root_hash (content-addressed, tamper-evident)
//! ```
//! The returned [`PaperPackage`] is then committed to a git repo and its
//! concepts appended to an append-only concept index (see [`crate::concept_index`])
//! so future agents can ask "was concept X already explored?".

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::artifact_format::DirectoryManifest;
use crate::error::Result;
use crate::exploration::{ExplorationGraph, ExplorationNode, NodeKind, NodeStatus, ProvenanceTag};
use crate::pkgs::chain_commitment::CommitmentAdapter;
use crate::protocol::Hash;
use crate::signing::{decode_signature_hex, verify_signature, AuthorSigner};

/// Provenance of the paper under review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperSource {
    /// Paper title.
    pub title: String,
    /// Author list.
    pub authors: Vec<String>,
    /// Venue, if published.
    pub venue: Option<String>,
    /// Publication year.
    pub year: Option<u32>,
    /// Source URL, if any.
    pub url: Option<String>,
    /// SHA-256 of the source document bytes (the PDF). This pins the exact
    /// artifact the review covers.
    pub source_hash: Hash,
}

/// One falsifiable claim extracted from the paper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimDigest {
    /// Short stable id (e.g. "C1").
    pub id: String,
    /// The claim, in the author's words.
    pub text: String,
    /// Whether the claim is testable / falsifiable.
    pub falsifiable: bool,
    /// Supporting evidence references (sections, citations, tables).
    pub evidence_refs: Vec<String>,
    /// Stated scope, if any.
    pub scope: Option<String>,
}

/// Kind of node in the paper's concept graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConceptKind {
    /// The paper's top-level topic.
    Topic,
    /// A stated contribution.
    Contribution,
    /// A claim or finding.
    Claim,
    /// A surveyed system / prior work.
    System,
    /// A method or technique.
    Method,
    /// An open problem.
    OpenProblem,
    /// An approach that was tried and rejected (do not re-explore).
    DeadEnd,
}

/// Lifecycle status of a concept node.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConceptStatus {
    /// Currently open / active.
    Active,
    /// Supported by the paper's evidence.
    Supported,
    /// Refuted by the paper's evidence.
    Refuted,
    /// Superseded by another node.
    Superseded,
    /// Deliberately set aside.
    Abandoned,
}

/// One node in the paper's concept graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptNode {
    /// Stable id (unique within the digest).
    pub id: String,
    /// Human label.
    pub label: String,
    /// Node kind.
    pub kind: ConceptKind,
    /// Parent concept id (forms a tree/DAG).
    pub parent: Option<String>,
    /// Lifecycle status.
    pub status: ConceptStatus,
    /// For dead-ends: why it was rejected.
    pub dead_end_reason: Option<String>,
    /// Optional longer description.
    pub description: Option<String>,
}

/// The structured output of an agent's paper ingestion. Input to the pipeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperDigest {
    /// Paper provenance.
    pub source: PaperSource,
    /// One-paragraph summary (abstract-level).
    pub summary: String,
    /// Extracted claims.
    pub claims: Vec<ClaimDigest>,
    /// Method description, if any.
    pub method: Option<String>,
    /// The paper's concept graph (topics, contributions, systems, dead-ends).
    pub concepts: Vec<ConceptNode>,
    /// Stated limitations.
    pub limitations: Vec<String>,
    /// Identity of the reviewing agent (e.g. "claude-opus-4.8", "founder").
    pub reviewer: String,
}

/// Mechanical review verdict over the digest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewVerdict {
    /// Strong accept.
    StrongAccept,
    /// Accept.
    Accept,
    /// Weak accept.
    WeakAccept,
    /// Weak reject.
    WeakReject,
    /// Reject.
    Reject,
}

/// A complete mechanical review of a paper digest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperReview {
    /// Overall verdict.
    pub verdict: ReviewVerdict,
    /// 0..=100 score.
    pub score: u8,
    /// Per-check findings (empty when all pass).
    pub findings: Vec<String>,
    /// Fraction of claims marked falsifiable (0..=1).
    pub falsifiable_fraction: f64,
    /// Mean evidence references per claim.
    pub mean_evidence_per_claim: f64,
    /// Number of disclosed limitations.
    pub limitations: usize,
    /// Number of recorded dead-ends (exploration awareness).
    pub dead_ends: usize,
}

/// Signed manifest for a packaged paper — the "hash that it was checked".
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperManifest {
    /// Manifest schema version.
    pub manifest_version: String,
    /// Package id (slug derived from the title + source hash).
    pub package_id: String,
    /// SHA-256 of the source document bytes.
    pub source_hash: Hash,
    /// Content-addressed root hash of the artifact directory (the proof).
    pub root_hash: Hash,
    /// Paper title.
    pub title: String,
    /// Authors.
    pub authors: Vec<String>,
    /// Reviewing agent identity.
    pub reviewer: String,
    /// The mechanical review verdict.
    pub review: PaperReview,
    /// Number of claims / concepts in the package.
    pub claim_count: usize,
    /// Number of concept nodes.
    pub concept_count: usize,
    /// On-chain commitment reference, if the root hash was submitted to a chain.
    pub chain_reference: Option<crate::protocol::ChainReference>,
    /// When the package was assembled.
    pub created_at: DateTime<Utc>,
    /// Ed25519 author signature over the signable bytes (this field blanked).
    pub author_signature: String,
}

impl PaperManifest {
    /// Canonical signable bytes (signature field blanked).
    fn signable_bytes(&self) -> Result<Vec<u8>> {
        let mut copy = self.clone();
        copy.author_signature.clear();
        crate::canonical::signable_bytes(&copy)
    }

    /// Verify the manifest's signature against a public key.
    pub fn verify_author(&self, public_key: &[u8; 32]) -> Result<()> {
        let sig = decode_signature_hex(&self.author_signature)?;
        let bytes = self.signable_bytes()?;
        verify_signature(public_key, &bytes, &sig)
    }
}

/// A packaged, reviewable paper ready to commit.
#[derive(Debug, Clone)]
pub struct PaperPackage {
    /// Where the artifact directory was written.
    pub dir: std::path::PathBuf,
    /// Content-addressed root hash (the proof / on-chain hash).
    pub root_hash: Hash,
    /// Signed manifest.
    pub manifest: PaperManifest,
    /// The concept exploration graph.
    pub graph: ExplorationGraph,
}

/// Assemble a paper package: write the ARA-style artifact directory, build the
/// concept graph, compute the mechanical review, optionally commit the root hash
/// on-chain, sign the manifest, and return the content-addressed root hash.
///
/// Deterministic given `(digest, signer_seed, now, output_dir, chain)`. The
/// directory is fully content-addressed: `root_hash` is the canonical hash of a
/// [`DirectoryManifest`] mapping every file to its content hash.
///
/// When `chain` is supplied, the package's `root_hash` is submitted on-chain
/// (the on-chain proof), the returned [`ChainReference`] is attached to the
/// manifest, and the manifest is signed *after* attaching so the chain
/// reference is covered by the author signature.
pub fn assemble_package(
    digest: &PaperDigest,
    signer: &AuthorSigner,
    now: DateTime<Utc>,
    output_dir: &Path,
    chain: Option<&dyn CommitmentAdapter>,
) -> Result<PaperPackage> {
    let graph = build_graph(&digest.concepts);
    let review = review(digest);

    // Build every file's bytes once; the same bytes are hashed and written.
    let files = build_files(digest, &graph, &review)?;

    let mut dir = DirectoryManifest {
        files: BTreeMap::new(),
    };
    for (rel, bytes) in &files {
        dir.insert(rel, bytes);
        write_file(output_dir, rel, bytes)?;
    }
    let root_hash = dir.root_hash()?;

    // Optionally commit the root hash on-chain BEFORE signing so the manifest
    // (and its signature) binds the on-chain reference.
    let chain_reference = match chain {
        Some(adapter) => Some(adapter.submit(&root_hash)?),
        None => None,
    };

    let manifest = PaperManifest {
        manifest_version: "1.0.0".to_string(),
        package_id: package_id(digest),
        source_hash: digest.source.source_hash.clone(),
        root_hash: root_hash.clone(),
        title: digest.source.title.clone(),
        authors: digest.source.authors.clone(),
        reviewer: digest.reviewer.clone(),
        review,
        claim_count: digest.claims.len(),
        concept_count: digest.concepts.len(),
        chain_reference,
        created_at: now,
        author_signature: String::new(),
    };
    // Sign over the manifest with root_hash + chain_reference + review bound in.
    let mut signed = manifest.clone();
    let signable = signed.signable_bytes()?;
    signed.author_signature = hex::encode(signer.sign_bytes(&signable));

    // Persist the (now-signed) manifest + the directory manifest.
    write_file(
        output_dir,
        "evidence/manifest.json",
        &crate::canonical::canonical_json(&signed)?,
    )?;
    write_file(
        output_dir,
        "directory-manifest.json",
        &crate::canonical::canonical_json(&dir)?,
    )?;

    Ok(PaperPackage {
        dir: output_dir.to_path_buf(),
        root_hash,
        manifest: signed,
        graph,
    })
}

/// Map concept nodes into an [`ExplorationGraph`] (reusing the AR-05 type).
fn build_graph(concepts: &[ConceptNode]) -> ExplorationGraph {
    let mut graph = ExplorationGraph::new();
    for c in concepts {
        let kind = match c.kind {
            ConceptKind::Topic | ConceptKind::Contribution => NodeKind::Approach,
            ConceptKind::Claim => NodeKind::Hypothesis,
            ConceptKind::System | ConceptKind::Method => NodeKind::Strategy,
            ConceptKind::OpenProblem => NodeKind::Approach,
            ConceptKind::DeadEnd => NodeKind::DeadEnd,
        };
        let status = match c.status {
            ConceptStatus::Active => NodeStatus::Active,
            ConceptStatus::Supported => NodeStatus::Proven,
            ConceptStatus::Refuted => NodeStatus::Disproven,
            ConceptStatus::Superseded => NodeStatus::Superseded,
            ConceptStatus::Abandoned => NodeStatus::Abandoned,
        };
        let _ = graph.add_node(ExplorationNode {
            id: c.id.clone(),
            kind,
            status,
            description: c.label.clone(),
            outcome_summary: c.description.clone(),
            parent: c.parent.clone(),
            children: Vec::new(),
            evidence_ref: None,
            provenance: ProvenanceTag::Human,
            dead_end_reason: c.dead_end_reason.clone(),
        });
    }
    graph
}

/// Mechanical review of a digest (no LLM): falsifiability, evidence density,
/// limitation honesty, exploration awareness (dead-ends), and source pinning.
pub fn review(digest: &PaperDigest) -> PaperReview {
    let mut findings = Vec::new();
    let claim_count = digest.claims.len().max(1);

    let falsifiable_count = digest.claims.iter().filter(|c| c.falsifiable).count();
    let falsifiable_fraction = falsifiable_count as f64 / claim_count as f64;
    if falsifiable_fraction < 0.5 {
        findings.push(format!(
            "only {:.0}% of claims are marked falsifiable",
            falsifiable_fraction * 100.0
        ));
    }

    let total_refs: usize = digest.claims.iter().map(|c| c.evidence_refs.len()).sum();
    let mean_evidence_per_claim = total_refs as f64 / claim_count as f64;
    if mean_evidence_per_claim < 1.0 {
        findings.push("claims cite little supporting evidence".to_string());
    }

    if digest.limitations.is_empty() {
        findings.push("no limitations disclosed".to_string());
    }

    if digest.source.source_hash.0.trim().is_empty() {
        findings
            .push("source document hash is missing (review not pinned to a source)".to_string());
    }

    let dead_ends = digest
        .concepts
        .iter()
        .filter(|c| c.kind == ConceptKind::DeadEnd)
        .count();

    // Score: start from evidence/falsifiability, reward honest limitations +
    // dead-end recording, penalize missing source pin.
    let mut score = 40u8;
    score = score.saturating_add((falsifiable_fraction * 30.0) as u8);
    score = score.saturating_add((mean_evidence_per_claim.min(2.0) * 15.0) as u8);
    if !digest.limitations.is_empty() {
        score = score.saturating_add(10);
    }
    if dead_ends > 0 {
        score = score.saturating_add(5);
    }
    if digest.source.source_hash.0.trim().is_empty() {
        score = score.saturating_sub(20);
    }
    let score = score.min(100);

    let verdict = if score >= 85 {
        ReviewVerdict::StrongAccept
    } else if score >= 70 {
        ReviewVerdict::Accept
    } else if score >= 55 {
        ReviewVerdict::WeakAccept
    } else if score >= 40 {
        ReviewVerdict::WeakReject
    } else {
        ReviewVerdict::Reject
    };

    PaperReview {
        verdict,
        score,
        findings,
        falsifiable_fraction,
        mean_evidence_per_claim,
        limitations: digest.limitations.len(),
        dead_ends,
    }
}

/// Build the `(relative_path, bytes)` for every file in the package.
fn build_files(
    digest: &PaperDigest,
    graph: &ExplorationGraph,
    review: &PaperReview,
) -> Result<Vec<(String, Vec<u8>)>> {
    let mut files = vec![
        (
            "PAPER.md".to_string(),
            paper_md(digest, review).into_bytes(),
        ),
        (
            "logic/claims.md".to_string(),
            claims_md(digest).into_bytes(),
        ),
        (
            "logic/method.md".to_string(),
            digest
                .method
                .clone()
                .unwrap_or_else(|| "_(no method section)_".to_string())
                .into_bytes(),
        ),
        (
            "logic/concepts.md".to_string(),
            concepts_md(digest).into_bytes(),
        ),
        ("src/source.md".to_string(), source_md(digest).into_bytes()),
        (
            "evidence/review.md".to_string(),
            review_md(review).into_bytes(),
        ),
        (
            "evidence/claims.json".to_string(),
            crate::canonical::canonical_json(&digest.claims)?,
        ),
        (
            "trace/concepts.json".to_string(),
            crate::canonical::canonical_json(graph)?,
        ),
    ];
    // manifest.json is written after signing in assemble_package; reserve none here.
    let _ = &mut files;
    Ok(files)
}

fn package_id(digest: &PaperDigest) -> String {
    let slug = digest
        .source
        .title
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .filter(|c| !c.is_whitespace())
        .collect::<String>();
    let short = &digest.source.source_hash.0[..8];
    format!("{slug}-{short}")
}

fn paper_md(digest: &PaperDigest, review: &PaperReview) -> String {
    format!(
        "# {title}\n\n\
         {authors}\n\n\
         {summary}\n\n\
         **Review:** {verdict:?} ({score}/100) by `{reviewer}`\n\
         **Source hash:** `{source_hash}`  ·  **Package root hash:** computed on commit\n\n\
         ## Layers\n\
         - `logic/claims.md` — extracted falsifiable claims\n\
         - `logic/method.md` — method\n\
         - `logic/concepts.md` — concept tree\n\
         - `src/source.md` — provenance + source pin\n\
         - `evidence/review.md` — mechanical review verdict\n\
         - `evidence/claims.json` — machine-readable claims\n\
         - `trace/concepts.json` — concept graph (dead-ends included)\n\
         - `evidence/manifest.json` — signed manifest (root hash = proof)\n",
        title = digest.source.title,
        authors = digest.source.authors.join(", "),
        summary = digest.summary,
        verdict = review.verdict,
        score = review.score,
        reviewer = digest.reviewer,
        source_hash = digest.source.source_hash.0,
    )
}

fn claims_md(digest: &PaperDigest) -> String {
    let mut s = String::from("# Claims\n\n");
    for c in &digest.claims {
        let falsifiable = if c.falsifiable {
            "falsifiable"
        } else {
            "non-falsifiable"
        };
        let evidence = if c.evidence_refs.is_empty() {
            "_(no evidence cited)_".to_string()
        } else {
            c.evidence_refs.join(", ")
        };
        s.push_str(&format!(
            "- **{id}** ({falsifiable}): {text}\n  - evidence: {evidence}\n",
            id = c.id,
            text = c.text,
            evidence = evidence
        ));
    }
    s
}

fn concepts_md(digest: &PaperDigest) -> String {
    let mut s = String::from("# Concept tree\n\n```\n");
    let roots: Vec<&ConceptNode> = digest
        .concepts
        .iter()
        .filter(|c| c.parent.is_none())
        .collect();
    for root in roots {
        render_node(root, 0, digest, &mut s);
    }
    s.push_str("```\n");
    s
}

fn render_node(node: &ConceptNode, depth: usize, digest: &PaperDigest, out: &mut String) {
    let indent = "  ".repeat(depth);
    let marker = if node.kind == ConceptKind::DeadEnd {
        " ✗(dead-end)"
    } else {
        ""
    };
    out.push_str(&format!(
        "{indent}- {label}{marker} [{kind:?}]\n",
        label = node.label,
        kind = node.kind
    ));
    if node.kind == ConceptKind::DeadEnd {
        if let Some(reason) = &node.dead_end_reason {
            out.push_str(&format!("{indent}    reason: {reason}\n"));
        }
    }
    let children: Vec<&ConceptNode> = digest
        .concepts
        .iter()
        .filter(|c| c.parent.as_deref() == Some(node.id.as_str()))
        .collect();
    for child in children {
        render_node(child, depth + 1, digest, out);
    }
}

fn source_md(digest: &PaperDigest) -> String {
    format!(
        "# Source\n\n\
         - Title: {title}\n\
         - Authors: {authors}\n\
         - Venue: {venue}\n\
         - Year: {year}\n\
         - URL: {url}\n\
         - Source hash (SHA-256 of the document bytes): `{source_hash}`\n",
        title = digest.source.title,
        authors = digest.source.authors.join(", "),
        venue = digest.source.venue.as_deref().unwrap_or("—"),
        year = digest
            .source
            .year
            .map(|y| y.to_string())
            .unwrap_or("—".to_string()),
        url = digest.source.url.as_deref().unwrap_or("—"),
        source_hash = digest.source.source_hash.0,
    )
}

fn review_md(review: &PaperReview) -> String {
    let mut s = format!(
        "# Mechanical review\n\n\
         - Verdict: {verdict:?}\n\
         - Score: {score}/100\n\
         - Falsifiable claims: {pct:.0}%\n\
         - Mean evidence/claim: {mean:.2}\n\
         - Limitations disclosed: {lim}\n\
         - Dead-ends recorded: {dead}\n\n",
        verdict = review.verdict,
        score = review.score,
        pct = review.falsifiable_fraction * 100.0,
        mean = review.mean_evidence_per_claim,
        lim = review.limitations,
        dead = review.dead_ends
    );
    if review.findings.is_empty() {
        s.push_str("_No findings._\n");
    } else {
        s.push_str("## Findings\n\n");
        for f in &review.findings {
            s.push_str(&format!("- {f}\n"));
        }
    }
    s.push_str(
        "\n_Mechanical review only (no LLM judgment). A human or agent peer review \
         should layer on top of this verdict._\n",
    );
    s
}

fn write_file(root: &Path, rel: &str, bytes: &[u8]) -> Result<()> {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signing::AuthorSigner;

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }

    fn sample_digest() -> PaperDigest {
        PaperDigest {
            source: PaperSource {
                title: "Synthetic Test Paper".to_string(),
                authors: vec!["A. Researcher".to_string()],
                venue: Some("Test Venue".to_string()),
                year: Some(2026),
                url: None,
                source_hash: Hash::new(b"fake-pdf-bytes"),
            },
            summary: "A synthetic paper for testing.".to_string(),
            claims: vec![
                ClaimDigest {
                    id: "C1".to_string(),
                    text: "Claim one is testable.".to_string(),
                    falsifiable: true,
                    evidence_refs: vec!["§3".to_string()],
                    scope: Some("synthetic".to_string()),
                },
                ClaimDigest {
                    id: "C2".to_string(),
                    text: "Claim two.".to_string(),
                    falsifiable: false,
                    evidence_refs: vec![],
                    scope: None,
                },
            ],
            method: Some("We did X.".to_string()),
            concepts: vec![
                ConceptNode {
                    id: "topic".to_string(),
                    label: "test topic".to_string(),
                    kind: ConceptKind::Topic,
                    parent: None,
                    status: ConceptStatus::Active,
                    dead_end_reason: None,
                    description: None,
                },
                ConceptNode {
                    id: "d1".to_string(),
                    label: "rejected approach".to_string(),
                    kind: ConceptKind::DeadEnd,
                    parent: Some("topic".to_string()),
                    status: ConceptStatus::Refuted,
                    dead_end_reason: Some("did not generalize".to_string()),
                    description: None,
                },
            ],
            limitations: vec!["small sample".to_string()],
            reviewer: "test-agent".to_string(),
        }
    }

    #[test]
    fn assemble_produces_signed_package_with_root_hash() {
        let signer = AuthorSigner::from_seed(&[0x42; 32]);
        let pk = signer.public_key();
        let dir = std::env::temp_dir().join(format!("fractal-paper-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let pkg = assemble_package(&sample_digest(), &signer, now(), &dir, None).unwrap();

        // Root hash is a real sha256 hex.
        assert_eq!(pkg.root_hash.0.len(), 64);
        // Manifest signature verifies.
        pkg.manifest.verify_author(&pk).unwrap();
        // Manifest binds the root hash.
        assert_eq!(pkg.manifest.root_hash, pkg.root_hash);
        // Graph carries the dead-end.
        assert_eq!(pkg.graph.dead_ends().len(), 1);
        // Files were written.
        assert!(dir.join("PAPER.md").exists());
        assert!(dir.join("evidence/manifest.json").exists());
        assert!(dir.join("trace/concepts.json").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn weak_digest_scores_low() {
        let mut d = sample_digest();
        d.claims = vec![ClaimDigest {
            id: "C1".to_string(),
            text: "vague claim".to_string(),
            falsifiable: false,
            evidence_refs: vec![],
            scope: None,
        }];
        d.limitations = vec![];
        d.source.source_hash = Hash(String::new());
        let r = review(&d);
        assert!(r.score < 50);
        assert!(matches!(
            r.verdict,
            ReviewVerdict::WeakReject | ReviewVerdict::Reject
        ));
    }

    #[test]
    fn tampering_a_file_changes_root_hash() {
        let signer = AuthorSigner::from_seed(&[0x42; 32]);
        let dir = std::env::temp_dir().join(format!("fractal-paper-tamper-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let pkg = assemble_package(&sample_digest(), &signer, now(), &dir, None).unwrap();
        let original = std::fs::read(dir.join("PAPER.md")).unwrap();
        std::fs::write(dir.join("PAPER.md"), b"tampered").unwrap();

        // Recompute the directory manifest from the known files.
        let files = [
            "PAPER.md",
            "logic/claims.md",
            "logic/method.md",
            "logic/concepts.md",
            "src/source.md",
            "evidence/review.md",
            "evidence/claims.json",
            "trace/concepts.json",
        ];
        let mut dm = DirectoryManifest {
            files: BTreeMap::new(),
        };
        for f in files {
            let bytes = std::fs::read(dir.join(f)).unwrap();
            dm.insert(f, &bytes);
        }
        let tampered_root = dm.root_hash().unwrap();
        assert_ne!(tampered_root, pkg.root_hash);

        // Restore; root hash returns.
        std::fs::write(dir.join("PAPER.md"), &original).unwrap();
        let mut dm2 = DirectoryManifest {
            files: BTreeMap::new(),
        };
        for f in files {
            let bytes = std::fs::read(dir.join(f)).unwrap();
            dm2.insert(f, &bytes);
        }
        // Note: evidence/manifest.json is not in the hashed set, so it doesn't affect root.
        assert_eq!(dm2.root_hash().unwrap(), pkg.root_hash);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
