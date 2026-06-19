//! End-to-end research-pipeline orchestrator (serial integration, package 67).
//!
//! Chains the research pipeline stages into a single call:
//! `run → score → verify → proof → outcome/bundle`. It composes the stable spine
//! packages (`proof_manifest`, `verifier_summary`-style aggregation via
//! `reward_gate`, `pipeline_contract`, `run_bundle`) with the generic kernel and
//! the trading scorecard builder.
//!
//! Deterministic: identical inputs (adapter/agent/seed/config/baselines/
//! verifiers/signer/timestamp) produce identical `proof_manifest` and
//! `RunBundle` hashes. No wall-clock or OS randomness is read here.

use std::sync::Arc;

use chrono::{DateTime, Utc};

use crate::adapters::trading::{build_scorecard, TradingAdapter, TradingConfig};
use crate::kernel::{self, KernelConfig, RunOutcome};
use crate::pkgs::pipeline_contract::PipelineOutcome;
use crate::pkgs::proof_manifest;
use crate::pkgs::reward_gate;
use crate::pkgs::run_bundle::RunBundle;
use crate::protocol::{EvidenceBundle, Hash, ProofManifest};
use crate::signing::AuthorSigner;
use crate::simulation::Agent;
use crate::verifier::{Scorecard, VerifierReport};

/// A verifier function run over a run's evidence.
pub type VerifierFn = Arc<dyn Fn(&EvidenceBundle) -> VerifierReport + Send + Sync>;

/// Full output of a pipeline run.
#[derive(Debug, Clone)]
pub struct PipelineResult {
    /// Underlying kernel run outcome (evidence, metrics, manifest, evidence hash).
    pub run: RunOutcome,
    /// Scorecard built from the candidate run + baselines.
    pub scorecard: Scorecard,
    /// Verifier reports produced by the supplied verifier set.
    pub verifier_reports: Vec<VerifierReport>,
    /// Signed proof manifest.
    pub proof_manifest: ProofManifest,
    /// Aggregated pipeline outcome (stage / completion).
    pub outcome: PipelineOutcome,
    /// Portable run bundle (tamper-evident hash of the whole run).
    pub bundle: RunBundle,
}

/// Run the full research pipeline for one candidate against `baselines`.
///
/// Stages: `kernel::run` → `build_scorecard` → run each verifier over the
/// candidate evidence → `reward_gate::evaluate` → `proof_manifest::build` →
/// assemble `PipelineOutcome` + `RunBundle`. The reward gate treats the
/// challenge window as closed and requires at least one passing verifier.
#[allow(clippy::too_many_arguments)] // orchestrator entry point; inputs are genuinely independent
pub async fn run_pipeline(
    adapter: TradingAdapter,
    agent: impl Agent<TradingAdapter>,
    seed: u64,
    kernel_config: KernelConfig,
    trading_config: TradingConfig,
    baselines: Vec<(String, RunOutcome)>,
    verifiers: Vec<VerifierFn>,
    signer: &AuthorSigner,
    timestamp: DateTime<Utc>,
) -> crate::Result<PipelineResult> {
    // 1. Run the candidate through the generic kernel.
    let run = kernel::run(adapter, agent, seed, &kernel_config).await?;

    // 2. Score against the supplied baselines.
    let scorecard = build_scorecard(&run, &baselines, &trading_config, timestamp);

    // 3. Run every verifier over the candidate evidence.
    let verifier_reports: Vec<VerifierReport> = verifiers
        .iter()
        .map(|verify| verify(&run.evidence))
        .collect();

    // 4. Reward gate decides release from verifier pass-state (closed window).
    let reward_released = matches!(
        reward_gate::evaluate(&verifier_reports, false, 1),
        reward_gate::RewardDecision::Release
    );

    // 5. Build and sign the proof manifest.
    let proof_manifest = proof_manifest::build(&run, &scorecard, signer, timestamp)?;

    // 6. Assemble the pipeline outcome + portable bundle.
    let scorecard_hash = Hash::of(&scorecard)?;
    let run_manifest_hash = Hash::of(&run.manifest)?;
    let proof_hash = Hash::of(&proof_manifest)?;

    let outcome = PipelineOutcome {
        evidence_hash: run.evidence_hash.clone(),
        scorecard_hash: scorecard_hash.clone(),
        verifier_reports: verifier_reports.clone(),
        committed: true,
        reward_released,
    };

    let bundle = RunBundle::new(
        run_manifest_hash,
        run.evidence_hash.clone(),
        scorecard_hash,
        proof_hash,
        run.manifest.agent_id.clone(),
    );

    Ok(PipelineResult {
        run,
        scorecard,
        verifier_reports,
        proof_manifest,
        outcome,
        bundle,
    })
}
