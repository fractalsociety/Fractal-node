pub mod adapters;
pub mod api;
pub mod chain;
pub mod data;
pub mod evals;
pub mod rewards;
pub mod rubrics;
pub mod simulator;
pub mod tracing;
pub mod trainer;
pub mod verifier;

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use adapters::export::{
    default_target_modules, export_adapter_bundle, load_adapter_bundle, synthesize_weights,
    AdapterArtifactRole, AdapterConfig, AdapterEvalReport, AdapterExportInput, AdapterExportReport,
    AdapterManifest, AdapterManifestFile, AdapterModelCard, AdapterPrivacyStatement, AdapterTensor,
    AdapterTensorKind, AdapterTrainingSummary, AdapterWeights, LoadedAdapterBundle,
    RewardPolicyArtifact, ADAPTER_BUNDLE_FORMAT_VERSION, ADAPTER_WEIGHTS_FORMAT,
    DEFAULT_ADAPTER_RANK, DEFAULT_MODEL_DIM,
};
pub use adapters::{
    list_adapter_metadata, register_adapter_metadata, AdapterMetadata, AdapterRegistry,
    AdapterRegistryStore, AdapterTrainingMode,
};
pub use api::{
    fractal_create_rlvr_proof_object, fractal_export_rlvr_adapter, fractal_get_rlvr_proof,
    fractal_list_local_traces, fractal_make_rlvr_rubrics, fractal_run_rlvr_eval,
    fractal_run_rlvr_rollout, fractal_submit_rlvr_proof, CreateProofObjectRequest,
    CreateProofObjectResponse, ExportAdapterRequest, ExportAdapterResponse, GetRlvrProofResponse,
    ListLocalTracesResponse, LocalTraceSummary, MakeRubricsRequest, MakeRubricsResponse,
    RunEvalRequest, RunEvalResponse, RunRolloutRequest, RunRolloutResponse,
    SubmitRlvrProofResponse,
};
pub use chain::{
    apply_rlvr_proof_block_payload, CommittedRlvrProof, NodeSigningKey, RlvrAcceptedProofState,
    RlvrBlockApplyReport, RlvrCommittedProofIndex, RlvrCommittedProofIndexMetrics,
    RlvrDisputeRecord, RlvrDisputeStore, RlvrDisputeStoreMetrics, RlvrDisputeTarget,
    RlvrPooledProof, RlvrProofBlockPayloadItem, RlvrProofBlockReference, RlvrProofObject,
    RlvrProofPool, RlvrProofPoolMetrics, RlvrProofStatus, RlvrProofType,
};
pub use data::{
    scan_privacy_tags, Checkpoint, CheckpointType, DialogueTrace, DialogueTurn, Difficulty,
    PrivacyPolicy, PrivacyScan, PrivacyTag, RedactedDialogueTrace, RedactedDialogueTurn,
    RewardVector, RoutePolicy, RouteRule, TraceHashCommitment, TrainingItem, TrainingMode,
    VerifierOutput,
};
pub use evals::baseline::{
    askmind_baseline_eval_set, askoverconfidence_baseline_eval_set, baseline_eval_set,
    baseline_eval_set_manifest, compare_baseline_eval_set, compressionloss_baseline_eval_set,
    default_baseline_eval_sets, routecorrectness_baseline_eval_set, score_baseline_eval_set,
    tooluse_baseline_eval_set, user_trace_replay_baseline_eval_set, BaselineComparisonReport,
    BaselineEvalItemScore, BaselineEvalSet, BaselineEvalSetKind, BaselineEvalSetManifest,
    BaselineEvalSetManifestEntry, BaselineEvalSystemReport,
};
pub use evals::{
    build_eval_metrics_report, evaluate_adapter_promotion_gate, read_eval_traces,
    render_eval_report_html, run_adversarial_privacy_suite, run_proof_route_benchmark,
    v01_release_gate_report, write_eval_report, AdapterPromotionDecision,
    AdapterPromotionGatePolicy, AdapterRollbackMetadata, AdversarialPrivacyReport,
    EvalMetricsReport, EvalReportFiles, EvalTraceMetrics, Phase11TestCoverageReport,
    PromotionGateCheck, ProofRouteBenchmarkReport, V01ReleaseGateReport,
};
pub use rewards::{
    compute_reward_vector, detect_anti_reward_hacking, score_mvp_reward_v01,
    score_mvp_reward_with_policy, write_reward_vector_json, AntiRewardHackingInput,
    AntiRewardHackingReport, MvpRewardPolicyInput, MvpRewardPolicyReport, MvpRewardPolicyV01,
    RewardSignalInput, RewardVectorArtifact, MVP_REWARD_POLICY_V01_ID,
};
pub use rubrics::{
    generate_ask_mind_rubric, generate_ask_overconfidence_rubric, generate_compression_loss_rubric,
    generate_route_correctness_rubric, generate_tool_use_rubric, sample_ask_mind_fixtures,
    sample_fixtures, AskMindFixture, AskMindMissingFact, AskMindRubric, AskMindRubricInput,
    AskOverconfidenceFixture, AskOverconfidenceRubric, AskOverconfidenceRubricInput,
    CompressionLossRubricInput, CompressionRequiredFact, ModelInventoryItem,
    RouteCorrectnessRubricInput, ToolInventoryItem, ToolUseRequirementKind, ToolUseRubricInput,
};
pub use simulator::{
    build_simulated_rollout_trace, simulate_local_user_reply, simulate_local_user_reply_with_mode,
    AdversarialSimulatorStyle, LocalUserSimulator, LocalUserSimulatorInput,
    LocalUserSimulatorReply, SimulatedRolloutTraceInput, SimulatorMode,
};
pub use tracing::{
    LocalTraceStore, RouteTraceInput, RouteTraceLogger, RouteTraceRow, TraceStoreMetadata,
};
pub use trainer::{
    demo_rollout_tasks, run_rollout_batch, sample_rollout_tasks, train_grpo_adapter,
    validate_training_resources, write_rollout_traces, GrpoEvalSummary, GrpoRolloutAdvantage,
    GrpoTrainerInput, GrpoTrainerReport, RolloutRunReport, RolloutRunnerInput, RolloutTaskBatch,
    RolloutTaskFilter, RolloutTaskSamplerInput, RolloutTaskSource, SampledRolloutTask,
    TrainingComputeMode, TrainingResourceGuardInput, TrainingResourceLimits,
    TrainingResourceReport, TrainingResourceSnapshot,
};
pub use trainer::{
    ActorRole, ActorRuntime, ActorRuntimeRequest, ActorRuntimeResponse,
    DeterministicLocalActorRuntime,
};
pub use verifier::{
    evaluate_single_local_verifier_for_item, evaluate_verifier_panel_for_item,
    parse_strict_verifier_output, parse_verifier_output_with_retries,
    parse_verifier_output_with_retries_and_logger, score_checkpoint_coverage,
    score_checkpoint_coverage_for_item, score_final_answer_for_item,
    score_final_answer_from_coverage, CheckpointCoverageReport, FinalAnswerScoreReport,
    ParsedVerifierOutput, StrictVerifierOutput, UnparseableVerifierLogRow,
    UnparseableVerifierLogger, VerifierLogContext, VerifierPanelJudge, VerifierPanelReport,
    VerifierParseFailure, VerifierParseReport, VerifierQaExportRecord, VerifierQaRecord,
    VerifierQaRecordInput, VerifierQaStore,
};

