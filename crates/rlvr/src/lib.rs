pub mod adapters;
pub mod api;
pub mod chain;
pub mod data;
pub mod evals;
pub mod rewards;
pub mod rubrics;
pub mod simulator;
pub mod trainer;
pub mod verifier;

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

pub use data::{
    Checkpoint, CheckpointType, DialogueTrace, DialogueTurn, Difficulty, PrivacyPolicy,
    RewardVector, RoutePolicy, RouteRule, TrainingItem, TrainingMode, VerifierOutput,
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
        let raw_requested = env_flag("FRACTAL_RLVR_RAW_DATA_ON_CHAIN", false);
        Self {
            enabled: env_flag("FRACTAL_RLVR_ENABLED", false),
            chain_commit_enabled: env_flag("FRACTAL_RLVR_CHAIN_COMMIT_ENABLED", false),
            raw_data_on_chain: false,
            raw_data_on_chain_requested: raw_requested,
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

pub fn run_argv(argv: &[String]) -> Result<String, RlvrError> {
    let command = argv.get(1).map(String::as_str).unwrap_or("help");
    match command {
        "help" | "--help" | "-h" => Ok(help_text()),
        "init" => init_command(argv),
        "config" => config_command(argv),
        "collect-traces" | "make-rubrics" | "rollout" | "train" | "eval" | "promote" | "proof" => {
            Ok(format!(
                "{command}: command registered; implementation starts after Phase 0"
            ))
        }
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
        _ => Ok("usage: fractal-rlvr config validate [--config path]".into()),
    }
}

fn help_text() -> String {
    [
        "fractal-rlvr commands:",
        "  init [--root path]",
        "  config validate [--config path]",
        "  collect-traces",
        "  make-rubrics",
        "  rollout",
        "  train",
        "  eval",
        "  promote",
        "  proof",
    ]
    .join("\n")
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

fn env_flag(name: &str, default: bool) -> bool {
    match std::env::var(name) {
        Ok(raw) => matches!(
            raw.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on" | "enabled"
        ),
        Err(_) => default,
    }
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
}
