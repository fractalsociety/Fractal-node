//! RLVR-036: Baseline evaluation sets and the base-model-vs-adapter comparison
//! harness.
//!
//! Every training mode ships a small, deterministic baseline eval set built from
//! the same rubric generators that feed training (`generate_route_correctness_rubric`,
//! `generate_tool_use_rubric`, `generate_compression_loss_rubric`,
//! `generate_ask_overconfidence_rubric`) plus a hand-authored AskMind set (its
//! rubric generator lands with RLVR-011). The sets reuse the hash-only
//! [`TrainingItem`] shape so the exact same commitments flow through rollout and
//! evaluation, and the comparison harness scores a "base" run against an
//! "adapter" run over a shared set to prove base vs adapter can be compared
//! without ever committing raw data.

use serde::{Deserialize, Serialize};

use crate::rubrics::{
    generate_ask_overconfidence_rubric, generate_compression_loss_rubric,
    generate_route_correctness_rubric, generate_tool_use_rubric, CompressionLossRubricInput,
    CompressionRequiredFact, ModelInventoryItem, RouteCorrectnessRubricInput, ToolInventoryItem,
    ToolUseRubricInput,
};
use crate::tracing::{RouteTraceInput, RouteTraceRow};
use crate::verifier::{score_final_answer_for_item, StrictVerifierOutput};
use crate::{
    stable_hash, Checkpoint, CheckpointType, Difficulty, PrivacyPolicy, RlvrError, RoutePolicy,
    TrainingItem, TrainingMode,
};

/// The six baseline eval sets required by RLVR-036.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BaselineEvalSetKind {
    AskMind,
    AskOverconfidence,
    RouteCorrectness,
    ToolUse,
    CompressionLoss,
    /// Replay items derived from real captured route traces; item mode is
    /// [`TrainingMode::RouteCorrectness`] but provenance is "user trace replay".
    UserTraceReplay,
}

impl BaselineEvalSetKind {
    /// All six kinds in checklist order.
    pub const ALL: &'static [BaselineEvalSetKind] = &[
        Self::AskMind,
        Self::AskOverconfidence,
        Self::RouteCorrectness,
        Self::ToolUse,
        Self::CompressionLoss,
        Self::UserTraceReplay,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AskMind => "askmind",
            Self::AskOverconfidence => "askoverconfidence",
            Self::RouteCorrectness => "routecorrectness",
            Self::ToolUse => "tooluse",
            Self::CompressionLoss => "compressionloss",
            Self::UserTraceReplay => "usertracereplay",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().trim_matches('"').to_ascii_lowercase().as_str() {
            "askmind" | "ask-mind" => Some(Self::AskMind),
            "askoverconfidence" | "ask-overconfidence" => Some(Self::AskOverconfidence),
            "routecorrectness" | "route-correctness" => Some(Self::RouteCorrectness),
            "tooluse" | "tool-use" => Some(Self::ToolUse),
            "compressionloss" | "compression-loss" => Some(Self::CompressionLoss),
            "usertracereplay" | "user-trace-replay" | "replay" => Some(Self::UserTraceReplay),
            _ => None,
        }
    }

    /// Training mode of the items inside this set. `UserTraceReplay` holds
    /// route-correctness items derived from captured traces.
    pub const fn item_mode(self) -> TrainingMode {
        match self {
            Self::AskMind => TrainingMode::AskMind,
            Self::AskOverconfidence => TrainingMode::AskOverconfidence,
            Self::RouteCorrectness => TrainingMode::RouteCorrectness,
            Self::ToolUse => TrainingMode::ToolUse,
            Self::CompressionLoss => TrainingMode::CompressionLoss,
            Self::UserTraceReplay => TrainingMode::RouteCorrectness,
        }
    }

    pub const fn description(self) -> &'static str {
        match self {
            Self::AskMind => "AskMind baseline: degraded prompts with missing-info checkpoints.",
            Self::AskOverconfidence => "AskOverconfidence baseline: false-premise correction checkpoints.",
            Self::RouteCorrectness => "RouteCorrectness baseline: route/tool/privacy checkpoints from traces.",
            Self::ToolUse => "ToolUse baseline: required-tool checkpoints across tool categories.",
            Self::CompressionLoss => "CompressionLoss baseline: dropped-fact, numeric, citation, and constraint checkpoints.",
            Self::UserTraceReplay => "User trace replay baseline: route checkpoints derived from captured user traces.",
        }
    }
}

/// A single baseline eval set: a named kind plus its ordered, validated items.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineEvalSet {
    pub kind: BaselineEvalSetKind,
    pub items: Vec<TrainingItem>,
}

impl BaselineEvalSet {
    pub fn new(kind: BaselineEvalSetKind, items: Vec<TrainingItem>) -> Result<Self, RlvrError> {
        let set = Self { kind, items };
        set.validate()?;
        Ok(set)
    }

    pub fn task_count(&self) -> usize {
        self.items.len()
    }

    pub fn task_ids(&self) -> Vec<String> {
        self.items.iter().map(|item| item.task_id.clone()).collect()
    }

    pub fn validate(&self) -> Result<(), RlvrError> {
        if self.items.is_empty() {
            return Err(RlvrError::Config(format!(
                "baseline eval set {:?} must contain at least one item",
                self.kind
            )));
        }
        let expected_mode = self.kind.item_mode();
        let mut seen = std::collections::BTreeSet::new();
        for item in &self.items {
            item.validate()?;
            if item.mode != expected_mode {
                return Err(RlvrError::Config(format!(
                    "baseline eval set {:?} item {:?} has mode {:?}, expected {:?}",
                    self.kind, item.task_id, item.mode, expected_mode
                )));
            }
            if !seen.insert(item.task_id.clone()) {
                return Err(RlvrError::Config(format!(
                    "baseline eval set {:?} has duplicate task_id {:?}",
                    self.kind, item.task_id
                )));
            }
        }
        Ok(())
    }

