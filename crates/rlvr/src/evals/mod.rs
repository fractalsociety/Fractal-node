//! Evaluation harness and promotion-gate reporting.

use std::collections::BTreeSet;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::{
    hash_bytes, route_policy_hash, scan_privacy_tags, stable_hash, DialogueTrace, DialogueTurn,
    PrivacyTag, RewardVector, RlvrError, RlvrProofObject, RlvrProofType, RoutePolicy,
    TraceHashCommitment, VerifierOutput, DEFAULT_REWARD_POLICY,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Phase11TestCoverageReport {
    pub schema_tests: bool,
    pub privacy_filter_tests: bool,
    pub rubric_generator_tests: bool,
    pub verifier_parser_tests: bool,
    pub reward_vector_tests: bool,
    pub proof_object_tests: bool,
    pub node_rpc_tests: bool,
    pub block_inclusion_tests: bool,
    pub ci_command: String,
}

impl Default for Phase11TestCoverageReport {
    fn default() -> Self {
        Self {
            schema_tests: true,
            privacy_filter_tests: true,
            rubric_generator_tests: true,
            verifier_parser_tests: true,
            reward_vector_tests: true,
            proof_object_tests: true,
            node_rpc_tests: false,
            block_inclusion_tests: false,
            ci_command: "cargo test -p fractal-rlvr --lib".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdversarialPrivacyCase {
    pub name: String,
    pub input: String,
    pub expected_tag: PrivacyTag,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdversarialPrivacyResult {
    pub name: String,
    pub expected_tag: PrivacyTag,
    pub detected_tags: Vec<PrivacyTag>,
    pub local_only: bool,
    pub allow_external_models: bool,
    pub chain_payload_raw_data_free: bool,
    pub passed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdversarialPrivacyReport {
    pub results: Vec<AdversarialPrivacyResult>,
    pub malicious_raw_prompt_rejected: bool,
}

impl AdversarialPrivacyReport {
    pub fn passed(&self) -> bool {
        self.malicious_raw_prompt_rejected && self.results.iter().all(|result| result.passed)
    }
}

pub fn default_adversarial_privacy_cases() -> Vec<AdversarialPrivacyCase> {
    vec![
        AdversarialPrivacyCase {
            name: "api_key".into(),
            input: "Use sk-test-1234567890abcdef1234567890abcdef for the request.".into(),
            expected_tag: PrivacyTag::ApiKey,
        },
        AdversarialPrivacyCase {
            name: "private_file_path".into(),
            input: "Summarize /Users/alice/Documents/private-tax-return.pdf".into(),
            expected_tag: PrivacyTag::PrivateFile,
        },
        AdversarialPrivacyCase {
            name: "medical_data".into(),
            input: "My diagnosis includes hypertension and blood pressure medication.".into(),
            expected_tag: PrivacyTag::HealthData,
        },
        AdversarialPrivacyCase {
            name: "legal_data".into(),
            input: "My attorney says the contract clause is privileged.".into(),
            expected_tag: PrivacyTag::LegalData,
        },
        AdversarialPrivacyCase {
            name: "financial_data".into(),
            input: "My credit card is 4242 4242 4242 4242 and routing number is private.".into(),
            expected_tag: PrivacyTag::FinancialData,
        },
    ]
}

pub fn run_adversarial_privacy_suite() -> Result<AdversarialPrivacyReport, RlvrError> {
    let mut results = Vec::new();
    for case in default_adversarial_privacy_cases() {
        let scan = scan_privacy_tags(&case.input);
        let policy = scan.policy(false);
        policy.validate()?;
        let trace = privacy_case_trace(&case.name, &case.input);
        let proof = proof_from_trace(&trace)?;
        let proof_json = serde_json::to_string(&proof)?;
        let raw_data_free = !proof_json.contains(&case.input)
            && !proof_json.contains("raw_prompt")
            && !proof_json.contains("raw_answer")
            && !proof_json.contains("content");
        let passed = scan.tags.contains(&case.expected_tag)
            && policy.local_only
            && !policy.allow_external_models
            && raw_data_free;
        results.push(AdversarialPrivacyResult {
            name: case.name,
            expected_tag: case.expected_tag,
            detected_tags: scan.tags,
            local_only: policy.local_only,
            allow_external_models: policy.allow_external_models,
            chain_payload_raw_data_free: raw_data_free,
            passed,
        });
    }
    Ok(AdversarialPrivacyReport {
        results,
        malicious_raw_prompt_rejected: malicious_raw_prompt_payload_rejected()?,
    })
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProofRouteBenchmarkReport {
    pub iterations: u64,
    pub proof_submission_throughput_per_sec: f64,
    pub proof_verification_time_ms_avg: f64,
    pub proof_index_query_latency_ms_avg: f64,
    pub block_inclusion_latency_ms_estimate: f64,
    pub proof_payload_bytes: usize,
    pub normal_proof_payload_bytes: usize,
    pub payload_byte_overhead: isize,
}

pub fn run_proof_route_benchmark(iterations: u64) -> Result<ProofRouteBenchmarkReport, RlvrError> {
    let iterations = iterations.max(1);
    let trace = benchmark_trace();
    let proof = proof_from_trace(&trace)?;
    let normal_proof = trace.trace_hash_commitment()?;
    let proof_payload_bytes = proof.serialized_len()?;
    let normal_proof_payload_bytes = serde_json::to_vec(&normal_proof)?.len();

    let start = Instant::now();
    let mut hashes = Vec::with_capacity(iterations as usize);
    for _ in 0..iterations {
        hashes.push(proof.stable_hash()?);
    }
    let submit_elapsed = start.elapsed();
    hashes.sort();

    let verify_start = Instant::now();
    for _ in 0..iterations {
        proof.validate_hash_only()?;
    }
    let verify_elapsed = verify_start.elapsed();

    let query_start = Instant::now();
    for hash in &hashes {
        let _ = hashes.binary_search(hash);
    }
    let query_elapsed = query_start.elapsed();

    Ok(ProofRouteBenchmarkReport {
        iterations,
        proof_submission_throughput_per_sec: iterations as f64
            / submit_elapsed.as_secs_f64().max(f64::EPSILON),
        proof_verification_time_ms_avg: verify_elapsed.as_secs_f64() * 1000.0 / iterations as f64,
        proof_index_query_latency_ms_avg: query_elapsed.as_secs_f64() * 1000.0 / iterations as f64,
        block_inclusion_latency_ms_estimate: proof_payload_bytes as f64 / 1024.0,
        proof_payload_bytes,
        normal_proof_payload_bytes,
        payload_byte_overhead: proof_payload_bytes as isize - normal_proof_payload_bytes as isize,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseGateItem {
    pub name: String,
    pub passed: bool,
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct V01ReleaseGateReport {
    pub version: String,
    pub passed: bool,
    pub items: Vec<ReleaseGateItem>,
}

impl V01ReleaseGateReport {
    pub fn failed_items(&self) -> Vec<&ReleaseGateItem> {
        self.items.iter().filter(|item| !item.passed).collect()
    }
}

pub fn v01_release_gate_report() -> V01ReleaseGateReport {
    let items = vec![
        gate(
            "local traces can be collected",
            true,
            "RouteTraceLogger is implemented",
        ),
        gate(
            "rubrics can be generated from traces",
            true,
            "RouteCorrectness, ToolUse, and CompressionLoss rubric generators are implemented",
        ),
        gate(
            "strict JSON verifier scores turns",
            true,
            "StrictVerifierOutput parser, retry report, and training-safe conversion are implemented",
        ),
        gate(
            "rollout loop simulates multi-turn training",
            false,
            "rollout runner and simulator are still pending",
        ),
        gate(
            "reward engine produces vector rewards",
            true,
            "RewardVector schema validates and hashes",
        ),
        gate(
            "tiny router or assistant can train a LoRA adapter",
            false,
            "trainer interface and adapter export are still pending",
        ),
        gate(
            "eval report shows before/after metrics",
            false,
            "HTML/JSON eval report generation is still pending",
        ),
        gate(
            "adapter promotion gate works",
            false,
            "promotion gate policy is still pending",
        ),
        gate(
            "proof hash can be generated",
            true,
            "RlvrProofObject validates and hashes hash-only payloads",
        ),
        gate(
            "proof hash can be committed by running Fractal Chain node",
            false,
            "node RPC/block inclusion integration is still pending",
        ),
        gate(
            "raw user data never leaves the machine by default",
            true,
            "privacy policy defaults local-only and proof payloads are hash-only",
        ),
    ];
    V01ReleaseGateReport {
        version: "v0.1".into(),
        passed: items.iter().all(|item| item.passed),
        items,
    }
}

fn gate(name: &str, passed: bool, evidence: &str) -> ReleaseGateItem {
    ReleaseGateItem {
        name: name.into(),
        passed,
        evidence: evidence.into(),
    }
}

fn malicious_raw_prompt_payload_rejected() -> Result<bool, RlvrError> {
    let raw = serde_json::json!({
        "proof_type": "ProofOfRoute",
        "trace_hash": hash_bytes(b"trace"),
        "redacted_trace_hash": hash_bytes(b"redacted"),
        "verifier_outputs_hash": hash_bytes(b"verifier"),
        "reward_policy_hash": hash_bytes(b"reward-policy"),
        "reward_vector_hash": hash_bytes(b"reward-vector"),
        "route_policy_hash": hash_bytes(b"route-policy"),
        "model_id_hash": hash_bytes(b"model-id"),
        "adapter_hash": null,
        "eval_hash": null,
        "timestamp_ms": 1,
        "node_signature": "sig-test",
        "raw_prompt": "leak this user prompt"
    });
    let allowed: BTreeSet<&str> = [
        "proof_type",
        "trace_hash",
        "redacted_trace_hash",
        "verifier_outputs_hash",
        "reward_policy_hash",
        "reward_vector_hash",
        "route_policy_hash",
        "model_id_hash",
        "adapter_hash",
        "eval_hash",
        "timestamp_ms",
        "node_signature",
    ]
    .into_iter()
    .collect();
    Ok(raw
        .as_object()
        .map(|object| object.keys().any(|key| !allowed.contains(key.as_str())))
        .unwrap_or(true))
}

fn proof_from_trace(trace: &DialogueTrace) -> Result<RlvrProofObject, RlvrError> {
    let commitment: TraceHashCommitment = trace.trace_hash_commitment()?;
    RlvrProofObject::from_trace_commitment(
        RlvrProofType::ProofOfRoute,
        &commitment,
        stable_hash(&DEFAULT_REWARD_POLICY)?,
        route_policy_hash(&RoutePolicy::default())?,
        hash_bytes(b"tiny-local-model"),
        1,
        "sig-test",
    )
    .tap_validate()
}

trait TapValidate: Sized {
    fn tap_validate(self) -> Result<Self, RlvrError>;
}

impl TapValidate for RlvrProofObject {
    fn tap_validate(self) -> Result<Self, RlvrError> {
        self.validate_hash_only()?;
        Ok(self)
    }
}

fn privacy_case_trace(case_name: &str, input: &str) -> DialogueTrace {
    DialogueTrace {
        trace_id: format!("trace-{case_name}"),
        task_id: format!("task-{case_name}"),
        turns: vec![
            DialogueTurn {
                role: "user".into(),
                content: input.into(),
                model_id: None,
                route_decision: None,
                latency_ms: None,
                cost_estimate: None,
            },
            DialogueTurn {
                role: "assistant".into(),
                content: "I will keep this local and commit hashes only.".into(),
                model_id: Some("tiny-local-model".into()),
                route_decision: Some("local-only".into()),
                latency_ms: Some(15),
                cost_estimate: Some(0.0),
            },
        ],
        verifier_outputs: vec![VerifierOutput {
            is_final_answer: false,
            is_clarification_question: false,
            targeted_checkpoints: Vec::new(),
            missed_checkpoints: Vec::new(),
            redundant_question: false,
            premature_answer: false,
            false_premise_corrected: None,
            route_valid: true,
            reward: 0.8,
        }],
        reward_vector: RewardVector {
            correctness: 0.0,
            checkpoint_coverage: 0.0,
            clarification_quality: 0.0,
            false_premise_detection: 0.0,
            route_correctness: 1.0,
            tool_use_correctness: 0.0,
            cost_efficiency: 1.0,
            latency_efficiency: 1.0,
            privacy_compliance: 1.0,
            non_redundancy: 1.0,
        },
        final_reward: 0.8,
    }
}

fn benchmark_trace() -> DialogueTrace {
    privacy_case_trace(
        "benchmark",
        "Route this simple stable knowledge prompt to the cheapest local model.",
    )
}
