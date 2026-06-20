//! Epistemic rigor reviewer (AR-07).
//!
//! A quality gate **distinct** from the five integrity verifiers. The
//! integrity verifiers check whether the accounting is honest (accounting,
//! cost, risk, temporal, sandbox); this module scores whether the *research
//! claim* is well-formed and well-supported — falsifiable, evidenced,
//! reproducible, honestly scoped.
//!
//! The rubric is **mechanical and deterministic**: identical inputs always
//! produce a byte-identical [`RigorReport`] (so a rigor score can itself be
//! committed). An LLM-assisted overlay that targets the same schema can live in
//! the TypeScript app; the crate defines the rubric and the schema.

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::exploration::ExplorationGraph;
use crate::protocol::{Hash, ProofManifest};
use crate::verifier::{ProofLevel, Scorecard};

/// A dimension of epistemic quality scored by the reviewer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RigorDimension {
    /// Is the claim testable?
    Falsifiability,
    /// Does the cited evidence actually support the claim?
    EvidenceRelevance,
    /// Does the claim match the tested scope?
    ScopeCalibration,
    /// Are seeds/configs/env fully specified?
    Reproducibility,
    /// Is every claim linked to evidence?
    ClaimEvidenceBinding,
    /// Are limitations disclosed?
    LimitationHonesty,
}

/// A per-dimension score (0..=100) with supporting findings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimensionScore {
    /// The dimension scored.
    pub dimension: RigorDimension,
    /// Score in 0..=100.
    pub score: u8,
    /// What drove the score (empty when perfect).
    pub findings: Vec<String>,
}

/// Overall recommendation derived from the dimension scores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Recommendation {
    /// Strong accept: excellent rigor across dimensions.
    StrongAccept,
    /// Accept: solid rigor, minor gaps.
    Accept,
    /// Weak accept: passes but with notable gaps.
    WeakAccept,
    /// Weak reject: notable rigor problems.
    WeakReject,
    /// Reject: serious rigor problems.
    Reject,
}

/// A complete rigor review of one research claim.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RigorReport {
    /// Per-dimension scores (fixed order, so the report hashes deterministically).
    pub dimensions: Vec<DimensionScore>,
    /// Mean of the dimension scores, 0..=100.
    pub overall: u8,
    /// Derived recommendation.
    pub recommendation: Recommendation,
    /// One-line human-readable summary.
    pub summary: String,
}

impl RigorReport {
    /// Deterministic content hash of the report (commit the rigor score itself).
    pub fn content_hash(&self) -> Result<Hash> {
        Hash::of(self)
    }
}

/// A research claim under review. No canonical `Claim` type exists elsewhere in
/// the protocol yet, so the reviewer owns this lightweight schema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claim {
    /// The claim text (must be non-empty and falsifiable to score well).
    pub text: String,
    /// Whether the claim is falsifiable / testable.
    pub falsifiable: bool,
    /// Content hashes of evidence the claim cites.
    pub evidence_refs: Vec<Hash>,
    /// Optional explicit scope statement.
    pub scope: Option<String>,
}