    pub fn stable_hash(&self) -> Result<String, RlvrError> {
        self.validate()?;
        stable_hash(self)
    }
}

/// Lightweight manifest entry listing a set without its (potentially large)
/// item payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineEvalSetManifestEntry {
    pub kind: BaselineEvalSetKind,
    pub item_mode: TrainingMode,
    pub item_count: usize,
    pub set_hash: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaselineEvalSetManifest {
    pub version: String,
    pub set_count: usize,
    pub total_items: usize,
    pub entries: Vec<BaselineEvalSetManifestEntry>,
}

/// Per-item score for one evaluated system (base or adapter) on a baseline set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineEvalItemScore {
    pub task_id: String,
    pub final_score: f64,
    pub answer_correctness: f64,
    pub rubric_coverage: f64,
    pub passed: bool,
}

/// Aggregated score for one evaluated system over a whole baseline set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineEvalSystemReport {
    pub system_id: String,
    pub item_count: usize,
    pub mean_final_score: f64,
    pub mean_answer_correctness: f64,
    pub mean_rubric_coverage: f64,
    pub pass_rate: f64,
    pub item_scores: Vec<BaselineEvalItemScore>,
}

impl BaselineEvalSystemReport {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("baseline_system.system_id", &self.system_id)?;
        if self.item_count == 0 {
            return Err(RlvrError::Config(
                "baseline_system.item_count must be greater than zero".into(),
            ));
        }
        if self.item_scores.len() != self.item_count {
            return Err(RlvrError::Config(
                "baseline_system.item_scores length must match item_count".into(),
            ));
        }
        for field in [
            self.mean_final_score,
            self.mean_answer_correctness,
            self.mean_rubric_coverage,
            self.pass_rate,
        ] {
            require_bounded_unit("baseline_system mean", field)?;
        }
        Ok(())
    }
}

/// Base-vs-adapter comparison over a single baseline eval set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaselineComparisonReport {
    pub kind: BaselineEvalSetKind,
    pub set_hash: String,
    pub item_count: usize,
    pub base: BaselineEvalSystemReport,
    pub adapter: BaselineEvalSystemReport,
    /// `adapter - base` for each metric.
    pub final_score_delta: f64,
    pub answer_correctness_delta: f64,
    pub coverage_delta: f64,
    pub pass_rate_delta: f64,
    /// `true` when the adapter's mean final score beats the base's.
    pub adapter_improves: bool,
}

/// Build every baseline eval set in checklist order.
pub fn default_baseline_eval_sets() -> Result<Vec<BaselineEvalSet>, RlvrError> {
    BaselineEvalSetKind::ALL
        .iter()
        .map(|kind| baseline_eval_set(*kind))
        .collect()
}

/// Build one baseline eval set by kind.
pub fn baseline_eval_set(kind: BaselineEvalSetKind) -> Result<BaselineEvalSet, RlvrError> {
    let items = match kind {
        BaselineEvalSetKind::AskMind => askmind_items()?,
        BaselineEvalSetKind::AskOverconfidence => askoverconfidence_items()?,
        BaselineEvalSetKind::RouteCorrectness => routecorrectness_items()?,
        BaselineEvalSetKind::ToolUse => tooluse_items()?,
        BaselineEvalSetKind::CompressionLoss => compressionloss_items()?,
        BaselineEvalSetKind::UserTraceReplay => user_trace_replay_items()?,
    };
    BaselineEvalSet::new(kind, items)
}

/// AskMind local set: degraded prompts with missing-info checkpoints.
pub fn askmind_baseline_eval_set() -> Result<BaselineEvalSet, RlvrError> {
    baseline_eval_set(BaselineEvalSetKind::AskMind)
}

/// AskOverconfidence local set: false-premise correction checkpoints.
pub fn askoverconfidence_baseline_eval_set() -> Result<BaselineEvalSet, RlvrError> {
    baseline_eval_set(BaselineEvalSetKind::AskOverconfidence)
}

/// RouteCorrectness local set: route/tool/privacy checkpoints from traces.
pub fn routecorrectness_baseline_eval_set() -> Result<BaselineEvalSet, RlvrError> {
    baseline_eval_set(BaselineEvalSetKind::RouteCorrectness)
}

/// ToolUse local set: required-tool checkpoints across tool categories.
pub fn tooluse_baseline_eval_set() -> Result<BaselineEvalSet, RlvrError> {
    baseline_eval_set(BaselineEvalSetKind::ToolUse)
}

/// CompressionLoss local set: dropped-fact/numeric/citation/constraint checkpoints.
pub fn compressionloss_baseline_eval_set() -> Result<BaselineEvalSet, RlvrError> {
    baseline_eval_set(BaselineEvalSetKind::CompressionLoss)
}

/// User trace replay set: route checkpoints derived from captured user traces.
pub fn user_trace_replay_baseline_eval_set() -> Result<BaselineEvalSet, RlvrError> {
    baseline_eval_set(BaselineEvalSetKind::UserTraceReplay)
}

