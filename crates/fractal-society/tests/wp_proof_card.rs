use std::collections::HashMap;

use fractal_society::pkgs::proof_card::build;
use fractal_society::protocol::{Hash, ProofManifest, Visibility};
use fractal_society::verifier::{
    CostAssumptions, MetricValue, ProofLevel, RiskMetrics, Scorecard, SimulationTier,
    VerifierSummary,
};

fn manifest() -> ProofManifest {
    ProofManifest {
        manifest_version: "1.0.0".to_string(),
        claim_id: "claim-1".to_string(),
        protocol_hash: Hash::new(b"protocol"),
        agent_hash: Hash::new(b"agent"),
        dataset_hash: Hash::new(b"dataset"),
        environment_hash: Hash::new(b"environment"),
        trace_merkle_root: Hash::new(b"trace"),
        verifier_set_hash: Hash::new(b"verifiers"),
        scorecard_hash: Hash::new(b"scorecard"),
        disclosure: Visibility::CommittedPrivate,
        author_signature: "signature".to_string(),
        platform_attestation: None,
        chain_reference: None,
        timestamp: chrono::DateTime::from_timestamp(0, 0).unwrap(),
    }
}

fn scorecard() -> Scorecard {
    let mut primary_metrics = HashMap::new();
    primary_metrics.insert(
        "net_return".to_string(),
        MetricValue {
            value: 0.42,
            higher_is_better: true,
            unit: "fraction".to_string(),
        },
    );

    Scorecard {
        id: "scorecard-1".to_string(),
        agent_id: "agent-1".to_string(),
        agent_version: "0.1.0".to_string(),
        protocol_id: "protocol-1".to_string(),
        primary_metrics,
        baselines: HashMap::new(),
        risk_metrics: RiskMetrics {
            max_drawdown: 0.12,
            volatility: 0.03,
            cvar_95: -0.02,
            worst_day: -0.04,
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
            fee_model: "5 bps notional".to_string(),
            latency_ms: 0,
            slippage_model: "none".to_string(),
            starting_capital: 100_000,
        },
        confidence_intervals: HashMap::new(),
        proof_level: ProofLevel::Committed,
        limitations: Vec::new(),
        disclaimer: "SIMULATION ONLY - deterministic fixture.".to_string(),
        timestamp: chrono::DateTime::from_timestamp(0, 0).unwrap(),
    }
}

#[test]
fn disclaimer_is_present_and_simulation_labeled() {
    let card = build(&manifest(), &scorecard());

    assert!(!card.disclaimer.is_empty());
    assert!(card.disclaimer.contains("SIMULATION"));
}

#[test]
fn net_return_comes_from_scorecard_primary_metric() {
    let card = build(&manifest(), &scorecard());

    assert_eq!(card.net_return, 0.42);
}

#[test]
fn proof_hash_uses_manifest_trace_merkle_root() {
    let manifest = manifest();
    let card = build(&manifest, &scorecard());

    assert_eq!(card.proof_hash, manifest.trace_merkle_root);
}

#[test]
fn simulation_tier_matches_scorecard() {
    let scorecard = scorecard();
    let card = build(&manifest(), &scorecard);

    assert_eq!(card.simulation_tier, scorecard.simulation_tier);
}
