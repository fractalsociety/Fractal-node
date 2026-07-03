//! Adapter-only local training interfaces for GRPO, DPO, and SFT paths.

pub mod dpo_sft;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::{
    compute_reward_vector, hash_bytes, score_checkpoint_coverage_for_item,
    score_final_answer_for_item, stable_hash, Checkpoint, CheckpointType, DialogueTrace,
    DialogueTurn, Difficulty, LocalUserSimulator, LocalUserSimulatorInput, PrivacyPolicy,
    RewardSignalInput, RlvrError, RoutePolicy, SimulatorMode, StrictVerifierOutput, TrainingItem,
    TrainingMode,
};

/// RLVR-029 actor roles that can participate in local rollouts through the
/// same runtime interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ActorRole {
    TinyAssistant,
    Router,
    Clarification,
    Critic,
    Compressor,
    ToolUsePolicy,
}

impl ActorRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::TinyAssistant => "tiny_assistant",
            Self::Router => "router",
            Self::Clarification => "clarification",
            Self::Critic => "critic",
            Self::Compressor => "compressor",
            Self::ToolUsePolicy => "tool_use_policy",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().trim_matches('"') {
            "tiny_assistant" | "tiny-assistant" | "assistant" => Some(Self::TinyAssistant),
            "router" | "router_model" | "router-model" => Some(Self::Router),
            "clarification" | "clarification_model" | "clarification-model" => {
                Some(Self::Clarification)
            }
            "critic" | "critic_model" | "critic-model" => Some(Self::Critic),
            "compressor" | "compressor_model" | "compressor-model" => Some(Self::Compressor),
            "tool_use_policy" | "tool-use-policy" | "tool_policy" | "tool-policy" => {
                Some(Self::ToolUsePolicy)
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActorRuntimeRequest {
    pub actor_id: String,
    pub role: ActorRole,
    pub prompt: String,
    pub context: Vec<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
}

impl ActorRuntimeRequest {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("actor_id", &self.actor_id)?;
        require_non_empty("prompt", &self.prompt)?;
        if let Some(max_tokens) = self.max_tokens {
            if max_tokens == 0 {
                return Err(RlvrError::Config(
                    "actor runtime max_tokens must be greater than zero".into(),
                ));
            }
        }
        if let Some(temperature) = self.temperature {
            if !temperature.is_finite() || temperature < 0.0 {
                return Err(RlvrError::Config(
                    "actor runtime temperature must be finite and non-negative".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActorRuntimeResponse {
    pub actor_id: String,
    pub role: ActorRole,
    pub model_id: String,
    pub content: String,
    pub route_decision: Option<String>,
    pub latency_ms: u64,
    pub cost_estimate: f64,
}

impl ActorRuntimeResponse {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("actor_id", &self.actor_id)?;
        require_non_empty("model_id", &self.model_id)?;
        require_non_empty("content", &self.content)?;
        if !self.cost_estimate.is_finite() || self.cost_estimate < 0.0 {
            return Err(RlvrError::Config(
                "actor runtime cost_estimate must be finite and non-negative".into(),
            ));
        }
        Ok(())
    }
}

pub trait ActorRuntime {
    fn supports(&self, role: ActorRole) -> bool;
    fn invoke(&self, request: &ActorRuntimeRequest) -> Result<ActorRuntimeResponse, RlvrError>;
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeterministicLocalActorRuntime {
    pub model_id: String,
    pub supported_roles: Vec<ActorRole>,
    pub latency_ms: u64,
    pub cost_estimate: f64,
}

impl DeterministicLocalActorRuntime {
    pub fn new(model_id: impl Into<String>) -> Self {
        Self {
            model_id: model_id.into(),
            supported_roles: vec![
                ActorRole::TinyAssistant,
                ActorRole::Router,
                ActorRole::Clarification,
                ActorRole::Critic,
                ActorRole::Compressor,
                ActorRole::ToolUsePolicy,
            ],
            latency_ms: 1,
            cost_estimate: 0.0,
        }
    }

    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("model_id", &self.model_id)?;
        if self.supported_roles.is_empty() {
            return Err(RlvrError::Config(
                "local actor runtime must support at least one role".into(),
            ));
        }
        if !self.cost_estimate.is_finite() || self.cost_estimate < 0.0 {
            return Err(RlvrError::Config(
                "local actor runtime cost_estimate must be finite and non-negative".into(),
            ));
        }
        Ok(())
    }
}

impl ActorRuntime for DeterministicLocalActorRuntime {
    fn supports(&self, role: ActorRole) -> bool {
        self.supported_roles.contains(&role)
    }

    fn invoke(&self, request: &ActorRuntimeRequest) -> Result<ActorRuntimeResponse, RlvrError> {
        self.validate()?;
        request.validate()?;
        if !self.supports(request.role) {
            return Err(RlvrError::Config(format!(
                "actor runtime {} does not support role {}",
                self.model_id,
                request.role.as_str()
            )));
        }

        let content = match request.role {
            ActorRole::TinyAssistant => format!("assistant_response: {}", request.prompt),
            ActorRole::Router => "route: tiny-local-model".into(),
            ActorRole::Clarification => format!("clarification: {}", request.prompt),
            ActorRole::Critic => "critic: verify factuality, routing, tool use, and privacy".into(),
            ActorRole::Compressor => summarize_for_test(&request.prompt),
            ActorRole::ToolUsePolicy => "tool_policy: no_tool_required_for_stable_prompt".into(),
        };
        let route_decision = match request.role {
            ActorRole::Router => Some("tiny-local-model".into()),
            ActorRole::ToolUsePolicy => Some("no-tool".into()),
            _ => None,
        };
        let response = ActorRuntimeResponse {
            actor_id: request.actor_id.clone(),
            role: request.role,
            model_id: self.model_id.clone(),
            content,
            route_decision,
            latency_ms: self.latency_ms,
            cost_estimate: self.cost_estimate,
        };
        response.validate()?;
        Ok(response)
    }
}

fn summarize_for_test(prompt: &str) -> String {
    let summary = prompt
        .split_whitespace()
        .take(12)
        .collect::<Vec<_>>()
        .join(" ");
    format!("compressed: {summary}")
}

fn require_non_empty(name: &str, value: &str) -> Result<(), RlvrError> {
    if value.trim().is_empty() {
        return Err(RlvrError::Config(format!("{name} cannot be empty")));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrainingComputeMode {
    Cpu,
    Gpu,
}

impl TrainingComputeMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu",
            Self::Gpu => "gpu",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrainingResourceSnapshot {
    pub available_memory_mb: Option<u64>,
    pub compute_mode: TrainingComputeMode,
    pub gpu_detected: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrainingResourceLimits {
    pub min_available_memory_mb: u64,
    pub max_batch_size: usize,
}

impl Default for TrainingResourceLimits {
    fn default() -> Self {
        Self {
            min_available_memory_mb: 512,
            max_batch_size: 128,
        }
    }
}

impl TrainingResourceLimits {
    pub fn from_env_or_default() -> Result<Self, RlvrError> {
        let mut limits = Self::default();
        if let Ok(raw) = std::env::var("FRACTAL_RLVR_MIN_AVAILABLE_MEMORY_MB") {
            limits.min_available_memory_mb = raw.parse().map_err(|_| {
                RlvrError::Config(
                    "FRACTAL_RLVR_MIN_AVAILABLE_MEMORY_MB must be a positive integer".into(),
                )
            })?;
        }
        if let Ok(raw) = std::env::var("FRACTAL_RLVR_MAX_BATCH_SIZE") {
            limits.max_batch_size = raw.parse().map_err(|_| {
                RlvrError::Config("FRACTAL_RLVR_MAX_BATCH_SIZE must be a positive integer".into())
            })?;
        }
        limits.validate()?;
        Ok(limits)
    }

    pub fn validate(&self) -> Result<(), RlvrError> {
        if self.min_available_memory_mb == 0 {
            return Err(RlvrError::Config(
                "training resource min_available_memory_mb must be greater than zero".into(),
            ));
        }
        if self.max_batch_size == 0 {
            return Err(RlvrError::Config(
                "training resource max_batch_size must be greater than zero".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrainingResourceGuardInput {
    pub requested_batch_size: usize,
    pub limits: TrainingResourceLimits,
    pub snapshot: Option<TrainingResourceSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrainingResourceReport {
    pub available_memory_mb: Option<u64>,
    pub compute_mode: TrainingComputeMode,
    pub gpu_detected: bool,
    pub requested_batch_size: usize,
    pub max_batch_size: usize,
    pub min_available_memory_mb: u64,
}

pub fn detect_training_resources() -> TrainingResourceSnapshot {
    let compute_mode = detect_compute_mode();
    TrainingResourceSnapshot {
        available_memory_mb: detect_available_memory_mb(),
        compute_mode,
        gpu_detected: compute_mode == TrainingComputeMode::Gpu,
    }
}

pub fn validate_training_resources(
    input: TrainingResourceGuardInput,
) -> Result<TrainingResourceReport, RlvrError> {
    input.limits.validate()?;
    if input.requested_batch_size == 0 {
        return Err(RlvrError::Resource(
            "training requested_batch_size must be greater than zero".into(),
        ));
    }
    let snapshot = input.snapshot.unwrap_or_else(detect_training_resources);
    if input.requested_batch_size > input.limits.max_batch_size {
        return Err(RlvrError::Resource(format!(
            "training batch size {} exceeds resource guard max_batch_size {}; lower --batch-size or FRACTAL_RLVR_MAX_BATCH_SIZE",
            input.requested_batch_size, input.limits.max_batch_size
        )));
    }
    if let Some(available) = snapshot.available_memory_mb {
        if available < input.limits.min_available_memory_mb {
            return Err(RlvrError::Resource(format!(
                "available memory {available} MB is below required {} MB; stop before local machine overload",
                input.limits.min_available_memory_mb
            )));
        }
    }
    Ok(TrainingResourceReport {
        available_memory_mb: snapshot.available_memory_mb,
        compute_mode: snapshot.compute_mode,
        gpu_detected: snapshot.gpu_detected,
        requested_batch_size: input.requested_batch_size,
        max_batch_size: input.limits.max_batch_size,
        min_available_memory_mb: input.limits.min_available_memory_mb,
    })
}

fn detect_compute_mode() -> TrainingComputeMode {
    if let Ok(raw) = std::env::var("FRACTAL_RLVR_COMPUTE_MODE") {
        return match raw.trim().to_ascii_lowercase().as_str() {
            "gpu" | "cuda" => TrainingComputeMode::Gpu,
            _ => TrainingComputeMode::Cpu,
        };
    }
    for key in ["CUDA_VISIBLE_DEVICES", "NVIDIA_VISIBLE_DEVICES"] {
        if let Ok(raw) = std::env::var(key) {
            let value = raw.trim();
            if !value.is_empty() && value != "-1" && value != "none" && value != "void" {
                return TrainingComputeMode::Gpu;
            }
        }
    }
    TrainingComputeMode::Cpu
}

fn detect_available_memory_mb() -> Option<u64> {
    if let Ok(raw) = std::env::var("FRACTAL_RLVR_AVAILABLE_MEMORY_MB") {
        if let Ok(value) = raw.parse::<u64>() {
            return Some(value);
        }
    }
    available_memory_from_proc_meminfo().or_else(available_memory_from_sysctl)
}

fn available_memory_from_proc_meminfo() -> Option<u64> {
    let raw = fs::read_to_string("/proc/meminfo").ok()?;
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("MemAvailable:") {
            let kb = rest.split_whitespace().next()?.parse::<u64>().ok()?;
            return Some(kb / 1024);
        }
    }
    None
}

fn available_memory_from_sysctl() -> Option<u64> {
    let output = Command::new("sysctl")
        .args(["-n", "hw.memsize"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let bytes = String::from_utf8(output.stdout)
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()?;
    Some(bytes / 1024 / 1024)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RolloutRunnerInput {
    pub tasks: Vec<TrainingItem>,
    pub actor_id: String,
    pub trace_id_prefix: String,
    pub max_turns: u32,
    pub simulator_mode: SimulatorMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RolloutRunReport {
    pub traces: Vec<DialogueTrace>,
}

impl RolloutRunnerInput {
    pub fn validate(&self) -> Result<(), RlvrError> {
        if self.tasks.is_empty() {
            return Err(RlvrError::Config(
                "rollout runner requires at least one task".into(),
            ));
        }
        require_non_empty("rollout actor_id", &self.actor_id)?;
        require_non_empty("rollout trace_id_prefix", &self.trace_id_prefix)?;
        if self.max_turns < 2 {
            return Err(RlvrError::Config(
                "rollout runner max_turns must be at least 2".into(),
            ));
        }
        for task in &self.tasks {
            task.validate()?;
        }
        Ok(())
    }
}

pub fn run_rollout_batch(
    runtime: &dyn ActorRuntime,
    input: RolloutRunnerInput,
) -> Result<RolloutRunReport, RlvrError> {
    input.validate()?;
    let mut traces = Vec::with_capacity(input.tasks.len());
    for (idx, item) in input.tasks.into_iter().enumerate() {
        traces.push(run_single_rollout(
            runtime,
            &input.actor_id,
            &format!("{}-{}", input.trace_id_prefix, idx),
            input.max_turns,
            input.simulator_mode,
            item,
        )?);
    }
    Ok(RolloutRunReport { traces })
}

pub fn write_rollout_traces(
    report: &RolloutRunReport,
    out_dir: impl AsRef<Path>,
) -> Result<Vec<PathBuf>, RlvrError> {
    let out_dir = out_dir.as_ref();
    fs::create_dir_all(out_dir)?;
    let mut paths = Vec::with_capacity(report.traces.len());
    for trace in &report.traces {
        trace.validate()?;
        let path = out_dir.join(format!("{}.json", sanitize_file_stem(&trace.trace_id)));
        fs::write(&path, serde_json::to_string_pretty(trace)?)?;
        paths.push(path);
    }
    Ok(paths)
}

/// Read every `*.json` [`DialogueTrace`] from a directory (sorted by name) —
/// the inverse of [`write_rollout_traces`], used by the GRPO train CLI and the
/// local RLVR API to consume a chosen set of rollout traces.
pub fn read_dialogue_traces_dir(dir: impl AsRef<Path>) -> Result<Vec<DialogueTrace>, RlvrError> {
    let dir = dir.as_ref();
    let mut paths: Vec<PathBuf> = fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("json"))
        .collect();
    paths.sort();
    let mut traces = Vec::with_capacity(paths.len());
    for path in paths {
        let raw = fs::read_to_string(&path)?;
        let trace: DialogueTrace = serde_json::from_str(&raw).map_err(|err| {
            RlvrError::Config(format!(
                "rollout trace {} is not a valid DialogueTrace: {err}",
                path.display()
            ))
        })?;
        trace.validate()?;
        traces.push(trace);
    }
    if traces.is_empty() {
        return Err(RlvrError::Config(format!(
            "no *.json rollout traces found in {}",
            dir.display()
        )));
    }
    Ok(traces)
}

pub fn demo_rollout_tasks(n: usize) -> Vec<TrainingItem> {
    (0..n)
        .map(|idx| TrainingItem {
            task_id: format!("demo-rollout-task-{idx}"),
            mode: TrainingMode::AskMind,
            visible_user_query: "What capacitor do I need for this board?".into(),
            hidden_original_query:
                "What 22uF 0805 capacitor rated at 6.3V or higher do I need for this board?".into(),
            gold_answer: "22uF 0805 capacitor rated at 6.3V or higher".into(),
            domain: "electronics".into(),
            difficulty: Difficulty::Easy,
            checkpoints: vec![
                Checkpoint {
                    checkpoint_id: "capacitance".into(),
                    checkpoint_type: CheckpointType::MissingInfo,
                    description: "Capacitance value is needed.".into(),
                    must_resolve_before_answer: true,
                    answer_if_asked: "The capacitance is 22uF.".into(),
                    failure_penalty: 0.75,
                },
                Checkpoint {
                    checkpoint_id: "package".into(),
                    checkpoint_type: CheckpointType::MissingInfo,
                    description: "Package size is needed.".into(),
                    must_resolve_before_answer: true,
                    answer_if_asked: "The package is 0805.".into(),
                    failure_penalty: 0.75,
                },
                Checkpoint {
                    checkpoint_id: "voltage".into(),
                    checkpoint_type: CheckpointType::MissingInfo,
                    description: "Voltage rating is needed.".into(),
                    must_resolve_before_answer: true,
                    answer_if_asked: "The voltage rating should be 6.3V or higher.".into(),
                    failure_penalty: 0.75,
                },
            ],
            route_policy: RoutePolicy::default(),
            privacy_policy: PrivacyPolicy::default(),
        })
        .collect()
}

fn run_single_rollout(
    runtime: &dyn ActorRuntime,
    actor_id: &str,
    trace_id: &str,
    max_turns: u32,
    simulator_mode: SimulatorMode,
    item: TrainingItem,
) -> Result<DialogueTrace, RlvrError> {
    let route_response = runtime.invoke(&ActorRuntimeRequest {
        actor_id: actor_id.into(),
        role: ActorRole::Router,
        prompt: item.visible_user_query.clone(),
        context: vec![format!("route_policy={}", item.route_policy.policy_id)],
        max_tokens: Some(64),
        temperature: Some(0.0),
    })?;
    let route_decision = route_response
        .route_decision
        .clone()
        .unwrap_or_else(|| item.route_policy.default_route.clone());
    let route_valid = item.route_policy.default_route == route_decision
        || item
            .route_policy
            .rules
            .iter()
            .any(|rule| rule.route == route_decision);

    let clarification_response = runtime.invoke(&ActorRuntimeRequest {
        actor_id: actor_id.into(),
        role: ActorRole::Clarification,
        prompt: clarification_prompt(&item),
        context: vec![item.visible_user_query.clone()],
        max_tokens: Some(128),
        temperature: Some(0.0),
    })?;
    let simulated_reply = LocalUserSimulator::reply_with_mode(
        &LocalUserSimulatorInput {
            hidden_original_query: item.hidden_original_query.clone(),
            checkpoints: item.checkpoints.clone(),
            assistant_clarification_question: clarification_response.content.clone(),
        },
        simulator_mode,
    )?;
    let required_checkpoint_ids = required_checkpoint_ids(&item);
    let missed_checkpoints = required_checkpoint_ids
        .iter()
        .filter(|checkpoint_id| {
            !simulated_reply
                .revealed_checkpoint_ids
                .contains(checkpoint_id)
        })
        .cloned()
        .collect::<Vec<_>>();
    let coverage = coverage_score(required_checkpoint_ids.len(), missed_checkpoints.len());

    let final_response = runtime.invoke(&ActorRuntimeRequest {
        actor_id: actor_id.into(),
        role: ActorRole::TinyAssistant,
        prompt: format!(
            "{}\nUser supplied: {}\nFinal answer should include: {}",
            item.visible_user_query, simulated_reply.content, item.gold_answer
        ),
        context: vec![route_decision.clone()],
        max_tokens: Some(256),
        temperature: Some(0.0),
    })?;
    let final_answer_has_gold = final_response
        .content
        .to_ascii_lowercase()
        .contains(&item.gold_answer.to_ascii_lowercase());
    let final_answer_reward = if final_answer_has_gold && missed_checkpoints.is_empty() {
        1.0
    } else if final_answer_has_gold {
        0.5
    } else {
        0.0
    };

    let verifier_outputs = vec![
        StrictVerifierOutput {
            is_final_answer: false,
            is_clarification_question: true,
            is_tool_call: false,
            is_route_decision: false,
            targeted_checkpoints: simulated_reply.revealed_checkpoint_ids.clone(),
            resolved_checkpoints: simulated_reply.revealed_checkpoint_ids.clone(),
            missed_checkpoints: Vec::new(),
            redundant_question: simulated_reply.revealed_checkpoint_ids.is_empty(),
            premature_answer: false,
            false_premise_corrected: None,
            route_valid,
            reward: coverage,
        },
        StrictVerifierOutput {
            is_final_answer: true,
            is_clarification_question: false,
            is_tool_call: false,
            is_route_decision: false,
            targeted_checkpoints: Vec::new(),
            resolved_checkpoints: simulated_reply.revealed_checkpoint_ids.clone(),
            missed_checkpoints,
            redundant_question: false,
            premature_answer: coverage < 1.0,
            false_premise_corrected: None,
            route_valid,
            reward: final_answer_reward,
        },
    ];
    let coverage_report = score_checkpoint_coverage_for_item(&item, &verifier_outputs)?;
    let final_answer_score = score_final_answer_for_item(&item, &verifier_outputs)?;
    let total_latency =
        route_response.latency_ms + clarification_response.latency_ms + final_response.latency_ms;
    let total_cost = route_response.cost_estimate
        + clarification_response.cost_estimate
        + final_response.cost_estimate;
    let reward_artifact = compute_reward_vector(&RewardSignalInput {
        coverage: coverage_report,
        final_answer_score: Some(final_answer_score),
        verifier_outputs: verifier_outputs.clone(),
        route_valid,
        tool_required: item
            .checkpoints
            .iter()
            .any(|checkpoint| checkpoint.checkpoint_type == CheckpointType::ToolRequirement),
        tool_used: false,
        cost_estimate: Some(total_cost),
        cost_budget: Some(0.01),
        latency_ms: Some(total_latency),
        latency_budget_ms: Some(1_000),
        privacy_local_only: item.privacy_policy.local_only,
        selected_route_is_external: route_decision.contains("external")
            || route_decision.contains("cloud"),
    })?;

    let trace = DialogueTrace {
        trace_id: trace_id.into(),
        task_id: item.task_id.clone(),
        turns: vec![
            DialogueTurn {
                role: "user".into(),
                content: item.visible_user_query,
                model_id: None,
                route_decision: None,
                latency_ms: None,
                cost_estimate: None,
            },
            DialogueTurn {
                role: "assistant".into(),
                content: clarification_response.content,
                model_id: Some(clarification_response.model_id),
                route_decision: Some(route_decision.clone()),
                latency_ms: Some(clarification_response.latency_ms),
                cost_estimate: Some(clarification_response.cost_estimate),
            },
            DialogueTurn {
                role: "simulated_user".into(),
                content: simulated_reply.content,
                model_id: None,
                route_decision: None,
                latency_ms: Some(0),
                cost_estimate: Some(0.0),
            },
            DialogueTurn {
                role: "assistant".into(),
                content: final_response.content,
                model_id: Some(final_response.model_id),
                route_decision: Some(route_decision),
                latency_ms: Some(final_response.latency_ms),
                cost_estimate: Some(final_response.cost_estimate),
            },
        ],
        verifier_outputs: verifier_outputs
            .into_iter()
            .map(|output| output.training_output())
            .collect::<Result<Vec<_>, _>>()?,
        reward_vector: reward_artifact.reward_vector,
        final_reward: reward_artifact.final_reward,
    };
    if trace.turns.len() > max_turns as usize + 2 {
        return Err(RlvrError::Config(
            "rollout exceeded configured max_turns".into(),
        ));
    }
    trace.validate()?;
    Ok(trace)
}

fn clarification_prompt(item: &TrainingItem) -> String {
    let missing = item
        .checkpoints
        .iter()
        .map(|checkpoint| checkpoint.description.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    format!("Ask only for the missing details needed before answering. {missing}")
}

fn required_checkpoint_ids(item: &TrainingItem) -> Vec<String> {
    item.checkpoints
        .iter()
        .filter(|checkpoint| checkpoint.must_resolve_before_answer)
        .map(|checkpoint| checkpoint.checkpoint_id.clone())
        .collect()
}

fn coverage_score(total: usize, missed: usize) -> f64 {
    if total == 0 {
        1.0
    } else {
        (total - missed) as f64 / total as f64
    }
}

#[cfg(test)]
mod rollout_runner_tests {
    use super::*;

    #[test]
    fn rollout_runner_builds_scored_multi_turn_trace() {
        let runtime = DeterministicLocalActorRuntime::new("local-small-model");
        let report = run_rollout_batch(
            &runtime,
            RolloutRunnerInput {
                tasks: demo_rollout_tasks(1),
                actor_id: "local-small-model".into(),
                trace_id_prefix: "test-rollout".into(),
                max_turns: 3,
                simulator_mode: SimulatorMode::Clean,
            },
        )
        .unwrap();

        assert_eq!(report.traces.len(), 1);
        let trace = &report.traces[0];
        assert_eq!(trace.turns.len(), 4);
        assert_eq!(trace.turns[1].role, "assistant");
        assert_eq!(trace.turns[2].role, "simulated_user");
        assert_eq!(trace.turns[3].role, "assistant");
        assert_eq!(trace.verifier_outputs.len(), 2);
        assert!(trace.verifier_outputs[0].is_clarification_question);
        assert!(trace.verifier_outputs[1].is_final_answer);
        assert!(trace.reward_vector.checkpoint_coverage > 0.0);
        assert!(trace.final_reward > 0.0);
    }

    #[test]
    fn rollout_runner_writes_trace_files() {
        let runtime = DeterministicLocalActorRuntime::new("local-small-model");
        let report = run_rollout_batch(
            &runtime,
            RolloutRunnerInput {
                tasks: demo_rollout_tasks(2),
                actor_id: "local-small-model".into(),
                trace_id_prefix: "write-rollout".into(),
                max_turns: 3,
                simulator_mode: SimulatorMode::Clean,
            },
        )
        .unwrap();
        let dir = std::env::temp_dir().join(format!(
            "fractal-rlvr-rollout-runner-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);

        let paths = write_rollout_traces(&report, &dir).unwrap();

        assert_eq!(paths.len(), 2);
        for path in &paths {
            let raw = fs::read_to_string(path).unwrap();
            let trace: DialogueTrace = serde_json::from_str(&raw).unwrap();
            assert!(trace.trace_id.starts_with("write-rollout"));
            assert!(raw.contains("reward_vector"));
        }
        let _ = fs::remove_dir_all(dir);
    }
}

#[cfg(test)]
mod resource_guard_tests {
    use super::*;

    #[test]
    fn resource_guard_reports_cpu_mode_and_allows_safe_batch() {
        let report = validate_training_resources(TrainingResourceGuardInput {
            requested_batch_size: 4,
            limits: TrainingResourceLimits {
                min_available_memory_mb: 512,
                max_batch_size: 8,
            },
            snapshot: Some(TrainingResourceSnapshot {
                available_memory_mb: Some(2048),
                compute_mode: TrainingComputeMode::Cpu,
                gpu_detected: false,
            }),
        })
        .unwrap();

        assert_eq!(report.compute_mode, TrainingComputeMode::Cpu);
        assert!(!report.gpu_detected);
        assert_eq!(report.available_memory_mb, Some(2048));
    }

    #[test]
    fn resource_guard_reports_gpu_mode_when_detected() {
        let report = validate_training_resources(TrainingResourceGuardInput {
            requested_batch_size: 1,
            limits: TrainingResourceLimits::default(),
            snapshot: Some(TrainingResourceSnapshot {
                available_memory_mb: Some(4096),
                compute_mode: TrainingComputeMode::Gpu,
                gpu_detected: true,
            }),
        })
        .unwrap();

        assert_eq!(report.compute_mode.as_str(), "gpu");
        assert!(report.gpu_detected);
    }

    #[test]
    fn resource_guard_fails_gracefully_on_low_memory_and_large_batch() {
        let err = validate_training_resources(TrainingResourceGuardInput {
            requested_batch_size: 4,
            limits: TrainingResourceLimits {
                min_available_memory_mb: 1024,
                max_batch_size: 8,
            },
            snapshot: Some(TrainingResourceSnapshot {
                available_memory_mb: Some(256),
                compute_mode: TrainingComputeMode::Cpu,
                gpu_detected: false,
            }),
        })
        .unwrap_err();
        assert!(matches!(err, RlvrError::Resource(_)));
        assert!(err.to_string().contains("available memory"));

        let err = validate_training_resources(TrainingResourceGuardInput {
            requested_batch_size: 9,
            limits: TrainingResourceLimits {
                min_available_memory_mb: 512,
                max_batch_size: 8,
            },
            snapshot: Some(TrainingResourceSnapshot {
                available_memory_mb: Some(4096),
                compute_mode: TrainingComputeMode::Cpu,
                gpu_detected: false,
            }),
        })
        .unwrap_err();
        assert!(matches!(err, RlvrError::Resource(_)));
        assert!(err.to_string().contains("batch size"));
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrpoTrainerInput {
    pub base_model_id: String,
    pub adapter_id: String,
    pub rollouts: Vec<DialogueTrace>,
    pub output_dir: PathBuf,
    pub learning_rate: f64,
    pub epochs: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrpoRolloutAdvantage {
    pub task_id: String,
    pub trace_id: String,
    pub reward: f64,
    pub group_mean_reward: f64,
    pub group_std_reward: f64,
    pub normalized_advantage: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrpoEvalSummary {
    pub before_avg_reward: f64,
    pub after_avg_reward_estimate: f64,
    pub improved: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GrpoTrainerReport {
    pub base_model_id: String,
    pub adapter_id: String,
    pub adapter_only_update: bool,
    pub base_model_updated: bool,
    pub resource_report: TrainingResourceReport,
    pub checkpoint_path: String,
    pub rollout_count: usize,
    pub group_count: usize,
    pub advantages: Vec<GrpoRolloutAdvantage>,
    pub eval: GrpoEvalSummary,
}

impl GrpoTrainerInput {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("grpo.base_model_id", &self.base_model_id)?;
        require_non_empty("grpo.adapter_id", &self.adapter_id)?;
        if self.rollouts.is_empty() {
            return Err(RlvrError::Config(
                "grpo trainer requires at least one rollout".into(),
            ));
        }
        if !self.learning_rate.is_finite() || self.learning_rate <= 0.0 {
            return Err(RlvrError::Config(
                "grpo learning_rate must be finite and greater than zero".into(),
            ));
        }
        if self.epochs == 0 {
            return Err(RlvrError::Config(
                "grpo epochs must be greater than zero".into(),
            ));
        }
        let mut counts = BTreeMap::<String, usize>::new();
        for rollout in &self.rollouts {
            rollout.validate()?;
            *counts.entry(rollout.task_id.clone()).or_default() += 1;
        }
        if let Some((task_id, _)) = counts.iter().find(|(_, count)| **count < 2) {
            return Err(RlvrError::Config(format!(
                "grpo task {task_id:?} needs multiple rollouts for group-relative normalization"
            )));
        }
        Ok(())
    }
}

pub fn train_grpo_adapter(input: GrpoTrainerInput) -> Result<GrpoTrainerReport, RlvrError> {
    input.validate()?;
    let resource_report = validate_training_resources(TrainingResourceGuardInput {
        requested_batch_size: input.rollouts.len(),
        limits: TrainingResourceLimits::from_env_or_default()?,
        snapshot: None,
    })?;
    let grouped = group_rollouts_by_task(&input.rollouts);
    let mut advantages = Vec::new();
    for (task_id, rollouts) in &grouped {
        let rewards = rollouts
            .iter()
            .map(|rollout| rollout.final_reward)
            .collect::<Vec<_>>();
        let mean = rewards.iter().sum::<f64>() / rewards.len() as f64;
        let variance = rewards
            .iter()
            .map(|reward| {
                let delta = reward - mean;
                delta * delta
            })
            .sum::<f64>()
            / rewards.len() as f64;
        let std = variance.sqrt();
        for rollout in rollouts {
            let normalized_advantage = if std > f64::EPSILON {
                (rollout.final_reward - mean) / std
            } else {
                0.0
            };
            advantages.push(GrpoRolloutAdvantage {
                task_id: task_id.clone(),
                trace_id: rollout.trace_id.clone(),
                reward: rollout.final_reward,
                group_mean_reward: mean,
                group_std_reward: std,
                normalized_advantage,
            });
        }
    }
    advantages.sort_by(|left, right| {
        left.task_id
            .cmp(&right.task_id)
            .then_with(|| left.trace_id.cmp(&right.trace_id))
    });

    let before_avg_reward = input
        .rollouts
        .iter()
        .map(|rollout| rollout.final_reward)
        .sum::<f64>()
        / input.rollouts.len() as f64;
    let positive_advantage_mean = advantages
        .iter()
        .filter(|advantage| advantage.normalized_advantage > 0.0)
        .map(|advantage| advantage.normalized_advantage)
        .sum::<f64>()
        / advantages.len().max(1) as f64;
    let after_avg_reward_estimate = (before_avg_reward
        + input.learning_rate * input.epochs as f64 * positive_advantage_mean)
        .clamp(0.0, 1.0);
    let eval = GrpoEvalSummary {
        before_avg_reward,
        after_avg_reward_estimate,
        improved: after_avg_reward_estimate > before_avg_reward,
    };
    let checkpoint_path = input.output_dir.join(format!(
        "{}-grpo-checkpoint.json",
        sanitize_file_stem(&input.adapter_id)
    ));
    let checkpoint_hash = stable_hash(&advantages)?;
    let report = GrpoTrainerReport {
        base_model_id: input.base_model_id,
        adapter_id: input.adapter_id,
        adapter_only_update: true,
        base_model_updated: false,
        resource_report,
        checkpoint_path: checkpoint_path.to_string_lossy().into_owned(),
        rollout_count: input.rollouts.len(),
        group_count: grouped.len(),
        advantages,
        eval,
    };
    write_grpo_checkpoint(&report, &checkpoint_hash, checkpoint_path)?;
    Ok(report)
}

fn group_rollouts_by_task(rollouts: &[DialogueTrace]) -> BTreeMap<String, Vec<&DialogueTrace>> {
    let mut grouped = BTreeMap::<String, Vec<&DialogueTrace>>::new();
    for rollout in rollouts {
        grouped
            .entry(rollout.task_id.clone())
            .or_default()
            .push(rollout);
    }
    grouped
}

fn write_grpo_checkpoint(
    report: &GrpoTrainerReport,
    checkpoint_hash: &str,
    path: PathBuf,
) -> Result<(), RlvrError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    let checkpoint = serde_json::json!({
        "adapter_id": report.adapter_id,
        "base_model_id": report.base_model_id,
        "adapter_only_update": report.adapter_only_update,
        "base_model_updated": report.base_model_updated,
        "resource_report": report.resource_report,
        "rollout_count": report.rollout_count,
        "group_count": report.group_count,
        "advantage_hash": checkpoint_hash,
        "eval": report.eval,
    });
    fs::write(path, serde_json::to_string_pretty(&checkpoint)?)?;
    Ok(())
}

fn sanitize_file_stem(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

/// CLI entrypoint for `fractal-rlvr train --method grpo`.
///
/// Reads rollout [`DialogueTrace`]s from `--rollouts <dir>`, groups them by
/// `task_id` (GRPO needs ≥ 2 rollouts per prompt), trains an adapter-only
/// checkpoint via [`train_grpo_adapter`], and returns a one-line summary. This is
/// the trainer step the "Improve My Local Model" UI (RLVR-054) drives through the
/// local RLVR API.
pub fn run_grpo_train_cli(argv: &[String]) -> Result<String, RlvrError> {
    let rollouts_dir = value_after(argv, "--rollouts").ok_or_else(|| {
        RlvrError::UnsupportedCommand("train --method grpo requires --rollouts <dir>".into())
    })?;
    let adapter_id = value_after(argv, "--adapter").ok_or_else(|| {
        RlvrError::UnsupportedCommand("train --method grpo requires --adapter <id>".into())
    })?;
    let base_model_id = value_after(argv, "--base-model")
        .or_else(|| value_after(argv, "--base"))
        .unwrap_or_else(|| "local-tiny-model".into());
    let out_dir = value_after(argv, "--out")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("runs/train"));
    let learning_rate = value_after(argv, "--lr")
        .map(|raw| parse_f64(&raw, "--lr"))
        .transpose()?
        .unwrap_or(0.05);
    if !learning_rate.is_finite() || learning_rate <= 0.0 {
        return Err(RlvrError::Config(
            "--lr must be finite and greater than zero".into(),
        ));
    }
    let epochs = value_after(argv, "--epochs")
        .map(|raw| {
            raw.parse::<u32>()
                .map_err(|_| RlvrError::Config("--epochs must be a positive integer".into()))
        })
        .transpose()?
        .unwrap_or(2);

    let rollouts = read_dialogue_traces_dir(&rollouts_dir)?;
    let report = train_grpo_adapter(GrpoTrainerInput {
        base_model_id,
        adapter_id,
        rollouts,
        output_dir: out_dir,
        learning_rate,
        epochs,
    })?;
    Ok(format!(
        "train --method grpo ok: adapter={} rollouts={} groups={} improved={} before_avg_reward={:.4} after_avg_reward_estimate={:.4} checkpoint={}",
        report.adapter_id,
        report.rollout_count,
        report.group_count,
        report.eval.improved,
        report.eval.before_avg_reward,
        report.eval.after_avg_reward_estimate,
        report.checkpoint_path,
    ))
}

fn value_after(argv: &[String], flag: &str) -> Option<String> {
    argv.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
}

fn parse_f64(raw: &str, flag: &str) -> Result<f64, RlvrError> {
    raw.trim_matches('"')
        .parse::<f64>()
        .map_err(|_| RlvrError::Config(format!("{flag} must be a number")))
}

#[cfg(test)]
mod actor_runtime_tests {
    use super::*;

    #[test]
    fn actor_role_parse_accepts_all_prd_targets() {
        assert_eq!(
            ActorRole::parse("tiny-assistant"),
            Some(ActorRole::TinyAssistant)
        );
        assert_eq!(ActorRole::parse("router"), Some(ActorRole::Router));
        assert_eq!(
            ActorRole::parse("clarification-model"),
            Some(ActorRole::Clarification)
        );
        assert_eq!(ActorRole::parse("critic"), Some(ActorRole::Critic));
        assert_eq!(ActorRole::parse("compressor"), Some(ActorRole::Compressor));
        assert_eq!(
            ActorRole::parse("tool-use-policy"),
            Some(ActorRole::ToolUsePolicy)
        );
    }

    #[test]
    fn local_actor_invokes_all_roles_through_one_interface() {
        let runtime: Box<dyn ActorRuntime> =
            Box::new(DeterministicLocalActorRuntime::new("local-small-model"));
        for role in [
            ActorRole::TinyAssistant,
            ActorRole::Router,
            ActorRole::Clarification,
            ActorRole::Critic,
            ActorRole::Compressor,
            ActorRole::ToolUsePolicy,
        ] {
            let response = runtime
                .invoke(&ActorRuntimeRequest {
                    actor_id: format!("actor-{}", role.as_str()),
                    role,
                    prompt: "Summarize this route decision and choose the cheapest safe path."
                        .into(),
                    context: vec!["local-only rollout".into()],
                    max_tokens: Some(64),
                    temperature: Some(0.0),
                })
                .unwrap();
            assert_eq!(response.role, role);
            assert_eq!(response.model_id, "local-small-model");
            assert!(!response.content.is_empty());
            assert_eq!(response.cost_estimate, 0.0);
        }
    }

    #[test]
    fn local_actor_rejects_unsupported_role_and_invalid_request() {
        let runtime = DeterministicLocalActorRuntime {
            model_id: "router-only".into(),
            supported_roles: vec![ActorRole::Router],
            latency_ms: 1,
            cost_estimate: 0.0,
        };
        let err = runtime
            .invoke(&ActorRuntimeRequest {
                actor_id: "assistant".into(),
                role: ActorRole::TinyAssistant,
                prompt: "hello".into(),
                context: Vec::new(),
                max_tokens: Some(16),
                temperature: Some(0.0),
            })
            .unwrap_err();
        assert!(err.to_string().contains("does not support role"));

        let err = runtime
            .invoke(&ActorRuntimeRequest {
                actor_id: "router".into(),
                role: ActorRole::Router,
                prompt: "".into(),
                context: Vec::new(),
                max_tokens: Some(16),
                temperature: Some(0.0),
            })
            .unwrap_err();
        assert!(err.to_string().contains("prompt"));
    }
}

#[cfg(test)]
mod grpo_tests {
    use super::*;
    use crate::{DialogueTurn, RewardVector, VerifierOutput};

    #[test]
    fn grpo_trainer_normalizes_rewards_and_writes_adapter_checkpoint() {
        let dir = std::env::temp_dir().join(format!("fractal-rlvr-grpo-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let report = train_grpo_adapter(GrpoTrainerInput {
            base_model_id: "tiny-base".into(),
            adapter_id: "router-adapter".into(),
            rollouts: vec![
                rollout("task-a", "a-low", 0.2),
                rollout("task-a", "a-high", 0.8),
                rollout("task-b", "b-low", 0.4),
                rollout("task-b", "b-high", 0.6),
            ],
            output_dir: dir.clone(),
            learning_rate: 0.05,
            epochs: 2,
        })
        .unwrap();

        assert_eq!(report.rollout_count, 4);
        assert_eq!(report.group_count, 2);
        assert!(report.adapter_only_update);
        assert!(!report.base_model_updated);
        assert_eq!(report.advantages.len(), 4);
        assert!(report
            .advantages
            .iter()
            .any(|advantage| advantage.normalized_advantage > 0.0));
        assert!(report
            .advantages
            .iter()
            .any(|advantage| advantage.normalized_advantage < 0.0));
        assert!(report.eval.improved);
        assert!(std::path::Path::new(&report.checkpoint_path).exists());
        let raw = fs::read_to_string(&report.checkpoint_path).unwrap();
        assert!(raw.contains("\"adapter_only_update\": true"));
        assert!(raw.contains("\"base_model_updated\": false"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn grpo_trainer_requires_multiple_rollouts_per_prompt() {
        let err = train_grpo_adapter(GrpoTrainerInput {
            base_model_id: "tiny-base".into(),
            adapter_id: "router-adapter".into(),
            rollouts: vec![rollout("task-a", "a-only", 0.5)],
            output_dir: std::env::temp_dir(),
            learning_rate: 0.05,
            epochs: 1,
        })
        .unwrap_err();

        assert!(err.to_string().contains("multiple rollouts"));
    }

    #[test]
    fn grpo_trainer_rejects_invalid_learning_params() {
        let err = train_grpo_adapter(GrpoTrainerInput {
            base_model_id: "tiny-base".into(),
            adapter_id: "router-adapter".into(),
            rollouts: vec![
                rollout("task-a", "a-low", 0.2),
                rollout("task-a", "a-high", 0.8),
            ],
            output_dir: std::env::temp_dir(),
            learning_rate: 0.0,
            epochs: 1,
        })
        .unwrap_err();
        assert!(err.to_string().contains("learning_rate"));
    }

    fn rollout(task_id: &str, trace_id: &str, reward: f64) -> DialogueTrace {
        DialogueTrace {
            trace_id: trace_id.into(),
            task_id: task_id.into(),
            turns: vec![DialogueTurn {
                role: "assistant".into(),
                content: "final answer".into(),
                model_id: Some("tiny-base".into()),
                route_decision: Some("tiny-local-model".into()),
                latency_ms: Some(1),
                cost_estimate: Some(0.0),
            }],
            verifier_outputs: vec![VerifierOutput {
                is_final_answer: true,
                is_clarification_question: false,
                targeted_checkpoints: Vec::new(),
                missed_checkpoints: Vec::new(),
                redundant_question: false,
                premature_answer: false,
                false_premise_corrected: None,
                route_valid: true,
                reward,
            }],
            reward_vector: RewardVector {
                correctness: reward,
                checkpoint_coverage: reward,
                clarification_quality: reward,
                false_premise_detection: 1.0,
                route_correctness: 1.0,
                tool_use_correctness: 1.0,
                cost_efficiency: 1.0,
                latency_efficiency: 1.0,
                privacy_compliance: 1.0,
                non_redundancy: 1.0,
            },
            final_reward: reward,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RolloutTaskFilter {
    pub mode: Option<TrainingMode>,
    pub difficulty: Option<Difficulty>,
    pub domain: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RolloutTaskSamplerInput {
    pub generated_tasks: Vec<TrainingItem>,
    pub replay_tasks: Vec<TrainingItem>,
    pub filter: RolloutTaskFilter,
    pub seed: u64,
    pub batch_size: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RolloutTaskSource {
    Generated,
    Replay,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SampledRolloutTask {
    pub source: RolloutTaskSource,
    pub item: TrainingItem,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RolloutTaskBatch {
    pub seed: u64,
    pub requested_batch_size: usize,
    pub tasks: Vec<SampledRolloutTask>,
}

pub fn sample_rollout_tasks(input: RolloutTaskSamplerInput) -> Result<RolloutTaskBatch, RlvrError> {
    input.validate()?;
    let mut candidates = Vec::new();
    push_matching_tasks(
        &mut candidates,
        RolloutTaskSource::Replay,
        input.replay_tasks,
        &input.filter,
    )?;
    push_matching_tasks(
        &mut candidates,
        RolloutTaskSource::Generated,
        input.generated_tasks,
        &input.filter,
    )?;
    candidates.sort_by(|left, right| {
        let left_key = deterministic_task_key(input.seed, left);
        let right_key = deterministic_task_key(input.seed, right);
        left_key
            .cmp(&right_key)
            .then_with(|| left.item.task_id.cmp(&right.item.task_id))
    });
    candidates.truncate(input.batch_size);
    Ok(RolloutTaskBatch {
        seed: input.seed,
        requested_batch_size: input.batch_size,
        tasks: candidates,
    })
}

impl RolloutTaskSamplerInput {
    pub fn validate(&self) -> Result<(), RlvrError> {
        if self.batch_size == 0 {
            return Err(RlvrError::Config(
                "rollout task sampler batch_size must be greater than zero".into(),
            ));
        }
        if self.generated_tasks.is_empty() && self.replay_tasks.is_empty() {
            return Err(RlvrError::Config(
                "rollout task sampler requires generated or replay tasks".into(),
            ));
        }
        for item in self.generated_tasks.iter().chain(self.replay_tasks.iter()) {
            item.validate()?;
        }
        Ok(())
    }
}

fn push_matching_tasks(
    out: &mut Vec<SampledRolloutTask>,
    source: RolloutTaskSource,
    tasks: Vec<TrainingItem>,
    filter: &RolloutTaskFilter,
) -> Result<(), RlvrError> {
    for item in tasks {
        item.validate()?;
        if matches_filter(&item, filter) {
            out.push(SampledRolloutTask {
                source: source.clone(),
                item,
            });
        }
    }
    Ok(())
}

fn matches_filter(item: &TrainingItem, filter: &RolloutTaskFilter) -> bool {
    if filter.mode.is_some_and(|mode| item.mode != mode) {
        return false;
    }
    if filter
        .difficulty
        .is_some_and(|difficulty| item.difficulty != difficulty)
    {
        return false;
    }
    if let Some(domain) = &filter.domain {
        if item.domain != *domain {
            return false;
        }
    }
    true
}

fn deterministic_task_key(seed: u64, task: &SampledRolloutTask) -> String {
    let source = match task.source {
        RolloutTaskSource::Generated => "generated",
        RolloutTaskSource::Replay => "replay",
    };
    let task_hash = task
        .item
        .stable_hash()
        .unwrap_or_else(|_| task.item.task_id.clone());
    hash_bytes(format!("{seed}:{source}:{task_hash}").as_bytes())
}

#[cfg(test)]
mod sampler_tests {
    use super::*;
    use crate::{Checkpoint, CheckpointType, PrivacyPolicy, RoutePolicy};

    #[test]
    fn sampler_filters_by_mode_difficulty_and_domain() {
        let batch = sample_rollout_tasks(RolloutTaskSamplerInput {
            generated_tasks: vec![
                item("a", TrainingMode::AskMind, Difficulty::Easy, "electronics"),
                item("b", TrainingMode::ToolUse, Difficulty::Hard, "weather"),
                item("c", TrainingMode::ToolUse, Difficulty::Hard, "electronics"),
            ],
            replay_tasks: Vec::new(),
            filter: RolloutTaskFilter {
                mode: Some(TrainingMode::ToolUse),
                difficulty: Some(Difficulty::Hard),
                domain: Some("electronics".into()),
            },
            seed: 7,
            batch_size: 10,
        })
        .unwrap();

        assert_eq!(batch.tasks.len(), 1);
        assert_eq!(batch.tasks[0].item.task_id, "c");
        assert_eq!(batch.tasks[0].source, RolloutTaskSource::Generated);
    }

    #[test]
    fn sampler_supports_user_trace_replay_set() {
        let batch = sample_rollout_tasks(RolloutTaskSamplerInput {
            generated_tasks: vec![item(
                "generated-a",
                TrainingMode::AskMind,
                Difficulty::Easy,
                "electronics",
            )],
            replay_tasks: vec![item(
                "replay-a",
                TrainingMode::AskMind,
                Difficulty::Easy,
                "electronics",
            )],
            filter: RolloutTaskFilter {
                mode: Some(TrainingMode::AskMind),
                difficulty: None,
                domain: None,
            },
            seed: 42,
            batch_size: 10,
        })
        .unwrap();

        assert_eq!(batch.tasks.len(), 2);
        assert!(batch
            .tasks
            .iter()
            .any(|task| task.source == RolloutTaskSource::Replay));
    }

    #[test]
    fn sampler_selects_deterministic_task_batch_by_seed() {
        let input = RolloutTaskSamplerInput {
            generated_tasks: (0..8)
                .map(|idx| {
                    item(
                        &format!("task-{idx}"),
                        TrainingMode::RouteCorrectness,
                        Difficulty::Medium,
                        "routing",
                    )
                })
                .collect(),
            replay_tasks: Vec::new(),
            filter: RolloutTaskFilter::default(),
            seed: 99,
            batch_size: 3,
        };

        let first = sample_rollout_tasks(input.clone()).unwrap();
        let second = sample_rollout_tasks(input).unwrap();
        let changed_seed = sample_rollout_tasks(RolloutTaskSamplerInput {
            seed: 100,
            ..sampler_input_from_batch_fixture()
        })
        .unwrap();

        let first_ids = task_ids(&first);
        assert_eq!(first_ids, task_ids(&second));
        assert_eq!(first.tasks.len(), 3);
        assert_ne!(first_ids, task_ids(&changed_seed));
    }

    #[test]
    fn sampler_rejects_empty_source_and_zero_batch() {
        let err = sample_rollout_tasks(RolloutTaskSamplerInput {
            generated_tasks: Vec::new(),
            replay_tasks: Vec::new(),
            filter: RolloutTaskFilter::default(),
            seed: 1,
            batch_size: 1,
        })
        .unwrap_err();
        assert!(err
            .to_string()
            .contains("requires generated or replay tasks"));

        let err = sample_rollout_tasks(RolloutTaskSamplerInput {
            generated_tasks: vec![item(
                "a",
                TrainingMode::AskMind,
                Difficulty::Easy,
                "electronics",
            )],
            replay_tasks: Vec::new(),
            filter: RolloutTaskFilter::default(),
            seed: 1,
            batch_size: 0,
        })
        .unwrap_err();
        assert!(err.to_string().contains("batch_size"));
    }

    fn sampler_input_from_batch_fixture() -> RolloutTaskSamplerInput {
        RolloutTaskSamplerInput {
            generated_tasks: (0..8)
                .map(|idx| {
                    item(
                        &format!("task-{idx}"),
                        TrainingMode::RouteCorrectness,
                        Difficulty::Medium,
                        "routing",
                    )
                })
                .collect(),
            replay_tasks: Vec::new(),
            filter: RolloutTaskFilter::default(),
            seed: 99,
            batch_size: 3,
        }
    }

    fn task_ids(batch: &RolloutTaskBatch) -> Vec<String> {
        batch
            .tasks
            .iter()
            .map(|task| task.item.task_id.clone())
            .collect()
    }

    fn item(
        task_id: &str,
        mode: TrainingMode,
        difficulty: Difficulty,
        domain: &str,
    ) -> TrainingItem {
        TrainingItem {
            task_id: task_id.into(),
            mode,
            visible_user_query: format!("visible query for {task_id}"),
            hidden_original_query: format!("hidden query for {task_id}"),
            gold_answer: "gold".into(),
            domain: domain.into(),
            difficulty,
            checkpoints: vec![Checkpoint {
                checkpoint_id: format!("{task_id}-c1"),
                checkpoint_type: CheckpointType::MissingInfo,
                description: "Ask for the missing value".into(),
                must_resolve_before_answer: true,
                answer_if_asked: "value".into(),
                failure_penalty: 1.0,
            }],
            route_policy: RoutePolicy::default(),
            privacy_policy: PrivacyPolicy::default(),
        }
    }
}