/// Build the lightweight manifest (kind, mode, counts, hashes) without items.
pub fn baseline_eval_set_manifest() -> Result<BaselineEvalSetManifest, RlvrError> {
    let mut entries = Vec::new();
    let mut total_items = 0usize;
    for set in default_baseline_eval_sets()? {
        total_items += set.task_count();
        entries.push(BaselineEvalSetManifestEntry {
            kind: set.kind,
            item_mode: set.kind.item_mode(),
            item_count: set.task_count(),
            set_hash: set.stable_hash()?,
        });
    }
    Ok(BaselineEvalSetManifest {
        version: "baseline-evals-v0.1".into(),
        set_count: entries.len(),
        total_items,
        entries,
    })
}

/// Score one evaluated system over a baseline set.
///
/// `verifier_outputs` must contain exactly one final-answer verifier output per
/// item, in item order. Each output is scored against its matching item with the
/// shared final-answer scorer, so base and adapter runs are directly comparable.
pub fn score_baseline_eval_set(
    set: &BaselineEvalSet,
    system_id: &str,
    verifier_outputs: &[StrictVerifierOutput],
) -> Result<BaselineEvalSystemReport, RlvrError> {
    set.validate()?;
    require_non_empty("baseline_system.system_id", system_id)?;
    if verifier_outputs
        .iter()
        .any(|output| !output.is_final_answer)
    {
        return Err(RlvrError::Config(
            "final answer scoring requires one final-answer verifier output per baseline item"
                .into(),
        ));
    }
    if verifier_outputs.len() != set.items.len() {
        return Err(RlvrError::Config(format!(
            "baseline eval set {:?} has {} items but {} verifier outputs were supplied",
            set.kind,
            set.items.len(),
            verifier_outputs.len()
        )));
    }

    let mut item_scores = Vec::with_capacity(set.items.len());
    let mut sum_final = 0.0;
    let mut sum_correctness = 0.0;
    let mut sum_coverage = 0.0;
    let mut passed = 0usize;
    for (item, output) in set.items.iter().zip(verifier_outputs.iter()) {
        output.validate()?;
        let report = score_final_answer_for_item(item, std::slice::from_ref(output))?;
        sum_final += report.final_score;
        sum_correctness += report.answer_correctness;
        sum_coverage += report.rubric_completion;
        if report.passed {
            passed += 1;
        }
        item_scores.push(BaselineEvalItemScore {
            task_id: item.task_id.clone(),
            final_score: report.final_score,
            answer_correctness: report.answer_correctness,
            rubric_coverage: report.rubric_completion,
            passed: report.passed,
        });
    }

    let count = set.items.len() as f64;
    let report = BaselineEvalSystemReport {
        system_id: system_id.into(),
        item_count: set.items.len(),
        mean_final_score: sum_final / count,
        mean_answer_correctness: sum_correctness / count,
        mean_rubric_coverage: sum_coverage / count,
        pass_rate: passed as f64 / count,
        item_scores,
    };
    report.validate()?;
    Ok(report)
}

/// Compare a base run against an adapter run over a shared baseline eval set.
///
/// Both systems are scored over the same items; the report exposes per-metric
/// deltas (`adapter - base`) and `adapter_improves`, satisfying the RLVR-036
/// "base model and trained adapter can be compared" gate.
pub fn compare_baseline_eval_set(
    set: &BaselineEvalSet,
    base_system_id: &str,
    base_outputs: &[StrictVerifierOutput],
    adapter_system_id: &str,
    adapter_outputs: &[StrictVerifierOutput],
) -> Result<BaselineComparisonReport, RlvrError> {
    let base = score_baseline_eval_set(set, base_system_id, base_outputs)?;
    let adapter = score_baseline_eval_set(set, adapter_system_id, adapter_outputs)?;
    let report = BaselineComparisonReport {
        kind: set.kind,
        set_hash: set.stable_hash()?,
        item_count: set.task_count(),
        final_score_delta: adapter.mean_final_score - base.mean_final_score,
        answer_correctness_delta: adapter.mean_answer_correctness - base.mean_answer_correctness,
        coverage_delta: adapter.mean_rubric_coverage - base.mean_rubric_coverage,
        pass_rate_delta: adapter.pass_rate - base.pass_rate,
        adapter_improves: adapter.mean_final_score > base.mean_final_score,
        base,
        adapter,
    };
    Ok(report)
}

// ---------------------------------------------------------------------------
// Per-mode set builders
// ---------------------------------------------------------------------------

fn askmind_items() -> Result<Vec<TrainingItem>, RlvrError> {
    let mut items = Vec::new();
    items.push(askmind_item(
        "askmind-capacitor",
        "What capacitor should I use on this board?",
        "What 22uF 0805 capacitor rated at 6.3V or higher should I use on this board?",
        "22uF 0805 capacitor rated at 6.3V or higher.",
        "electronics",
        Difficulty::Easy,
        vec![
            missing(
                "am-capacitor-value",
                "Capacitance value is needed.",
                "The capacitance is 22uF.",
            ),
            missing(
                "am-capacitor-package",
                "Package size is needed.",
                "The package is 0805.",
            ),
            missing(
                "am-capacitor-voltage",
                "Voltage rating is needed.",
                "The rating should be 6.3V or higher.",
            ),
        ],
    ));
    items.push(askmind_item(
        "askmind-paint",
        "How much paint should I buy?",
        "How much matte interior paint do I need for two coats on 400 square feet of wall?",
        "Enough matte interior paint for two coats on 400 square feet.",
        "home_improvement",
        Difficulty::Easy,
        vec![
            missing(
                "am-paint-area",
                "Wall area is needed.",
                "The wall area is 400 square feet.",
            ),
            missing(
                "am-paint-coats",
                "Number of coats is needed.",
                "Two coats are required.",
            ),
            missing(
                "am-paint-finish",
                "Paint finish is needed.",
                "The finish is matte interior.",
            ),
        ],
    ));
    items.push(askmind_item(
        "askmind-flight",
        "Book me a flight to London.",
        "Book a round-trip economy flight for 2 adults from New York to London departing 2026-08-10.",
        "Round-trip economy flight, 2 adults, New York to London, 2026-08-10.",
        "travel",
        Difficulty::Medium,
        vec![
            missing("am-flight-origin", "Origin airport/city is needed.", "Departing from New York."),
            missing("am-flight-dates", "Travel dates are needed.", "Round trip departing 2026-08-10."),
            missing("am-flight-cabin", "Cabin class is needed.", "Economy cabin."),
            missing("am-flight-passengers", "Passenger count is needed.", "2 adults."),
        ],
    ));
    items.push(askmind_item(
        "askmind-project-cost",
        "How much will the project cost?",
        "How much will a 3-month software project staffed by 2 engineers at $120/hour cost?",
        "A 3-month project, 2 engineers at $120/hour.",
        "project_planning",
        Difficulty::Medium,
        vec![
            missing("am-cost-team", "Team size is needed.", "2 engineers."),
            missing("am-cost-rate", "Hourly rate is needed.", "$120 per hour."),
            missing(
                "am-cost-duration",
                "Project duration is needed.",
                "3 months.",
            ),
        ],
    ));
    Ok(items)
}