/// Score a research claim's epistemic rigor.
///
/// Deterministic: identical `(manifest, scorecard, claim, graph)` inputs always
/// produce a byte-identical [`RigorReport`]. The rubric is purely mechanical —
/// it never calls an LLM — so scores are reproducible.
#[allow(clippy::too_many_arguments)]
pub fn review(
    manifest: &ProofManifest,
    scorecard: &Scorecard,
    claim: &Claim,
    graph: Option<&ExplorationGraph>,
) -> Result<RigorReport> {
    let reproducible = (scorecard.proof_level as u8) >= (ProofLevel::Reproducible as u8);

    let mut dims = Vec::with_capacity(6);

    // Falsifiability: empty or non-falsifiable claims score 0.
    let (score, findings) = if claim.text.trim().is_empty() {
        (0, vec!["claim text is empty".to_string()])
    } else if !claim.falsifiable {
        (0, vec!["claim is not marked falsifiable".to_string()])
    } else {
        (100, Vec::new())
    };
    dims.push(score_dim(RigorDimension::Falsifiability, score, findings));

    // Evidence relevance: rewards cited evidence + measured metrics.
    let mut score = 40u8;
    let mut findings = Vec::new();
    if claim.evidence_refs.is_empty() {
        findings.push("claim cites no evidence".to_string());
    } else {
        score = score.saturating_add(30);
    }
    if scorecard.primary_metrics.is_empty() {
        findings.push("scorecard has no primary metrics".to_string());
    } else {
        score = score.saturating_add(30);
    }
    dims.push(score_dim(
        RigorDimension::EvidenceRelevance,
        score,
        findings,
    ));

    // Scope calibration: explicit scope + disclosed limitations score higher.
    let (mut score, findings): (u8, Vec<String>) = match &claim.scope {
        None => (40, vec!["no explicit scope stated".to_string()]),
        Some(_) => (80, Vec::new()),
    };
    if !scorecard.limitations.is_empty() {
        score = score.saturating_add(20);
    }
    dims.push(score_dim(RigorDimension::ScopeCalibration, score, findings));

    // Reproducibility: needs a real environment hash + reproducible proof level.
    let (mut score, findings): (u8, Vec<String>) = if is_zero_hash(&manifest.environment_hash) {
        (20, vec!["environment hash is missing/zero".to_string()])
    } else {
        (60, Vec::new())
    };
    if reproducible {
        score = score.saturating_add(40);
    }
    dims.push(score_dim(RigorDimension::Reproducibility, score, findings));

    // Claim–evidence binding: every claim must link to evidence.
    let (score, findings): (u8, Vec<String>) = if claim.evidence_refs.is_empty() {
        (0, vec!["claim has no evidence links".to_string()])
    } else if reproducible {
        (100, Vec::new())
    } else {
        (70, Vec::new())
    };
    dims.push(score_dim(
        RigorDimension::ClaimEvidenceBinding,
        score,
        findings,
    ));

    // Limitation honesty: undisclosed limitations / disclaimer are penalized.
    let mut score = 100u8;
    let mut findings = Vec::new();
    if scorecard.limitations.is_empty() {
        score = score.saturating_sub(50);
        findings.push("no limitations disclosed".to_string());
    }
    if scorecard.disclaimer.trim().is_empty() {
        score = score.saturating_sub(30);
        findings.push("no disclaimer".to_string());
    }
    dims.push(score_dim(
        RigorDimension::LimitationHonesty,
        score,
        findings,
    ));

    let overall = dimension_average(&dims);
    let recommendation = recommend(overall);
    let dead_ends = graph.map(|g| g.dead_ends().len()).unwrap_or(0);
    let summary = format!(
        "overall {overall}/100 ({}); {dead_ends} dead-ends recorded",
        recommendation_label(recommendation)
    );

    Ok(RigorReport {
        dimensions: dims,
        overall,
        recommendation,
        summary,
    })
}

/// Map an overall score to a recommendation.
pub fn recommend(overall: u8) -> Recommendation {
    if overall >= 85 {
        Recommendation::StrongAccept
    } else if overall >= 70 {
        Recommendation::Accept
    } else if overall >= 55 {
        Recommendation::WeakAccept
    } else if overall >= 40 {
        Recommendation::WeakReject
    } else {
        Recommendation::Reject
    }
}

fn recommendation_label(r: Recommendation) -> &'static str {
    match r {
        Recommendation::StrongAccept => "strong accept",
        Recommendation::Accept => "accept",
        Recommendation::WeakAccept => "weak accept",
        Recommendation::WeakReject => "weak reject",
        Recommendation::Reject => "reject",
    }
}

fn score_dim(dimension: RigorDimension, score: u8, findings: Vec<String>) -> DimensionScore {
    DimensionScore {
        dimension,
        score,
        findings,
    }
}

fn dimension_average(dims: &[DimensionScore]) -> u8 {
    if dims.is_empty() {
        return 0;
    }
    let total: u32 = dims.iter().map(|d| d.score as u32).sum();
    (total / dims.len() as u32).min(100) as u8
}

