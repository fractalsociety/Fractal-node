//! Core RLVR schemas used by trace collection, rubrics, verifier outputs, rewards,
//! and hash-only chain commitments.

use serde::{Deserialize, Serialize};

use crate::{stable_hash, RlvrError, DEFAULT_ROUTE_POLICY_ID};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrainingMode {
    AskMind,
    AskOverconfidence,
    RouteCorrectness,
    ToolUse,
    CompressionLoss,
}

impl TrainingMode {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().trim_matches('"') {
            "AskMind" | "askmind" | "ask-mind" => Some(Self::AskMind),
            "AskOverconfidence" | "askoverconfidence" | "ask-overconfidence" => {
                Some(Self::AskOverconfidence)
            }
            "RouteCorrectness" | "routecorrectness" | "route-correctness" => {
                Some(Self::RouteCorrectness)
            }
            "ToolUse" | "tooluse" | "tool-use" => Some(Self::ToolUse),
            "CompressionLoss" | "compressionloss" | "compression-loss" => {
                Some(Self::CompressionLoss)
            }
            _ => None,
        }
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AskMind => "AskMind",
            Self::AskOverconfidence => "AskOverconfidence",
            Self::RouteCorrectness => "RouteCorrectness",
            Self::ToolUse => "ToolUse",
            Self::CompressionLoss => "CompressionLoss",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckpointType {
    MissingInfo,
    FalsePremise,
    RouteRequirement,
    ToolRequirement,
    CompressionFact,
    CostPolicy,
    AnswerQuality,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub checkpoint_id: String,
    pub checkpoint_type: CheckpointType,
    pub description: String,
    pub must_resolve_before_answer: bool,
    pub answer_if_asked: String,
    pub failure_penalty: f64,
}

impl Checkpoint {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("checkpoint_id", &self.checkpoint_id)?;
        require_non_empty("checkpoint.description", &self.description)?;
        require_finite_non_negative("checkpoint.failure_penalty", self.failure_penalty)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrivacyPolicy {
    pub local_only: bool,
    pub allow_external_models: bool,
    pub allow_export: bool,
    pub pii_tags: Vec<String>,
}

impl Default for PrivacyPolicy {
    fn default() -> Self {
        Self {
            local_only: true,
            allow_external_models: false,
            allow_export: false,
            pii_tags: Vec::new(),
        }
    }
}

impl PrivacyPolicy {
    pub fn validate(&self) -> Result<(), RlvrError> {
        if self.local_only && self.allow_export {
            return Err(RlvrError::Config(
                "local_only privacy policy cannot allow export by default".into(),
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteRule {
    pub condition: String,
    pub task_type: String,
    pub privacy_requirement: String,
    pub required_capability: String,
    pub max_cost: Option<f64>,
    pub max_latency_ms: Option<u64>,
    pub tool_required: Option<String>,
    pub escalation: Option<String>,
    pub route: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoutePolicy {
    pub policy_id: String,
    pub description: String,
    pub default_route: String,
    pub rules: Vec<RouteRule>,
}

impl Default for RoutePolicy {
    fn default() -> Self {
        Self {
            policy_id: DEFAULT_ROUTE_POLICY_ID.into(),
            description: "Default local-first RLVR router policy for proof-of-route checks".into(),
            default_route: "tiny-local-model".into(),
            rules: vec![
                RouteRule {
                    condition: "simple stable knowledge".into(),
                    task_type: "stable_knowledge".into(),
                    privacy_requirement: "any".into(),
                    required_capability: "general_qa".into(),
                    max_cost: Some(0.0),
                    max_latency_ms: Some(2_000),
                    tool_required: None,
                    escalation: None,
                    route: "tiny-local-model".into(),
                },
                RouteRule {
                    condition: "current public information".into(),
                    task_type: "current_public_info".into(),
                    privacy_requirement: "public_or_user_approved_cloud".into(),
                    required_capability: "web_or_current_info".into(),
                    max_cost: Some(0.01),
                    max_latency_ms: Some(15_000),
                    tool_required: Some("web_search".into()),
                    escalation: Some("web-enabled model".into()),
                    route: "web-enabled model".into(),
                },
                RouteRule {
                    condition: "private user file".into(),
                    task_type: "private_file_analysis".into(),
                    privacy_requirement: "local_only".into(),
                    required_capability: "local_file_analysis".into(),
                    max_cost: Some(0.0),
                    max_latency_ms: Some(10_000),
                    tool_required: Some("local_file_reader".into()),
                    escalation: Some("ask_user_for_explicit_cloud_approval".into()),
                    route: "local-file-model".into(),
                },
                RouteRule {
                    condition: "high-stakes medical/legal/financial".into(),
                    task_type: "high_stakes_advice".into(),
                    privacy_requirement: "user_approved_or_local_only".into(),
                    required_capability: "high_stakes_reasoning_with_verification".into(),
                    max_cost: None,
                    max_latency_ms: Some(30_000),
                    tool_required: Some("domain_verifier".into()),
                    escalation: Some("ask-clarifying-question-or-escalate".into()),
                    route: "ask-clarifying-question-or-escalate".into(),
                },
                RouteRule {
                    condition: "code implementation".into(),
                    task_type: "code_implementation".into(),
                    privacy_requirement: "respect_prompt_privacy".into(),
                    required_capability: "code_generation".into(),
                    max_cost: Some(0.05),
                    max_latency_ms: Some(30_000),
                    tool_required: None,
                    escalation: Some("coding-specialist-model".into()),
                    route: "coding-specialist-model".into(),
                },
            ],
        }
    }
}

impl RoutePolicy {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("route_policy.policy_id", &self.policy_id)?;
        require_non_empty("route_policy.description", &self.description)?;
        require_non_empty("route_policy.default_route", &self.default_route)?;
        if self.rules.is_empty() {
            return Err(RlvrError::Config(
                "route_policy.rules must contain at least one rule".into(),
            ));
        }
        for (idx, rule) in self.rules.iter().enumerate() {
            require_non_empty(
                &format!("route_policy.rules[{idx}].condition"),
                &rule.condition,
            )?;
            require_non_empty(
                &format!("route_policy.rules[{idx}].task_type"),
                &rule.task_type,
            )?;
            require_non_empty(
                &format!("route_policy.rules[{idx}].privacy_requirement"),
                &rule.privacy_requirement,
            )?;
            require_non_empty(
                &format!("route_policy.rules[{idx}].required_capability"),
                &rule.required_capability,
            )?;
            require_non_empty(&format!("route_policy.rules[{idx}].route"), &rule.route)?;
            if let Some(max_cost) = rule.max_cost {
                require_finite_non_negative(
                    &format!("route_policy.rules[{idx}].max_cost"),
                    max_cost,
                )?;
            }
            if let Some(tool) = &rule.tool_required {
                require_non_empty(&format!("route_policy.rules[{idx}].tool_required"), tool)?;
            }
            if let Some(escalation) = &rule.escalation {
                require_non_empty(&format!("route_policy.rules[{idx}].escalation"), escalation)?;
            }
        }
        Ok(())
    }

    pub fn stable_hash(&self) -> Result<String, RlvrError> {
        self.validate()?;
        stable_hash(self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrainingItem {
    pub task_id: String,
    pub mode: TrainingMode,
    pub visible_user_query: String,
    pub hidden_original_query: String,
    pub gold_answer: String,
    pub domain: String,
    pub difficulty: Difficulty,
    pub checkpoints: Vec<Checkpoint>,
    pub route_policy: RoutePolicy,
    pub privacy_policy: PrivacyPolicy,
}

impl TrainingItem {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("training_item.task_id", &self.task_id)?;
        require_non_empty("training_item.visible_user_query", &self.visible_user_query)?;
        require_non_empty("training_item.domain", &self.domain)?;
        if self.checkpoints.is_empty() {
            return Err(RlvrError::Config(
                "training_item.checkpoints must contain at least one checkpoint".into(),
            ));
        }
        for checkpoint in &self.checkpoints {
            checkpoint.validate()?;
        }
        self.route_policy.validate()?;
        self.privacy_policy.validate()
    }

    pub fn stable_hash(&self) -> Result<String, RlvrError> {
        self.validate()?;
        stable_hash(self)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DialogueTurn {
    pub role: String,
    pub content: String,
    pub model_id: Option<String>,
    pub route_decision: Option<String>,
    pub latency_ms: Option<u64>,
    pub cost_estimate: Option<f64>,
}

impl DialogueTurn {
    pub fn validate(&self) -> Result<(), RlvrError> {
        match self.role.as_str() {
            "user" | "assistant" | "tool" | "system" | "simulated_user" => {}
            _ => {
                return Err(RlvrError::Config(format!(
                    "dialogue_turn.role {:?} is not supported",
                    self.role
                )));
            }
        }
        require_non_empty("dialogue_turn.content", &self.content)?;
        if let Some(cost) = self.cost_estimate {
            require_finite_non_negative("dialogue_turn.cost_estimate", cost)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerifierOutput {
    pub is_final_answer: bool,
    pub is_clarification_question: bool,
    pub targeted_checkpoints: Vec<String>,
    pub missed_checkpoints: Vec<String>,
    pub redundant_question: bool,
    pub premature_answer: bool,
    pub false_premise_corrected: Option<bool>,
    pub route_valid: bool,
    pub reward: f64,
}

impl VerifierOutput {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_finite("verifier_output.reward", self.reward)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RewardVector {
    pub correctness: f64,
    pub checkpoint_coverage: f64,
    pub clarification_quality: f64,
    pub false_premise_detection: f64,
    pub route_correctness: f64,
    pub tool_use_correctness: f64,
    pub cost_efficiency: f64,
    pub latency_efficiency: f64,
    pub privacy_compliance: f64,
    pub non_redundancy: f64,
}

impl RewardVector {
    pub fn validate(&self) -> Result<(), RlvrError> {
        for (name, value) in [
            ("correctness", self.correctness),
            ("checkpoint_coverage", self.checkpoint_coverage),
            ("clarification_quality", self.clarification_quality),
            ("false_premise_detection", self.false_premise_detection),
            ("route_correctness", self.route_correctness),
            ("tool_use_correctness", self.tool_use_correctness),
            ("cost_efficiency", self.cost_efficiency),
            ("latency_efficiency", self.latency_efficiency),
            ("privacy_compliance", self.privacy_compliance),
            ("non_redundancy", self.non_redundancy),
        ] {
            require_finite(&format!("reward_vector.{name}"), value)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DialogueTrace {
    pub trace_id: String,
    pub task_id: String,
    pub turns: Vec<DialogueTurn>,
    pub verifier_outputs: Vec<VerifierOutput>,
    pub reward_vector: RewardVector,
    pub final_reward: f64,
}

impl DialogueTrace {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("dialogue_trace.trace_id", &self.trace_id)?;
        require_non_empty("dialogue_trace.task_id", &self.task_id)?;
        if self.turns.is_empty() {
            return Err(RlvrError::Config(
                "dialogue_trace.turns must contain at least one turn".into(),
            ));
        }
        for turn in &self.turns {
            turn.validate()?;
        }
        for output in &self.verifier_outputs {
            output.validate()?;
        }
        self.reward_vector.validate()?;
        require_finite("dialogue_trace.final_reward", self.final_reward)
    }

    pub fn stable_hash(&self) -> Result<String, RlvrError> {
        self.validate()?;
        stable_hash(self)
    }
}

fn require_non_empty(name: &str, value: &str) -> Result<(), RlvrError> {
    if value.trim().is_empty() {
        return Err(RlvrError::Config(format!("{name} cannot be empty")));
    }
    Ok(())
}

fn require_finite(name: &str, value: f64) -> Result<(), RlvrError> {
    if !value.is_finite() {
        return Err(RlvrError::Config(format!("{name} must be finite")));
    }
    Ok(())
}

fn require_finite_non_negative(name: &str, value: f64) -> Result<(), RlvrError> {
    require_finite(name, value)?;
    if value < 0.0 {
        return Err(RlvrError::Config(format!("{name} cannot be negative")));
    }
    Ok(())
}
