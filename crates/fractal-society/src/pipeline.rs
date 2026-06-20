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
use crate::persistence::artifact_store::ArtifactStore;
use crate::persistence::event_log::EventLog;
use crate::pkgs::chain_commitment::CommitmentAdapter;
use crate::pkgs::pipeline_contract::PipelineOutcome;
use crate::pkgs::proof_manifest;
use crate::pkgs::reward_gate;
use crate::pkgs::run_bundle::RunBundle;
use crate::pkgs::{
    accounting_integrity, cost_completeness, risk_policy, sandbox_policy, temporal_leakage,
};
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
    run_pipeline_with_commitment(
        adapter,
        agent,
        seed,
        kernel_config,
        trading_config,
        baselines,
        verifiers,
        signer,
        timestamp,
        None,
    )
    .await
}

/// Run the full research pipeline and optionally commit the proof hash on-chain.
///
/// When `commitment_adapter` is supplied, the pipeline submits the pre-chain
/// proof hash, attaches the returned chain reference, re-signs the proof
/// manifest, and builds the final bundle from that signed manifest.
#[allow(clippy::too_many_arguments)] // orchestrator entry point; inputs are genuinely independent
pub async fn run_pipeline_with_commitment(
    adapter: TradingAdapter,
    agent: impl Agent<TradingAdapter>,
    seed: u64,
    kernel_config: KernelConfig,
    trading_config: TradingConfig,
    baselines: Vec<(String, RunOutcome)>,
    verifiers: Vec<VerifierFn>,
    signer: &AuthorSigner,
    timestamp: DateTime<Utc>,
    commitment_adapter: Option<&dyn CommitmentAdapter>,
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
    let mut proof_manifest = proof_manifest::build(&run, &scorecard, signer, timestamp)?;
    if let Some(adapter) = commitment_adapter {
        let proof_hash = Hash::of(&proof_manifest)?;
        proof_manifest.chain_reference = Some(adapter.submit(&proof_hash)?);
        proof_manifest.author_signature = proof_manifest.author_signature_hex(signer)?;
    }

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

/// Default accounting tolerance used by the canonical trading verifier pack.
pub const DEFAULT_TOLERANCE: f64 = 1e-3;

/// The canonical verifier set for a trading simulation run.
///
/// Returns the core trading verifiers — accounting integrity, cost
/// completeness, risk-policy consistency, temporal ordering, and sandbox policy
/// — wrapped as [`VerifierFn`]s. Pass to [`run_pipeline`] or use
/// [`run_pipeline_default`].
pub fn trading_verifier_pack(tolerance: f64, allowed_tools: Vec<String>) -> Vec<VerifierFn> {
    vec![
        Arc::new(move |evidence: &EvidenceBundle| {
            accounting_integrity::verify(evidence, tolerance)
        }),
        Arc::new(move |evidence: &EvidenceBundle| cost_completeness::verify(evidence, tolerance)),
        Arc::new(move |evidence: &EvidenceBundle| risk_policy::verify(evidence)),
        Arc::new(|evidence: &EvidenceBundle| temporal_leakage::verify(evidence)),
        Arc::new(move |evidence: &EvidenceBundle| sandbox_policy::verify(evidence, &allowed_tools)),
    ]
}

fn default_allowed_tools() -> Vec<String> {
    vec![
        "hold".to_string(),
        "place_order".to_string(),
        "reduce_position".to_string(),
        "cancel_order".to_string(),
    ]
}

/// Run the pipeline using the canonical trading verifier pack.
///
/// Convenience wrapper around [`run_pipeline`] that supplies
/// [`trading_verifier_pack`] with [`DEFAULT_TOLERANCE`] and the standard trading
/// action allowlist. Supply everything else as for [`run_pipeline`].
#[allow(clippy::too_many_arguments)] // orchestrator convenience; inputs are genuinely independent
pub async fn run_pipeline_default(
    adapter: TradingAdapter,
    agent: impl Agent<TradingAdapter>,
    seed: u64,
    kernel_config: KernelConfig,
    trading_config: TradingConfig,
    baselines: Vec<(String, RunOutcome)>,
    signer: &AuthorSigner,
    timestamp: DateTime<Utc>,
) -> crate::Result<PipelineResult> {
    let verifiers = trading_verifier_pack(DEFAULT_TOLERANCE, default_allowed_tools());
    run_pipeline(
        adapter,
        agent,
        seed,
        kernel_config,
        trading_config,
        baselines,
        verifiers,
        signer,
        timestamp,
    )
    .await
}

/// Run the full research pipeline and persist its durable artifacts.
#[allow(clippy::too_many_arguments)] // persisted orchestrator entry point
pub async fn run_pipeline_persisted(
    adapter: TradingAdapter,
    agent: impl Agent<TradingAdapter>,
    seed: u64,
    kernel_config: KernelConfig,
    trading_config: TradingConfig,
    baselines: Vec<(String, RunOutcome)>,
    verifiers: Vec<VerifierFn>,
    signer: &AuthorSigner,
    timestamp: DateTime<Utc>,
    artifact_store: &mut dyn ArtifactStore,
    event_log: &mut dyn EventLog,
) -> crate::Result<PipelineResult> {
    let result = run_pipeline(
        adapter,
        agent,
        seed,
        kernel_config,
        trading_config,
        baselines,
        verifiers,
        signer,
        timestamp,
    )
    .await?;
    crate::persistence::persist_pipeline_result(&result, artifact_store, event_log)?;
    Ok(result)
}