fn askoverconfidence_items() -> Result<Vec<TrainingItem>, RlvrError> {
    let mut items = Vec::new();
    for (idx, fixture) in crate::rubrics::sample_fixtures().into_iter().enumerate() {
        let rubric = generate_ask_overconfidence_rubric(
            fixture.to_input(format!("askoverconfidence-{idx}")),
        )?;
        let item = rubric.into_training_item(RoutePolicy::default(), PrivacyPolicy::default())?;
        items.push(item);
    }
    Ok(items)
}

fn routecorrectness_items() -> Result<Vec<TrainingItem>, RlvrError> {
    let policy = RoutePolicy::default();
    let models = standard_models();
    let tools = standard_tools();
    let scenarios = [
        // (trace_id, prompt, answer, route, reason, latency_ms, cost, rating, correction, local_only, visible_prompt)
        RouteScenario {
            trace_id: "rc-stable",
            prompt: "What is the capital of France?",
            answer: Some("Paris is the capital of France."),
            route: "tiny-local-model",
            reason: "stable_knowledge; local model sufficient",
            latency_ms: Some(15),
            cost: Some(0.0),
            rating: Some(5),
            correction: None,
            local_only: false,
            visible: Some("What is the capital of France?"),
        },
        RouteScenario {
            trace_id: "rc-current-info",
            prompt: "What is today's top news headline?",
            answer: Some("Let me check a current source first."),
            route: "web-enabled model",
            reason: "current_public_info; web lookup required",
            latency_ms: Some(1_200),
            cost: Some(0.002),
            rating: None,
            correction: None,
            local_only: false,
            visible: Some("What is today's top news headline?"),
        },
        RouteScenario {
            trace_id: "rc-private-file",
            prompt: "Summarize the spreadsheet at /Users/bob/private-budget.xlsx",
            answer: Some("Keeping this local; reading the file with the local tool."),
            route: "local-file-model",
            reason: "private_file_analysis; local only",
            latency_ms: Some(80),
            cost: Some(0.0),
            rating: None,
            correction: None,
            local_only: true,
            visible: None,
        },
        RouteScenario {
            trace_id: "rc-high-stakes",
            prompt:
                "Explain the trade-offs of filing taxes as an LLC versus a sole proprietorship.",
            answer: Some("This is high-stakes; let me clarify your situation before answering."),
            route: "ask-clarifying-question-or-escalate",
            reason: "high_stakes_advice; clarify before answering",
            latency_ms: Some(2_000),
            cost: Some(0.004),
            rating: None,
            correction: None,
            local_only: false,
            visible: Some(
                "Explain the trade-offs of filing taxes as an LLC versus a sole proprietorship.",
            ),
        },
        RouteScenario {
            trace_id: "rc-code",
            prompt: "Implement a binary search function in Rust.",
            answer: Some("Escalating to the coding-specialist model."),
            route: "coding-specialist-model",
            reason: "code_implementation; coding specialist required",
            latency_ms: Some(3_000),
            cost: Some(0.03),
            rating: None,
            correction: None,
            local_only: false,
            visible: Some("Implement a binary search function in Rust."),
        },
    ];

    let mut items = Vec::new();
    for scenario in scenarios {
        let trace = scenario.to_row(&policy)?;
        let item = generate_route_correctness_rubric(RouteCorrectnessRubricInput {
            trace,
            visible_prompt: scenario.visible.map(str::to_string),
            models: models.clone(),
            tools: tools.clone(),
            route_policy: policy.clone(),
        })?;
        items.push(item);
    }
    Ok(items)
}