pub const DEFAULT_CONFIG_FILE: &str = "fractal_rlvr/config.yaml";
pub const DEFAULT_REWARD_POLICY: &str = "reward-v0.1";
pub const DEFAULT_TRAINING_MODE: TrainingMode = TrainingMode::RouteCorrectness;
pub const DEFAULT_ROUTE_POLICY_ID: &str = "default-router-v0.1";
pub const DEFAULT_CONFIG_YAML: &str = include_str!("../config/default.yaml");

#[derive(Debug, Error)]
pub enum RlvrError {
    #[error("config error: {0}")]
    Config(String),
    #[error("resource error: {0}")]
    Resource(String),
    #[error("unsupported command: {0}")]
    UnsupportedCommand(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RlvrConfig {
    pub local_only: bool,
    pub default_actor_model: String,
    pub default_judge_model: String,
    pub max_turns: u32,
    pub training_mode: TrainingMode,
    pub reward_policy: String,
    pub chain_commit_enabled: bool,
    pub raw_data_on_chain: bool,
}

impl Default for RlvrConfig {
    fn default() -> Self {
        Self {
            local_only: true,
            default_actor_model: String::new(),
            default_judge_model: String::new(),
            max_turns: 3,
            training_mode: DEFAULT_TRAINING_MODE,
            reward_policy: DEFAULT_REWARD_POLICY.into(),
            chain_commit_enabled: false,
            raw_data_on_chain: false,
        }
    }
}

impl RlvrConfig {
    pub fn validate(&self) -> Result<(), RlvrError> {
        if self.max_turns == 0 {
            return Err(RlvrError::Config(
                "max_turns must be greater than zero".into(),
            ));
        }
        if self.reward_policy.trim().is_empty() {
            return Err(RlvrError::Config("reward_policy cannot be empty".into()));
        }
        if self.raw_data_on_chain {
            return Err(RlvrError::Config(
                "raw_data_on_chain must remain false; only hashes may be committed".into(),
            ));
        }
        if self.chain_commit_enabled && !self.local_only {
            return Err(RlvrError::Config(
                "chain commits require local_only trace handling in v0.1".into(),
            ));
        }
        Ok(())
    }

    pub fn to_yaml(&self) -> String {
        format!(
            "local_only: {}\ndefault_actor_model: \"{}\"\ndefault_judge_model: \"{}\"\nmax_turns: {}\ntraining_mode: \"{}\"\nreward_policy: \"{}\"\nchain_commit_enabled: {}\nraw_data_on_chain: {}\n",
            self.local_only,
            escape_yaml_string(&self.default_actor_model),
            escape_yaml_string(&self.default_judge_model),
            self.max_turns,
            self.training_mode.as_str(),
            escape_yaml_string(&self.reward_policy),
            self.chain_commit_enabled,
            self.raw_data_on_chain
        )
    }

    pub fn from_yaml_str(raw: &str) -> Result<Self, RlvrError> {
        let mut cfg = Self::default();
        if raw.trim().is_empty() {
            cfg.validate()?;
            return Ok(cfg);
        }
        for (idx, line) in raw.lines().enumerate() {
            let cleaned = line.split('#').next().unwrap_or("").trim();
            if cleaned.is_empty() {
                continue;
            }
            let Some((key, value)) = cleaned.split_once(':') else {
                return Err(RlvrError::Config(format!(
                    "line {} is not key: value",
                    idx + 1
                )));
            };
            let key = key.trim();
            let value = value.trim();
            match key {
                "local_only" => cfg.local_only = parse_bool(value, key)?,
                "default_actor_model" => cfg.default_actor_model = parse_string(value),
                "default_judge_model" => cfg.default_judge_model = parse_string(value),
                "max_turns" => {
                    cfg.max_turns = value
                        .trim_matches('"')
                        .parse()
                        .map_err(|_| RlvrError::Config("max_turns must be a number".into()))?;
                }
                "training_mode" => {
                    cfg.training_mode = TrainingMode::parse(value).ok_or_else(|| {
                        RlvrError::Config(format!("unsupported training_mode {value:?}"))
                    })?;
                }
                "reward_policy" => cfg.reward_policy = parse_string(value),
                "chain_commit_enabled" => {
                    cfg.chain_commit_enabled = parse_bool(value, key)?;
                }
                "raw_data_on_chain" => cfg.raw_data_on_chain = parse_bool(value, key)?,
                other => {
                    return Err(RlvrError::Config(format!("unknown config key {other:?}")));
                }
            }
        }
        cfg.validate()?;
        Ok(cfg)
    }

    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, RlvrError> {
        let raw = fs::read_to_string(path)?;
        Self::from_yaml_str(&raw)
    }

    pub fn with_env_overrides(mut self) -> Result<Self, RlvrError> {
        if let Ok(raw) = std::env::var("FRACTAL_RLVR_LOCAL_ONLY") {
            self.local_only = parse_bool(&raw, "FRACTAL_RLVR_LOCAL_ONLY")?;
        }
        if let Ok(raw) = std::env::var("FRACTAL_RLVR_DEFAULT_ACTOR_MODEL") {
            self.default_actor_model = raw;
        }
        if let Ok(raw) = std::env::var("FRACTAL_RLVR_DEFAULT_JUDGE_MODEL") {
            self.default_judge_model = raw;
        }
        if let Ok(raw) = std::env::var("FRACTAL_RLVR_MAX_TURNS") {
            self.max_turns = raw
                .trim()
                .parse()
                .map_err(|_| RlvrError::Config("FRACTAL_RLVR_MAX_TURNS must be a number".into()))?;
        }
        if let Ok(raw) = std::env::var("FRACTAL_RLVR_TRAINING_MODE") {
            self.training_mode = TrainingMode::parse(&raw).ok_or_else(|| {
                RlvrError::Config(format!("unsupported FRACTAL_RLVR_TRAINING_MODE {raw:?}"))
            })?;
        }
        if let Ok(raw) = std::env::var("FRACTAL_RLVR_REWARD_POLICY") {
            self.reward_policy = raw;
        }
        if let Ok(raw) = std::env::var("FRACTAL_RLVR_CHAIN_COMMIT_ENABLED") {
            self.chain_commit_enabled = parse_bool(&raw, "FRACTAL_RLVR_CHAIN_COMMIT_ENABLED")?;
        }
        if let Ok(raw) = std::env::var("FRACTAL_RLVR_RAW_DATA_ON_CHAIN") {
            self.raw_data_on_chain = parse_bool(&raw, "FRACTAL_RLVR_RAW_DATA_ON_CHAIN")?;
        }
        self.validate()?;
        Ok(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RlvrNodeFlags {
    pub enabled: bool,
    pub chain_commit_enabled: bool,
    pub raw_data_on_chain: bool,
    pub raw_data_on_chain_requested: bool,
}

impl RlvrNodeFlags {
    pub fn from_env() -> Self {
        Self::from_values(
            std::env::var("FRACTAL_RLVR_ENABLED").ok().as_deref(),
            std::env::var("FRACTAL_RLVR_CHAIN_COMMIT_ENABLED")
                .ok()
                .as_deref(),
            std::env::var("FRACTAL_RLVR_RAW_DATA_ON_CHAIN")
                .ok()
                .as_deref(),
        )
    }

    pub fn from_values(
        enabled: Option<&str>,
        chain_commit_enabled: Option<&str>,
        raw_data_on_chain: Option<&str>,
    ) -> Self {
        Self {
            enabled: parse_flag_value(enabled, false),
            chain_commit_enabled: parse_flag_value(chain_commit_enabled, false),
            raw_data_on_chain: false,
            raw_data_on_chain_requested: parse_flag_value(raw_data_on_chain, false),
        }
    }
}

pub fn config_hash(config: &RlvrConfig) -> Result<String, RlvrError> {
    stable_hash(config)
}

pub fn route_policy_hash(policy: &RoutePolicy) -> Result<String, RlvrError> {
    stable_hash(policy)
}

pub fn default_config_yaml() -> &'static str {
    DEFAULT_CONFIG_YAML
}

pub fn stable_hash<T: Serialize>(value: &T) -> Result<String, RlvrError> {
    let bytes = serde_json::to_vec(value)?;
    Ok(hex::encode(blake3::hash(&bytes).as_bytes()))
}

/// Raw blake3 of `bytes`, hex-encoded. Used for content-addressing prompts,
/// answers, and corrections in trace rows where JSON-wrapping ([`stable_hash`])
/// would be inappropriate.
pub fn hash_bytes(bytes: &[u8]) -> String {
    hex::encode(blake3::hash(bytes).as_bytes())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CliCommand {
    name: &'static str,
    usage: &'static str,
    description: &'static str,
}

const CLI_COMMANDS: &[CliCommand] = &[
    CliCommand {
        name: "init",
        usage: "fractal-rlvr init [--root path]",
        description: "Create the local RLVR workspace folders and default config.",
    },
    CliCommand {
        name: "config",
        usage: "fractal-rlvr config validate [--config path]",
        description: "Validate the RLVR config file or embedded default config.",
    },
    CliCommand {
        name: "collect-traces",
        usage: "fractal-rlvr collect-traces --source fractal-chat --out data/traces.jsonl",
        description: "Register the trace collection command surface.",
    },
    CliCommand {
        name: "make-rubrics",
        usage: "fractal-rlvr make-rubrics --mode route-correctness --input data/traces.jsonl --out data/rubrics.jsonl",
        description: "Register the rubric generation command surface.",
    },
    CliCommand {
        name: "rollout",
        usage: "fractal-rlvr rollout --n 100 --out runs/rollout-001 [--per-task 2] [--actor local-tiny-model]",
        description: "Run deterministic local RLVR rollout traces.",
    },
    CliCommand {
        name: "train",
        usage: "fractal-rlvr train --method grpo --actor local-tiny-model --rollouts runs/rollout-001 --out adapters/router-rlvr-v0.1",
        description: "Register the adapter training command surface.",
    },
    CliCommand {
        name: "eval",
        usage: "fractal-rlvr eval --base local-tiny-model --adapter adapters/router-rlvr-v0.1 --out reports/router-rlvr-v0.1",
        description: "Register the before/after evaluation command surface.",
    },
    CliCommand {
        name: "eval-report",
        usage: "fractal-rlvr eval-report --input runs/rollout-001 --out reports/router-rlvr-v0.1",
        description: "Create RLVR metrics reports as JSON and HTML from local rollout traces.",
    },
    CliCommand {
        name: "promote",
        usage: "fractal-rlvr promote --adapter adapters/router-rlvr-v0.1 --if-passes-gate",
        description: "Register the adapter promotion command surface.",
    },
    CliCommand {
        name: "proof",
        usage: "fractal-rlvr proof --adapter adapters/router-rlvr-v0.1 --report reports/router-rlvr-v0.1 --local-only",
        description: "Register the hash-only proof generation command surface.",
    },
    CliCommand {
        name: "bench-proof-route",
        usage: "fractal-rlvr bench-proof-route [--iterations 1000]",
        description: "Run the local Proof of Route overhead benchmark.",
    },
    CliCommand {
        name: "release-gate",
        usage: "fractal-rlvr release-gate",
        description: "Print the v0.1 RLVR release-gate report.",
    },
    CliCommand {
        name: "export",
        usage: "fractal-rlvr export --adapter <id> --base-model <id> --method grpo --out adapters/<id> [--registry adapters/registry.json] [--rank 8]",
        description: "Export a loadable, hash-verified adapter bundle (weights, config, reward policy, eval report, model card, manifest).",
    },
];

pub fn run_argv(argv: &[String]) -> Result<String, RlvrError> {
    let command = argv.get(1).map(String::as_str).unwrap_or("help");
    if matches!(
        argv.get(2).map(String::as_str),
        Some("--help" | "-h" | "help")
    ) {
        return command_help(command).ok_or_else(|| RlvrError::UnsupportedCommand(command.into()));
    }
    match command {
        "help" | "--help" | "-h" => Ok(help_text()),
        "init" => init_command(argv),
        "config" => config_command(argv),
        "collect-traces" | "make-rubrics" | "eval" | "promote" | "proof" => {
            command_registered(command)
        }
        "eval-report" => eval_report_command(argv),
        "export" => export_command(argv),
        "train" => train_command(argv),
        "rollout" => rollout_command(argv),
        "bench-proof-route" => bench_proof_route_command(argv),
        "release-gate" => release_gate_command(),
        other => Err(RlvrError::UnsupportedCommand(other.into())),
    }
}

fn init_command(argv: &[String]) -> Result<String, RlvrError> {
    let root = value_after(argv, "--root")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let base = root.join("fractal_rlvr");
    for dir in [
        "data",
        "rubrics",
        "verifier",
        "simulator",
        "rewards",
        "trainer",
        "adapters",
        "evals",
        "chain",
        "api",
    ] {
        fs::create_dir_all(base.join(dir))?;
    }
    let config_path = root.join(DEFAULT_CONFIG_FILE);
    if !config_path.exists() {
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&config_path, default_config_yaml())?;
    }
    Ok(format!("initialized RLVR workspace at {}", base.display()))
}

fn config_command(argv: &[String]) -> Result<String, RlvrError> {
    match argv.get(2).map(String::as_str).unwrap_or("help") {
        "validate" => {
            let cfg = if let Some(path) = value_after(argv, "--config") {
                RlvrConfig::from_path(path)?
            } else if let Ok(path) = std::env::var("FRACTAL_RLVR_CONFIG") {
                RlvrConfig::from_path(path)?
            } else if Path::new(DEFAULT_CONFIG_FILE).exists() {
                RlvrConfig::from_path(DEFAULT_CONFIG_FILE)?
            } else {
                RlvrConfig::default()
            }
            .with_env_overrides()?;
            cfg.validate()?;
            Ok(format!(
                "config ok: mode={} local_only={} chain_commit_enabled={} hash={}",
                cfg.training_mode.as_str(),
                cfg.local_only,
                cfg.chain_commit_enabled,
                config_hash(&cfg)?
            ))
        }
        _ => Ok(command_help("config").expect("config command help exists")),
    }
}

fn help_text() -> String {
    let mut out = String::from("fractal-rlvr commands:");
    for command in CLI_COMMANDS {
        out.push_str("\n  ");
        out.push_str(command.usage);
    }
    out
}

fn command_help(command: &str) -> Option<String> {
    CLI_COMMANDS
        .iter()
        .find(|spec| spec.name == command)
        .map(|spec| format!("{}\n\n{}", spec.usage, spec.description))
}

fn command_registered(command: &str) -> Result<String, RlvrError> {
    let Some(spec) = CLI_COMMANDS.iter().find(|spec| spec.name == command) else {
        return Err(RlvrError::UnsupportedCommand(command.into()));
    };
    Ok(format!(
        "{}: registered for later implementation.\n{}",
        spec.name, spec.usage
    ))
}

/// RLVR-032: `train --mode dpo|sft` builds a fallback DPO/SFT dataset from
/// scored rollouts on a small machine. Other modes stay registered for GRPO.
fn train_command(argv: &[String]) -> Result<String, RlvrError> {
    if let Some(mode) = value_after(argv, "--mode") {
        if matches!(mode.as_str(), "dpo" | "sft") {
            return crate::trainer::dpo_sft::run_fallback_train_cli(argv);
        }
    }
    if let Some(method) = value_after(argv, "--method") {
        if matches!(method.as_str(), "grpo") {
            return crate::trainer::run_grpo_train_cli(argv);
        }
    }
    command_registered("train")
}

fn rollout_command(argv: &[String]) -> Result<String, RlvrError> {
    let n = value_after(argv, "--n")
        .as_deref()
        .unwrap_or("100")
        .parse::<usize>()
        .map_err(|_| RlvrError::Config("--n must be a positive integer".into()))?;
    if n == 0 {
        return Err(RlvrError::Config("--n must be greater than zero".into()));
    }
    let per_task = value_after(argv, "--per-task")
        .as_deref()
        .unwrap_or("1")
        .parse::<usize>()
        .map_err(|_| RlvrError::Config("--per-task must be a positive integer".into()))?;
    if per_task == 0 {
        return Err(RlvrError::Config(
            "--per-task must be greater than zero".into(),
        ));
    }
    let out = value_after(argv, "--out")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("runs/rollout-001"));
    let actor_model = value_after(argv, "--actor").unwrap_or_else(|| "local-tiny-model".into());
    let runtime = DeterministicLocalActorRuntime::new(actor_model.clone());
    // Repeat each generated task `per_task` times so GRPO (which needs ≥ 2
    // rollouts per prompt) can train from the demo data.
    let base_tasks = demo_rollout_tasks(n);
    let mut tasks = Vec::with_capacity(n * per_task);
    for _ in 0..per_task {
        tasks.extend(base_tasks.iter().cloned());
    }
    let report = run_rollout_batch(
        &runtime,
        RolloutRunnerInput {
            tasks,
            actor_id: actor_model,
            trace_id_prefix: "rollout".into(),
            max_turns: 3,
            simulator_mode: SimulatorMode::Clean,
        },
    )?;
    let paths = write_rollout_traces(&report, &out)?;
    Ok(format!(
        "rollout ok: traces={} out={}",
        paths.len(),
        out.to_string_lossy()
    ))
}

fn bench_proof_route_command(argv: &[String]) -> Result<String, RlvrError> {
    let iterations = value_after(argv, "--iterations")
        .as_deref()
        .unwrap_or("1000")
        .parse()
        .map_err(|_| RlvrError::Config("--iterations must be a positive integer".into()))?;
    let report = run_proof_route_benchmark(iterations)?;
    serde_json::to_string_pretty(&report).map_err(RlvrError::from)
}

fn release_gate_command() -> Result<String, RlvrError> {
    serde_json::to_string_pretty(&v01_release_gate_report()).map_err(RlvrError::from)
}

fn eval_report_command(argv: &[String]) -> Result<String, RlvrError> {
    let input = value_after(argv, "--input")
        .or_else(|| value_after(argv, "--traces"))
        .map(PathBuf::from)
        .ok_or_else(|| {
            RlvrError::UnsupportedCommand("eval-report requires --input <path>".into())
        })?;
    let out = value_after(argv, "--out")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("reports/rlvr-eval"));
    let files = write_eval_report(&input, &out)?;
    Ok(format!(
        "eval-report ok: json={} html={}",
        files.json_path, files.html_path
    ))
}

/// RLVR-035: export a loadable, hash-verified adapter bundle.
///
/// Builds a deterministic demo GRPO report (so the CLI produces a real,
/// end-to-end-loadable bundle from the shell); programmatic callers pass a real
/// `GrpoTrainerReport` to [`export_adapter_bundle`] directly.
fn export_command(argv: &[String]) -> Result<String, RlvrError> {
    let adapter_id = value_after(argv, "--adapter")
        .ok_or_else(|| RlvrError::UnsupportedCommand("export requires --adapter <id>".into()))?;
    let base_model_id = value_after(argv, "--base-model")
        .or_else(|| value_after(argv, "--base"))
        .ok_or_else(|| RlvrError::UnsupportedCommand("export requires --base-model <id>".into()))?;
    let out = value_after(argv, "--out")
        .map(PathBuf::from)
        .ok_or_else(|| RlvrError::UnsupportedCommand("export requires --out <dir>".into()))?;
    let rank = value_after(argv, "--rank")
        .map(|raw| {
            raw.parse::<u32>()
                .map_err(|_| RlvrError::Config("--rank must be a positive integer".into()))
        })
        .transpose()?
        .unwrap_or(DEFAULT_ADAPTER_RANK);
    let registry_path = value_after(argv, "--registry").map(PathBuf::from);

    let runtime = DeterministicLocalActorRuntime::new(&base_model_id);
    let rollouts = run_rollout_batch(
        &runtime,
        RolloutRunnerInput {
            tasks: demo_rollout_tasks(2),
            actor_id: base_model_id.clone(),
            trace_id_prefix: "export-rollout".into(),
            max_turns: 3,
            simulator_mode: SimulatorMode::Clean,
        },
    )?;
    // GRPO needs >= 2 rollouts per task_id, so duplicate the demo traces.
    let mut traces = rollouts.traces;
    let cloned = traces.clone();
    traces.extend(cloned);
    let report = train_grpo_adapter(GrpoTrainerInput {
        base_model_id: base_model_id.clone(),
        adapter_id: adapter_id.clone(),
        rollouts: traces,
        output_dir: std::env::temp_dir(),
        learning_rate: 0.05,
        epochs: 2,
    })?;

    let weights = synthesize_weights(&report, rank, default_target_modules(), DEFAULT_MODEL_DIM)?;
    let config = AdapterConfig {
        adapter_id: weights.adapter_id.clone(),
        base_model_id: weights.base_model_id.clone(),
        training_mode: weights.training_mode,
        rank: weights.rank,
        target_modules: weights.target_modules.clone(),
        max_turns: 3,
        data_local_only: true,
        base_model_hash: hash_bytes(weights.base_model_id.as_bytes()),
        created_from_checkpoint: Some(report.checkpoint_path.clone()),
    };
    let timestamp_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(1)
        .max(1);

    let export_report = export_adapter_bundle(
        AdapterExportInput {
            weights,
            config,
            reward_version: MVP_REWARD_POLICY_V01_ID.into(),
            timestamp_ms,
            registry_path,
        },
        &report,
        &out,
    )?;
    // Self-verify: the bundle the Fractal router/chat runtime would load.
    let _loaded = load_adapter_bundle(&export_report.out_dir)?;
    Ok(format!(
        "export ok: adapter={} adapter_hash={} out={} files={} loadable=true registered={}",
        export_report.adapter_id,
        export_report.adapter_hash,
        export_report.out_dir.display(),
        export_report.files.len(),
        export_report.registered,
    ))
}

fn value_after(argv: &[String], flag: &str) -> Option<String> {
    argv.windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
}

fn parse_bool(raw: &str, key: &str) -> Result<bool, RlvrError> {
    match raw.trim().trim_matches('"').to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        _ => Err(RlvrError::Config(format!("{key} must be true or false"))),
    }
}

fn parse_string(raw: &str) -> String {
    raw.trim().trim_matches('"').to_string()
}

fn escape_yaml_string(raw: &str) -> String {
    raw.replace('\\', "\\\\").replace('"', "\\\"")
}

fn parse_flag_value(raw: Option<&str>, default: bool) -> bool {
    raw.map(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on" | "enabled"
        )
    })
    .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_validates() {
        let cfg = RlvrConfig::default();
        cfg.validate().unwrap();
        assert!(cfg.local_only);
        assert!(!cfg.chain_commit_enabled);
        assert!(!cfg.raw_data_on_chain);
    }

    #[test]
    fn empty_config_loads_as_default() {
        let cfg = RlvrConfig::from_yaml_str("").unwrap();
        assert_eq!(cfg, RlvrConfig::default());
    }

    #[test]
    fn yaml_round_trip_preserves_default() {
        let cfg = RlvrConfig::default();
        let parsed = RlvrConfig::from_yaml_str(&cfg.to_yaml()).unwrap();
        assert_eq!(parsed, cfg);
    }

    #[test]
    fn committed_default_config_file_matches_embedded_defaults() {
        let parsed = RlvrConfig::from_yaml_str(default_config_yaml()).unwrap();
        assert_eq!(parsed, RlvrConfig::default());
        assert!(default_config_yaml().contains("local_only: true"));
        assert!(default_config_yaml().contains("raw_data_on_chain: false"));
    }

    #[test]
    fn init_writes_committed_default_config_template() {
        let root =
            std::env::temp_dir().join(format!("fractal-rlvr-init-config-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let argv = vec![
            "fractal-rlvr".into(),
            "init".into(),
            "--root".into(),
            root.display().to_string(),
        ];
        run_argv(&argv).unwrap();
        let written = fs::read_to_string(root.join(DEFAULT_CONFIG_FILE)).unwrap();
        assert_eq!(written, default_config_yaml());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn raw_data_on_chain_is_rejected() {
        let err = RlvrConfig::from_yaml_str("raw_data_on_chain: true").unwrap_err();
        assert!(err.to_string().contains("raw_data_on_chain"));
    }

    #[test]
    fn cli_help_registers_phase_zero_commands() {
        let out = run_argv(&["fractal-rlvr".into(), "--help".into()]).unwrap();
        assert!(out.contains("config validate"));
        assert!(out.contains("rollout"));
    }

    #[test]
    fn cli_each_registered_command_has_help_text() {
        for command in [
            "init",
            "config",
            "collect-traces",
            "make-rubrics",
            "rollout",
            "train",
            "eval",
            "promote",
            "proof",
            "bench-proof-route",
            "release-gate",
            "export",
        ] {
            let out = run_argv(&["fractal-rlvr".into(), command.into(), "--help".into()]).unwrap();
            assert!(out.contains("fractal-rlvr"), "{command} help missing usage");
            assert!(out.contains(command), "{command} help missing command name");
        }
    }

    #[test]
    fn cli_future_phase_commands_exit_cleanly_with_registered_usage() {
        for command in [
            "collect-traces",
            "make-rubrics",
            "train",
            "eval",
            "promote",
            "proof",
        ] {
            let out = run_argv(&["fractal-rlvr".into(), command.into()]).unwrap();
            assert!(out.contains("registered for later implementation"));
            assert!(out.contains(command));
        }
    }

    #[test]
    fn cli_rollout_writes_requested_trace_files() {
        let out_dir =
            std::env::temp_dir().join(format!("fractal-rlvr-cli-rollout-{}", std::process::id()));
        let _ = fs::remove_dir_all(&out_dir);
        let out = run_argv(&[
            "fractal-rlvr".into(),
            "rollout".into(),
            "--n".into(),
            "3".into(),
            "--out".into(),
            out_dir.display().to_string(),
        ])
        .unwrap();

        assert!(out.contains("rollout ok: traces=3"));
        let trace_count = fs::read_dir(&out_dir).unwrap().count();
        assert_eq!(trace_count, 3);
        let _ = fs::remove_dir_all(out_dir);
    }

    #[test]
    fn cli_export_writes_loadable_bundle() {
        let out_dir =
            std::env::temp_dir().join(format!("fractal-rlvr-cli-export-{}", std::process::id()));
        let _ = fs::remove_dir_all(&out_dir);
        let out = run_argv(&[
            "fractal-rlvr".into(),
            "export".into(),
            "--adapter".into(),
            "cli-demo-router".into(),
            "--base-model".into(),
            "tiny-router-base".into(),
            "--rank".into(),
            "4".into(),
            "--out".into(),
            out_dir.display().to_string(),
        ])
        .unwrap();

        assert!(out.contains("export ok:"), "got: {out}");
        assert!(out.contains("loadable=true"), "got: {out}");
        for file in [
            "adapter_weights.json",
            "adapter_config.json",
            "reward_policy.json",
            "eval_report.json",
            "model_card.json",
            "manifest.json",
        ] {
            assert!(out_dir.join(file).exists(), "missing {file}");
        }
        let loaded = load_adapter_bundle(&out_dir).unwrap();
        assert_eq!(loaded.weights.adapter_id, "cli-demo-router");
        assert_eq!(loaded.weights.rank, 4);
        let _ = fs::remove_dir_all(out_dir);
    }

    #[test]
    fn rlvr_node_flags_default_to_disabled_and_local_hash_only() {
        let flags = RlvrNodeFlags::from_values(None, None, None);
        assert!(!flags.enabled);
        assert!(!flags.chain_commit_enabled);
        assert!(!flags.raw_data_on_chain);
        assert!(!flags.raw_data_on_chain_requested);
    }

    #[test]
    fn rlvr_node_flags_record_but_never_enable_raw_on_chain_data() {
        let flags = RlvrNodeFlags::from_values(Some("true"), Some("1"), Some("enabled"));
        assert!(flags.enabled);
        assert!(flags.chain_commit_enabled);
        assert!(flags.raw_data_on_chain_requested);
        assert!(!flags.raw_data_on_chain);
    }

    #[test]
    fn route_policy_hash_is_stable_and_field_sensitive() {
        let policy = RoutePolicy::default();
        policy.validate().unwrap();
        assert_eq!(policy.policy_id, DEFAULT_ROUTE_POLICY_ID);
        assert_eq!(policy.default_route, "tiny-local-model");
        assert!(policy.rules.iter().any(|rule| {
            rule.task_type == "current_public_info"
                && rule.tool_required.as_deref() == Some("web_search")
                && rule.max_cost == Some(0.01)
                && rule.max_latency_ms == Some(15_000)
        }));
        assert!(policy.rules.iter().any(|rule| {
            rule.privacy_requirement == "local_only"
                && rule.route == "local-file-model"
                && rule.escalation.as_deref() == Some("ask_user_for_explicit_cloud_approval")
        }));
        let hash = route_policy_hash(&policy).unwrap();
        assert_eq!(hash, route_policy_hash(&policy).unwrap());
        let mut changed = policy.clone();
        changed.rules[0].required_capability = "frontier_reasoning".into();
        assert_ne!(hash, route_policy_hash(&changed).unwrap());
    }

    #[test]
    fn core_schemas_round_trip_and_hash() {
        let item = TrainingItem {
            task_id: "task-1".into(),
            mode: TrainingMode::RouteCorrectness,
            visible_user_query: "What is the current price?".into(),
            hidden_original_query: "What is the current price of this mini PC?".into(),
            gold_answer: "Use a current lookup before answering.".into(),
            domain: "shopping".into(),
            difficulty: Difficulty::Medium,
            checkpoints: vec![Checkpoint {
                checkpoint_id: "c1".into(),
                checkpoint_type: CheckpointType::ToolRequirement,
                description: "Current price requires lookup.".into(),
                must_resolve_before_answer: true,
                answer_if_asked: "Use web/product search.".into(),
                failure_penalty: 0.75,
            }],
            route_policy: RoutePolicy::default(),
            privacy_policy: PrivacyPolicy::default(),
        };
        let json = serde_json::to_string(&item).unwrap();
        let parsed: TrainingItem = serde_json::from_str(&json).unwrap();
        parsed.validate().unwrap();
        assert_eq!(parsed, item);
        assert_eq!(stable_hash(&item).unwrap(), stable_hash(&parsed).unwrap());
        assert_eq!(item.stable_hash().unwrap(), parsed.stable_hash().unwrap());

        let trace = DialogueTrace {
            trace_id: "trace-1".into(),
            task_id: item.task_id.clone(),
            turns: vec![DialogueTurn {
                role: "assistant".into(),
                content: "I need current price data before answering.".into(),
                model_id: Some("tiny-router".into()),
                route_decision: Some("web-enabled model".into()),
                latency_ms: Some(12),
                cost_estimate: Some(0.0),
            }],
            verifier_outputs: vec![VerifierOutput {
                is_final_answer: false,
                is_clarification_question: true,
                targeted_checkpoints: vec!["c1".into()],
                missed_checkpoints: Vec::new(),
                redundant_question: false,
                premature_answer: false,
                false_premise_corrected: None,
                route_valid: true,
                reward: 0.6,
            }],
            reward_vector: RewardVector {
                correctness: 0.0,
                checkpoint_coverage: 1.0,
                clarification_quality: 0.8,
                false_premise_detection: 0.0,
                route_correctness: 1.0,
                tool_use_correctness: 1.0,
                cost_efficiency: 1.0,
                latency_efficiency: 1.0,
                privacy_compliance: 1.0,
                non_redundancy: 1.0,
            },
            final_reward: 0.72,
        };
        let parsed_trace: DialogueTrace =
            serde_json::from_str(&serde_json::to_string(&trace).unwrap()).unwrap();
        parsed_trace.validate().unwrap();
        assert_eq!(parsed_trace, trace);
        assert_eq!(
            trace.stable_hash().unwrap(),
            parsed_trace.stable_hash().unwrap()
        );
    }

    #[test]
    fn core_schema_validation_rejects_invalid_records() {
        let mut policy = RoutePolicy::default();
        policy.rules.clear();
        assert!(policy.validate().is_err());
        let mut invalid_rule = RoutePolicy::default();
        invalid_rule.rules[0].max_cost = Some(f64::NAN);
        assert!(invalid_rule.validate().is_err());

        let mut item = TrainingItem {
            task_id: String::new(),
            mode: TrainingMode::AskMind,
            visible_user_query: "What capacitor do I need?".into(),
            hidden_original_query: "What capacitor do I need for an Xbox board?".into(),
            gold_answer: "Ask for value, voltage, package, and board context.".into(),
            domain: "electronics".into(),
            difficulty: Difficulty::Easy,
            checkpoints: vec![Checkpoint {
                checkpoint_id: "c1".into(),
                checkpoint_type: CheckpointType::MissingInfo,
                description: "Voltage rating is needed.".into(),
                must_resolve_before_answer: true,
                answer_if_asked: "6.3V or higher preferred.".into(),
                failure_penalty: 0.75,
            }],
            route_policy: RoutePolicy::default(),
            privacy_policy: PrivacyPolicy::default(),
        };
        assert!(item.validate().is_err());
        item.task_id = "task-askmind-1".into();
        item.checkpoints[0].failure_penalty = f64::NAN;
        assert!(item.validate().is_err());

        let trace = DialogueTrace {
            trace_id: "trace-invalid".into(),
            task_id: "task-askmind-1".into(),
            turns: Vec::new(),
            verifier_outputs: Vec::new(),
            reward_vector: RewardVector {
                correctness: 0.0,
                checkpoint_coverage: 0.0,
                clarification_quality: 0.0,
                false_premise_detection: 0.0,
                route_correctness: 0.0,
                tool_use_correctness: 0.0,
                cost_efficiency: 0.0,
                latency_efficiency: 0.0,
                privacy_compliance: 1.0,
                non_redundancy: 1.0,
            },
            final_reward: 0.0,
        };
        assert!(trace.validate().is_err());
    }

    #[test]
    fn privacy_scan_detects_required_private_trace_tags() {
        let text = concat!(
            "Email me at user@example.com or call (555) 123-4567. ",
            "Ship to 123 Main Street. ",
            "Use key sk-or-v1-abcdef1234567890abcdef1234567890. ",
            "Credit card 4242 4242 4242 4242 and routing number are in the file. ",
            "Patient diagnosis includes blood pressure medication. ",
            "Attorney says the contract is privileged. ",
            "Private file: /Users/alice/Documents/tax.pdf"
        );
        let scan = scan_privacy_tags(text);
        assert!(scan.is_private);
        for tag in [
            PrivacyTag::Email,
            PrivacyTag::PhoneNumber,
            PrivacyTag::Address,
            PrivacyTag::ApiKey,
            PrivacyTag::FinancialData,
            PrivacyTag::HealthData,
            PrivacyTag::LegalData,
            PrivacyTag::PrivateFile,
        ] {
            assert!(scan.tags.contains(&tag), "missing privacy tag {tag:?}");
        }
    }

    #[test]
    fn private_scan_enforces_local_only_and_blocks_export_without_approval() {
        let scan = scan_privacy_tags("My API key is sk-test-secret and my file is ~/private.txt");
        let policy = scan.policy(false);
        assert!(policy.local_only);
        assert!(!policy.allow_external_models);
        assert!(!policy.allow_export);
        assert!(policy.pii_tags.contains(&"api_key".to_string()));
        assert!(policy.pii_tags.contains(&"private_file".to_string()));
        policy.validate().unwrap();

        let approved_policy = scan.policy(true);
        assert!(approved_policy.allow_export);
        assert!(approved_policy.local_only);
        assert!(!approved_policy.allow_external_models);
        approved_policy.validate().unwrap();
    }

    #[test]
    fn public_scan_allows_non_private_external_policy() {
        let scan =
            scan_privacy_tags("Explain why proof-of-route hashes should not include raw data.");
        assert!(!scan.is_private);
        assert!(scan.tags.is_empty());
        let policy = scan.policy(false);
        assert!(!policy.local_only);
        assert!(policy.allow_external_models);
        assert!(!policy.allow_export);
        policy.validate().unwrap();
    }

    #[test]
    fn trace_hash_commitment_hashes_raw_redacted_verifier_and_reward_data() {
        let trace = private_trace_fixture();
        let commitment = trace.trace_hash_commitment().unwrap();
        assert_eq!(commitment.trace_id, "trace-private-1");
        assert_eq!(commitment.task_id, "task-private-1");
        assert_eq!(commitment.trace_hash, trace.raw_trace_hash().unwrap());
        assert_eq!(
            commitment.redacted_trace_hash,
            trace.redacted_trace_hash().unwrap()
        );
        assert_eq!(
            commitment.verifier_outputs_hash,
            trace.verifier_outputs_hash().unwrap()
        );
        assert_eq!(
            commitment.reward_vector_hash,
            trace.reward_vector_hash().unwrap()
        );
        assert!(commitment.privacy_tags.contains(&"email".to_string()));
        assert!(commitment.privacy_tags.contains(&"api_key".to_string()));
    }

    #[test]
    fn redacted_trace_and_commitment_do_not_serialize_raw_content() {
        let trace = private_trace_fixture();
        let redacted_json = serde_json::to_string(&trace.redacted_trace().unwrap()).unwrap();
        let commitment_json =
            serde_json::to_string(&trace.trace_hash_commitment().unwrap()).unwrap();
        for raw in [
            "user@example.com",
            "sk-test-super-secret-token-1234567890",
            "My private answer",
        ] {
            assert!(!redacted_json.contains(raw), "redacted trace leaked {raw}");
            assert!(!commitment_json.contains(raw), "commitment leaked {raw}");
        }
        assert!(redacted_json.contains("content_hash"));
        assert!(commitment_json.contains("trace_hash"));
    }

    #[test]
    fn proof_object_is_hash_only_and_rejects_malformed_hashes() {
        let trace = private_trace_fixture();
        let commitment = trace.trace_hash_commitment().unwrap();
        let proof = RlvrProofObject::from_trace_commitment(
            RlvrProofType::ProofOfRoute,
            &commitment,
            stable_hash(&DEFAULT_REWARD_POLICY).unwrap(),
            route_policy_hash(&RoutePolicy::default()).unwrap(),
            hash_bytes(b"tiny-local-model"),
            1,
            "sig-test",
        );
        proof.validate_hash_only().unwrap();
        let proof_json = serde_json::to_string(&proof).unwrap();
        for raw in [
            "user@example.com",
            "sk-test-super-secret-token-1234567890",
            "My private answer",
            "raw_prompt",
        ] {
            assert!(!proof_json.contains(raw), "proof object leaked {raw}");
        }

        let mut invalid = proof;
        invalid.trace_hash = "raw prompt leak".into();
        assert!(invalid.validate_hash_only().is_err());
    }

    #[test]
    fn adversarial_privacy_suite_blocks_chain_payload_leaks() {
        let report = run_adversarial_privacy_suite().unwrap();
        assert!(report.passed());
        assert_eq!(report.results.len(), 5);
        assert!(report.malicious_raw_prompt_rejected);
        assert!(report.results.iter().all(|result| {
            result.local_only && !result.allow_external_models && result.chain_payload_raw_data_free
        }));
    }

    #[test]
    fn proof_of_route_benchmark_reports_overhead_metrics() {
        let report = run_proof_route_benchmark(8).unwrap();
        assert_eq!(report.iterations, 8);
        assert!(report.proof_submission_throughput_per_sec > 0.0);
        assert!(report.proof_verification_time_ms_avg >= 0.0);
        assert!(report.proof_index_query_latency_ms_avg >= 0.0);
        assert!(report.block_inclusion_latency_ms_estimate > 0.0);
        assert!(report.proof_payload_bytes > report.normal_proof_payload_bytes);
        assert!(report.payload_byte_overhead > 0);
    }

    #[test]
    fn cli_bench_proof_route_returns_json_report() {
        let out = run_argv(&[
            "fractal-rlvr".into(),
            "bench-proof-route".into(),
            "--iterations".into(),
            "4".into(),
        ])
        .unwrap();
        let report: ProofRouteBenchmarkReport = serde_json::from_str(&out).unwrap();

        assert_eq!(report.iterations, 4);
        assert!(report.proof_submission_throughput_per_sec > 0.0);
        assert!(report.proof_verification_time_ms_avg >= 0.0);
        assert!(report.proof_index_query_latency_ms_avg >= 0.0);
        assert!(report.block_inclusion_latency_ms_estimate > 0.0);
        assert!(report.proof_payload_bytes > report.normal_proof_payload_bytes);
        assert_eq!(
            report.payload_byte_overhead,
            report.proof_payload_bytes as isize - report.normal_proof_payload_bytes as isize
        );
    }

    #[test]
    fn release_gate_report_passes_all_v01_items() {
        let report = v01_release_gate_report();
        assert_eq!(report.version, "v0.1");
        assert!(report.passed);
        assert_eq!(report.items.len(), 11);
        assert!(report.items.iter().all(|item| item.passed));
        assert!(report
            .items
            .iter()
            .any(|item| item.name == "proof hash can be generated" && item.passed));
        assert!(report.items.iter().any(|item| {
            item.name == "proof hash can be committed by running Fractal Chain node" && item.passed
        }));
        assert!(report.failed_items().is_empty());
    }

    #[test]
    fn cli_release_gate_returns_passing_json_report() {
        let out = run_argv(&["fractal-rlvr".into(), "release-gate".into()]).unwrap();
        let report: V01ReleaseGateReport = serde_json::from_str(&out).unwrap();

        assert_eq!(report.version, "v0.1");
        assert!(report.passed);
        assert!(report.failed_items().is_empty());
    }

    fn private_trace_fixture() -> DialogueTrace {
        DialogueTrace {
            trace_id: "trace-private-1".into(),
            task_id: "task-private-1".into(),
            turns: vec![
                DialogueTurn {
                    role: "user".into(),
                    content: "My email is user@example.com and key is sk-test-super-secret-token-1234567890".into(),
                    model_id: None,
                    route_decision: Some("local-only".into()),
                    latency_ms: Some(0),
                    cost_estimate: Some(0.0),
                },
                DialogueTurn {
                    role: "assistant".into(),
                    content: "My private answer should not appear on-chain.".into(),
                    model_id: Some("tiny-local-model".into()),
                    route_decision: Some("local-only".into()),
                    latency_ms: Some(15),
                    cost_estimate: Some(0.0),
                },
            ],
            verifier_outputs: vec![VerifierOutput {
                is_final_answer: true,
                is_clarification_question: false,
                targeted_checkpoints: vec!["privacy".into()],
                missed_checkpoints: Vec::new(),
                redundant_question: false,
                premature_answer: false,
                false_premise_corrected: None,
                route_valid: true,
                reward: 1.0,
            }],
            reward_vector: RewardVector {
                correctness: 1.0,
                checkpoint_coverage: 1.0,
                clarification_quality: 0.0,
                false_premise_detection: 0.0,
                route_correctness: 1.0,
                tool_use_correctness: 0.0,
                cost_efficiency: 1.0,
                latency_efficiency: 1.0,
                privacy_compliance: 1.0,
                non_redundancy: 1.0,
            },
            final_reward: 0.9,
        }
    }
}