/// True when a hash is all-zero (unset / missing).
fn is_zero_hash(hash: &Hash) -> bool {
    !hash.0.is_empty() && hash.0.bytes().all(|b| b == b'0')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{ChainReference, Hash, ProofManifest, Visibility};
    use crate::verifier::{
        BaselineResult, CostAssumptions, MetricValue, ProofLevel, RiskMetrics, Scorecard,
        SimulationTier, VerifierSummary,
    };
    use std::collections::HashMap;

    fn ts() -> chrono::DateTime<chrono::Utc> {
        chrono::DateTime::from_timestamp(0, 0).unwrap()
    }

    fn non_zero_hash() -> Hash {
        Hash::new(b"some-real-content")
    }

    fn zero_hash() -> Hash {
        Hash::from_hex(&"0".repeat(64)).unwrap()
    }

    fn manifest(env_zero: bool) -> ProofManifest {
        ProofManifest {
            manifest_version: "1.0.0".to_string(),
            claim_id: "claim-1".to_string(),
            protocol_hash: non_zero_hash(),
            agent_hash: non_zero_hash(),
            dataset_hash: non_zero_hash(),
            environment_hash: if env_zero {
                zero_hash()
            } else {
                non_zero_hash()
            },
            trace_merkle_root: non_zero_hash(),
            verifier_set_hash: non_zero_hash(),
            scorecard_hash: non_zero_hash(),
            disclosure: Visibility::Open,
            author_signature: String::new(),
            platform_attestation: None,
            chain_reference: Some(ChainReference {
                network: "fractalchain-41".to_string(),
                transaction_hash: "0xabc".to_string(),
                block_number: 1,
                finalized: true,
            }),
            timestamp: ts(),
        }
    }

    fn scorecard(
        proof_level: ProofLevel,
        limitations: Vec<String>,
        disclaimer: &str,
        with_metrics: bool,
    ) -> Scorecard {
        let mut primary_metrics = HashMap::new();
        if with_metrics {
            primary_metrics.insert(
                "net_return".to_string(),
                MetricValue {
                    value: 0.01,
                    higher_is_better: true,
                    unit: "ratio".to_string(),
                },
            );
        }
        Scorecard {
            id: "score-1".to_string(),
            agent_id: "agent-1".to_string(),
            agent_version: "1.0.0".to_string(),
            protocol_id: "proto-1".to_string(),
            primary_metrics,
            baselines: HashMap::<String, BaselineResult>::new(),
            risk_metrics: RiskMetrics {
                max_drawdown: 0.0,
                volatility: 0.0,
                cvar_95: 0.0,
                worst_day: 0.0,
                policy_violations: 0,
            },
            verifier_summary: VerifierSummary {
                total_verifiers: 0,
                verifiers_passed: 0,
                verifiers_failed: 0,
                required_passed: 0,
                required_total: 0,
            },
            simulation_tier: SimulationTier::S0,
            cost_assumptions: CostAssumptions {
                fee_model: String::new(),
                latency_ms: 0,
                slippage_model: String::new(),
                starting_capital: 0,
            },
            confidence_intervals: HashMap::new(),
            proof_level,
            limitations,
            disclaimer: disclaimer.to_string(),
            timestamp: ts(),
        }
    }

    fn strong_claim() -> Claim {
        Claim {
            text: "The strategy beats buy-and-hold out-of-sample.".to_string(),
            falsifiable: true,
            evidence_refs: vec![non_zero_hash()],
            scope: Some("BTC/ETH 1-minute candles, 2h window".to_string()),
        }
    }

    fn weak_claim() -> Claim {
        Claim {
            text: String::new(),
            falsifiable: false,
            evidence_refs: Vec::new(),
            scope: None,
        }
    }

    #[test]
    fn weak_claim_is_rejected() {
        let report = review(
            &manifest(true),
            &scorecard(ProofLevel::PrivateDraft, Vec::new(), "", false),
            &weak_claim(),
            None,
        )
        .unwrap();
        assert!(report.overall < 40, "overall was {}", report.overall);
        assert_eq!(report.recommendation, Recommendation::Reject);
    }

    #[test]
    fn strong_claim_is_strongly_accepted() {
        let report = review(
            &manifest(false),
            &scorecard(
                ProofLevel::Reproducible,
                vec!["limited to 2h".to_string()],
                "synthetic data only",
                true,
            ),
            &strong_claim(),
            None,
        )
        .unwrap();
        assert_eq!(report.overall, 100);
        assert_eq!(report.recommendation, Recommendation::StrongAccept);
    }

    #[test]
    fn identical_inputs_yield_identical_report_hash() {
        let m = manifest(false);
        let s = scorecard(ProofLevel::Reproducible, vec!["x".to_string()], "d", true);
        let c = strong_claim();

        let h1 = review(&m, &s, &c, None).unwrap().content_hash().unwrap();
        let h2 = review(&m, &s, &c, None).unwrap().content_hash().unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn dead_end_graph_is_noted_in_summary() {
        let mut graph = crate::exploration::ExplorationGraph::new();
        graph
            .add_node(crate::exploration::ExplorationNode {
                id: "d1".to_string(),
                kind: crate::exploration::NodeKind::DeadEnd,
                status: crate::exploration::NodeStatus::Disproven,
                description: "overfit".to_string(),
                outcome_summary: None,
                parent: None,
                children: Vec::new(),
                evidence_ref: None,
                provenance: crate::exploration::ProvenanceTag::Human,
                dead_end_reason: Some("overfit".to_string()),
            })
            .unwrap();

        let report = review(
            &manifest(false),
            &scorecard(ProofLevel::Auditable, vec!["l".to_string()], "d", true),
            &strong_claim(),
            Some(&graph),
        )
        .unwrap();
        assert!(report.summary.contains("1 dead-ends recorded"));
    }
}