fn tooluse_items() -> Result<Vec<TrainingItem>, RlvrError> {
    let policy = RoutePolicy::default();
    let tools = standard_tools();
    let scenarios = [
        RouteScenario {
            trace_id: "tu-multi-tool",
            prompt: "What is the latest AAPL stock price and the current weather forecast?",
            answer: Some("This needs current finance and weather tools before answering."),
            route: "web-enabled model",
            reason: "current_public_info; finance; weather",
            latency_ms: Some(2_500),
            cost: Some(0.006),
            rating: None,
            correction: None,
            local_only: false,
            visible: Some("What is the latest AAPL stock price and the current weather forecast?"),
        },
        RouteScenario {
            trace_id: "tu-no-tool",
            prompt: "Explain what recursion is.",
            answer: Some("Recursion is when a function calls itself."),
            route: "tiny-local-model",
            reason: "stable_knowledge; local model sufficient",
            latency_ms: Some(20),
            cost: Some(0.0),
            rating: Some(5),
            correction: None,
            local_only: false,
            visible: Some("Explain what recursion is."),
        },
    ];

    let mut items = Vec::new();
    for scenario in scenarios {
        let trace = scenario.to_row(&policy)?;
        let item = generate_tool_use_rubric(ToolUseRubricInput {
            trace,
            visible_prompt: scenario.visible.map(str::to_string),
            tools: tools.clone(),
            route_policy: policy.clone(),
        })?;
        items.push(item);
    }
    Ok(items)
}

fn compressionloss_items() -> Result<Vec<TrainingItem>, RlvrError> {
    let policy = RoutePolicy::default();
    let privacy = PrivacyPolicy::default();
    let items = vec![
        generate_compression_loss_rubric(CompressionLossRubricInput {
            task_id: "cl-capacitor-spec".into(),
            visible_source: Some("Source says: keep 22uF, 6.3V, citation [A], and local-only.".into()),
            compressed_output: Some("Keep the capacitor details.".into()),
            source_hash: crate::hash_bytes(b"source 22uF 6.3V citation A local-only"),
            compressed_output_hash: crate::hash_bytes(b"compressed capacitor details"),
            required_facts: vec![CompressionRequiredFact {
                fact_id: "capacitor-spec".into(),
                description: "22uF capacitor rated 6.3V or higher with citation [A] and local-only constraint".into(),
                expected_preserved_answer: "22uF, 6.3V or higher, citation [A], local-only".into(),
                numeric_fidelity_required: true,
                citation_required: true,
                constraint_required: true,
            }],
            route_policy: policy.clone(),
            privacy_policy: privacy.clone(),
        })?,
        generate_compression_loss_rubric(CompressionLossRubricInput {
            task_id: "cl-trip-budget".into(),
            visible_source: Some("Trip: 2 passengers, $1,200 total, must stay under $1,500 budget, source [B].".into()),
            compressed_output: Some("Trip details summarized.".into()),
            source_hash: crate::hash_bytes(b"trip 2 passengers 1200 budget 1500 source B"),
            compressed_output_hash: crate::hash_bytes(b"trip summary"),
            required_facts: vec![CompressionRequiredFact {
                fact_id: "trip-budget".into(),
                description: "Trip budget of $1,200 must stay under the $1,500 cap for 2 passengers".into(),
                expected_preserved_answer: "2 passengers, $1,200 total, under $1,500, citation [B]".into(),
                numeric_fidelity_required: true,
                citation_required: true,
                constraint_required: true,
            }],
            route_policy: policy.clone(),
            privacy_policy: privacy.clone(),
        })?,
    ];
    Ok(items)
}

fn user_trace_replay_items() -> Result<Vec<TrainingItem>, RlvrError> {
    let policy = RoutePolicy::default();
    let models = standard_models();
    let tools = standard_tools();
    let scenarios = [
        RouteScenario {
            trace_id: "replay-user-001",
            prompt: "How do I center a div with CSS?",
            answer: Some("Use flexbox with justify-content and align-items center."),
            route: "tiny-local-model",
            reason: "stable_knowledge; local model sufficient",
            latency_ms: Some(40),
            cost: Some(0.0),
            rating: Some(5),
            correction: None,
            local_only: false,
            visible: Some("How do I center a div with CSS?"),
        },
        RouteScenario {
            trace_id: "replay-user-002",
            prompt: "What is the current population of Tokyo?",
            answer: Some("Checking a current source."),
            route: "web-enabled model",
            reason: "current_public_info; web lookup required",
            latency_ms: Some(1_500),
            cost: Some(0.003),
            rating: Some(3),
            correction: Some("Needed a fresher source than 2020."),
            local_only: false,
            visible: Some("What is the current population of Tokyo?"),
        },
        RouteScenario {
            trace_id: "replay-user-003",
            prompt: "Summarize the contract at /Users/carol/private-contract.pdf",
            answer: Some("Keeping this local with the file reader."),
            route: "local-file-model",
            reason: "private_file_analysis; local only",
            latency_ms: Some(120),
            cost: Some(0.0),
            rating: None,
            correction: None,
            local_only: true,
            visible: None,
        },
    ];

    let mut items = Vec::new();
    for scenario in scenarios {
        let trace = scenario.to_row(&policy)?;
        let item = generate_route_correctness_rubric(RouteCorrectnessRubricInput {
            trace,
            visible_prompt: scenario.visible.map(str::to_string),
            models: models.clone(),
            tools: tools.clone(),
            route_policy: policy.clone(),
        })?;
        items.push(item);
    }
    Ok(items)
}

// ---------------------------------------------------------------------------
// Fixtures and helpers
// ---------------------------------------------------------------------------

struct RouteScenario {
    trace_id: &'static str,
    prompt: &'static str,
    answer: Option<&'static str>,
    route: &'static str,
    reason: &'static str,
    latency_ms: Option<u64>,
    cost: Option<f64>,
    rating: Option<u32>,
    correction: Option<&'static str>,
    local_only: bool,
    visible: Option<&'static str>,
}

