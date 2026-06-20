//! Artifact directory writer (AR-06).

use std::fs;
use std::path::Path;

use crate::artifact_format::{DirectoryManifest, EXPLORATION_JSON, PAPER_MD};
use crate::error::Result;
use crate::exploration::ExplorationGraph;
use crate::pipeline::PipelineResult;
use crate::protocol::Hash;

/// Write a completed pipeline result (and optional exploration graph) to a
/// navigable artifact directory at `root`, returning the directory root hash.
///
/// The directory is fully content-addressed: the returned hash is the canonical
/// hash of a [`DirectoryManifest`] mapping every file to its content hash.
pub fn write_artifact_dir(
    root: &Path,
    result: &PipelineResult,
    graph: Option<&ExplorationGraph>,
) -> Result<Hash> {
    let files = build_files(result, graph)?;

    let mut dir = DirectoryManifest {
        files: std::collections::BTreeMap::new(),
    };
    for (rel, bytes) in &files {
        dir.insert(rel, bytes);
        write_file(root, rel, bytes)?;
    }

    // Persist the directory manifest for transparency. It documents the root
    // hash but is deliberately NOT part of the hashed set.
    let dir_bytes = canonical_bytes(&dir)?;
    write_file(root, "directory-manifest.json", &dir_bytes)?;

    dir.root_hash()
}

/// Build the full set of `(relative_path, bytes)` for the artifact, in a fixed
/// order. The same bytes are used for content hashing and writing, so the
/// directory manifest cannot diverge from what is on disk.
fn build_files(
    result: &PipelineResult,
    graph: Option<&ExplorationGraph>,
) -> Result<Vec<(String, Vec<u8>)>> {
    let manifest = &result.proof_manifest;
    let scorecard = &result.scorecard;
    let bundle = &result.bundle;

    let mut files: Vec<(String, Vec<u8>)> = vec![
        (PAPER_MD.to_string(), paper_md(result).into_bytes()),
        (
            "logic/claims.md".to_string(),
            claims_md(manifest).into_bytes(),
        ),
        (
            "logic/experiments.md".to_string(),
            experiments_md(manifest).into_bytes(),
        ),
        (
            "logic/architecture.md".to_string(),
            architecture_md(result).into_bytes(),
        ),
        (
            "src/configs.md".to_string(),
            configs_md(scorecard).into_bytes(),
        ),
        (
            "src/environment.md".to_string(),
            environment_md(manifest).into_bytes(),
        ),
        (
            "evidence/manifest.json".to_string(),
            canonical_bytes(manifest)?,
        ),
        (
            "evidence/scorecard.json".to_string(),
            canonical_bytes(scorecard)?,
        ),
        ("evidence/bundle.json".to_string(), canonical_bytes(bundle)?),
        (
            "evidence/proof_card.md".to_string(),
            proof_card_md(result).into_bytes(),
        ),
    ];

    if let Some(graph) = graph {
        files.push((EXPLORATION_JSON.to_string(), canonical_bytes(graph)?));
    }

    Ok(files)
}

fn write_file(root: &Path, rel: &str, bytes: &[u8]) -> Result<()> {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, bytes)?;
    Ok(())
}

/// Canonical JSON bytes (sorted keys, compact, Rust-Display floats). Using the
/// crate's own canonical form for the stored JSON files guarantees they
/// round-trip exactly under `Hash::of` — serde's float formatting can differ
/// from Rust's `Display`, which would otherwise shift a float's canonical form
/// after a serialize→deserialize round-trip.
fn canonical_bytes<T: serde::Serialize + ?Sized>(value: &T) -> Result<Vec<u8>> {
    crate::canonical::canonical_json(value)
}

fn paper_md(result: &PipelineResult) -> String {
    let manifest = &result.proof_manifest;
    format!(
        "# Research Artifact\n\n\
         Claim: {claim}\n\
         Proof level: {pl:?}\n\n\
         ## Layers\n\
         - `logic/claims.md` — the falsifiable claim under test\n\
         - `logic/experiments.md` — protocol + experiment hashes\n\
         - `logic/architecture.md` — adapter + agent identity\n\
         - `src/configs.md` — metric + verifier configuration\n\
         - `src/environment.md` — environment + dataset hashes\n\
         - `trace/exploration.json` — exploration graph (dead ends)\n\
         - `evidence/manifest.json` — signed proof manifest\n\
         - `evidence/scorecard.json` — machine-readable scorecard\n\
         - `evidence/bundle.json` — tamper-evident run bundle\n\
         - `evidence/proof_card.md` — human-readable proof card\n",
        claim = manifest.claim_id,
        pl = result.scorecard.proof_level,
    )
}

fn claims_md(manifest: &crate::protocol::ProofManifest) -> String {
    format!(
        "# Claims\n\nClaim `{claim}` is supported by the committed evidence.\n\
         Scorecard hash: `{scorecard}`\n",
        claim = manifest.claim_id,
        scorecard = manifest.scorecard_hash.0,
    )
}

fn experiments_md(manifest: &crate::protocol::ProofManifest) -> String {
    format!(
        "# Experiments\n\n\
         Protocol hash: `{proto}`\n\
         Trace merkle root: `{trace}`\n\
         Verifier set hash: `{verifiers}`\n",
        proto = manifest.protocol_hash.0,
        trace = manifest.trace_merkle_root.0,
        verifiers = manifest.verifier_set_hash.0,
    )
}

fn architecture_md(result: &PipelineResult) -> String {
    format!(
        "# Architecture\n\nAdapter: `{adapter}` v{adapter_ver}\nAgent: `{agent}`\n",
        adapter = result.run.manifest.adapter_id,
        adapter_ver = result.run.manifest.adapter_version,
        agent = result.run.manifest.agent_id,
    )
}

fn configs_md(scorecard: &crate::verifier::Scorecard) -> String {
    format!(
        "# Configuration\n\n\
         Primary metrics: {metrics}\n\
         Simulation tier: {tier:?}\n\
         Proof level: {pl:?}\n",
        metrics = scorecard
            .primary_metrics
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(", "),
        tier = scorecard.simulation_tier,
        pl = scorecard.proof_level,
    )
}

fn environment_md(manifest: &crate::protocol::ProofManifest) -> String {
    format!(
        "# Environment\n\n\
         Environment hash: `{env}`\n\
         Dataset hash: `{dataset}`\n\
         Agent hash: `{agent}`\n",
        env = manifest.environment_hash.0,
        dataset = manifest.dataset_hash.0,
        agent = manifest.agent_hash.0,
    )
}

fn proof_card_md(result: &PipelineResult) -> String {
    let manifest = &result.proof_manifest;
    let passed = result.verifier_reports.iter().filter(|r| r.passed).count();
    format!(
        "# Proof Card\n\n\
         - Claim: `{claim}`\n\
         - Proof level: `{pl:?}`\n\
         - Disclosure: `{disc:?}`\n\
         - Verifiers passed: {passed}/{total}\n\
         - Scorecard hash: `{proof}`\n\
         - Chain: {chain}\n",
        claim = manifest.claim_id,
        pl = result.scorecard.proof_level,
        disc = manifest.disclosure,
        passed = passed,
        total = result.verifier_reports.len(),
        proof = manifest.scorecard_hash.0,
        chain = chain_ref(manifest),
    )
}

fn chain_ref(manifest: &crate::protocol::ProofManifest) -> String {
    match &manifest.chain_reference {
        Some(c) => format!(
            "{} @ block {} (tx {}, finalized={})",
            c.network, c.block_number, c.transaction_hash, c.finalized
        ),
        None => "none (pre-chain)".to_string(),
    }
}
