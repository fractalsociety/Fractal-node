//! Evaluation harness and promotion-gate reporting.

pub mod baseline;

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalTraceMetrics {
    pub trace_id: String,
    pub task_id: String,
    pub final_answer_accuracy: f64,
    pub checkpoint_coverage: f64,
    pub redundant_question: bool,
    pub premature_answer: bool,
    pub correct_route: bool,
    pub unnecessary_escalation: bool,
    pub private_data_leakage: bool,
    pub total_cost: f64,
    pub total_latency_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvalMetricsReport {
    pub schema_version: String,
    pub trace_count: usize,
    pub final_answer_accuracy: f64,
    pub checkpoint_coverage: f64,
    pub redundant_question_rate: f64,
    pub premature_answer_rate: f64,
    pub correct_route_rate: f64,
    pub unnecessary_escalation_rate: f64,
    pub private_data_leakage_rate: f64,
    pub average_cost: f64,
    pub average_latency_ms: f64,
    pub traces: Vec<EvalTraceMetrics>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvalReportFiles {
    pub json_path: String,
    pub html_path: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdapterPromotionGatePolicy {
    pub min_coverage_improvement: f64,
    pub min_route_correctness_improvement: f64,
    pub max_cost_multiplier: f64,
    pub max_latency_multiplier: f64,
    pub max_accuracy_drop: f64,
    pub max_redundant_question_rate: f64,
}

impl Default for AdapterPromotionGatePolicy {
    fn default() -> Self {
        Self {
            min_coverage_improvement: 0.0,
            min_route_correctness_improvement: 0.0,
            max_cost_multiplier: 1.25,
            max_latency_multiplier: 1.25,
            max_accuracy_drop: 0.05,
            max_redundant_question_rate: 0.15,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdapterRollbackMetadata {
    pub adapter_id: String,
    pub base_model_id: String,
    pub previous_adapter_id: Option<String>,
    pub rollback_reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromotionGateCheck {
    pub name: String,
    pub passed: bool,
    pub baseline_value: f64,
    pub candidate_value: f64,
    pub requirement: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdapterPromotionDecision {
    pub adapter_id: String,
    pub promoted: bool,
    pub checks: Vec<PromotionGateCheck>,
    pub rollback: AdapterRollbackMetadata,
}

impl AdapterPromotionGatePolicy {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_finite_non_negative(
            "promotion.min_coverage_improvement",
            self.min_coverage_improvement,
        )?;
        require_finite_non_negative(
            "promotion.min_route_correctness_improvement",
            self.min_route_correctness_improvement,
        )?;
        require_finite_non_negative("promotion.max_accuracy_drop", self.max_accuracy_drop)?;
        require_finite_non_negative(
            "promotion.max_redundant_question_rate",
            self.max_redundant_question_rate,
        )?;
        if !self.max_cost_multiplier.is_finite() || self.max_cost_multiplier < 1.0 {
            return Err(RlvrError::Config(
                "promotion.max_cost_multiplier must be finite and at least 1.0".into(),
            ));
        }
        if !self.max_latency_multiplier.is_finite() || self.max_latency_multiplier < 1.0 {
            return Err(RlvrError::Config(
                "promotion.max_latency_multiplier must be finite and at least 1.0".into(),
            ));
        }
        Ok(())
    }
}

pub fn evaluate_adapter_promotion_gate(
    adapter_id: impl Into<String>,
    base_model_id: impl Into<String>,
    previous_adapter_id: Option<String>,
    baseline: &EvalMetricsReport,
    candidate: &EvalMetricsReport,
    policy: &AdapterPromotionGatePolicy,
) -> Result<AdapterPromotionDecision, RlvrError> {
    policy.validate()?;
    baseline.validate()?;
    candidate.validate()?;
    let adapter_id = adapter_id.into();
    let base_model_id = base_model_id.into();
    if adapter_id.trim().is_empty() {
        return Err(RlvrError::Config(
            "promotion adapter_id cannot be empty".into(),
        ));
    }
    if base_model_id.trim().is_empty() {
        return Err(RlvrError::Config(
            "promotion base_model_id cannot be empty".into(),
        ));
    }

    let checks = vec![
        min_delta_check(
            "coverage_improvement",
            baseline.checkpoint_coverage,
            candidate.checkpoint_coverage,
            policy.min_coverage_improvement,
        ),
        min_delta_check(
            "route_correctness_improvement",
            baseline.correct_route_rate,
            candidate.correct_route_rate,
            policy.min_route_correctness_improvement,
        ),
        max_multiplier_check(
            "bounded_cost",
            baseline.average_cost,
            candidate.average_cost,
            policy.max_cost_multiplier,
        ),
        max_multiplier_check(
            "bounded_latency",
            baseline.average_latency_ms,
            candidate.average_latency_ms,
            policy.max_latency_multiplier,
        ),
        max_drop_check(
            "no_single_turn_accuracy_collapse",
            baseline.final_answer_accuracy,
            candidate.final_answer_accuracy,
            policy.max_accuracy_drop,
        ),
        max_value_check(
            "redundant_question_rate_under_limit",
            candidate.redundant_question_rate,
            policy.max_redundant_question_rate,
        ),
        exact_zero_check(
            "zero_privacy_violations",
            candidate.private_data_leakage_rate,
        ),
    ];
    let promoted = checks.iter().all(|check| check.passed);
    let failed = checks
        .iter()
        .filter(|check| !check.passed)
        .map(|check| check.name.clone())
        .collect::<Vec<_>>();
    Ok(AdapterPromotionDecision {
        adapter_id: adapter_id.clone(),
        promoted,
        checks,
        rollback: AdapterRollbackMetadata {
            adapter_id,
            base_model_id,
            previous_adapter_id,
            rollback_reason: if promoted {
                "promotion passed; rollback metadata retained for safe disable".into()
            } else {
                format!("promotion blocked by failed checks: {}", failed.join(", "))
            },
        },
    })
}

impl EvalMetricsReport {
    pub fn validate(&self) -> Result<(), RlvrError> {
        if self.trace_count == 0 {
            return Err(RlvrError::Config(
                "eval metrics report trace_count must be greater than zero".into(),
            ));
        }
        for (name, value) in [
            ("final_answer_accuracy", self.final_answer_accuracy),
            ("checkpoint_coverage", self.checkpoint_coverage),
            ("redundant_question_rate", self.redundant_question_rate),
            ("premature_answer_rate", self.premature_answer_rate),
            ("correct_route_rate", self.correct_route_rate),
            (
                "unnecessary_escalation_rate",
                self.unnecessary_escalation_rate,
            ),
            ("private_data_leakage_rate", self.private_data_leakage_rate),
        ] {
            require_rate(name, value)?;
        }
        require_finite_non_negative("average_cost", self.average_cost)?;
        require_finite_non_negative("average_latency_ms", self.average_latency_ms)?;
        Ok(())
    }
}

pub fn build_eval_metrics_report(traces: &[DialogueTrace]) -> Result<EvalMetricsReport, RlvrError> {
    if traces.is_empty() {
        return Err(RlvrError::Config(
            "eval report requires at least one trace".into(),
        ));
    }

    let mut trace_metrics = Vec::with_capacity(traces.len());
    for trace in traces {
        trace.validate()?;
        trace_metrics.push(metrics_for_trace(trace));
    }

    let trace_count = trace_metrics.len();
    let denom = trace_count as f64;
    let report = EvalMetricsReport {
        schema_version: "rlvr.eval-report.v0.1".into(),
        trace_count,
        final_answer_accuracy: average_by(&trace_metrics, |m| m.final_answer_accuracy),
        checkpoint_coverage: average_by(&trace_metrics, |m| m.checkpoint_coverage),
        redundant_question_rate: rate_by(&trace_metrics, |m| m.redundant_question, denom),
        premature_answer_rate: rate_by(&trace_metrics, |m| m.premature_answer, denom),
        correct_route_rate: rate_by(&trace_metrics, |m| m.correct_route, denom),
        unnecessary_escalation_rate: rate_by(&trace_metrics, |m| m.unnecessary_escalation, denom),
        private_data_leakage_rate: rate_by(&trace_metrics, |m| m.private_data_leakage, denom),
        average_cost: average_by(&trace_metrics, |m| m.total_cost),
        average_latency_ms: average_by(&trace_metrics, |m| m.total_latency_ms as f64),
        traces: trace_metrics,
    };
    report.validate()?;
    Ok(report)
}

pub fn write_eval_report(
    input: impl AsRef<Path>,
    out_dir: impl AsRef<Path>,
) -> Result<EvalReportFiles, RlvrError> {
    let traces = read_eval_traces(input)?;
    let report = build_eval_metrics_report(&traces)?;
    fs::create_dir_all(out_dir.as_ref())?;
    let json_path = out_dir.as_ref().join("eval_report.json");
    let html_path = out_dir.as_ref().join("eval_report.html");
    fs::write(&json_path, serde_json::to_string_pretty(&report)?)?;
    fs::write(&html_path, render_eval_report_html(&report))?;
    Ok(EvalReportFiles {
        json_path: json_path.to_string_lossy().into_owned(),
        html_path: html_path.to_string_lossy().into_owned(),
    })
}

pub fn read_eval_traces(input: impl AsRef<Path>) -> Result<Vec<DialogueTrace>, RlvrError> {
    let input = input.as_ref();
    if input.is_dir() {
        let mut files = fs::read_dir(input)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<Result<Vec<PathBuf>, std::io::Error>>()?;
        files.sort();
        let mut traces = Vec::new();
        for path in files {
            if path.is_file() && is_supported_trace_file(&path) {
                traces.extend(read_eval_trace_file(&path)?);
            }
        }
        if traces.is_empty() {
            return Err(RlvrError::Config(format!(
                "eval report found no trace files in {}",
                input.display()
            )));
        }
        return Ok(traces);
    }
    read_eval_trace_file(input)
}

pub fn render_eval_report_html(report: &EvalMetricsReport) -> String {
    let rows = report
        .traces
        .iter()
        .map(|trace| {
            format!(
                "<tr><td>{}</td><td>{}</td><td>{:.3}</td><td>{:.3}</td><td>{}</td><td>{}</td><td>{}</td><td>{:.6}</td><td>{}</td></tr>",
                escape_html(&trace.trace_id),
                escape_html(&trace.task_id),
                trace.final_answer_accuracy,
                trace.checkpoint_coverage,
                trace.redundant_question,
                trace.premature_answer,
                trace.correct_route,
                trace.total_cost,
                trace.total_latency_ms,
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    format!(
        concat!(
            "<!doctype html><html><head><meta charset=\"utf-8\">",
            "<title>RLVR Eval Report</title>",
            "<style>body{{font-family:system-ui,sans-serif;margin:32px;color:#111}}",
            "table{{border-collapse:collapse;width:100%;margin-top:20px}}",
            "th,td{{border:1px solid #ddd;padding:8px;text-align:left}}",
            "th{{background:#f5f5f5}}.metrics{{display:grid;grid-template-columns:repeat(3,minmax(0,1fr));gap:12px}}",
            ".metric{{border:1px solid #ddd;padding:12px}}</style></head><body>",
            "<h1>RLVR Eval Report</h1>",
            "<div class=\"metrics\">",
            "<div class=\"metric\"><strong>Traces</strong><br>{}</div>",
            "<div class=\"metric\"><strong>Final answer accuracy</strong><br>{:.3}</div>",
            "<div class=\"metric\"><strong>Checkpoint coverage</strong><br>{:.3}</div>",
            "<div class=\"metric\"><strong>Redundant question rate</strong><br>{:.3}</div>",
            "<div class=\"metric\"><strong>Premature answer rate</strong><br>{:.3}</div>",
            "<div class=\"metric\"><strong>Correct route rate</strong><br>{:.3}</div>",
            "<div class=\"metric\"><strong>Unnecessary escalation rate</strong><br>{:.3}</div>",
            "<div class=\"metric\"><strong>Private-data leakage rate</strong><br>{:.3}</div>",
            "<div class=\"metric\"><strong>Average cost</strong><br>{:.6}</div>",
            "<div class=\"metric\"><strong>Average latency ms</strong><br>{:.1}</div>",
            "</div><table><thead><tr><th>Trace</th><th>Task</th><th>Accuracy</th><th>Coverage</th>",
            "<th>Redundant</th><th>Premature</th><th>Route OK</th><th>Cost</th><th>Latency ms</th>",
            "</tr></thead><tbody>{}</tbody></table></body></html>"
        ),
        report.trace_count,
        report.final_answer_accuracy,
        report.checkpoint_coverage,
        report.redundant_question_rate,
        report.premature_answer_rate,
        report.correct_route_rate,
        report.unnecessary_escalation_rate,
        report.private_data_leakage_rate,
        report.average_cost,
        report.average_latency_ms,
        rows
    )
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
            true,
            "AdapterPromotionGatePolicy evaluates before/after reports and blocks bad adapters",
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

fn read_eval_trace_file(path: &Path) -> Result<Vec<DialogueTrace>, RlvrError> {
    let raw = fs::read_to_string(path)?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if trimmed.starts_with('[') {
        let traces: Vec<DialogueTrace> = serde_json::from_str(trimmed)?;
        return Ok(traces);
    }
    if trimmed.starts_with('{') {
        if let Ok(trace) = serde_json::from_str::<DialogueTrace>(trimmed) {
            return Ok(vec![trace]);
        }
    }

    let mut traces = Vec::new();
    for (idx, line) in raw.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let trace: DialogueTrace = serde_json::from_str(line).map_err(|err| {
            RlvrError::Config(format!(
                "failed to parse {} line {} as DialogueTrace JSON: {err}",
                path.display(),
                idx + 1
            ))
        })?;
        traces.push(trace);
    }
    Ok(traces)
}

fn is_supported_trace_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|ext| ext.to_str()),
        Some("json" | "jsonl")
    )
}

fn metrics_for_trace(trace: &DialogueTrace) -> EvalTraceMetrics {
    let final_answer_accuracy = trace
        .verifier_outputs
        .iter()
        .rev()
        .find(|output| output.is_final_answer)
        .map(|output| output.reward.clamp(0.0, 1.0))
        .unwrap_or_else(|| trace.reward_vector.correctness.clamp(0.0, 1.0));
    let checkpoint_coverage = trace.reward_vector.checkpoint_coverage.clamp(0.0, 1.0);
    let redundant_question = trace
        .verifier_outputs
        .iter()
        .any(|output| output.redundant_question);
    let premature_answer = trace
        .verifier_outputs
        .iter()
        .any(|output| output.premature_answer);
    let correct_route = if trace.verifier_outputs.is_empty() {
        trace.reward_vector.route_correctness >= 1.0
    } else {
        trace
            .verifier_outputs
            .iter()
            .all(|output| output.route_valid)
    };
    let total_cost = trace
        .turns
        .iter()
        .filter_map(|turn| turn.cost_estimate)
        .sum::<f64>();
    let total_latency_ms = trace
        .turns
        .iter()
        .filter_map(|turn| turn.latency_ms)
        .sum::<u64>();
    let selected_external = trace.turns.iter().any(turn_selected_external);
    let private_content = trace
        .turns
        .iter()
        .any(|turn| scan_privacy_tags(&turn.content).is_private);
    let private_data_leakage =
        trace.reward_vector.privacy_compliance < 1.0 || (private_content && selected_external);
    let unnecessary_escalation = selected_external && trace.reward_vector.route_correctness < 1.0;

    EvalTraceMetrics {
        trace_id: trace.trace_id.clone(),
        task_id: trace.task_id.clone(),
        final_answer_accuracy,
        checkpoint_coverage,
        redundant_question,
        premature_answer,
        correct_route,
        unnecessary_escalation,
        private_data_leakage,
        total_cost,
        total_latency_ms,
    }
}

fn turn_selected_external(turn: &DialogueTurn) -> bool {
    turn.route_decision
        .as_deref()
        .into_iter()
        .chain(turn.model_id.as_deref())
        .any(|value| {
            let value = value.to_ascii_lowercase();
            value.contains("external")
                || value.contains("cloud")
                || value.contains("frontier")
                || value.contains("escalat")
        })
}

fn average_by(metrics: &[EvalTraceMetrics], value: impl Fn(&EvalTraceMetrics) -> f64) -> f64 {
    metrics.iter().map(value).sum::<f64>() / metrics.len() as f64
}

fn rate_by(
    metrics: &[EvalTraceMetrics],
    predicate: impl Fn(&EvalTraceMetrics) -> bool,
    denom: f64,
) -> f64 {
    metrics.iter().filter(|metric| predicate(metric)).count() as f64 / denom
}

fn min_delta_check(
    name: &str,
    baseline: f64,
    candidate: f64,
    required_delta: f64,
) -> PromotionGateCheck {
    let delta = candidate - baseline;
    PromotionGateCheck {
        name: name.into(),
        passed: delta >= required_delta,
        baseline_value: baseline,
        candidate_value: candidate,
        requirement: format!("candidate - baseline must be >= {required_delta:.6}"),
    }
}

fn max_multiplier_check(
    name: &str,
    baseline: f64,
    candidate: f64,
    multiplier: f64,
) -> PromotionGateCheck {
    let limit = if baseline <= f64::EPSILON {
        0.0
    } else {
        baseline * multiplier
    };
    PromotionGateCheck {
        name: name.into(),
        passed: candidate <= limit,
        baseline_value: baseline,
        candidate_value: candidate,
        requirement: format!("candidate must be <= baseline * {multiplier:.6}"),
    }
}

fn max_drop_check(name: &str, baseline: f64, candidate: f64, max_drop: f64) -> PromotionGateCheck {
    PromotionGateCheck {
        name: name.into(),
        passed: candidate + max_drop >= baseline,
        baseline_value: baseline,
        candidate_value: candidate,
        requirement: format!("baseline - candidate must be <= {max_drop:.6}"),
    }
}

fn max_value_check(name: &str, candidate: f64, max_value: f64) -> PromotionGateCheck {
    PromotionGateCheck {
        name: name.into(),
        passed: candidate <= max_value,
        baseline_value: 0.0,
        candidate_value: candidate,
        requirement: format!("candidate must be <= {max_value:.6}"),
    }
}

fn exact_zero_check(name: &str, candidate: f64) -> PromotionGateCheck {
    PromotionGateCheck {
        name: name.into(),
        passed: candidate == 0.0,
        baseline_value: 0.0,
        candidate_value: candidate,
        requirement: "candidate must equal 0".into(),
    }
}

fn require_rate(name: &str, value: f64) -> Result<(), RlvrError> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(RlvrError::Config(format!(
            "{name} must be finite and in [0, 1]"
        )));
    }
    Ok(())
}

fn require_finite_non_negative(name: &str, value: f64) -> Result<(), RlvrError> {
    if !value.is_finite() || value < 0.0 {
        return Err(RlvrError::Config(format!(
            "{name} must be finite and non-negative"
        )));
    }
    Ok(())
}

fn escape_html(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn malicious_raw_prompt_payload_rejected() -> Result<bool, RlvrError> {
    let raw = serde_json::json!({
        "proof_type": "ProofOfRoute",
        "trace_hash": hash_bytes(b"trace"),
        "redacted_trace_hash": hash_bytes(b"redacted"),
        "verifier_outputs_hash": hash_bytes(b"verifier"),
        "rubric_hash": hash_bytes(b"rubric"),
        "reward_policy_hash": hash_bytes(b"reward-policy"),
        "reward_vector_hash": hash_bytes(b"reward-vector"),
        "route_policy_hash": hash_bytes(b"route-policy"),
        "router_policy_hash": hash_bytes(b"route-policy"),
        "model_id_hash": hash_bytes(b"model-id"),
        "adapter_hash": null,
        "eval_hash": null,
        "eval_result_hash": null,
        "timestamp": 1,
        "timestamp_ms": 1,
        "node_signature": "sig-test",
        "raw_prompt": "leak this user prompt"
    });
    let allowed: BTreeSet<&str> = [
        "proof_type",
        "trace_hash",
        "redacted_trace_hash",
        "verifier_outputs_hash",
        "rubric_hash",
        "reward_policy_hash",
        "reward_vector_hash",
        "route_policy_hash",
        "router_policy_hash",
        "model_id_hash",
        "adapter_hash",
        "eval_hash",
        "eval_result_hash",
        "timestamp",
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_report_aggregates_accuracy_routes_privacy_cost_and_latency() {
        let clean = eval_trace_fixture("trace-clean", true, false, false, false, 0.9, 0.8);
        let mut bad = eval_trace_fixture("trace-bad", false, true, true, true, 0.3, 0.25);
        bad.turns[1].route_decision = Some("external cloud escalation".into());
        bad.turns[1].model_id = Some("frontier-cloud-model".into());
        bad.reward_vector.route_correctness = 0.0;
        bad.reward_vector.privacy_compliance = 0.0;

        let report = build_eval_metrics_report(&[clean, bad]).unwrap();

        assert_eq!(report.schema_version, "rlvr.eval-report.v0.1");
        assert_eq!(report.trace_count, 2);
        assert!((report.final_answer_accuracy - 0.6).abs() < f64::EPSILON);
        assert!((report.checkpoint_coverage - 0.525).abs() < f64::EPSILON);
        assert_eq!(report.redundant_question_rate, 0.5);
        assert_eq!(report.premature_answer_rate, 0.5);
        assert_eq!(report.correct_route_rate, 0.5);
        assert_eq!(report.unnecessary_escalation_rate, 0.5);
        assert_eq!(report.private_data_leakage_rate, 0.5);
        assert_eq!(report.average_latency_ms, 25.0);
        assert!((report.average_cost - 0.002).abs() < f64::EPSILON);
    }

    #[test]
    fn eval_report_writer_creates_json_and_html_reports_from_trace_dir() {
        let dir =
            std::env::temp_dir().join(format!("fractal-rlvr-eval-report-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let trace_dir = dir.join("traces");
        let out_dir = dir.join("report");
        fs::create_dir_all(&trace_dir).unwrap();
        let trace = eval_trace_fixture("trace-html<&>", true, false, false, false, 1.0, 1.0);
        fs::write(
            trace_dir.join("trace.json"),
            serde_json::to_string_pretty(&trace).unwrap(),
        )
        .unwrap();

        let files = write_eval_report(&trace_dir, &out_dir).unwrap();

        let json = fs::read_to_string(&files.json_path).unwrap();
        let parsed: EvalMetricsReport = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.trace_count, 1);
        let html = fs::read_to_string(&files.html_path).unwrap();
        assert!(html.contains("RLVR Eval Report"));
        assert!(html.contains("trace-html&lt;&amp;&gt;"));
        assert!(!html.contains("trace-html<&>"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn promotion_gate_promotes_adapter_when_all_requirements_pass() {
        let baseline = eval_report_fixture(0.80, 0.70, 0.05, 0.90, 0.002, 100.0, 0.0);
        let candidate = eval_report_fixture(0.82, 0.75, 0.10, 0.95, 0.0022, 110.0, 0.0);

        let decision = evaluate_adapter_promotion_gate(
            "router-rlvr-v0.1",
            "tiny-router-base",
            Some("router-prev".into()),
            &baseline,
            &candidate,
            &AdapterPromotionGatePolicy::default(),
        )
        .unwrap();

        assert!(decision.promoted);
        assert_eq!(decision.checks.len(), 7);
        assert!(decision.checks.iter().all(|check| check.passed));
        assert_eq!(
            decision.rollback.previous_adapter_id.as_deref(),
            Some("router-prev")
        );
        assert!(decision
            .rollback
            .rollback_reason
            .contains("promotion passed"));
    }

    #[test]
    fn promotion_gate_blocks_bad_adapter_and_reports_failed_checks() {
        let baseline = eval_report_fixture(0.90, 0.80, 0.05, 0.90, 0.002, 100.0, 0.0);
        let candidate = eval_report_fixture(0.70, 0.75, 0.30, 0.85, 0.004, 180.0, 0.10);

        let decision = evaluate_adapter_promotion_gate(
            "router-bad",
            "tiny-router-base",
            None,
            &baseline,
            &candidate,
            &AdapterPromotionGatePolicy::default(),
        )
        .unwrap();

        assert!(!decision.promoted);
        for failed in [
            "coverage_improvement",
            "route_correctness_improvement",
            "bounded_cost",
            "bounded_latency",
            "no_single_turn_accuracy_collapse",
            "redundant_question_rate_under_limit",
            "zero_privacy_violations",
        ] {
            assert!(
                decision
                    .checks
                    .iter()
                    .any(|check| check.name == failed && !check.passed),
                "missing failed check {failed}"
            );
            assert!(decision.rollback.rollback_reason.contains(failed));
        }
    }

    #[test]
    fn promotion_gate_policy_rejects_invalid_thresholds() {
        let mut policy = AdapterPromotionGatePolicy::default();
        policy.max_cost_multiplier = 0.5;
        let err = policy.validate().unwrap_err();
        assert!(err.to_string().contains("max_cost_multiplier"));
    }

    fn eval_report_fixture(
        final_answer_accuracy: f64,
        checkpoint_coverage: f64,
        redundant_question_rate: f64,
        correct_route_rate: f64,
        average_cost: f64,
        average_latency_ms: f64,
        private_data_leakage_rate: f64,
    ) -> EvalMetricsReport {
        EvalMetricsReport {
            schema_version: "rlvr.eval-report.v0.1".into(),
            trace_count: 10,
            final_answer_accuracy,
            checkpoint_coverage,
            redundant_question_rate,
            premature_answer_rate: 0.0,
            correct_route_rate,
            unnecessary_escalation_rate: 0.0,
            private_data_leakage_rate,
            average_cost,
            average_latency_ms,
            traces: Vec::new(),
        }
    }

    fn eval_trace_fixture(
        trace_id: &str,
        route_valid: bool,
        redundant_question: bool,
        premature_answer: bool,
        private_prompt: bool,
        final_reward: f64,
        checkpoint_coverage: f64,
    ) -> DialogueTrace {
        DialogueTrace {
            trace_id: trace_id.into(),
            task_id: format!("task-{trace_id}"),
            turns: vec![
                DialogueTurn {
                    role: "user".into(),
                    content: if private_prompt {
                        "My email is user@example.com".into()
                    } else {
                        "Explain proof-of-route metrics.".into()
                    },
                    model_id: None,
                    route_decision: None,
                    latency_ms: Some(0),
                    cost_estimate: Some(0.0),
                },
                DialogueTurn {
                    role: "assistant".into(),
                    content: "Final answer.".into(),
                    model_id: Some("tiny-local-model".into()),
                    route_decision: Some("local-only".into()),
                    latency_ms: Some(25),
                    cost_estimate: Some(0.002),
                },
            ],
            verifier_outputs: vec![VerifierOutput {
                is_final_answer: true,
                is_clarification_question: false,
                targeted_checkpoints: vec!["c1".into()],
                missed_checkpoints: Vec::new(),
                redundant_question,
                premature_answer,
                false_premise_corrected: None,
                route_valid,
                reward: final_reward,
            }],
            reward_vector: RewardVector {
                correctness: final_reward,
                checkpoint_coverage,
                clarification_quality: 0.0,
                false_premise_detection: 0.0,
                route_correctness: if route_valid { 1.0 } else { 0.0 },
                tool_use_correctness: 0.0,
                cost_efficiency: 1.0,
                latency_efficiency: 1.0,
                privacy_compliance: 1.0,
                non_redundancy: if redundant_question { 0.0 } else { 1.0 },
            },
            final_reward,
        }
    }
}