impl RouteScenario {
    fn to_row(&self, policy: &RoutePolicy) -> Result<RouteTraceRow, RlvrError> {
        RouteTraceRow::build(
            &RouteTraceInput {
                prompt: self.prompt,
                answer: self.answer,
                selected_route: self.route,
                router_reason: self.reason,
                route_policy: policy,
                latency_ms: self.latency_ms,
                cost_estimate: self.cost,
                user_rating: self.rating,
                user_correction: self.correction,
            },
            self.trace_id.into(),
            1,
            self.local_only,
        )
    }
}

fn standard_models() -> Vec<ModelInventoryItem> {
    vec![
        ModelInventoryItem {
            model_id: "tiny-local-model".into(),
            local: true,
            capabilities: vec!["general_qa".into(), "stable_knowledge".into()],
            max_cost: Some(0.0),
            max_latency_ms: Some(2_000),
        },
        ModelInventoryItem {
            model_id: "local-file-model".into(),
            local: true,
            capabilities: vec!["local_file_analysis".into()],
            max_cost: Some(0.0),
            max_latency_ms: Some(10_000),
        },
        ModelInventoryItem {
            model_id: "web-enabled model".into(),
            local: false,
            capabilities: vec!["web_or_current_info".into(), "current_public_info".into()],
            max_cost: Some(0.01),
            max_latency_ms: Some(15_000),
        },
        ModelInventoryItem {
            model_id: "coding-specialist-model".into(),
            local: false,
            capabilities: vec!["code_generation".into(), "code_implementation".into()],
            max_cost: Some(0.05),
            max_latency_ms: Some(30_000),
        },
    ]
}

fn standard_tools() -> Vec<ToolInventoryItem> {
    vec![
        ToolInventoryItem {
            tool_id: "web_search".into(),
            supports_current_info: true,
            safe_for_private_data: false,
        },
        ToolInventoryItem {
            tool_id: "local_file_reader".into(),
            supports_current_info: false,
            safe_for_private_data: true,
        },
        ToolInventoryItem {
            tool_id: "domain_verifier".into(),
            supports_current_info: true,
            safe_for_private_data: true,
        },
        ToolInventoryItem {
            tool_id: "finance_lookup".into(),
            supports_current_info: true,
            safe_for_private_data: false,
        },
        ToolInventoryItem {
            tool_id: "weather_lookup".into(),
            supports_current_info: true,
            safe_for_private_data: false,
        },
    ]
}

fn askmind_item(
    task_id: &str,
    visible_user_query: &str,
    hidden_original_query: &str,
    gold_answer: &str,
    domain: &str,
    difficulty: Difficulty,
    checkpoints: Vec<Checkpoint>,
) -> TrainingItem {
    TrainingItem {
        task_id: task_id.into(),
        mode: TrainingMode::AskMind,
        visible_user_query: visible_user_query.into(),
        hidden_original_query: hidden_original_query.into(),
        gold_answer: gold_answer.into(),
        domain: domain.into(),
        difficulty,
        checkpoints,
        route_policy: RoutePolicy::default(),
        privacy_policy: PrivacyPolicy::default(),
    }
}

fn missing(checkpoint_id: &str, description: &str, answer_if_asked: &str) -> Checkpoint {
    Checkpoint {
        checkpoint_id: checkpoint_id.into(),
        checkpoint_type: CheckpointType::MissingInfo,
        description: description.into(),
        must_resolve_before_answer: true,
        answer_if_asked: answer_if_asked.into(),
        failure_penalty: 0.75,
    }
}

fn require_non_empty(name: &str, value: &str) -> Result<(), RlvrError> {
    if value.trim().is_empty() {
        return Err(RlvrError::Config(format!("{name} cannot be empty")));
    }
    Ok(())
}

