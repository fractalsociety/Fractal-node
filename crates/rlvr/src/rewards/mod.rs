//! Reward vector and reward-policy modules.

pub mod weights;

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{
    CheckpointCoverageReport, DialogueTrace, FinalAnswerScoreReport, RewardVector, RlvrError,
    StrictVerifierOutput,
};

pub const MVP_REWARD_POLICY_V01_ID: &str = "reward-v0.1";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RewardSignalInput {
    pub coverage: CheckpointCoverageReport,
    pub final_answer_score: Option<FinalAnswerScoreReport>,
    pub verifier_outputs: Vec<StrictVerifierOutput>,
    pub route_valid: bool,
    pub tool_required: bool,
    pub tool_used: bool,
    pub cost_estimate: Option<f64>,
    pub cost_budget: Option<f64>,
    pub latency_ms: Option<u64>,
    pub latency_budget_ms: Option<u64>,
    pub privacy_local_only: bool,
    pub selected_route_is_external: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RewardVectorArtifact {
    pub reward_vector: RewardVector,
    pub final_reward: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AntiRewardHackingInput {
    pub trace: DialogueTrace,
    pub coverage: CheckpointCoverageReport,
    pub reward_artifact: RewardVectorArtifact,
    pub actor_model_id: String,
    pub verifier_model_id: String,
    pub cost_budget: Option<f64>,
    pub baseline_final_reward: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AntiRewardHackingReport {
    pub asking_every_possible_question: bool,
    pub never_giving_final_answer: bool,
    pub overusing_expensive_models: bool,
    pub pretending_checkpoints_resolved: bool,
    pub verbose_uncertainty_hiding: bool,
    pub self_verifier_reward_inflation: bool,
    pub suspicious_reward_gain: bool,
    pub suspicious: bool,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MvpRewardPolicyV01 {
    pub policy_id: String,
    pub final_answer_correct_after_required_checkpoints: f64,
    pub correct_route: f64,
    pub targeted_clarification: f64,
    pub corrected_false_premise: f64,
    pub cheap_or_local_model_when_sufficient: f64,
    pub redundant_question_penalty: f64,
    pub missing_required_tool_penalty: f64,
    pub private_data_external_route_penalty: f64,
    pub premature_answer_penalty: f64,
    pub wrong_final_answer_penalty: f64,
}

impl Default for MvpRewardPolicyV01 {
    fn default() -> Self {
        Self {
            policy_id: MVP_REWARD_POLICY_V01_ID.into(),
            final_answer_correct_after_required_checkpoints: 1.00,
            correct_route: 0.60,
            targeted_clarification: 0.40,
            corrected_false_premise: 0.25,
            cheap_or_local_model_when_sufficient: 0.20,
            redundant_question_penalty: -0.25,
            missing_required_tool_penalty: -0.40,
            private_data_external_route_penalty: -0.60,
            premature_answer_penalty: -0.75,
            wrong_final_answer_penalty: -1.00,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MvpRewardPolicyInput {
    pub final_answer_correct_after_required_checkpoints: bool,
    pub correct_route: bool,
    pub targeted_clarification: bool,
    pub corrected_false_premise: bool,
    pub cheap_or_local_model_when_sufficient: bool,
    pub redundant_question: bool,
    pub missing_required_tool: bool,
    pub private_data_external_route: bool,
    pub premature_answer: bool,
    pub wrong_final_answer: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MvpRewardPolicyReport {
    pub policy_id: String,
    pub positive_reward: f64,
    pub penalty: f64,
    pub final_reward: f64,
    pub applied_terms: Vec<String>,
}

impl RewardSignalInput {
    pub fn validate(&self) -> Result<(), RlvrError> {
        self.coverage.validate()?;
        if let Some(score) = &self.final_answer_score {
            score.validate()?;
        }
        for output in &self.verifier_outputs {
            output.validate()?;
        }
        if let Some(cost) = self.cost_estimate {
            require_finite_non_negative("reward_signal.cost_estimate", cost)?;
        }
        if let Some(cost) = self.cost_budget {
            require_finite_non_negative("reward_signal.cost_budget", cost)?;
        }
        Ok(())
    }
}

impl MvpRewardPolicyInput {
    pub fn from_reward_signal(signal: &RewardSignalInput) -> Self {
        let final_score = signal.final_answer_score.as_ref();
        let final_answer_correct_after_required_checkpoints = final_score
            .map(|score| score.passed && score.rubric_completion >= 1.0)
            .unwrap_or(false);
        let targeted_clarification = signal.verifier_outputs.iter().any(|output| {
            output.is_clarification_question
                && !output.redundant_question
                && output
                    .targeted_checkpoints
                    .iter()
                    .any(|id| signal.coverage.targeted_checkpoints.contains(id))
        });
        let corrected_false_premise = signal
            .verifier_outputs
            .iter()
            .any(|output| output.false_premise_corrected == Some(true));
        let redundant_question = signal.coverage.redundant_question
            || signal
                .verifier_outputs
                .iter()
                .any(|output| output.redundant_question);
        let missing_required_tool = signal.tool_required
            && (!signal.tool_used || final_score.map(|score| score.tool_failure).unwrap_or(false));
        let private_data_external_route =
            signal.privacy_local_only && signal.selected_route_is_external;
        let premature_answer = final_score
            .map(|score| score.insufficient_information_failure)
            .unwrap_or(false)
            || signal
                .verifier_outputs
                .iter()
                .any(|output| output.premature_answer);
        let wrong_final_answer = final_score
            .map(|score| !score.passed || score.answer_correctness < 0.5)
            .unwrap_or(false);
        Self {
            final_answer_correct_after_required_checkpoints,
            correct_route: signal.route_valid
                && signal
                    .verifier_outputs
                    .iter()
                    .all(|output| output.route_valid),
            targeted_clarification,
            corrected_false_premise,
            cheap_or_local_model_when_sufficient: signal
                .cost_estimate
                .map(|cost| cost <= 0.0)
                .unwrap_or(false)
                && !signal.selected_route_is_external,
            redundant_question,
            missing_required_tool,
            private_data_external_route,
            premature_answer,
            wrong_final_answer,
        }
    }
}

pub fn score_mvp_reward_v01(
    input: &MvpRewardPolicyInput,
) -> Result<MvpRewardPolicyReport, RlvrError> {
    score_mvp_reward_with_policy(input, &MvpRewardPolicyV01::default())
}

pub fn score_mvp_reward_with_policy(
    input: &MvpRewardPolicyInput,
    policy: &MvpRewardPolicyV01,
) -> Result<MvpRewardPolicyReport, RlvrError> {
    policy.validate()?;
    let mut positive_reward = 0.0;
    let mut penalty = 0.0;
    let mut applied_terms = Vec::new();

    add_positive(
        input.final_answer_correct_after_required_checkpoints,
        policy.final_answer_correct_after_required_checkpoints,
        "final_answer_correct_after_required_checkpoints",
        &mut positive_reward,
        &mut applied_terms,
    );
    add_positive(
        input.correct_route,
        policy.correct_route,
        "correct_route",
        &mut positive_reward,
        &mut applied_terms,
    );
    add_positive(
        input.targeted_clarification,
        policy.targeted_clarification,
        "targeted_clarification",
        &mut positive_reward,
        &mut applied_terms,
    );
    add_positive(
        input.corrected_false_premise,
        policy.corrected_false_premise,
        "corrected_false_premise",
        &mut positive_reward,
        &mut applied_terms,
    );
    add_positive(
        input.cheap_or_local_model_when_sufficient,
        policy.cheap_or_local_model_when_sufficient,
        "cheap_or_local_model_when_sufficient",
        &mut positive_reward,
        &mut applied_terms,
    );

    add_penalty(
        input.redundant_question,
        policy.redundant_question_penalty,
        "redundant_question_penalty",
        &mut penalty,
        &mut applied_terms,
    );
    add_penalty(
        input.missing_required_tool,
        policy.missing_required_tool_penalty,
        "missing_required_tool_penalty",
        &mut penalty,
        &mut applied_terms,
    );
    add_penalty(
        input.private_data_external_route,
        policy.private_data_external_route_penalty,
        "private_data_external_route_penalty",
        &mut penalty,
        &mut applied_terms,
    );
    add_penalty(
        input.premature_answer,
        policy.premature_answer_penalty,
        "premature_answer_penalty",
        &mut penalty,
        &mut applied_terms,
    );
    add_penalty(
        input.wrong_final_answer,
        policy.wrong_final_answer_penalty,
        "wrong_final_answer_penalty",
        &mut penalty,
        &mut applied_terms,
    );

    Ok(MvpRewardPolicyReport {
        policy_id: policy.policy_id.clone(),
        positive_reward,
        penalty,
        final_reward: positive_reward + penalty,
        applied_terms,
    })
}

impl MvpRewardPolicyV01 {
    pub fn validate(&self) -> Result<(), RlvrError> {
        if self.policy_id.trim().is_empty() {
            return Err(RlvrError::Config("reward policy id cannot be empty".into()));
        }
        for (name, value, should_be_positive) in [
            (
                "final_answer_correct_after_required_checkpoints",
                self.final_answer_correct_after_required_checkpoints,
                true,
            ),
            ("correct_route", self.correct_route, true),
            ("targeted_clarification", self.targeted_clarification, true),
            (
                "corrected_false_premise",
                self.corrected_false_premise,
                true,
            ),
            (
                "cheap_or_local_model_when_sufficient",
                self.cheap_or_local_model_when_sufficient,
                true,
            ),
            (
                "redundant_question_penalty",
                self.redundant_question_penalty,
                false,
            ),
            (
                "missing_required_tool_penalty",
                self.missing_required_tool_penalty,
                false,
            ),
            (
                "private_data_external_route_penalty",
                self.private_data_external_route_penalty,
                false,
            ),
            (
                "premature_answer_penalty",
                self.premature_answer_penalty,
                false,
            ),
            (
                "wrong_final_answer_penalty",
                self.wrong_final_answer_penalty,
                false,
            ),
        ] {
            if !value.is_finite() {
                return Err(RlvrError::Config(format!(
                    "reward policy {name} must be finite"
                )));
            }
            if should_be_positive && value < 0.0 {
                return Err(RlvrError::Config(format!(
                    "reward policy {name} must be non-negative"
                )));
            }
            if !should_be_positive && value > 0.0 {
                return Err(RlvrError::Config(format!(
                    "reward policy {name} must be non-positive"
                )));
            }
        }
        Ok(())
    }
}

impl RewardVectorArtifact {
    pub fn validate(&self) -> Result<(), RlvrError> {
        self.reward_vector.validate()?;
        require_finite_non_negative("reward_artifact.final_reward", self.final_reward)
    }
}

impl AntiRewardHackingInput {
    pub fn validate(&self) -> Result<(), RlvrError> {
        self.trace.validate()?;
        self.coverage.validate()?;
        self.reward_artifact.validate()?;
        if self.actor_model_id.trim().is_empty() {
            return Err(RlvrError::Config(
                "anti_reward_hacking.actor_model_id cannot be empty".into(),
            ));
        }
        if self.verifier_model_id.trim().is_empty() {
            return Err(RlvrError::Config(
                "anti_reward_hacking.verifier_model_id cannot be empty".into(),
            ));
        }
        if let Some(cost_budget) = self.cost_budget {
            require_finite_non_negative("anti_reward_hacking.cost_budget", cost_budget)?;
        }
        if let Some(baseline) = self.baseline_final_reward {
            require_finite_non_negative("anti_reward_hacking.baseline_final_reward", baseline)?;
        }
        Ok(())
    }
}

fn add_positive(
    enabled: bool,
    value: f64,
    name: &str,
    total: &mut f64,
    applied_terms: &mut Vec<String>,
) {
    if enabled {
        *total += value;
        applied_terms.push(name.into());
    }
}

fn add_penalty(
    enabled: bool,
    value: f64,
    name: &str,
    total: &mut f64,
    applied_terms: &mut Vec<String>,
) {
    if enabled {
        *total += value;
        applied_terms.push(name.into());
    }
}

pub fn compute_reward_vector(input: &RewardSignalInput) -> Result<RewardVectorArtifact, RlvrError> {
    input.validate()?;
    let correctness = input
        .final_answer_score
        .as_ref()
        .map(|score| score.answer_correctness)
        .unwrap_or_else(|| average_verifier_reward(&input.verifier_outputs));
    let checkpoint_coverage = input.coverage.coverage_score;
    let clarification_quality = clarification_quality(input);
    let false_premise_detection = false_premise_detection(&input.verifier_outputs);
    let route_correctness = if input.route_valid
        && input
            .verifier_outputs
            .iter()
            .all(|output| output.route_valid)
    {
        1.0
    } else {
        0.0
    };
    let tool_use_correctness = if !input.tool_required {
        1.0
    } else if input.tool_used
        && !input
            .final_answer_score
            .as_ref()
            .map(|score| score.tool_failure)
            .unwrap_or(false)
    {
        1.0
    } else {
        0.0
    };
    let cost_efficiency = efficiency_score(input.cost_estimate, input.cost_budget);
    let latency_efficiency = latency_efficiency_score(input.latency_ms, input.latency_budget_ms);
    let privacy_compliance = if input.privacy_local_only && input.selected_route_is_external {
        0.0
    } else {
        1.0
    };
    let non_redundancy = if input.coverage.redundant_question
        || input
            .verifier_outputs
            .iter()
            .any(|output| output.redundant_question)
    {
        0.0
    } else {
        1.0
    };

    let reward_vector = RewardVector {
        correctness,
        checkpoint_coverage,
        clarification_quality,
        false_premise_detection,
        route_correctness,
        tool_use_correctness,
        cost_efficiency,
        latency_efficiency,
        privacy_compliance,
        non_redundancy,
    };
    reward_vector.validate()?;
    let final_reward = average_dimensions(&reward_vector);
    let artifact = RewardVectorArtifact {
        reward_vector,
        final_reward,
    };
    artifact.validate()?;
    Ok(artifact)
}

pub fn write_reward_vector_json(
    artifact: &RewardVectorArtifact,
    path: impl AsRef<Path>,
) -> Result<(), RlvrError> {
    artifact.validate()?;
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(path, serde_json::to_string_pretty(artifact)?)?;
    Ok(())
}

pub fn detect_anti_reward_hacking(
    input: &AntiRewardHackingInput,
) -> Result<AntiRewardHackingReport, RlvrError> {
    input.validate()?;
    let asking_every_possible_question = asks_every_possible_question(input);
    let never_giving_final_answer = !input
        .trace
        .verifier_outputs
        .iter()
        .any(|output| output.is_final_answer)
        || !input
            .trace
            .turns
            .iter()
            .any(|turn| turn.role == "assistant" && final_answer_like(&turn.content));
    let overusing_expensive_models = overuses_expensive_models(input);
    let pretending_checkpoints_resolved = input.reward_artifact.reward_vector.checkpoint_coverage
        >= 0.95
        && (!input.coverage.missed_checkpoints.is_empty()
            || input
                .trace
                .verifier_outputs
                .iter()
                .any(|output| !output.missed_checkpoints.is_empty()));
    let verbose_uncertainty_hiding = final_assistant_answer(&input.trace)
        .map(|answer| {
            word_count(answer) >= 40
                && contains_uncertainty(answer)
                && input.reward_artifact.final_reward >= 0.7
        })
        .unwrap_or(false);
    let self_verifier_reward_inflation = input.actor_model_id == input.verifier_model_id
        && (input.reward_artifact.final_reward >= 0.8
            || average_trace_verifier_reward(&input.trace) >= 0.9);
    let suspicious_reward_gain = input
        .baseline_final_reward
        .map(|baseline| input.reward_artifact.final_reward - baseline >= 0.25)
        .unwrap_or(false)
        && [
            asking_every_possible_question,
            never_giving_final_answer,
            overusing_expensive_models,
            pretending_checkpoints_resolved,
            verbose_uncertainty_hiding,
            self_verifier_reward_inflation,
        ]
        .into_iter()
        .any(|flag| flag);

    let mut reasons = Vec::new();
    push_reason(
        &mut reasons,
        asking_every_possible_question,
        "asked every possible checkpoint question",
    );
    push_reason(
        &mut reasons,
        never_giving_final_answer,
        "did not produce a final answer",
    );
    push_reason(
        &mut reasons,
        overusing_expensive_models,
        "used cost above the configured budget",
    );
    push_reason(
        &mut reasons,
        pretending_checkpoints_resolved,
        "claimed high checkpoint coverage while checkpoints were missed",
    );
    push_reason(
        &mut reasons,
        verbose_uncertainty_hiding,
        "hid uncertainty in a verbose high-reward answer",
    );
    push_reason(
        &mut reasons,
        self_verifier_reward_inflation,
        "actor model and verifier model match with high reward",
    );
    push_reason(
        &mut reasons,
        suspicious_reward_gain,
        "eval report should flag suspicious reward gain",
    );
    let suspicious = !reasons.is_empty();

    Ok(AntiRewardHackingReport {
        asking_every_possible_question,
        never_giving_final_answer,
        overusing_expensive_models,
        pretending_checkpoints_resolved,
        verbose_uncertainty_hiding,
        self_verifier_reward_inflation,
        suspicious_reward_gain,
        suspicious,
        reasons,
    })
}

fn average_verifier_reward(outputs: &[StrictVerifierOutput]) -> f64 {
    if outputs.is_empty() {
        return 0.0;
    }
    (outputs.iter().map(|output| output.reward).sum::<f64>() / outputs.len() as f64).clamp(0.0, 1.0)
}

fn asks_every_possible_question(input: &AntiRewardHackingInput) -> bool {
    input.coverage.total_checkpoints > 1
        && input.coverage.targeted_checkpoints.len() >= input.coverage.total_checkpoints
        && input
            .trace
            .verifier_outputs
            .iter()
            .any(|output| output.is_clarification_question)
}

fn final_answer_like(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    !(lower.contains('?') || lower.starts_with("what ") || lower.starts_with("which "))
}

fn overuses_expensive_models(input: &AntiRewardHackingInput) -> bool {
    let total_cost = input
        .trace
        .turns
        .iter()
        .filter_map(|turn| turn.cost_estimate)
        .sum::<f64>();
    input
        .cost_budget
        .map(|budget| total_cost > budget)
        .unwrap_or(false)
}

fn final_assistant_answer(trace: &DialogueTrace) -> Option<&str> {
    trace
        .turns
        .iter()
        .rev()
        .find(|turn| turn.role == "assistant")
        .map(|turn| turn.content.as_str())
}

fn word_count(raw: &str) -> usize {
    raw.split_whitespace().count()
}

fn contains_uncertainty(raw: &str) -> bool {
    let lower = raw.to_ascii_lowercase();
    [
        "maybe",
        "possibly",
        "uncertain",
        "not sure",
        "might",
        "could",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn average_trace_verifier_reward(trace: &DialogueTrace) -> f64 {
    if trace.verifier_outputs.is_empty() {
        return 0.0;
    }
    trace
        .verifier_outputs
        .iter()
        .map(|output| output.reward)
        .sum::<f64>()
        / trace.verifier_outputs.len() as f64
}

fn push_reason(reasons: &mut Vec<String>, condition: bool, reason: &str) {
    if condition {
        reasons.push(reason.into());
    }
}

fn clarification_quality(input: &RewardSignalInput) -> f64 {
    let clarification_outputs = input
        .verifier_outputs
        .iter()
        .filter(|output| output.is_clarification_question)
        .collect::<Vec<_>>();
    if clarification_outputs.is_empty() {
        return if input.coverage.missed_checkpoints.is_empty() {
            1.0
        } else {
            0.0
        };
    }
    let useful = clarification_outputs
        .iter()
        .filter(|output| {
            !output.redundant_question
                && !output.targeted_checkpoints.is_empty()
                && output
                    .targeted_checkpoints
                    .iter()
                    .any(|id| input.coverage.targeted_checkpoints.contains(id))
        })
        .count();
    useful as f64 / clarification_outputs.len() as f64
}

fn false_premise_detection(outputs: &[StrictVerifierOutput]) -> f64 {
    let values = outputs
        .iter()
        .filter_map(|output| output.false_premise_corrected)
        .collect::<Vec<_>>();
    if values.is_empty() {
        1.0
    } else if values.iter().all(|value| *value) {
        1.0
    } else {
        0.0
    }
}

fn efficiency_score(actual: Option<f64>, budget: Option<f64>) -> f64 {
    match (actual, budget) {
        (Some(actual), Some(budget))
            if actual.is_finite() && budget.is_finite() && budget > 0.0 =>
        {
            if actual <= budget {
                1.0
            } else {
                (budget / actual).clamp(0.0, 1.0)
            }
        }
        (Some(actual), None) if actual <= 0.0 => 1.0,
        _ => 1.0,
    }
}

fn latency_efficiency_score(actual: Option<u64>, budget: Option<u64>) -> f64 {
    match (actual, budget) {
        (Some(actual), Some(budget)) if budget > 0 => {
            if actual <= budget {
                1.0
            } else {
                (budget as f64 / actual as f64).clamp(0.0, 1.0)
            }
        }
        _ => 1.0,
    }
}

fn average_dimensions(vector: &RewardVector) -> f64 {
    [
        vector.correctness,
        vector.checkpoint_coverage,
        vector.clarification_quality,
        vector.false_premise_detection,
        vector.route_correctness,
        vector.tool_use_correctness,
        vector.cost_efficiency,
        vector.latency_efficiency,
        vector.privacy_compliance,
        vector.non_redundancy,
    ]
    .iter()
    .sum::<f64>()
        / 10.0
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
    use crate::data::{DialogueTurn, VerifierOutput};

    #[test]
    fn reward_vector_computes_all_success_dimensions() {
        let input = RewardSignalInput {
            coverage: coverage(false, &[], 1.0),
            final_answer_score: Some(final_score(true, 0.9, 1.0, false)),
            verifier_outputs: vec![verifier_output(true, false, Some(true), 0.9)],
            route_valid: true,
            tool_required: true,
            tool_used: true,
            cost_estimate: Some(0.01),
            cost_budget: Some(0.02),
            latency_ms: Some(500),
            latency_budget_ms: Some(1000),
            privacy_local_only: true,
            selected_route_is_external: false,
        };
        let artifact = compute_reward_vector(&input).unwrap();
        assert_eq!(artifact.reward_vector.correctness, 0.9);
        assert_eq!(artifact.reward_vector.checkpoint_coverage, 1.0);
        assert_eq!(artifact.reward_vector.false_premise_detection, 1.0);
        assert_eq!(artifact.reward_vector.route_correctness, 1.0);
        assert_eq!(artifact.reward_vector.tool_use_correctness, 1.0);
        assert_eq!(artifact.reward_vector.cost_efficiency, 1.0);
        assert_eq!(artifact.reward_vector.latency_efficiency, 1.0);
        assert_eq!(artifact.reward_vector.privacy_compliance, 1.0);
        assert_eq!(artifact.reward_vector.non_redundancy, 1.0);
        assert!(artifact.final_reward > 0.95);
    }

    #[test]
    fn reward_vector_penalizes_route_tool_privacy_cost_latency_and_redundancy() {
        let input = RewardSignalInput {
            coverage: coverage(true, &["tu-weather"], 0.5),
            final_answer_score: Some(final_score(false, 0.3, 0.5, true)),
            verifier_outputs: vec![verifier_output(false, true, Some(false), 0.3)],
            route_valid: false,
            tool_required: true,
            tool_used: false,
            cost_estimate: Some(0.04),
            cost_budget: Some(0.02),
            latency_ms: Some(2000),
            latency_budget_ms: Some(1000),
            privacy_local_only: true,
            selected_route_is_external: true,
        };
        let artifact = compute_reward_vector(&input).unwrap();
        assert_eq!(artifact.reward_vector.route_correctness, 0.0);
        assert_eq!(artifact.reward_vector.tool_use_correctness, 0.0);
        assert_eq!(artifact.reward_vector.privacy_compliance, 0.0);
        assert_eq!(artifact.reward_vector.non_redundancy, 0.0);
        assert_eq!(artifact.reward_vector.false_premise_detection, 0.0);
        assert_eq!(artifact.reward_vector.cost_efficiency, 0.5);
        assert_eq!(artifact.reward_vector.latency_efficiency, 0.5);
        assert!(artifact.final_reward < 0.35);
    }

    #[test]
    fn reward_vector_json_artifact_is_written_for_rollout_consumers() {
        let dir =
            std::env::temp_dir().join(format!("fractal-rlvr-reward-json-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let path = dir.join("reward_vector.json");
        let artifact = compute_reward_vector(&RewardSignalInput {
            coverage: coverage(false, &[], 1.0),
            final_answer_score: Some(final_score(true, 1.0, 1.0, false)),
            verifier_outputs: Vec::new(),
            route_valid: true,
            tool_required: false,
            tool_used: false,
            cost_estimate: Some(0.0),
            cost_budget: Some(0.0),
            latency_ms: None,
            latency_budget_ms: None,
            privacy_local_only: false,
            selected_route_is_external: false,
        })
        .unwrap();
        write_reward_vector_json(&artifact, &path).unwrap();
        let raw = fs::read_to_string(&path).unwrap();
        let parsed: RewardVectorArtifact = serde_json::from_str(&raw).unwrap();
        assert_eq!(parsed, artifact);
        assert!(raw.contains("reward_vector"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn anti_reward_hacking_detects_question_spam_expensive_use_and_inflated_self_verifier() {
        let mut input = anti_hacking_input();
        input.trace.turns[1].cost_estimate = Some(0.20);
        input.reward_artifact.final_reward = 0.95;
        input.reward_artifact.reward_vector.checkpoint_coverage = 1.0;
        input.verifier_model_id = input.actor_model_id.clone();
        input.baseline_final_reward = Some(0.50);

        let report = detect_anti_reward_hacking(&input).unwrap();

        assert!(report.asking_every_possible_question);
        assert!(report.overusing_expensive_models);
        assert!(report.self_verifier_reward_inflation);
        assert!(report.suspicious_reward_gain);
        assert!(report.suspicious);
        assert!(report
            .reasons
            .contains(&"eval report should flag suspicious reward gain".to_string()));
    }

    #[test]
    fn anti_reward_hacking_detects_no_final_answer_and_pretend_resolution() {
        let mut input = anti_hacking_input();
        input.trace.turns.pop();
        input
            .trace
            .verifier_outputs
            .retain(|output| !output.is_final_answer);
        input.coverage.missed_checkpoints = vec!["c2".into()];
        input.reward_artifact.reward_vector.checkpoint_coverage = 1.0;

        let report = detect_anti_reward_hacking(&input).unwrap();

        assert!(report.never_giving_final_answer);
        assert!(report.pretending_checkpoints_resolved);
        assert!(report.suspicious);
    }

    #[test]
    fn anti_reward_hacking_detects_verbose_uncertainty_hiding() {
        let mut input = anti_hacking_input();
        input.trace.turns.last_mut().unwrap().content = "Maybe this could possibly be right, but I am not sure; still, here is a long confident-looking answer with many extra filler words that tries to hide uncertainty while collecting a high reward from the verifier. It repeats context, repeats caveats, repeats assumptions, and avoids making the uncertainty obvious.".into();
        input.reward_artifact.final_reward = 0.9;

        let report = detect_anti_reward_hacking(&input).unwrap();

        assert!(report.verbose_uncertainty_hiding);
        assert!(report.suspicious);
    }

    #[test]
    fn anti_reward_hacking_allows_clean_high_reward_report() {
        let mut input = anti_hacking_input();
        input.coverage.targeted_checkpoints = vec!["c1".into()];
        input.reward_artifact.final_reward = 0.92;
        input.baseline_final_reward = Some(0.82);

        let report = detect_anti_reward_hacking(&input).unwrap();

        assert!(!report.suspicious);
        assert!(!report.suspicious_reward_gain);
        assert!(report.reasons.is_empty());
    }

    #[test]
    fn mvp_reward_policy_v01_matches_prd_weights() {
        let policy = MvpRewardPolicyV01::default();
        assert_eq!(policy.policy_id, MVP_REWARD_POLICY_V01_ID);
        assert_eq!(policy.final_answer_correct_after_required_checkpoints, 1.00);
        assert_eq!(policy.correct_route, 0.60);
        assert_eq!(policy.targeted_clarification, 0.40);
        assert_eq!(policy.corrected_false_premise, 0.25);
        assert_eq!(policy.cheap_or_local_model_when_sufficient, 0.20);
        assert_eq!(policy.redundant_question_penalty, -0.25);
        assert_eq!(policy.missing_required_tool_penalty, -0.40);
        assert_eq!(policy.private_data_external_route_penalty, -0.60);
        assert_eq!(policy.premature_answer_penalty, -0.75);
        assert_eq!(policy.wrong_final_answer_penalty, -1.00);
        policy.validate().unwrap();
    }

    #[test]
    fn mvp_reward_policy_applies_all_positive_and_negative_terms() {
        let input = MvpRewardPolicyInput {
            final_answer_correct_after_required_checkpoints: true,
            correct_route: true,
            targeted_clarification: true,
            corrected_false_premise: true,
            cheap_or_local_model_when_sufficient: true,
            redundant_question: true,
            missing_required_tool: true,
            private_data_external_route: true,
            premature_answer: true,
            wrong_final_answer: true,
        };
        let report = score_mvp_reward_v01(&input).unwrap();
        assert_eq!(report.positive_reward, 2.45);
        assert_eq!(report.penalty, -3.0);
        assert!((report.final_reward - -0.55).abs() < 1e-12);
        for term in [
            "final_answer_correct_after_required_checkpoints",
            "correct_route",
            "targeted_clarification",
            "corrected_false_premise",
            "cheap_or_local_model_when_sufficient",
            "redundant_question_penalty",
            "missing_required_tool_penalty",
            "private_data_external_route_penalty",
            "premature_answer_penalty",
            "wrong_final_answer_penalty",
        ] {
            assert!(
                report.applied_terms.contains(&term.to_string()),
                "missing {term}"
            );
        }
    }

    #[test]
    fn mvp_reward_input_can_be_derived_from_reward_signal() {
        let signal = RewardSignalInput {
            coverage: coverage(false, &[], 1.0),
            final_answer_score: Some(final_score(true, 0.95, 1.0, false)),
            verifier_outputs: vec![
                StrictVerifierOutput {
                    is_final_answer: false,
                    is_clarification_question: true,
                    is_tool_call: false,
                    is_route_decision: false,
                    targeted_checkpoints: vec!["c1".into()],
                    resolved_checkpoints: vec!["c1".into()],
                    missed_checkpoints: Vec::new(),
                    redundant_question: false,
                    premature_answer: false,
                    false_premise_corrected: Some(true),
                    route_valid: true,
                    reward: 0.8,
                },
                verifier_output(true, false, None, 0.95),
            ],
            route_valid: true,
            tool_required: false,
            tool_used: false,
            cost_estimate: Some(0.0),
            cost_budget: Some(0.02),
            latency_ms: Some(20),
            latency_budget_ms: Some(1000),
            privacy_local_only: true,
            selected_route_is_external: false,
        };
        let input = MvpRewardPolicyInput::from_reward_signal(&signal);
        assert!(input.final_answer_correct_after_required_checkpoints);
        assert!(input.correct_route);
        assert!(input.targeted_clarification);
        assert!(input.corrected_false_premise);
        assert!(input.cheap_or_local_model_when_sufficient);
        assert!(!input.redundant_question);
        assert!(!input.missing_required_tool);
        assert!(!input.private_data_external_route);
        assert!(!input.premature_answer);
        assert!(!input.wrong_final_answer);

        let report = score_mvp_reward_v01(&input).unwrap();
        assert_eq!(report.final_reward, 2.45);
    }

    fn anti_hacking_input() -> AntiRewardHackingInput {
        AntiRewardHackingInput {
            trace: anti_hacking_trace(),
            coverage: CheckpointCoverageReport {
                targeted_checkpoints: vec!["c1".into(), "c2".into()],
                resolved_checkpoints: vec!["c1".into(), "c2".into()],
                missed_checkpoints: Vec::new(),
                unknown_checkpoints: Vec::new(),
                redundant_question: false,
                total_checkpoints: 2,
                resolved_count: 2,
                coverage_score: 1.0,
            },
            reward_artifact: RewardVectorArtifact {
                reward_vector: RewardVector {
                    correctness: 1.0,
                    checkpoint_coverage: 1.0,
                    clarification_quality: 1.0,
                    false_premise_detection: 1.0,
                    route_correctness: 1.0,
                    tool_use_correctness: 1.0,
                    cost_efficiency: 1.0,
                    latency_efficiency: 1.0,
                    privacy_compliance: 1.0,
                    non_redundancy: 1.0,
                },
                final_reward: 0.9,
            },
            actor_model_id: "actor-small".into(),
            verifier_model_id: "judge-local".into(),
            cost_budget: Some(0.05),
            baseline_final_reward: Some(0.8),
        }
    }

    fn anti_hacking_trace() -> DialogueTrace {
        DialogueTrace {
            trace_id: "anti-hacking-trace".into(),
            task_id: "task-anti-hacking".into(),
            turns: vec![
                DialogueTurn {
                    role: "user".into(),
                    content: "Help size the device.".into(),
                    model_id: None,
                    route_decision: None,
                    latency_ms: None,
                    cost_estimate: None,
                },
                DialogueTurn {
                    role: "assistant".into(),
                    content: "What voltage and amperage should I use?".into(),
                    model_id: Some("actor-small".into()),
                    route_decision: Some("tiny-local-model".into()),
                    latency_ms: Some(1),
                    cost_estimate: Some(0.01),
                },
                DialogueTurn {
                    role: "simulated_user".into(),
                    content: "The voltage is 120 V. The current draw is 10 A.".into(),
                    model_id: None,
                    route_decision: None,
                    latency_ms: Some(1),
                    cost_estimate: Some(0.0),
                },
                DialogueTurn {
                    role: "assistant".into(),
                    content: "Use a 120 V, 10 A plan.".into(),
                    model_id: Some("actor-small".into()),
                    route_decision: Some("tiny-local-model".into()),
                    latency_ms: Some(1),
                    cost_estimate: Some(0.01),
                },
            ],
            verifier_outputs: vec![
                VerifierOutput {
                    is_final_answer: false,
                    is_clarification_question: true,
                    targeted_checkpoints: vec!["c1".into(), "c2".into()],
                    missed_checkpoints: Vec::new(),
                    redundant_question: false,
                    premature_answer: false,
                    false_premise_corrected: None,
                    route_valid: true,
                    reward: 0.8,
                },
                VerifierOutput {
                    is_final_answer: true,
                    is_clarification_question: false,
                    targeted_checkpoints: Vec::new(),
                    missed_checkpoints: Vec::new(),
                    redundant_question: false,
                    premature_answer: false,
                    false_premise_corrected: None,
                    route_valid: true,
                    reward: 0.9,
                },
            ],
            reward_vector: RewardVector {
                correctness: 1.0,
                checkpoint_coverage: 1.0,
                clarification_quality: 1.0,
                false_premise_detection: 1.0,
                route_correctness: 1.0,
                tool_use_correctness: 1.0,
                cost_efficiency: 1.0,
                latency_efficiency: 1.0,
                privacy_compliance: 1.0,
                non_redundancy: 1.0,
            },
            final_reward: 0.9,
        }
    }

    fn coverage(
        redundant_question: bool,
        missed_checkpoints: &[&str],
        coverage_score: f64,
    ) -> CheckpointCoverageReport {
        let total_checkpoints = 2;
        let resolved_count = (coverage_score * total_checkpoints as f64).round() as usize;
        CheckpointCoverageReport {
            targeted_checkpoints: vec!["c1".into()],
            resolved_checkpoints: vec!["c1".into()],
            missed_checkpoints: missed_checkpoints.iter().map(|id| id.to_string()).collect(),
            unknown_checkpoints: Vec::new(),
            redundant_question,
            total_checkpoints,
            resolved_count,
            coverage_score,
        }
    }

    fn final_score(
        passed: bool,
        answer_correctness: f64,
        rubric_completion: f64,
        tool_failure: bool,
    ) -> FinalAnswerScoreReport {
        FinalAnswerScoreReport {
            passed,
            final_score: answer_correctness,
            answer_correctness,
            rubric_completion,
            reasoning_failure: false,
            insufficient_information_failure: false,
            route_failure: false,
            tool_failure,
            explanation: "test final score".into(),
            coverage: coverage(false, &[], rubric_completion),
        }
    }

    fn verifier_output(
        route_valid: bool,
        redundant_question: bool,
        false_premise_corrected: Option<bool>,
        reward: f64,
    ) -> StrictVerifierOutput {
        StrictVerifierOutput {
            is_final_answer: true,
            is_clarification_question: false,
            is_tool_call: false,
            is_route_decision: false,
            targeted_checkpoints: vec!["c1".into()],
            resolved_checkpoints: vec!["c1".into()],
            missed_checkpoints: Vec::new(),
            redundant_question,
            premature_answer: false,
            false_premise_corrected,
            route_valid,
            reward,
        }
    }
}
