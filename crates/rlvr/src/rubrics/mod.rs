//! Rubric generation modules: AskMind, AskOverconfidence, RouteCorrectness, ToolUse, and CompressionLoss.

pub mod ask_overconfidence;

pub use ask_overconfidence::{
    generate_ask_overconfidence_rubric, sample_fixtures, AskOverconfidenceFixture,
    AskOverconfidenceRubric, AskOverconfidenceRubricInput,
};

use serde::{Deserialize, Serialize};

use crate::{
    Checkpoint, CheckpointType, Difficulty, PrivacyPolicy, RlvrError, RoutePolicy, RouteRule,
    RouteTraceRow, TrainingItem, TrainingMode,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelInventoryItem {
    pub model_id: String,
    pub local: bool,
    pub capabilities: Vec<String>,
    pub max_cost: Option<f64>,
    pub max_latency_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolInventoryItem {
    pub tool_id: String,
    pub supports_current_info: bool,
    pub safe_for_private_data: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteCorrectnessRubricInput {
    pub trace: RouteTraceRow,
    pub visible_prompt: Option<String>,
    pub models: Vec<ModelInventoryItem>,
    pub tools: Vec<ToolInventoryItem>,
    pub route_policy: RoutePolicy,
}

impl RouteCorrectnessRubricInput {
    pub fn validate(&self) -> Result<(), RlvrError> {
        self.trace.validate()?;
        self.route_policy.validate()?;
        for model in &self.models {
            require_non_empty("model_inventory.model_id", &model.model_id)?;
            if let Some(cost) = model.max_cost {
                require_finite_non_negative("model_inventory.max_cost", cost)?;
            }
        }
        for tool in &self.tools {
            require_non_empty("tool_inventory.tool_id", &tool.tool_id)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ToolUseRequirementKind {
    CurrentInfo,
    FileAnalysis,
    Finance,
    Law,
    Weather,
    Tracking,
    Pricing,
}

impl ToolUseRequirementKind {
    pub const fn checkpoint_id(self) -> &'static str {
        match self {
            Self::CurrentInfo => "tu-current-info",
            Self::FileAnalysis => "tu-file-analysis",
            Self::Finance => "tu-finance",
            Self::Law => "tu-law",
            Self::Weather => "tu-weather",
            Self::Tracking => "tu-tracking",
            Self::Pricing => "tu-pricing",
        }
    }

    pub const fn tool_hint(self) -> &'static str {
        match self {
            Self::CurrentInfo => "web_search",
            Self::FileAnalysis => "local_file_reader",
            Self::Finance => "finance_lookup",
            Self::Law => "legal/current_law_lookup",
            Self::Weather => "weather_lookup",
            Self::Tracking => "tracking_lookup",
            Self::Pricing => "product_price_lookup",
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::CurrentInfo => "Current or latest public information must be fetched with a freshness-aware tool.",
            Self::FileAnalysis => "User file analysis must use a local file tool and respect privacy constraints.",
            Self::Finance => "Financial market or account information must use a finance-capable tool or verified source.",
            Self::Law => "Current law, regulation, or legal status must be checked with an appropriate legal/current-law source.",
            Self::Weather => "Weather requests must use a weather/current conditions tool.",
            Self::Tracking => "Shipment, order, or ticket tracking must use a tracking lookup tool.",
            Self::Pricing => "Current product/service pricing must use current product search or pricing lookup.",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolUseRubricInput {
    pub trace: RouteTraceRow,
    pub visible_prompt: Option<String>,
    pub tools: Vec<ToolInventoryItem>,
    pub route_policy: RoutePolicy,
}

impl ToolUseRubricInput {
    pub fn validate(&self) -> Result<(), RlvrError> {
        self.trace.validate()?;
        self.route_policy.validate()?;
        for tool in &self.tools {
            require_non_empty("tool_inventory.tool_id", &tool.tool_id)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompressionRequiredFact {
    pub fact_id: String,
    pub description: String,
    pub expected_preserved_answer: String,
    pub numeric_fidelity_required: bool,
    pub citation_required: bool,
    pub constraint_required: bool,
}

impl CompressionRequiredFact {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("compression_fact.fact_id", &self.fact_id)?;
        require_non_empty("compression_fact.description", &self.description)?;
        require_non_empty(
            "compression_fact.expected_preserved_answer",
            &self.expected_preserved_answer,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompressionLossRubricInput {
    pub task_id: String,
    pub visible_source: Option<String>,
    pub compressed_output: Option<String>,
    pub source_hash: String,
    pub compressed_output_hash: String,
    pub required_facts: Vec<CompressionRequiredFact>,
    pub route_policy: RoutePolicy,
    pub privacy_policy: PrivacyPolicy,
}

impl CompressionLossRubricInput {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("compression_loss.task_id", &self.task_id)?;
        require_non_empty("compression_loss.source_hash", &self.source_hash)?;
        require_non_empty(
            "compression_loss.compressed_output_hash",
            &self.compressed_output_hash,
        )?;
        if self.required_facts.is_empty() {
            return Err(RlvrError::Config(
                "compression_loss.required_facts must contain at least one fact".into(),
            ));
        }
        for fact in &self.required_facts {
            fact.validate()?;
        }
        self.route_policy.validate()?;
        self.privacy_policy.validate()
    }
}

pub fn generate_route_correctness_rubric(
    input: RouteCorrectnessRubricInput,
) -> Result<TrainingItem, RlvrError> {
    input.validate()?;
    let task_type = classify_route_task(&input);
    let policy_rule = select_route_rule(&input.route_policy, &task_type, &input.trace);
    let local_model_available = input
        .models
        .iter()
        .any(|model| model.local && model_can_cover(model, policy_rule));
    let selected_model = input
        .models
        .iter()
        .find(|model| model.model_id == input.trace.selected_route);
    let selected_is_local = selected_model
        .map(|model| model.local)
        .unwrap_or_else(|| route_reads_local(&input.trace.selected_route));
    let required_tool = policy_rule.and_then(|rule| rule.tool_required.as_deref());
    let tool_available = required_tool
        .map(|tool| input.tools.iter().any(|item| item.tool_id == tool))
        .unwrap_or(false);
    let private_trace = input.trace.local_only || !input.trace.privacy_tags.is_empty();
    let external_selected = !selected_is_local && !route_reads_local(&input.trace.selected_route);
    let escalation_expected = policy_rule
        .and_then(|rule| rule.escalation.as_deref())
        .or_else(|| external_selected.then_some(input.trace.selected_route.as_str()));
    let visible_user_query = input.visible_prompt.unwrap_or_else(|| {
        format!(
            "[hash-only local trace prompt_hash={}]",
            input.trace.prompt_hash
        )
    });

    let checkpoints = vec![
        checkpoint(
            "rc-task-classification",
            CheckpointType::RouteRequirement,
            format!(
                "Classify the request as `{task_type}` before evaluating route `{}`.",
                input.trace.selected_route
            ),
            "The task classification should match the route policy condition that applies to the prompt.",
            0.60,
        ),
        checkpoint(
            "rc-local-sufficiency",
            CheckpointType::RouteRequirement,
            if local_model_available {
                format!(
                    "Determine whether an available local model is sufficient before using `{}`.",
                    input.trace.selected_route
                )
            } else {
                "Determine that no listed local model is sufficient before requiring escalation."
                    .into()
            },
            if local_model_available {
                "Use the cheapest capable local model unless policy, privacy, tool, or quality requirements justify escalation."
            } else {
                "Escalation may be justified when no local model has the required capability."
            },
            0.60,
        ),
        checkpoint(
            "rc-tool-required",
            CheckpointType::ToolRequirement,
            match required_tool {
                Some(tool) if tool_available => {
                    format!("The route should call required tool `{tool}` before final answering.")
                }
                Some(tool) => {
                    format!("The policy requires tool `{tool}`, but it is missing from inventory.")
                }
                None => "Confirm that no external tool is required for this route decision.".into(),
            },
            required_tool.unwrap_or("No required tool under the matched route policy."),
            0.40,
        ),
        checkpoint(
            "rc-external-escalation",
            CheckpointType::CostPolicy,
            match escalation_expected {
                Some(route) => format!(
                    "Verify whether escalation to `{route}` is justified by capability, freshness, safety, or policy."
                ),
                None => {
                    "Verify that the route avoids unnecessary external escalation and cost.".into()
                }
            },
            "Escalation is valid only when policy requirements cannot be met cheaply and locally.",
            0.60,
        ),
        checkpoint(
            "rc-privacy-protection",
            CheckpointType::RouteRequirement,
            if private_trace {
                format!(
                    "Protect private trace tags {:?}; route must remain local-only unless the user explicitly approves export.",
                    input.trace.privacy_tags
                )
            } else {
                "Confirm the route does not expose private data and respects the trace privacy policy."
                    .into()
            },
            if private_trace && external_selected {
                "External routing is not allowed for private traces without explicit approval."
            } else {
                "The trace may be routed only within its privacy policy."
            },
            1.00,
        ),
        checkpoint(
            "rc-final-answer-acceptable",
            CheckpointType::AnswerQuality,
            "Judge whether the final answer is acceptable for the classified task after route, tool, privacy, cost, and latency checks.",
            "The final answer should satisfy the prompt requirements using the selected route without hiding missing information.",
            1.00,
        ),
    ];

    let privacy_policy = PrivacyPolicy {
        local_only: private_trace,
        allow_external_models: !private_trace,
        allow_export: false,
        pii_tags: input.trace.privacy_tags.clone(),
    };
    privacy_policy.validate()?;

    let item = TrainingItem {
        task_id: format!("route-rubric-{}", input.trace.trace_id),
        mode: TrainingMode::RouteCorrectness,
        visible_user_query,
        hidden_original_query: input.trace.prompt_hash.clone(),
        gold_answer: format!(
            "Selected route `{}` should be judged against policy `{}` and trace `{}`.",
            input.trace.selected_route, input.route_policy.policy_id, input.trace.trace_hash
        ),
        domain: task_type,
        difficulty: route_difficulty(&input.trace, required_tool, private_trace),
        checkpoints,
        route_policy: input.route_policy,
        privacy_policy,
    };
    item.validate()?;
    Ok(item)
}

pub fn generate_tool_use_rubric(input: ToolUseRubricInput) -> Result<TrainingItem, RlvrError> {
    input.validate()?;
    let visible_user_query = input.visible_prompt.unwrap_or_else(|| {
        format!(
            "[hash-only local trace prompt_hash={}]",
            input.trace.prompt_hash
        )
    });
    let requirements = detect_tool_use_requirements(&visible_user_query, &input.trace);
    let checkpoints = if requirements.is_empty() {
        vec![checkpoint(
            "tu-no-tool-required",
            CheckpointType::ToolRequirement,
            "Confirm that this request does not require current lookup, file analysis, finance, law, weather, tracking, or pricing tools.",
            "No tool is required when all needed information is stable, non-private, and already available.",
            0.40,
        )]
    } else {
        requirements
            .iter()
            .map(|requirement| {
                let tool_hint = requirement.tool_hint();
                let available = input.tools.iter().any(|tool| tool.tool_id == tool_hint);
                checkpoint(
                    requirement.checkpoint_id(),
                    CheckpointType::ToolRequirement,
                    if available {
                        format!(
                            "{} Expected tool: `{tool_hint}`.",
                            requirement.description()
                        )
                    } else {
                        format!(
                            "{} Expected tool `{tool_hint}` is missing from inventory.",
                            requirement.description()
                        )
                    },
                    tool_hint,
                    0.75,
                )
            })
            .collect()
    };
    let private_trace = input.trace.local_only || !input.trace.privacy_tags.is_empty();
    let privacy_policy = PrivacyPolicy {
        local_only: private_trace,
        allow_external_models: !private_trace,
        allow_export: false,
        pii_tags: input.trace.privacy_tags.clone(),
    };
    privacy_policy.validate()?;
    let item = TrainingItem {
        task_id: format!("tool-use-rubric-{}", input.trace.trace_id),
        mode: TrainingMode::ToolUse,
        visible_user_query,
        hidden_original_query: input.trace.prompt_hash.clone(),
        gold_answer: format!(
            "Tool-use verifier should check {} tool requirement(s) for trace `{}`.",
            checkpoints.len(),
            input.trace.trace_hash
        ),
        domain: "tool_use".into(),
        difficulty: if checkpoints.len() > 2 || private_trace {
            Difficulty::Hard
        } else {
            Difficulty::Medium
        },
        checkpoints,
        route_policy: input.route_policy,
        privacy_policy,
    };
    item.validate()?;
    Ok(item)
}

pub fn generate_compression_loss_rubric(
    input: CompressionLossRubricInput,
) -> Result<TrainingItem, RlvrError> {
    input.validate()?;
    let mut checkpoints = Vec::new();
    for fact in &input.required_facts {
        checkpoints.push(checkpoint(
            &format!("cl-dropped-fact-{}", fact.fact_id),
            CheckpointType::CompressionFact,
            format!(
                "Required fact must be preserved during compression: {}",
                fact.description
            ),
            fact.expected_preserved_answer.clone(),
            0.75,
        ));
        if fact.numeric_fidelity_required {
            checkpoints.push(checkpoint(
                &format!("cl-numeric-fidelity-{}", fact.fact_id),
                CheckpointType::CompressionFact,
                format!(
                    "Numeric values, units, dates, counts, and thresholds for `{}` must remain exact.",
                    fact.description
                ),
                fact.expected_preserved_answer.clone(),
                0.75,
            ));
        }
        if fact.citation_required {
            checkpoints.push(checkpoint(
                &format!("cl-citation-{}", fact.fact_id),
                CheckpointType::CompressionFact,
                format!(
                    "Citation, source, or evidence marker for `{}` must be preserved.",
                    fact.description
                ),
                "Preserve citation/source evidence alongside the compressed fact.",
                0.50,
            ));
        }
        if fact.constraint_required {
            checkpoints.push(checkpoint(
                &format!("cl-constraint-{}", fact.fact_id),
                CheckpointType::CompressionFact,
                format!(
                    "Constraint, caveat, or user requirement for `{}` must be preserved.",
                    fact.description
                ),
                "Preserve the constraint exactly enough for downstream routing or answering.",
                0.60,
            ));
        }
    }
    let visible_user_query = input.visible_source.unwrap_or_else(|| {
        format!(
            "[hash-only compression source_hash={} compressed_output_hash={}]",
            input.source_hash, input.compressed_output_hash
        )
    });
    let item = TrainingItem {
        task_id: format!("compression-loss-rubric-{}", input.task_id),
        mode: TrainingMode::CompressionLoss,
        visible_user_query,
        hidden_original_query: input.source_hash,
        gold_answer: format!(
            "Compressed output `{}` must preserve {} required fact checkpoint(s).",
            input.compressed_output_hash,
            checkpoints.len()
        ),
        domain: "compression_loss".into(),
        difficulty: if checkpoints.len() > 3 {
            Difficulty::Hard
        } else {
            Difficulty::Medium
        },
        checkpoints,
        route_policy: input.route_policy,
        privacy_policy: input.privacy_policy,
    };
    item.validate()?;
    Ok(item)
}

fn classify_route_task(input: &RouteCorrectnessRubricInput) -> String {
    let prompt = input
        .visible_prompt
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let reason = input.trace.router_reason.to_ascii_lowercase();
    let route = input.trace.selected_route.to_ascii_lowercase();
    if !input.trace.privacy_tags.is_empty() || reason.contains("private") || route.contains("file")
    {
        "private_file_analysis".into()
    } else if contains_any(
        &prompt,
        &["current", "price", "today", "latest", "weather", "news"],
    ) || contains_any(&reason, &["current", "web", "fresh"])
    {
        "current_public_info".into()
    } else if contains_any(&prompt, &["medical", "legal", "tax", "financial"])
        || contains_any(&reason, &["medical", "legal", "financial", "high-stakes"])
    {
        "high_stakes_advice".into()
    } else if contains_any(&prompt, &["code", "implement", "bug", "api"])
        || contains_any(&reason, &["code", "coding"])
    {
        "code_implementation".into()
    } else {
        "stable_knowledge".into()
    }
}

fn detect_tool_use_requirements(
    visible_user_query: &str,
    trace: &RouteTraceRow,
) -> Vec<ToolUseRequirementKind> {
    let combined = format!(
        "{} {} {}",
        visible_user_query, trace.router_reason, trace.selected_route
    )
    .to_ascii_lowercase();
    let mut requirements = Vec::new();
    push_requirement(
        &mut requirements,
        ToolUseRequirementKind::CurrentInfo,
        contains_any(
            &combined,
            &["current", "latest", "today", "recent", "right now", "news"],
        ),
    );
    push_requirement(
        &mut requirements,
        ToolUseRequirementKind::FileAnalysis,
        contains_any(
            &combined,
            &["file", "/users/", "~/", "document", "pdf", "spreadsheet"],
        ) || trace.privacy_tags.iter().any(|tag| tag == "private_file"),
    );
    push_requirement(
        &mut requirements,
        ToolUseRequirementKind::Finance,
        contains_any(
            &combined,
            &[
                "stock",
                "ticker",
                "balance",
                "portfolio",
                "finance",
                "market price",
            ],
        ),
    );
    push_requirement(
        &mut requirements,
        ToolUseRequirementKind::Law,
        contains_any(
            &combined,
            &["law", "legal", "regulation", "statute", "tax", "attorney"],
        ),
    );
    push_requirement(
        &mut requirements,
        ToolUseRequirementKind::Weather,
        contains_any(&combined, &["weather", "forecast", "temperature", "rain"]),
    );
    push_requirement(
        &mut requirements,
        ToolUseRequirementKind::Tracking,
        contains_any(
            &combined,
            &[
                "tracking",
                "shipment",
                "package",
                "order status",
                "ups",
                "fedex",
            ],
        ),
    );
    push_requirement(
        &mut requirements,
        ToolUseRequirementKind::Pricing,
        contains_any(&combined, &["price", "pricing", "cost today", "buy it"]),
    );
    requirements
}

fn push_requirement(
    requirements: &mut Vec<ToolUseRequirementKind>,
    requirement: ToolUseRequirementKind,
    condition: bool,
) {
    if condition && !requirements.contains(&requirement) {
        requirements.push(requirement);
    }
}

fn select_route_rule<'a>(
    policy: &'a RoutePolicy,
    task_type: &str,
    trace: &RouteTraceRow,
) -> Option<&'a RouteRule> {
    policy
        .rules
        .iter()
        .find(|rule| rule.task_type == task_type)
        .or_else(|| {
            policy
                .rules
                .iter()
                .find(|rule| rule.route == trace.selected_route)
        })
}

fn model_can_cover(model: &ModelInventoryItem, rule: Option<&RouteRule>) -> bool {
    let Some(rule) = rule else {
        return true;
    };
    model
        .capabilities
        .iter()
        .any(|capability| capability == &rule.required_capability || capability == &rule.task_type)
}

fn route_reads_local(route: &str) -> bool {
    let route = route.to_ascii_lowercase();
    route.contains("local") || route.contains("tiny")
}

fn route_difficulty(
    trace: &RouteTraceRow,
    required_tool: Option<&str>,
    private_trace: bool,
) -> Difficulty {
    if private_trace || required_tool.is_some() || trace.cost_estimate.unwrap_or(0.0) > 0.01 {
        Difficulty::Hard
    } else if trace.latency_ms.unwrap_or(0) > 5_000 {
        Difficulty::Medium
    } else {
        Difficulty::Easy
    }
}

fn checkpoint(
    checkpoint_id: &str,
    checkpoint_type: CheckpointType,
    description: impl Into<String>,
    answer_if_asked: impl Into<String>,
    failure_penalty: f64,
) -> Checkpoint {
    Checkpoint {
        checkpoint_id: checkpoint_id.into(),
        checkpoint_type,
        description: description.into(),
        must_resolve_before_answer: true,
        answer_if_asked: answer_if_asked.into(),
        failure_penalty,
    }
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn require_non_empty(name: &str, value: &str) -> Result<(), RlvrError> {
    if value.trim().is_empty() {
        return Err(RlvrError::Config(format!("{name} cannot be empty")));
    }
    Ok(())
}

fn require_finite_non_negative(name: &str, value: f64) -> Result<(), RlvrError> {
    if !value.is_finite() {
        return Err(RlvrError::Config(format!("{name} must be finite")));
    }
    if value < 0.0 {
        return Err(RlvrError::Config(format!("{name} cannot be negative")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{hash_bytes, RouteTraceInput};

    #[test]
    fn route_correctness_generator_builds_six_checkpoint_training_item_from_trace() {
        let policy = RoutePolicy::default();
        let trace = RouteTraceRow::build(
            &RouteTraceInput {
                prompt: "What is the current price of this AI mini PC?",
                answer: Some("I need a current product lookup first."),
                selected_route: "web-enabled model",
                router_reason: "current_public_info; web lookup required",
                route_policy: &policy,
                latency_ms: Some(1200),
                cost_estimate: Some(0.002),
                user_rating: None,
                user_correction: None,
            },
            "rt-price".into(),
            1,
            true,
        )
        .unwrap();
        let item = generate_route_correctness_rubric(RouteCorrectnessRubricInput {
            trace,
            visible_prompt: Some("What is the current price of this AI mini PC?".into()),
            models: vec![ModelInventoryItem {
                model_id: "tiny-local-model".into(),
                local: true,
                capabilities: vec!["general_qa".into()],
                max_cost: Some(0.0),
                max_latency_ms: Some(1000),
            }],
            tools: vec![ToolInventoryItem {
                tool_id: "web_search".into(),
                supports_current_info: true,
                safe_for_private_data: false,
            }],
            route_policy: policy,
        })
        .unwrap();

        assert_eq!(item.mode, TrainingMode::RouteCorrectness);
        assert_eq!(item.domain, "current_public_info");
        assert_eq!(item.checkpoints.len(), 6);
        assert!(item.checkpoints.iter().any(|checkpoint| {
            checkpoint.checkpoint_id == "rc-tool-required"
                && checkpoint.description.contains("web_search")
        }));
        item.validate().unwrap();
    }

    #[test]
    fn route_correctness_generator_preserves_hash_only_prompt_when_raw_prompt_missing() {
        let policy = RoutePolicy::default();
        let trace = RouteTraceRow::build(
            &RouteTraceInput {
                prompt: "My API key is sk-test-1234567890abcdef1234567890abcdef",
                answer: Some("Keeping this local."),
                selected_route: "local-file-model",
                router_reason: "private_file_analysis; local only",
                route_policy: &policy,
                latency_ms: Some(50),
                cost_estimate: Some(0.0),
                user_rating: Some(5),
                user_correction: None,
            },
            "rt-private".into(),
            1,
            true,
        )
        .unwrap();
        let prompt_hash = trace.prompt_hash.clone();
        let item = generate_route_correctness_rubric(RouteCorrectnessRubricInput {
            trace,
            visible_prompt: None,
            models: vec![ModelInventoryItem {
                model_id: "local-file-model".into(),
                local: true,
                capabilities: vec!["local_file_analysis".into()],
                max_cost: Some(0.0),
                max_latency_ms: Some(5000),
            }],
            tools: vec![ToolInventoryItem {
                tool_id: "local_file_reader".into(),
                supports_current_info: false,
                safe_for_private_data: true,
            }],
            route_policy: policy,
        })
        .unwrap();

        assert!(item.visible_user_query.contains(&prompt_hash));
        assert_eq!(item.hidden_original_query, prompt_hash);
        assert!(item.privacy_policy.local_only);
        assert!(!item.privacy_policy.allow_external_models);
        assert!(item.checkpoints.iter().any(|checkpoint| {
            checkpoint.checkpoint_id == "rc-privacy-protection"
                && checkpoint.description.contains("api_key")
        }));
        assert_ne!(
            item.visible_user_query,
            "My API key is sk-test-1234567890abcdef1234567890abcdef"
        );
        assert_eq!(hash_bytes(item.visible_user_query.as_bytes()).len(), 64);
    }

    #[test]
    fn route_correctness_generator_rejects_invalid_inventory() {
        let policy = RoutePolicy::default();
        let trace = RouteTraceRow::build(
            &RouteTraceInput {
                prompt: "Simple question",
                answer: Some("Simple answer"),
                selected_route: "tiny-local-model",
                router_reason: "stable_knowledge; local model sufficient",
                route_policy: &policy,
                latency_ms: Some(10),
                cost_estimate: Some(0.0),
                user_rating: None,
                user_correction: None,
            },
            "rt-invalid-inventory".into(),
            1,
            true,
        )
        .unwrap();
        let err = generate_route_correctness_rubric(RouteCorrectnessRubricInput {
            trace,
            visible_prompt: Some("Simple question".into()),
            models: vec![ModelInventoryItem {
                model_id: String::new(),
                local: true,
                capabilities: Vec::new(),
                max_cost: Some(0.0),
                max_latency_ms: None,
            }],
            tools: Vec::new(),
            route_policy: policy,
        })
        .unwrap_err();
        assert!(err.to_string().contains("model_inventory.model_id"));
    }

    #[test]
    fn tool_use_generator_covers_required_tool_categories_from_sample_trace() {
        let policy = RoutePolicy::default();
        let trace = RouteTraceRow::build(
            &RouteTraceInput {
                prompt: concat!(
                    "Use my file /Users/alice/report.pdf to check the latest AAPL stock price, ",
                    "current tax law, weather forecast, UPS tracking status, and product pricing."
                ),
                answer: Some("This needs several tools before answering."),
                selected_route: "web-enabled model",
                router_reason:
                    "current_public_info; private file; finance; law; weather; tracking; pricing",
                route_policy: &policy,
                latency_ms: Some(2500),
                cost_estimate: Some(0.006),
                user_rating: None,
                user_correction: None,
            },
            "rt-tool-use-all".into(),
            1,
            true,
        )
        .unwrap();
        let item = generate_tool_use_rubric(ToolUseRubricInput {
            trace,
            visible_prompt: Some(
                "Use my file to check latest AAPL stock price, tax law, weather, tracking, and pricing."
                    .into(),
            ),
            tools: vec![
                tool("web_search", true, false),
                tool("local_file_reader", false, true),
                tool("finance_lookup", true, false),
                tool("legal/current_law_lookup", true, false),
                tool("weather_lookup", true, false),
                tool("tracking_lookup", true, false),
                tool("product_price_lookup", true, false),
            ],
            route_policy: policy,
        })
        .unwrap();

        assert_eq!(item.mode, TrainingMode::ToolUse);
        for checkpoint_id in [
            "tu-current-info",
            "tu-file-analysis",
            "tu-finance",
            "tu-law",
            "tu-weather",
            "tu-tracking",
            "tu-pricing",
        ] {
            assert!(
                item.checkpoints
                    .iter()
                    .any(|checkpoint| checkpoint.checkpoint_id == checkpoint_id),
                "missing {checkpoint_id}"
            );
        }
        item.validate().unwrap();
    }

    #[test]
    fn tool_use_generator_emits_no_tool_checkpoint_for_stable_prompt() {
        let policy = RoutePolicy::default();
        let trace = RouteTraceRow::build(
            &RouteTraceInput {
                prompt: "Explain what a binary tree is.",
                answer: Some("A binary tree is a tree where each node has at most two children."),
                selected_route: "tiny-local-model",
                router_reason: "stable_knowledge; local model sufficient",
                route_policy: &policy,
                latency_ms: Some(20),
                cost_estimate: Some(0.0),
                user_rating: Some(5),
                user_correction: None,
            },
            "rt-no-tool".into(),
            1,
            true,
        )
        .unwrap();
        let item = generate_tool_use_rubric(ToolUseRubricInput {
            trace,
            visible_prompt: Some("Explain what a binary tree is.".into()),
            tools: Vec::new(),
            route_policy: policy,
        })
        .unwrap();

        assert_eq!(item.mode, TrainingMode::ToolUse);
        assert_eq!(item.checkpoints.len(), 1);
        assert_eq!(item.checkpoints[0].checkpoint_id, "tu-no-tool-required");
    }

    #[test]
    fn compression_loss_generator_checks_dropped_facts_numbers_citations_and_constraints() {
        let item = generate_compression_loss_rubric(CompressionLossRubricInput {
            task_id: "compress-1".into(),
            visible_source: Some("Source says: keep 22uF, 6.3V, citation [A], and local-only.".into()),
            compressed_output: Some("Keep capacitor details.".into()),
            source_hash: hash_bytes(b"source text with 22uF 6.3V citation A local-only"),
            compressed_output_hash: hash_bytes(b"compressed text"),
            required_facts: vec![CompressionRequiredFact {
                fact_id: "capacitor-spec".into(),
                description: "22uF capacitor must be rated 6.3V or higher with citation [A] and local-only constraint".into(),
                expected_preserved_answer: "22uF, 6.3V or higher, citation [A], local-only".into(),
                numeric_fidelity_required: true,
                citation_required: true,
                constraint_required: true,
            }],
            route_policy: RoutePolicy::default(),
            privacy_policy: PrivacyPolicy::default(),
        })
        .unwrap();

        assert_eq!(item.mode, TrainingMode::CompressionLoss);
        for checkpoint_id in [
            "cl-dropped-fact-capacitor-spec",
            "cl-numeric-fidelity-capacitor-spec",
            "cl-citation-capacitor-spec",
            "cl-constraint-capacitor-spec",
        ] {
            assert!(
                item.checkpoints
                    .iter()
                    .any(|checkpoint| checkpoint.checkpoint_id == checkpoint_id),
                "missing {checkpoint_id}"
            );
        }
        item.validate().unwrap();
    }

    #[test]
    fn compression_loss_generator_rejects_empty_fact_fixture() {
        let err = generate_compression_loss_rubric(CompressionLossRubricInput {
            task_id: "compress-empty".into(),
            visible_source: None,
            compressed_output: None,
            source_hash: hash_bytes(b"source"),
            compressed_output_hash: hash_bytes(b"compressed"),
            required_facts: Vec::new(),
            route_policy: RoutePolicy::default(),
            privacy_policy: PrivacyPolicy::default(),
        })
        .unwrap_err();
        assert!(err.to_string().contains("required_facts"));
    }

    fn tool(
        tool_id: &str,
        supports_current_info: bool,
        safe_for_private_data: bool,
    ) -> ToolInventoryItem {
        ToolInventoryItem {
            tool_id: tool_id.into(),
            supports_current_info,
            safe_for_private_data,
        }
    }
}