fn require_bounded_unit(name: &str, value: f64) -> Result<(), RlvrError> {
    if !value.is_finite() {
        return Err(RlvrError::Config(format!("{name} must be finite")));
    }
    if !(0.0..=1.0).contains(&value) {
        return Err(RlvrError::Config(format!("{name} must be within [0, 1]")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_baseline_set_kind_builds_and_validates() {
        for kind in BaselineEvalSetKind::ALL {
            let set = baseline_eval_set(*kind).expect("set builds");
            assert_eq!(set.kind, *kind);
            assert!(set.task_count() > 0, "{kind:?} set is empty");
            set.validate().expect("set validates");
            assert_eq!(set.kind.item_mode(), kind.item_mode());
            // Every item carries the set's training mode.
            assert!(set.items.iter().all(|item| item.mode == kind.item_mode()));
            // Stable hashes are 64-char hex and reproducible.
            let hash = set.stable_hash().expect("hash");
            assert_eq!(hash.len(), 64);
            assert_eq!(hash, set.stable_hash().expect("hash again"));
        }
    }

    #[test]
    fn default_baseline_eval_sets_returns_one_set_per_kind_in_order() {
        let sets = default_baseline_eval_sets().expect("sets build");
        assert_eq!(sets.len(), BaselineEvalSetKind::ALL.len());
        for (set, kind) in sets.iter().zip(BaselineEvalSetKind::ALL.iter()) {
            assert_eq!(set.kind, *kind);
            set.validate().expect("set validates");
        }
    }

    #[test]
    fn baseline_sets_are_deterministic_across_builds() {
        let first = default_baseline_eval_sets().expect("first build");
        let second = default_baseline_eval_sets().expect("second build");
        assert_eq!(first, second);
        for (a, b) in first.iter().zip(second.iter()) {
            assert_eq!(a.stable_hash().unwrap(), b.stable_hash().unwrap());
        }
    }

    #[test]
    fn set_hash_is_field_sensitive() {
        let mut set = baseline_eval_set(BaselineEvalSetKind::AskMind).unwrap();
        let original = set.stable_hash().unwrap();
        set.items[0].checkpoints[0].failure_penalty = 0.9;
        assert_ne!(set.stable_hash().unwrap(), original);
    }

    #[test]
    fn askmind_set_items_carry_missing_info_checkpoints() {
        let set = baseline_eval_set(BaselineEvalSetKind::AskMind).unwrap();
        assert!(set.task_count() >= 4);
        for item in &set.items {
            assert_eq!(item.mode, TrainingMode::AskMind);
            assert!(item
                .checkpoints
                .iter()
                .all(|checkpoint| checkpoint.checkpoint_type == CheckpointType::MissingInfo));
            // AskMind items are degraded prompts: the hidden query carries more info.
            assert_ne!(item.visible_user_query, item.hidden_original_query);
        }
    }

    #[test]
    fn askoverconfidence_set_is_built_from_false_premise_fixtures() {
        let set = baseline_eval_set(BaselineEvalSetKind::AskOverconfidence).unwrap();
        assert_eq!(
            set.task_count(),
            crate::rubrics::sample_fixtures().len(),
            "one item per AskOverconfidence fixture"
        );
        for item in &set.items {
            assert_eq!(item.mode, TrainingMode::AskOverconfidence);
            // The visible prompt embeds the premise (inject mode) and the gold
            // answer is the correction.
            assert!(!item.gold_answer.is_empty());
        }
    }

    #[test]
    fn routecorrectness_set_covers_task_types_and_keeps_private_traces_local() {
        let set = baseline_eval_set(BaselineEvalSetKind::RouteCorrectness).unwrap();
        let task_types: Vec<&str> = set.items.iter().map(|item| item.domain.as_str()).collect();
        for expected in [
            "stable_knowledge",
            "current_public_info",
            "private_file_analysis",
            "high_stakes_advice",
            "code_implementation",
        ] {
            assert!(
                task_types.contains(&expected),
                "missing task type {expected}"
            );
        }

        // The private-file item must stay local-only and never expose its raw prompt.
        let private = set
            .items
            .iter()
            .find(|item| item.domain == "private_file_analysis")
            .expect("private item exists");
        assert!(private.privacy_policy.local_only);
        assert!(!private.privacy_policy.allow_external_models);
        assert!(
            private.visible_user_query.contains("hash-only")
                || private.visible_user_query.contains("prompt_hash="),
            "private visible query must be hash-only, got: {}",
            private.visible_user_query
        );
    }

    #[test]
    fn tooluse_set_emits_required_tool_and_no_tool_checkpoints() {
        let set = baseline_eval_set(BaselineEvalSetKind::ToolUse).unwrap();
        let checkpoint_ids: Vec<&str> = set
            .items
            .iter()
            .flat_map(|item| {
                item.checkpoints
                    .iter()
                    .map(|checkpoint| checkpoint.checkpoint_id.as_str())
            })
            .collect();
        assert!(
            checkpoint_ids.contains(&"tu-no-tool-required"),
            "missing no-tool checkpoint"
        );
        for required in ["tu-current-info", "tu-finance", "tu-weather"] {
            assert!(checkpoint_ids.contains(&required), "missing {required}");
        }
    }

    #[test]
    fn compressionloss_set_checks_facts_numbers_citations_and_constraints() {
        let set = baseline_eval_set(BaselineEvalSetKind::CompressionLoss).unwrap();
        let checkpoint_ids: Vec<&str> = set
            .items
            .iter()
            .flat_map(|item| {
                item.checkpoints
                    .iter()
                    .map(|checkpoint| checkpoint.checkpoint_id.as_str())
            })
            .collect();
        for required in [
            "cl-dropped-fact-capacitor-spec",
            "cl-numeric-fidelity-capacitor-spec",
            "cl-citation-capacitor-spec",
            "cl-constraint-capacitor-spec",
        ] {
            assert!(checkpoint_ids.contains(&required), "missing {required}");
        }
    }

    #[test]
    fn user_trace_replay_set_derives_route_items_from_captured_traces() {
        let set = baseline_eval_set(BaselineEvalSetKind::UserTraceReplay).unwrap();
        assert!(set.task_count() >= 3);
        for item in &set.items {
            assert_eq!(item.mode, TrainingMode::RouteCorrectness);
            // Replay provenance is encoded in the task id.
            assert!(
                item.task_id.starts_with("route-rubric-replay-user-"),
                "replay task id should encode provenance: {}",
                item.task_id
            );
        }
        // The private replay trace stays local-only with a hash-only prompt.
        let private = set
            .items
            .iter()
            .find(|item| item.privacy_policy.local_only)
            .expect("a private replay item exists");
        assert!(private.visible_user_query.contains("prompt_hash="));
    }

    #[test]
    fn manifest_lists_all_sets_with_counts_and_hashes() {
        let manifest = baseline_eval_set_manifest().unwrap();
        assert_eq!(manifest.version, "baseline-evals-v0.1");
        assert_eq!(manifest.set_count, BaselineEvalSetKind::ALL.len());
        assert_eq!(manifest.entries.len(), BaselineEvalSetKind::ALL.len());
        assert!(manifest.total_items > 0);
        assert_eq!(
            manifest
                .entries
                .iter()
                .map(|entry| entry.item_count)
                .sum::<usize>(),
            manifest.total_items
        );
        for entry in &manifest.entries {
            assert_eq!(entry.item_mode, entry.kind.item_mode());
            assert_eq!(entry.set_hash.len(), 64);
        }
    }

    // --- comparison harness ---

    fn final_output(reward: f64, resolved_ids: &[String]) -> StrictVerifierOutput {
        StrictVerifierOutput {
            is_final_answer: true,
            is_clarification_question: false,
            is_tool_call: false,
            is_route_decision: false,
            targeted_checkpoints: resolved_ids.to_vec(),
            resolved_checkpoints: resolved_ids.to_vec(),
            missed_checkpoints: Vec::new(),
            redundant_question: false,
            premature_answer: false,
            false_premise_corrected: None,
            route_valid: true,
            reward,
        }
    }

    fn outputs_for(
        set: &BaselineEvalSet,
        reward: f64,
        full_coverage: bool,
    ) -> Vec<StrictVerifierOutput> {
        set.items
            .iter()
            .map(|item| {
                let resolved = if full_coverage {
                    item.checkpoints
                        .iter()
                        .map(|checkpoint| checkpoint.checkpoint_id.clone())
                        .collect::<Vec<_>>()
                } else {
                    Vec::new()
                };
                final_output(reward, &resolved)
            })
            .collect()
    }

    #[test]
    fn scoring_requires_one_final_answer_output_per_item() {
        let set = baseline_eval_set(BaselineEvalSetKind::AskMind).unwrap();
        // Too few outputs.
        let err = score_baseline_eval_set(&set, "base", &[]).unwrap_err();
        assert!(err.to_string().contains("verifier outputs were supplied"));
        // Too many outputs.
        let too_many = outputs_for(&set, 1.0, true);
        let mut too_many_with_extra = too_many.clone();
        too_many_with_extra.push(final_output(1.0, &[]));
        let err = score_baseline_eval_set(&set, "base", &too_many_with_extra).unwrap_err();
        assert!(err.to_string().contains("verifier outputs were supplied"));
        // Empty system id.
        let err = score_baseline_eval_set(&set, "", &outputs_for(&set, 1.0, true)).unwrap_err();
        assert!(err.to_string().contains("system_id"));
    }

    #[test]
    fn scoring_rejects_non_final_answer_output() {
        let set = baseline_eval_set(BaselineEvalSetKind::AskMind).unwrap();
        // One output per item (correct count), but none is a final-answer output.
        let mut outputs = outputs_for(&set, 1.0, true);
        for output in &mut outputs {
            output.is_final_answer = false;
        }
        let err = score_baseline_eval_set(&set, "base", &outputs).unwrap_err();
        assert!(err.to_string().contains("final answer scoring"));
    }

    #[test]
    fn compare_baseline_eval_set_shows_adapter_improving_over_base() {
        let set = baseline_eval_set(BaselineEvalSetKind::RouteCorrectness).unwrap();

        // Base model: wrong answers, no checkpoint coverage.
        let base_outputs = outputs_for(&set, 0.0, false);
        // Adapter: correct answers, full coverage.
        let adapter_outputs = outputs_for(&set, 1.0, true);

        let report = compare_baseline_eval_set(
            &set,
            "base-model",
            &base_outputs,
            "router-rlvr-v0.1",
            &adapter_outputs,
        )
        .expect("comparison");

        assert_eq!(report.kind, BaselineEvalSetKind::RouteCorrectness);
        assert_eq!(report.item_count, set.task_count());
        assert_eq!(report.set_hash, set.stable_hash().unwrap());
        assert_eq!(report.base.system_id, "base-model");
        assert_eq!(report.adapter.system_id, "router-rlvr-v0.1");

        // Base is near-zero; adapter is high.
        assert!(report.base.mean_final_score < report.adapter.mean_final_score);
        assert!(report.adapter.mean_final_score > 0.9);
        assert!(report.adapter.mean_rubric_coverage > 0.9);
        assert_eq!(report.adapter.pass_rate, 1.0);

        assert!(report.final_score_delta > 0.0);
        assert!(report.coverage_delta > 0.0);
        assert!(report.pass_rate_delta > 0.0);
        assert!(report.adapter_improves);
    }

    #[test]
    fn compare_baseline_eval_set_flags_when_adapter_does_not_improve() {
        let set = baseline_eval_set(BaselineEvalSetKind::ToolUse).unwrap();
        // Base is strong; adapter is weak.
        let base_outputs = outputs_for(&set, 1.0, true);
        let adapter_outputs = outputs_for(&set, 0.0, false);
        let report =
            compare_baseline_eval_set(&set, "base", &base_outputs, "bad-adapter", &adapter_outputs)
                .unwrap();
        assert!(!report.adapter_improves);
        assert!(report.final_score_delta < 0.0);
        assert!(report.coverage_delta < 0.0);
    }

    #[test]
    fn every_set_supports_a_full_base_vs_adapter_comparison() {
        // "Base model and trained adapter can be compared" gate: every set must
        // produce a well-formed comparison report.
        for set in default_baseline_eval_sets().unwrap() {
            let base = outputs_for(&set, 0.2, false);
            let adapter = outputs_for(&set, 0.95, true);
            let report =
                compare_baseline_eval_set(&set, "base", &base, "adapter", &adapter).unwrap();
            report.base.validate().unwrap();
            report.adapter.validate().unwrap();
            assert_eq!(report.item_count, set.task_count());
            assert!(report.adapter_improves, "{:?} should improve", set.kind);
        }
    }

    #[test]
    fn baseline_set_kind_round_trips_through_string() {
        for kind in BaselineEvalSetKind::ALL {
            assert_eq!(BaselineEvalSetKind::parse(kind.as_str()), Some(*kind));
        }
        assert_eq!(
            BaselineEvalSetKind::parse("replay"),
            Some(BaselineEvalSetKind::UserTraceReplay)
        );
        assert!(BaselineEvalSetKind::parse("nonsense").is_none());
    }
}
