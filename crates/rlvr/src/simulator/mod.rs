//! Local user simulator interfaces for multi-turn RLVR rollouts.

use serde::{Deserialize, Serialize};

use crate::{
    Checkpoint, DialogueTrace, DialogueTurn, RewardVector, RlvrError, TrainingItem, VerifierOutput,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LocalUserSimulatorInput {
    pub hidden_original_query: String,
    pub checkpoints: Vec<Checkpoint>,
    pub assistant_clarification_question: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LocalUserSimulatorReply {
    pub content: String,
    pub revealed_checkpoint_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulatedRolloutTraceInput {
    pub trace_id: String,
    pub training_item: TrainingItem,
    pub assistant_clarification_question: String,
    pub assistant_final_answer: String,
    pub actor_model_id: String,
    pub route_decision: String,
    pub simulator_mode: SimulatorMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SimulatorMode {
    Clean,
    Adversarial(AdversarialSimulatorStyle),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdversarialSimulatorStyle {
    PartialAnswer,
    WrongAnswer,
    AmbiguousAnswer,
    AnnoyedAnswer,
    ContradictoryAnswer,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LocalUserSimulator;

impl LocalUserSimulator {
    pub fn reply(input: &LocalUserSimulatorInput) -> Result<LocalUserSimulatorReply, RlvrError> {
        Self::reply_with_mode(input, SimulatorMode::Clean)
    }

    pub fn reply_with_mode(
        input: &LocalUserSimulatorInput,
        mode: SimulatorMode,
    ) -> Result<LocalUserSimulatorReply, RlvrError> {
        input.validate()?;

        let question = normalize_text(&input.assistant_clarification_question);
        let mut revealed = Vec::new();

        for checkpoint in &input.checkpoints {
            if explicitly_asks_for_checkpoint(&question, checkpoint) {
                revealed.push(checkpoint);
            }
        }

        if revealed.is_empty() {
            return Ok(LocalUserSimulatorReply {
                content: vague_reply(mode),
                revealed_checkpoint_ids: Vec::new(),
            });
        }

        let raw_content = match mode {
            SimulatorMode::Clean => clean_content(&revealed),
            SimulatorMode::Adversarial(style) => adversarial_content(style, &revealed),
        };
        let content = privacy_guard_content(input, &revealed, raw_content);

        Ok(LocalUserSimulatorReply {
            content,
            revealed_checkpoint_ids: revealed
                .into_iter()
                .map(|checkpoint| checkpoint.checkpoint_id.clone())
                .collect(),
        })
    }
}

impl LocalUserSimulatorInput {
    pub fn validate(&self) -> Result<(), RlvrError> {
        if self.hidden_original_query.trim().is_empty() {
            return Err(RlvrError::Config(
                "simulator hidden_original_query cannot be empty".into(),
            ));
        }
        if self.checkpoints.is_empty() {
            return Err(RlvrError::Config(
                "simulator checkpoints cannot be empty".into(),
            ));
        }
        if self.assistant_clarification_question.trim().is_empty() {
            return Err(RlvrError::Config(
                "simulator assistant_clarification_question cannot be empty".into(),
            ));
        }
        for checkpoint in &self.checkpoints {
            checkpoint.validate()?;
            if checkpoint.answer_if_asked.trim().is_empty() {
                return Err(RlvrError::Config(format!(
                    "checkpoint {:?} answer_if_asked cannot be empty for simulator use",
                    checkpoint.checkpoint_id
                )));
            }
        }
        Ok(())
    }
}

pub fn simulate_local_user_reply(
    input: &LocalUserSimulatorInput,
) -> Result<LocalUserSimulatorReply, RlvrError> {
    LocalUserSimulator::reply(input)
}

pub fn simulate_local_user_reply_with_mode(
    input: &LocalUserSimulatorInput,
    mode: SimulatorMode,
) -> Result<LocalUserSimulatorReply, RlvrError> {
    LocalUserSimulator::reply_with_mode(input, mode)
}

pub fn build_simulated_rollout_trace(
    input: SimulatedRolloutTraceInput,
) -> Result<DialogueTrace, RlvrError> {
    input.validate()?;
    let simulator_input = LocalUserSimulatorInput {
        hidden_original_query: input.training_item.hidden_original_query.clone(),
        checkpoints: input.training_item.checkpoints.clone(),
        assistant_clarification_question: input.assistant_clarification_question.clone(),
    };
    let simulated_reply =
        LocalUserSimulator::reply_with_mode(&simulator_input, input.simulator_mode)?;
    let required_checkpoint_ids = input
        .training_item
        .checkpoints
        .iter()
        .filter(|checkpoint| checkpoint.must_resolve_before_answer)
        .map(|checkpoint| checkpoint.checkpoint_id.clone())
        .collect::<Vec<_>>();
    let missed_checkpoints = required_checkpoint_ids
        .iter()
        .filter(|checkpoint_id| {
            !simulated_reply
                .revealed_checkpoint_ids
                .contains(checkpoint_id)
        })
        .cloned()
        .collect::<Vec<_>>();
    let coverage = if required_checkpoint_ids.is_empty() {
        1.0
    } else {
        (required_checkpoint_ids.len() - missed_checkpoints.len()) as f64
            / required_checkpoint_ids.len() as f64
    };
    let route_valid = input
        .training_item
        .route_policy
        .rules
        .iter()
        .any(|rule| rule.route == input.route_decision)
        || input.training_item.route_policy.default_route == input.route_decision;
    let final_answer_has_gold = !input.training_item.gold_answer.trim().is_empty()
        && input
            .assistant_final_answer
            .to_ascii_lowercase()
            .contains(&input.training_item.gold_answer.to_ascii_lowercase());
    let correctness = if final_answer_has_gold && missed_checkpoints.is_empty() {
        1.0
    } else if final_answer_has_gold {
        0.5
    } else {
        0.0
    };
    let clarification_reward = if simulated_reply.revealed_checkpoint_ids.is_empty() {
        0.0
    } else {
        coverage
    };
    let final_reward = average_reward(&[
        correctness,
        coverage,
        clarification_reward,
        if route_valid { 1.0 } else { 0.0 },
        1.0,
        1.0,
    ]);
    let reward_vector = RewardVector {
        correctness,
        checkpoint_coverage: coverage,
        clarification_quality: clarification_reward,
        false_premise_detection: 0.0,
        route_correctness: if route_valid { 1.0 } else { 0.0 },
        tool_use_correctness: 0.0,
        cost_efficiency: 1.0,
        latency_efficiency: 1.0,
        privacy_compliance: 1.0,
        non_redundancy: if simulated_reply.revealed_checkpoint_ids.is_empty() {
            0.0
        } else {
            1.0
        },
    };
    let trace = DialogueTrace {
        trace_id: input.trace_id,
        task_id: input.training_item.task_id.clone(),
        turns: vec![
            DialogueTurn {
                role: "user".into(),
                content: input.training_item.visible_user_query,
                model_id: None,
                route_decision: None,
                latency_ms: None,
                cost_estimate: None,
            },
            DialogueTurn {
                role: "assistant".into(),
                content: input.assistant_clarification_question,
                model_id: Some(input.actor_model_id.clone()),
                route_decision: Some(input.route_decision.clone()),
                latency_ms: Some(0),
                cost_estimate: Some(0.0),
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
                content: input.assistant_final_answer,
                model_id: Some(input.actor_model_id),
                route_decision: Some(input.route_decision),
                latency_ms: Some(0),
                cost_estimate: Some(0.0),
            },
        ],
        verifier_outputs: vec![
            VerifierOutput {
                is_final_answer: false,
                is_clarification_question: true,
                targeted_checkpoints: simulated_reply.revealed_checkpoint_ids,
                missed_checkpoints: Vec::new(),
                redundant_question: false,
                premature_answer: false,
                false_premise_corrected: None,
                route_valid,
                reward: clarification_reward,
            },
            VerifierOutput {
                is_final_answer: true,
                is_clarification_question: false,
                targeted_checkpoints: Vec::new(),
                missed_checkpoints,
                redundant_question: false,
                premature_answer: coverage < 1.0,
                false_premise_corrected: None,
                route_valid,
                reward: final_reward,
            },
        ],
        reward_vector,
        final_reward,
    };
    trace.validate()?;
    Ok(trace)
}

impl SimulatedRolloutTraceInput {
    pub fn validate(&self) -> Result<(), RlvrError> {
        if self.trace_id.trim().is_empty() {
            return Err(RlvrError::Config("rollout trace_id cannot be empty".into()));
        }
        self.training_item.validate()?;
        if self.assistant_clarification_question.trim().is_empty() {
            return Err(RlvrError::Config(
                "rollout assistant_clarification_question cannot be empty".into(),
            ));
        }
        if self.assistant_final_answer.trim().is_empty() {
            return Err(RlvrError::Config(
                "rollout assistant_final_answer cannot be empty".into(),
            ));
        }
        if self.actor_model_id.trim().is_empty() {
            return Err(RlvrError::Config(
                "rollout actor_model_id cannot be empty".into(),
            ));
        }
        if self.route_decision.trim().is_empty() {
            return Err(RlvrError::Config(
                "rollout route_decision cannot be empty".into(),
            ));
        }
        Ok(())
    }
}

fn clean_content(revealed: &[&Checkpoint]) -> String {
    revealed
        .iter()
        .map(|checkpoint| checkpoint.answer_if_asked.trim())
        .filter(|answer| !answer.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn adversarial_content(style: AdversarialSimulatorStyle, revealed: &[&Checkpoint]) -> String {
    match style {
        AdversarialSimulatorStyle::PartialAnswer => revealed
            .first()
            .map(|checkpoint| checkpoint.answer_if_asked.trim().to_string())
            .unwrap_or_else(|| "I only know part of it.".into()),
        AdversarialSimulatorStyle::WrongAnswer => revealed
            .iter()
            .map(|checkpoint| {
                format!(
                    "{}: I think it is not {}.",
                    checkpoint.checkpoint_id,
                    checkpoint.answer_if_asked.trim()
                )
            })
            .collect::<Vec<_>>()
            .join(" "),
        AdversarialSimulatorStyle::AmbiguousAnswer => revealed
            .iter()
            .map(|checkpoint| {
                format!(
                    "{} is probably around that, maybe.",
                    checkpoint.checkpoint_id
                )
            })
            .collect::<Vec<_>>()
            .join(" "),
        AdversarialSimulatorStyle::AnnoyedAnswer => {
            format!("I already said this. {}", clean_content(revealed))
        }
        AdversarialSimulatorStyle::ContradictoryAnswer => revealed
            .iter()
            .map(|checkpoint| {
                format!(
                    "{}. Actually, maybe the opposite for {}.",
                    checkpoint.answer_if_asked.trim(),
                    checkpoint.checkpoint_id
                )
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn vague_reply(mode: SimulatorMode) -> String {
    match mode {
        SimulatorMode::Clean => "I'm not sure which detail you need.".into(),
        SimulatorMode::Adversarial(AdversarialSimulatorStyle::AnnoyedAnswer) => {
            "I don't know, you need to be more specific.".into()
        }
        SimulatorMode::Adversarial(AdversarialSimulatorStyle::AmbiguousAnswer) => {
            "Maybe, it depends.".into()
        }
        SimulatorMode::Adversarial(AdversarialSimulatorStyle::WrongAnswer) => {
            "Probably the usual value.".into()
        }
        SimulatorMode::Adversarial(AdversarialSimulatorStyle::ContradictoryAnswer) => {
            "Yes and no, I guess.".into()
        }
        SimulatorMode::Adversarial(AdversarialSimulatorStyle::PartialAnswer) => {
            "I can only answer part of that.".into()
        }
    }
}

fn privacy_guard_content(
    input: &LocalUserSimulatorInput,
    revealed: &[&Checkpoint],
    content: String,
) -> String {
    let mut guarded = content;
    let revealed_ids = revealed
        .iter()
        .map(|checkpoint| checkpoint.checkpoint_id.as_str())
        .collect::<Vec<_>>();

    for checkpoint in &input.checkpoints {
        if !revealed_ids.contains(&checkpoint.checkpoint_id.as_str()) {
            guarded = redact_exact_fragment(
                &guarded,
                checkpoint.answer_if_asked.trim(),
                "[redacted unrequested checkpoint]",
            );
        }
    }

    redact_exact_fragment(
        &guarded,
        input.hidden_original_query.trim(),
        "[redacted hidden original query]",
    )
}

fn redact_exact_fragment(content: &str, fragment: &str, replacement: &str) -> String {
    if fragment.is_empty() || !content.contains(fragment) {
        return content.to_string();
    }
    content.replace(fragment, replacement)
}

fn average_reward(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn explicitly_asks_for_checkpoint(question: &str, checkpoint: &Checkpoint) -> bool {
    checkpoint_terms(checkpoint)
        .into_iter()
        .any(|term| contains_token(question, &term))
}

fn checkpoint_terms(checkpoint: &Checkpoint) -> Vec<String> {
    let mut terms = significant_terms(&checkpoint.checkpoint_id);
    terms.extend(significant_terms(&checkpoint.description));
    terms.sort();
    terms.dedup();
    terms
}

fn significant_terms(raw: &str) -> Vec<String> {
    normalize_text(raw)
        .split_whitespace()
        .filter(|term| is_significant_term(term))
        .map(ToOwned::to_owned)
        .collect()
}

fn normalize_text(raw: &str) -> String {
    raw.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
}

fn contains_token(haystack: &str, needle: &str) -> bool {
    haystack.split_whitespace().any(|token| token == needle)
}

fn is_significant_term(term: &str) -> bool {
    term.len() >= 3
        && !matches!(
            term,
            "ask"
                | "asked"
                | "answer"
                | "assistant"
                | "before"
                | "checkpoint"
                | "clarify"
                | "detail"
                | "field"
                | "final"
                | "for"
                | "from"
                | "info"
                | "information"
                | "missing"
                | "must"
                | "need"
                | "needs"
                | "only"
                | "original"
                | "question"
                | "query"
                | "required"
                | "resolve"
                | "should"
                | "the"
                | "user"
                | "value"
                | "what"
                | "which"
                | "with"
        )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CheckpointType, Difficulty, PrivacyPolicy, RoutePolicy, TrainingMode};

    #[test]
    fn asking_for_voltage_reveals_voltage_only() {
        let input = LocalUserSimulatorInput {
            hidden_original_query:
                "Size a fuse for a private lab bench device: 120 volts, 10 amps.".into(),
            checkpoints: vec![
                checkpoint("voltage", "Ask for supply voltage", "The voltage is 120 V."),
                checkpoint(
                    "amperage",
                    "Ask for current draw",
                    "The current draw is 10 A.",
                ),
            ],
            assistant_clarification_question: "What voltage is the device using?".into(),
        };

        let reply = LocalUserSimulator::reply(&input).unwrap();

        assert_eq!(reply.content, "The voltage is 120 V.");
        assert_eq!(reply.revealed_checkpoint_ids, vec!["voltage"]);
        assert!(!reply.content.contains("10 A"));
        assert!(!reply.content.contains("private lab bench"));
    }

    #[test]
    fn vague_clarification_gets_vague_reply() {
        let input = LocalUserSimulatorInput {
            hidden_original_query:
                "Size a fuse for a private lab bench device: 120 volts, 10 amps.".into(),
            checkpoints: vec![
                checkpoint("voltage", "Ask for supply voltage", "The voltage is 120 V."),
                checkpoint(
                    "amperage",
                    "Ask for current draw",
                    "The current draw is 10 A.",
                ),
            ],
            assistant_clarification_question: "Can you clarify?".into(),
        };

        let reply = simulate_local_user_reply(&input).unwrap();

        assert_eq!(reply.content, "I'm not sure which detail you need.");
        assert!(reply.revealed_checkpoint_ids.is_empty());
        assert!(!reply.content.contains("120"));
        assert!(!reply.content.contains("10"));
    }

    #[test]
    fn asking_for_multiple_fields_reveals_only_those_fields() {
        let input = LocalUserSimulatorInput {
            hidden_original_query:
                "Configure a private endpoint with region us-east-1, budget $20, and token abc123."
                    .into(),
            checkpoints: vec![
                checkpoint(
                    "region",
                    "Ask for deployment region",
                    "The region is us-east-1.",
                ),
                checkpoint("budget", "Ask for cost budget", "The budget is $20."),
                checkpoint("secret", "Ask for API token", "The API token is abc123."),
            ],
            assistant_clarification_question: "Which region and budget should I use?".into(),
        };

        let reply = LocalUserSimulator::reply(&input).unwrap();

        assert_eq!(
            reply.revealed_checkpoint_ids,
            vec!["region".to_string(), "budget".to_string()]
        );
        assert!(reply.content.contains("us-east-1"));
        assert!(reply.content.contains("$20"));
        assert!(!reply.content.contains("abc123"));
    }

    #[test]
    fn adversarial_partial_answer_reveals_only_first_requested_field() {
        let input = multi_field_input();
        let reply = simulate_local_user_reply_with_mode(
            &input,
            SimulatorMode::Adversarial(AdversarialSimulatorStyle::PartialAnswer),
        )
        .unwrap();

        assert_eq!(reply.revealed_checkpoint_ids, vec!["voltage", "amperage"]);
        assert!(reply.content.contains("120 V"));
        assert!(!reply.content.contains("10 A"));
    }

    #[test]
    fn adversarial_wrong_answer_mentions_requested_fields_without_hidden_query_leak() {
        let input = multi_field_input();
        let reply = LocalUserSimulator::reply_with_mode(
            &input,
            SimulatorMode::Adversarial(AdversarialSimulatorStyle::WrongAnswer),
        )
        .unwrap();

        assert!(reply.content.contains("not The voltage is 120 V"));
        assert!(reply.content.contains("not The current draw is 10 A"));
        assert!(!reply.content.contains("private lab bench"));
    }

    #[test]
    fn adversarial_ambiguous_annoyed_and_contradictory_modes_are_selectable() {
        let input = multi_field_input();
        let ambiguous = LocalUserSimulator::reply_with_mode(
            &input,
            SimulatorMode::Adversarial(AdversarialSimulatorStyle::AmbiguousAnswer),
        )
        .unwrap();
        let annoyed = LocalUserSimulator::reply_with_mode(
            &input,
            SimulatorMode::Adversarial(AdversarialSimulatorStyle::AnnoyedAnswer),
        )
        .unwrap();
        let contradictory = LocalUserSimulator::reply_with_mode(
            &input,
            SimulatorMode::Adversarial(AdversarialSimulatorStyle::ContradictoryAnswer),
        )
        .unwrap();

        assert!(ambiguous.content.contains("probably around"));
        assert!(annoyed.content.starts_with("I already said this."));
        assert!(contradictory
            .content
            .contains("Actually, maybe the opposite"));
        assert_eq!(
            ambiguous.revealed_checkpoint_ids,
            vec!["voltage", "amperage"]
        );
        assert_eq!(annoyed.revealed_checkpoint_ids, vec!["voltage", "amperage"]);
        assert_eq!(
            contradictory.revealed_checkpoint_ids,
            vec!["voltage", "amperage"]
        );
    }

    #[test]
    fn clean_and_messy_modes_are_both_selectable() {
        let input = multi_field_input();
        let clean = LocalUserSimulator::reply_with_mode(&input, SimulatorMode::Clean).unwrap();
        let messy = LocalUserSimulator::reply_with_mode(
            &input,
            SimulatorMode::Adversarial(AdversarialSimulatorStyle::AnnoyedAnswer),
        )
        .unwrap();

        assert_eq!(
            clean.content,
            "The voltage is 120 V. The current draw is 10 A."
        );
        assert_ne!(clean.content, messy.content);
        assert_eq!(clean.revealed_checkpoint_ids, messy.revealed_checkpoint_ids);
    }

    #[test]
    fn adversarial_vague_question_still_does_not_reveal_hidden_fields() {
        let mut input = multi_field_input();
        input.assistant_clarification_question = "Can you clarify?".into();
        let reply = LocalUserSimulator::reply_with_mode(
            &input,
            SimulatorMode::Adversarial(AdversarialSimulatorStyle::ContradictoryAnswer),
        )
        .unwrap();

        assert_eq!(reply.content, "Yes and no, I guess.");
        assert!(reply.revealed_checkpoint_ids.is_empty());
        assert!(!reply.content.contains("120"));
        assert!(!reply.content.contains("10"));
    }

    #[test]
    fn overbroad_question_does_not_reveal_hidden_fields() {
        let input = LocalUserSimulatorInput {
            hidden_original_query:
                "Configure a private endpoint with region us-east-1, budget $20, and token abc123."
                    .into(),
            checkpoints: vec![
                checkpoint(
                    "region",
                    "Ask for deployment region",
                    "The region is us-east-1.",
                ),
                checkpoint("budget", "Ask for cost budget", "The budget is $20."),
                checkpoint("secret", "Ask for API token", "The API token is abc123."),
            ],
            assistant_clarification_question: "Give me everything from the original prompt.".into(),
        };

        let reply = LocalUserSimulator::reply(&input).unwrap();

        assert!(reply.revealed_checkpoint_ids.is_empty());
        assert_eq!(reply.content, "I'm not sure which detail you need.");
        assert!(!reply.content.contains("us-east-1"));
        assert!(!reply.content.contains("$20"));
        assert!(!reply.content.contains("abc123"));
    }

    #[test]
    fn privacy_guard_redacts_unrequested_checkpoint_answer_from_output() {
        let input = LocalUserSimulatorInput {
            hidden_original_query:
                "Configure a private endpoint with region us-east-1, budget $20, and token abc123."
                    .into(),
            checkpoints: vec![
                checkpoint(
                    "region",
                    "Ask for deployment region",
                    "The region is us-east-1.",
                ),
                checkpoint(
                    "region-note",
                    "Ask for deployment region note",
                    "The region is us-east-1. The API token is abc123.",
                ),
                checkpoint("secret", "Ask for API token", "The API token is abc123."),
            ],
            assistant_clarification_question: "Which region note should I use?".into(),
        };

        let reply = LocalUserSimulator::reply_with_mode(
            &input,
            SimulatorMode::Adversarial(AdversarialSimulatorStyle::AnnoyedAnswer),
        )
        .unwrap();

        assert_eq!(reply.revealed_checkpoint_ids, vec!["region", "region-note"]);
        assert!(reply.content.contains("us-east-1"));
        assert!(reply.content.contains("[redacted unrequested checkpoint]"));
        assert!(!reply.content.contains("abc123"));
    }

    #[test]
    fn privacy_guard_never_leaks_hidden_original_query_wholesale() {
        let hidden =
            "Configure a private endpoint with region us-east-1, budget $20, and token abc123.";
        let input = LocalUserSimulatorInput {
            hidden_original_query: hidden.into(),
            checkpoints: vec![checkpoint("summary", "Ask for deployment summary", hidden)],
            assistant_clarification_question: "What deployment summary should I use?".into(),
        };

        let reply = LocalUserSimulator::reply(&input).unwrap();

        assert_eq!(reply.revealed_checkpoint_ids, vec!["summary"]);
        assert_eq!(reply.content, "[redacted hidden original query]");
        assert!(!reply.content.contains(hidden));
        assert!(!reply.content.contains("abc123"));
    }

    #[test]
    fn simulated_rollout_builds_complete_dialogue_trace() {
        let trace = build_simulated_rollout_trace(SimulatedRolloutTraceInput {
            trace_id: "rollout-1".into(),
            training_item: training_item(),
            assistant_clarification_question: "What voltage and amperage should I use?".into(),
            assistant_final_answer: "Use a 120 V, 10 A fuse plan.".into(),
            actor_model_id: "tiny-router".into(),
            route_decision: "tiny-local-model".into(),
            simulator_mode: SimulatorMode::Clean,
        })
        .unwrap();

        trace.validate().unwrap();
        assert_eq!(trace.trace_id, "rollout-1");
        assert_eq!(trace.task_id, "sim-task");
        assert_eq!(trace.turns.len(), 4);
        assert_eq!(trace.turns[0].role, "user");
        assert_eq!(trace.turns[1].role, "assistant");
        assert_eq!(trace.turns[2].role, "simulated_user");
        assert_eq!(trace.turns[3].role, "assistant");
        assert_eq!(
            trace.turns[1].route_decision.as_deref(),
            Some("tiny-local-model")
        );
        assert_eq!(
            trace.turns[3].route_decision.as_deref(),
            Some("tiny-local-model")
        );
        assert_eq!(trace.verifier_outputs.len(), 2);
        assert!(trace.verifier_outputs[0].is_clarification_question);
        assert_eq!(
            trace.verifier_outputs[0].targeted_checkpoints,
            vec!["voltage".to_string(), "amperage".to_string()]
        );
        assert!(trace.verifier_outputs[1].is_final_answer);
        assert!(trace.verifier_outputs[1].missed_checkpoints.is_empty());
        assert_eq!(trace.reward_vector.checkpoint_coverage, 1.0);
        assert_eq!(trace.reward_vector.route_correctness, 1.0);
        assert!(trace.final_reward > 0.0);
        trace.trace_hash_commitment().unwrap();
    }

    #[test]
    fn simulated_rollout_records_missed_checkpoint_and_premature_answer() {
        let trace = build_simulated_rollout_trace(SimulatedRolloutTraceInput {
            trace_id: "rollout-missed".into(),
            training_item: training_item(),
            assistant_clarification_question: "What voltage should I use?".into(),
            assistant_final_answer: "Use a 120 V fuse plan.".into(),
            actor_model_id: "tiny-router".into(),
            route_decision: "tiny-local-model".into(),
            simulator_mode: SimulatorMode::Clean,
        })
        .unwrap();

        assert_eq!(
            trace.verifier_outputs[0].targeted_checkpoints,
            vec!["voltage".to_string()]
        );
        assert_eq!(
            trace.verifier_outputs[1].missed_checkpoints,
            vec!["amperage".to_string()]
        );
        assert!(trace.verifier_outputs[1].premature_answer);
        assert!(trace.reward_vector.checkpoint_coverage < 1.0);
        assert!(trace.final_reward < 1.0);
    }

    fn multi_field_input() -> LocalUserSimulatorInput {
        LocalUserSimulatorInput {
            hidden_original_query:
                "Size a fuse for a private lab bench device: 120 volts, 10 amps.".into(),
            checkpoints: vec![
                checkpoint("voltage", "Ask for supply voltage", "The voltage is 120 V."),
                checkpoint(
                    "amperage",
                    "Ask for current draw",
                    "The current draw is 10 A.",
                ),
                checkpoint("device", "Ask for device type", "It is a lab bench device."),
            ],
            assistant_clarification_question: "What voltage and amperage should I use?".into(),
        }
    }

    fn training_item() -> TrainingItem {
        TrainingItem {
            task_id: "sim-task".into(),
            mode: TrainingMode::AskMind,
            visible_user_query: "Help me size a fuse.".into(),
            hidden_original_query:
                "Size a fuse for a private lab bench device: 120 volts, 10 amps.".into(),
            gold_answer: "120 V, 10 A".into(),
            domain: "electronics".into(),
            difficulty: Difficulty::Easy,
            checkpoints: vec![
                checkpoint("voltage", "Ask for supply voltage", "The voltage is 120 V."),
                checkpoint(
                    "amperage",
                    "Ask for current draw",
                    "The current draw is 10 A.",
                ),
            ],
            route_policy: RoutePolicy::default(),
            privacy_policy: PrivacyPolicy::default(),
        }
    }

    fn checkpoint(id: &str, description: &str, answer_if_asked: &str) -> Checkpoint {
        Checkpoint {
            checkpoint_id: id.into(),
            checkpoint_type: CheckpointType::MissingInfo,
            description: description.into(),
            must_resolve_before_answer: true,
            answer_if_asked: answer_if_asked.into(),
            failure_penalty: 1.0,
        }
    }
}
