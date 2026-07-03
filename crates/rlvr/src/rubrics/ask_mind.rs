//! RLVR-011: AskMind rubric generator.
//!
//! AskMind trains the model to ask targeted clarification questions when the
//! visible prompt is underspecified. The generator takes a complete underlying
//! query plus required missing facts and emits a degraded prompt with
//! `MissingInfo` checkpoints. Each checkpoint carries the simulator
//! `answer_if_asked` value that can be revealed only when the assistant asks for
//! that specific detail.

use serde::{Deserialize, Serialize};

use crate::{
    stable_hash, Checkpoint, CheckpointType, Difficulty, PrivacyPolicy, RlvrError, RoutePolicy,
    TrainingItem, TrainingMode,
};

const MISSING_INFO_FAILURE_PENALTY: f64 = 0.75;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AskMindMissingFact {
    pub fact_id: String,
    pub description: String,
    pub answer_if_asked: String,
    pub must_resolve_before_answer: bool,
}

impl AskMindMissingFact {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("askmind_missing_fact.fact_id", &self.fact_id)?;
        require_non_empty("askmind_missing_fact.description", &self.description)?;
        require_non_empty(
            "askmind_missing_fact.answer_if_asked",
            &self.answer_if_asked,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AskMindRubricInput {
    pub task_id: String,
    pub complete_query: String,
    pub gold_answer: String,
    pub domain: String,
    pub difficulty: Difficulty,
    pub missing_facts: Vec<AskMindMissingFact>,
    pub degraded_query: Option<String>,
}

impl AskMindRubricInput {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("askmind_input.task_id", &self.task_id)?;
        require_non_empty("askmind_input.complete_query", &self.complete_query)?;
        require_non_empty("askmind_input.gold_answer", &self.gold_answer)?;
        require_non_empty("askmind_input.domain", &self.domain)?;
        if let Some(degraded_query) = &self.degraded_query {
            require_non_empty("askmind_input.degraded_query", degraded_query)?;
        }
        if self.missing_facts.is_empty() {
            return Err(RlvrError::Config(
                "askmind_input.missing_facts must contain at least one fact".into(),
            ));
        }
        for fact in &self.missing_facts {
            fact.validate()?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AskMindRubric {
    pub task_id: String,
    pub complete_query: String,
    pub degraded_query: String,
    pub gold_answer: String,
    pub missing_facts: Vec<AskMindMissingFact>,
    pub checkpoints: Vec<Checkpoint>,
    pub domain: String,
    pub difficulty: Difficulty,
    pub rubric_hash: String,
}

#[derive(Serialize)]
struct AskMindHashPayload<'a> {
    task_id: &'a str,
    complete_query: &'a str,
    degraded_query: &'a str,
    gold_answer: &'a str,
    missing_facts: &'a [AskMindMissingFact],
    checkpoints: &'a [Checkpoint],
    domain: &'a str,
    difficulty: Difficulty,
}

impl AskMindRubric {
    pub fn generate(input: &AskMindRubricInput) -> Result<Self, RlvrError> {
        input.validate()?;
        let degraded_query = input
            .degraded_query
            .clone()
            .unwrap_or_else(|| default_degraded_query(&input.domain, &input.complete_query));
        let checkpoints = input
            .missing_facts
            .iter()
            .map(|fact| missing_info_checkpoint(&input.task_id, fact))
            .collect::<Vec<_>>();
        let payload = AskMindHashPayload {
            task_id: &input.task_id,
            complete_query: &input.complete_query,
            degraded_query: &degraded_query,
            gold_answer: &input.gold_answer,
            missing_facts: &input.missing_facts,
            checkpoints: &checkpoints,
            domain: &input.domain,
            difficulty: input.difficulty,
        };
        let rubric_hash = stable_hash(&payload)?;
        let rubric = Self {
            task_id: input.task_id.clone(),
            complete_query: input.complete_query.clone(),
            degraded_query,
            gold_answer: input.gold_answer.clone(),
            missing_facts: input.missing_facts.clone(),
            checkpoints,
            domain: input.domain.clone(),
            difficulty: input.difficulty,
            rubric_hash,
        };
        rubric.validate()?;
        Ok(rubric)
    }

    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("askmind.task_id", &self.task_id)?;
        require_non_empty("askmind.complete_query", &self.complete_query)?;
        require_non_empty("askmind.degraded_query", &self.degraded_query)?;
        require_non_empty("askmind.gold_answer", &self.gold_answer)?;
        require_non_empty("askmind.domain", &self.domain)?;
        require_non_empty("askmind.rubric_hash", &self.rubric_hash)?;
        if self.missing_facts.is_empty() {
            return Err(RlvrError::Config(
                "askmind.missing_facts cannot be empty".into(),
            ));
        }
        if self.checkpoints.len() != self.missing_facts.len() {
            return Err(RlvrError::Config(
                "askmind.checkpoints must match missing_facts length".into(),
            ));
        }
        for (fact, checkpoint) in self.missing_facts.iter().zip(&self.checkpoints) {
            fact.validate()?;
            checkpoint.validate()?;
            if checkpoint.checkpoint_type != CheckpointType::MissingInfo {
                return Err(RlvrError::Config(
                    "askmind checkpoints must be MissingInfo".into(),
                ));
            }
            if checkpoint.answer_if_asked != fact.answer_if_asked {
                return Err(RlvrError::Config(
                    "askmind checkpoint answer_if_asked must match missing fact".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn required_checkpoints(&self) -> impl Iterator<Item = &Checkpoint> {
        self.checkpoints
            .iter()
            .filter(|checkpoint| checkpoint.must_resolve_before_answer)
    }

    pub fn into_training_item(
        self,
        route_policy: RoutePolicy,
        privacy_policy: PrivacyPolicy,
    ) -> Result<TrainingItem, RlvrError> {
        route_policy.validate()?;
        privacy_policy.validate()?;
        let item = TrainingItem {
            task_id: self.task_id,
            mode: TrainingMode::AskMind,
            visible_user_query: self.degraded_query,
            hidden_original_query: self.complete_query,
            gold_answer: self.gold_answer,
            domain: self.domain,
            difficulty: self.difficulty,
            checkpoints: self.checkpoints,
            route_policy,
            privacy_policy,
        };
        item.validate()?;
        Ok(item)
    }
}

pub fn generate_ask_mind_rubric(input: AskMindRubricInput) -> Result<AskMindRubric, RlvrError> {
    AskMindRubric::generate(&input)
}

#[derive(Debug, Clone, PartialEq)]
pub struct AskMindFixture {
    pub complete_query: String,
    pub gold_answer: String,
    pub domain: String,
    pub difficulty: Difficulty,
    pub missing_facts: Vec<AskMindMissingFact>,
    pub degraded_query: String,
}

impl AskMindFixture {
    pub fn to_input(&self, task_id: impl Into<String>) -> AskMindRubricInput {
        AskMindRubricInput {
            task_id: task_id.into(),
            complete_query: self.complete_query.clone(),
            gold_answer: self.gold_answer.clone(),
            domain: self.domain.clone(),
            difficulty: self.difficulty,
            missing_facts: self.missing_facts.clone(),
            degraded_query: Some(self.degraded_query.clone()),
        }
    }
}

pub fn sample_ask_mind_fixtures() -> Vec<AskMindFixture> {
    let domains = [
        ("electronics", "What capacitor do I need for this board?"),
        ("home_repair", "Which fastener should I use here?"),
        ("coding", "How should I fix this error?"),
        ("travel", "What should I book for this trip?"),
        ("finance", "Which option is cheaper for me?"),
    ];
    let fact_templates = [
        (
            "context",
            "The exact device, project, or situation is needed.",
        ),
        (
            "spec",
            "The required value, size, version, or constraint is needed.",
        ),
        (
            "environment",
            "Operating conditions or location are needed.",
        ),
        ("goal", "The user's desired outcome or tradeoff is needed."),
    ];

    let mut fixtures = Vec::with_capacity(100);
    for idx in 0..100 {
        let (domain, degraded_query) = domains[idx % domains.len()];
        let difficulty = match idx % 3 {
            0 => Difficulty::Easy,
            1 => Difficulty::Medium,
            _ => Difficulty::Hard,
        };
        let complete_query = format!(
            "{degraded_query} Context: project-{idx}; required spec: spec-{idx}; environment: environment-{idx}; goal: goal-{idx}."
        );
        let gold_answer = format!(
            "Use the option that satisfies project-{idx}, spec-{idx}, environment-{idx}, and goal-{idx}."
        );
        let missing_facts = fact_templates
            .iter()
            .enumerate()
            .map(|(fact_idx, (name, description))| AskMindMissingFact {
                fact_id: format!("{name}-{idx}"),
                description: (*description).into(),
                answer_if_asked: match fact_idx {
                    0 => format!("project-{idx}"),
                    1 => format!("spec-{idx}"),
                    2 => format!("environment-{idx}"),
                    _ => format!("goal-{idx}"),
                },
                must_resolve_before_answer: true,
            })
            .collect();
        fixtures.push(AskMindFixture {
            complete_query,
            gold_answer,
            domain: domain.into(),
            difficulty,
            missing_facts,
            degraded_query: degraded_query.into(),
        });
    }
    fixtures
}

fn default_degraded_query(domain: &str, complete_query: &str) -> String {
    let domain = domain.trim();
    let first_sentence = complete_query
        .split(['.', '?', '!'])
        .next()
        .unwrap_or(complete_query)
        .trim();
    if first_sentence.is_empty() {
        format!("I need help with {domain}.")
    } else if first_sentence.len() <= 90 {
        first_sentence.to_string()
    } else {
        format!("I need help with {domain}.")
    }
}

fn missing_info_checkpoint(task_id: &str, fact: &AskMindMissingFact) -> Checkpoint {
    Checkpoint {
        checkpoint_id: format!("{task_id}-{}", fact.fact_id),
        checkpoint_type: CheckpointType::MissingInfo,
        description: fact.description.clone(),
        must_resolve_before_answer: fact.must_resolve_before_answer,
        answer_if_asked: fact.answer_if_asked.clone(),
        failure_penalty: MISSING_INFO_FAILURE_PENALTY,
    }
}

fn require_non_empty(name: &str, value: &str) -> Result<(), RlvrError> {
    if value.trim().is_empty() {
        return Err(RlvrError::Config(format!("{name} cannot be empty")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generator_degrades_prompt_and_emits_missing_info_checkpoints() {
        let fixture = &sample_ask_mind_fixtures()[0];
        let rubric = AskMindRubric::generate(&fixture.to_input("am-basic")).unwrap();

        assert_eq!(rubric.degraded_query, fixture.degraded_query);
        assert_eq!(rubric.complete_query, fixture.complete_query);
        assert_eq!(rubric.checkpoints.len(), fixture.missing_facts.len());
        assert_eq!(rubric.rubric_hash.len(), 64);
        assert!(rubric.checkpoints.iter().all(|checkpoint| {
            checkpoint.checkpoint_type == CheckpointType::MissingInfo
                && checkpoint.must_resolve_before_answer
                && (checkpoint.failure_penalty - MISSING_INFO_FAILURE_PENALTY).abs() < f64::EPSILON
        }));
        rubric.validate().unwrap();
    }

    #[test]
    fn simulator_answer_if_asked_values_are_preserved() {
        let fixture = &sample_ask_mind_fixtures()[7];
        let rubric = AskMindRubric::generate(&fixture.to_input("am-sim")).unwrap();

        for (fact, checkpoint) in fixture.missing_facts.iter().zip(rubric.checkpoints.iter()) {
            assert_eq!(checkpoint.answer_if_asked, fact.answer_if_asked);
            assert!(checkpoint.description.contains("needed"));
        }
        assert_eq!(
            rubric.required_checkpoints().count(),
            fixture.missing_facts.len()
        );
    }

    #[test]
    fn into_training_item_round_trips_to_askmind_mode() {
        let fixture = &sample_ask_mind_fixtures()[12];
        let rubric = AskMindRubric::generate(&fixture.to_input("am-training")).unwrap();
        let item = rubric
            .into_training_item(RoutePolicy::default(), PrivacyPolicy::default())
            .unwrap();

        assert_eq!(item.mode, TrainingMode::AskMind);
        assert_eq!(item.visible_user_query, fixture.degraded_query);
        assert_eq!(item.hidden_original_query, fixture.complete_query);
        assert_eq!(item.gold_answer, fixture.gold_answer);
        assert!(item
            .checkpoints
            .iter()
            .all(|checkpoint| checkpoint.checkpoint_type == CheckpointType::MissingInfo));
        item.validate().unwrap();
    }

    #[test]
    fn one_hundred_sample_qa_pairs_produce_valid_askmind_rubrics() {
        let fixtures = sample_ask_mind_fixtures();
        assert_eq!(fixtures.len(), 100);

        for (idx, fixture) in fixtures.iter().enumerate() {
            let rubric =
                generate_ask_mind_rubric(fixture.to_input(format!("am-sample-{idx}"))).unwrap();
            rubric.validate().unwrap();
            let item = rubric
                .into_training_item(RoutePolicy::default(), PrivacyPolicy::default())
                .unwrap();
            assert_eq!(item.mode, TrainingMode::AskMind);
            assert!(!item.checkpoints.is_empty());
        }
    }

    #[test]
    fn rubric_hash_is_stable_and_field_sensitive() {
        let fixture = &sample_ask_mind_fixtures()[0];
        let a = AskMindRubric::generate(&fixture.to_input("am-hash")).unwrap();
        let b = AskMindRubric::generate(&fixture.to_input("am-hash")).unwrap();
        assert_eq!(a.rubric_hash, b.rubric_hash);

        let mut changed = fixture.to_input("am-hash");
        changed.missing_facts[0].answer_if_asked = "different answer".into();
        let c = AskMindRubric::generate(&changed).unwrap();
        assert_ne!(a.rubric_hash, c.rubric_hash);
    }

    #[test]
    fn generator_rejects_empty_fields_and_missing_facts() {
        let fixture = &sample_ask_mind_fixtures()[0];
        let mut input = fixture.to_input("am-empty");
        input.complete_query = " ".into();
        assert!(AskMindRubric::generate(&input).is_err());

        let mut input = fixture.to_input("am-no-facts");
        input.missing_facts.clear();
        assert!(AskMindRubric::generate(&input).is_err());
    }
}
