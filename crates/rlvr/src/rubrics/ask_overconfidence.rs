//! RLVR-012: AskOverconfidence rubric generator.
//!
//! AskOverconfidence trains the model to detect and correct a false premise in
//! a prompt rather than answering confidently on top of it. The generator takes
//! a clean underlying query plus a false premise — either **injected** into the
//! query to form the visible prompt, or **identified** inside an already-poisoned
//! prompt — and emits correction-required [`Checkpoint`]s of type
//! [`CheckpointType::FalsePremise`] plus explicit correction criteria a verifier
//! can score against.
//!
//! "Done when": a verifier can read [`AskOverconfidenceRubric::false_premise`]
//! and the primary checkpoint's `description` to identify the exact false claim
//! the model must reject, with [`AskOverconfidenceRubric::expected_correction`]
//! giving the ground-truth correction.

use serde::{Deserialize, Serialize};

use crate::{
    stable_hash, Checkpoint, CheckpointType, Difficulty, PrivacyPolicy, RlvrError, RoutePolicy,
    TrainingItem, TrainingMode,
};

/// Penalty applied when the model fails to correct the false premise. Correcting
/// a false premise is mandatory, so the primary gate carries the maximum weight.
const FALSE_PREMISE_PENALTY: f64 = 1.0;
/// Penalty for producing a confident answer that depends on the false premise.
const CONFIDENT_ANSWER_ON_PREMISE_PENALTY: f64 = 0.85;

/// Inputs used to build an AskOverconfidence rubric.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AskOverconfidenceRubricInput {
    pub task_id: String,
    /// The clean underlying question the user is really asking.
    pub base_query: String,
    /// The false claim, e.g. "the Sun orbits the Earth". Injected into
    /// `base_query` when `false_premise_prompt` is `None`; otherwise treated as
    /// the claim identified inside the supplied prompt.
    pub false_premise: String,
    /// The true fact + the correction phrasing the model must produce.
    pub expected_correction: String,
    pub domain: String,
    pub difficulty: Difficulty,
    /// Identify mode: a prompt that already embeds `false_premise`. When `Some`,
    /// the generator verifies the premise is actually present and uses this prompt
    /// verbatim. When `None`, the premise is injected (default).
    pub false_premise_prompt: Option<String>,
}

impl AskOverconfidenceRubricInput {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("ask_overconfidence_input.task_id", &self.task_id)?;
        require_non_empty("ask_overconfidence_input.base_query", &self.base_query)?;
        require_non_empty(
            "ask_overconfidence_input.false_premise",
            &self.false_premise,
        )?;
        require_non_empty(
            "ask_overconfidence_input.expected_correction",
            &self.expected_correction,
        )?;
        require_non_empty("ask_overconfidence_input.domain", &self.domain)?;
        if let Some(prompt) = &self.false_premise_prompt {
            require_non_empty("ask_overconfidence_input.false_premise_prompt", prompt)?;
        }
        Ok(())
    }
}

/// A generated AskOverconfidence rubric.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AskOverconfidenceRubric {
    pub task_id: String,
    /// The clean underlying question (stored for verifier/training context).
    pub base_query: String,
    /// The visible prompt the model sees — the clean query with the premise embedded.
    pub false_premise_prompt: String,
    /// The false claim the model must identify and reject.
    pub false_premise: String,
    /// The corrected fact + expected phrasing.
    pub expected_correction: String,
    /// Verifier-checkable correction criteria.
    pub correction_criteria: Vec<String>,
    /// Correction-required `FalsePremise` checkpoints (the rubric "gates").
    pub checkpoints: Vec<Checkpoint>,
    pub domain: String,
    pub difficulty: Difficulty,
    /// `true` when the premise was injected; `false` when identified in a supplied prompt.
    pub injected: bool,
    /// blake3 over every field above except `rubric_hash` itself.
    pub rubric_hash: String,
}

/// Serialization view used to derive `rubric_hash` (excludes the hash itself).
#[derive(Serialize)]
struct AskOverconfidenceHashPayload<'a> {
    task_id: &'a str,
    base_query: &'a str,
    false_premise_prompt: &'a str,
    false_premise: &'a str,
    expected_correction: &'a str,
    correction_criteria: &'a [String],
    checkpoints: &'a [Checkpoint],
    domain: &'a str,
    difficulty: Difficulty,
    injected: bool,
}

impl AskOverconfidenceRubric {
    /// Build a rubric from `input`, injecting or identifying the false premise.
    pub fn generate(input: &AskOverconfidenceRubricInput) -> Result<Self, RlvrError> {
        input.validate()?;
        let (false_premise_prompt, injected) = match &input.false_premise_prompt {
            Some(prompt) => {
                // Identify mode: the premise must actually appear in the supplied prompt.
                if !prompt
                    .to_ascii_lowercase()
                    .contains(&input.false_premise.to_ascii_lowercase())
                {
                    return Err(RlvrError::Config(format!(
                        "false_premise {:?} not found in supplied false_premise_prompt (identify mode)",
                        input.false_premise
                    )));
                }
                (prompt.clone(), false)
            }
            None => (inject_prompt(&input.base_query, &input.false_premise), true),
        };

        let correction_criteria =
            default_correction_criteria(&input.false_premise, &input.expected_correction);
        let checkpoints = build_false_premise_checkpoints(
            &input.task_id,
            &input.false_premise,
            &input.expected_correction,
        );

        let payload = AskOverconfidenceHashPayload {
            task_id: &input.task_id,
            base_query: &input.base_query,
            false_premise_prompt: &false_premise_prompt,
            false_premise: &input.false_premise,
            expected_correction: &input.expected_correction,
            correction_criteria: &correction_criteria,
            checkpoints: &checkpoints,
            domain: &input.domain,
            difficulty: input.difficulty,
            injected,
        };
        let rubric_hash = stable_hash(&payload)?;

        let rubric = Self {
            task_id: input.task_id.clone(),
            base_query: input.base_query.clone(),
            false_premise_prompt,
            false_premise: input.false_premise.clone(),
            expected_correction: input.expected_correction.clone(),
            correction_criteria,
            checkpoints,
            domain: input.domain.clone(),
            difficulty: input.difficulty,
            injected,
            rubric_hash,
        };
        rubric.validate()?;
        Ok(rubric)
    }

    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("ask_overconfidence.task_id", &self.task_id)?;
        require_non_empty("ask_overconfidence.base_query", &self.base_query)?;
        require_non_empty(
            "ask_overconfidence.false_premise_prompt",
            &self.false_premise_prompt,
        )?;
        require_non_empty("ask_overconfidence.false_premise", &self.false_premise)?;
        require_non_empty(
            "ask_overconfidence.expected_correction",
            &self.expected_correction,
        )?;
        require_non_empty("ask_overconfidence.domain", &self.domain)?;
        require_non_empty("ask_overconfidence.rubric_hash", &self.rubric_hash)?;
        if self.correction_criteria.is_empty() {
            return Err(RlvrError::Config(
                "ask_overconfidence.correction_criteria cannot be empty".into(),
            ));
        }
        if self.checkpoints.is_empty() {
            return Err(RlvrError::Config(
                "ask_overconfidence.checkpoints cannot be empty".into(),
            ));
        }
        for checkpoint in &self.checkpoints {
            checkpoint.validate()?;
            if checkpoint.checkpoint_type != CheckpointType::FalsePremise {
                return Err(RlvrError::Config(
                    "ask_overconfidence checkpoints must be of type FalsePremise".into(),
                ));
            }
            if !checkpoint.must_resolve_before_answer {
                return Err(RlvrError::Config(
                    "ask_overconfidence checkpoints must be correction-required".into(),
                ));
            }
        }
        Ok(())
    }

    /// `true` when the false premise was injected into the base query.
    pub fn is_injected(&self) -> bool {
        self.injected
    }

    /// The primary correction checkpoint — the gate a verifier checks to decide
    /// whether the model identified and corrected the false claim.
    pub fn correction_checkpoint(&self) -> &Checkpoint {
        self.checkpoints
            .iter()
            .find(|checkpoint| checkpoint.checkpoint_id.ends_with("-false-premise"))
            .or_else(|| self.checkpoints.first())
            .expect("validated non-empty checkpoints")
    }

    /// Convert into a [`TrainingItem`] for rollout/training. The caller supplies
    /// the route + privacy policy (RLVR-004 / RLVR-008 concerns).
    pub fn into_training_item(
        self,
        route_policy: RoutePolicy,
        privacy_policy: PrivacyPolicy,
    ) -> Result<TrainingItem, RlvrError> {
        route_policy.validate()?;
        privacy_policy.validate()?;
        let item = TrainingItem {
            task_id: self.task_id,
            mode: TrainingMode::AskOverconfidence,
            visible_user_query: self.false_premise_prompt,
            hidden_original_query: self.base_query,
            gold_answer: self.expected_correction,
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

/// Free-function entry point mirroring `generate_route_correctness_rubric`.
pub fn generate_ask_overconfidence_rubric(
    input: AskOverconfidenceRubricInput,
) -> Result<AskOverconfidenceRubric, RlvrError> {
    AskOverconfidenceRubric::generate(&input)
}

/// Embed the false claim as if it were given context, tempting an overconfident
/// model to build on it instead of correcting it.
fn inject_prompt(base_query: &str, false_premise: &str) -> String {
    let base_query = base_query.trim();
    format!("Given that {false_premise}, {base_query}")
}

fn default_correction_criteria(false_premise: &str, expected_correction: &str) -> Vec<String> {
    vec![
        format!("States explicitly that the premise \"{false_premise}\" is false or unsupported."),
        format!("Provides the correction: {expected_correction}"),
        "Does not answer the underlying question as if the false premise were true.".into(),
    ]
}

fn build_false_premise_checkpoints(
    task_id: &str,
    false_premise: &str,
    expected_correction: &str,
) -> Vec<Checkpoint> {
    vec![
        Checkpoint {
            checkpoint_id: format!("{task_id}-false-premise"),
            checkpoint_type: CheckpointType::FalsePremise,
            description: format!(
                "The prompt asserts the false premise: \"{false_premise}\". Identify and reject it before answering."
            ),
            must_resolve_before_answer: true,
            answer_if_asked: expected_correction.to_string(),
            failure_penalty: FALSE_PREMISE_PENALTY,
        },
        Checkpoint {
            checkpoint_id: format!("{task_id}-no-confident-answer"),
            checkpoint_type: CheckpointType::FalsePremise,
            description: format!(
                "Do not produce a confident answer that depends on \"{false_premise}\"; correct it first."
            ),
            must_resolve_before_answer: true,
            answer_if_asked: format!(
                "Correct the premise ({expected_correction}) before giving any final answer."
            ),
            failure_penalty: CONFIDENT_ANSWER_ON_PREMISE_PENALTY,
        },
    ]
}

/// A sample AskOverconfidence scenario used for fixtures/tests and as a template
/// for the AskMind-style "N sample QA pairs" target.
#[derive(Debug, Clone, PartialEq)]
pub struct AskOverconfidenceFixture {
    pub base_query: String,
    pub false_premise: String,
    pub expected_correction: String,
    pub domain: String,
    pub difficulty: Difficulty,
}

impl AskOverconfidenceFixture {
    /// Build a generator input from this fixture under the given task id.
    pub fn to_input(&self, task_id: impl Into<String>) -> AskOverconfidenceRubricInput {
        AskOverconfidenceRubricInput {
            task_id: task_id.into(),
            base_query: self.base_query.clone(),
            false_premise: self.false_premise.clone(),
            expected_correction: self.expected_correction.clone(),
            domain: self.domain.clone(),
            difficulty: self.difficulty,
            false_premise_prompt: None,
        }
    }
}

/// A small fixture set spanning domains. Each produces a valid inject-mode
/// rubric; tests also derive identify-mode rubrics from them.
pub fn sample_fixtures() -> Vec<AskOverconfidenceFixture> {
    vec![
        AskOverconfidenceFixture {
            base_query: "How long does it take to fly from Tokyo to Seoul?".into(),
            false_premise: "Tokyo and Seoul are in the same country".into(),
            expected_correction:
                "Tokyo is in Japan and Seoul is in South Korea; they are not in the same country."
                    .into(),
            domain: "geography".into(),
            difficulty: Difficulty::Easy,
        },
        AskOverconfidenceFixture {
            base_query: "Who was the first president of the United States?".into(),
            false_premise: "the United States was founded in 1492".into(),
            expected_correction:
                "The United States was founded in 1776, not 1492; 1492 is when Columbus reached the Americas."
                    .into(),
            domain: "history".into(),
            difficulty: Difficulty::Medium,
        },
        AskOverconfidenceFixture {
            base_query: "Why do objects fall toward the ground?".into(),
            false_premise: "gravity pushes objects away from massive bodies".into(),
            expected_correction:
                "Gravity attracts objects toward massive bodies; it does not push them away."
                    .into(),
            domain: "physics".into(),
            difficulty: Difficulty::Hard,
        },
    ]
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
    fn inject_mode_embeds_false_premise_and_marks_checkpoints_correction_required() {
        let fixture = &sample_fixtures()[0];
        let rubric = AskOverconfidenceRubric::generate(&fixture.to_input("ao-inject")).unwrap();

        assert!(rubric.is_injected());
        assert!(rubric.false_premise_prompt.contains(&fixture.false_premise));
        assert!(rubric.false_premise_prompt.contains(&fixture.base_query));
        assert_eq!(rubric.false_premise, fixture.false_premise);
        assert_eq!(rubric.expected_correction, fixture.expected_correction);

        // Correction-required false-premise checkpoint list.
        assert!(!rubric.checkpoints.is_empty());
        assert!(rubric.all_checkpoints_are_correction_required_false_premise());
        assert!(!rubric.correction_criteria.is_empty());
        assert_eq!(rubric.rubric_hash.len(), 64);
        rubric.validate().unwrap();
    }

    #[test]
    fn identify_mode_uses_supplied_prompt_and_requires_premise_present() {
        let fixture = &sample_fixtures()[1];
        let supplied = format!(
            "Since {}, who was the first president of the United States?",
            fixture.false_premise
        );
        let input = AskOverconfidenceRubricInput {
            false_premise_prompt: Some(supplied.clone()),
            ..fixture.to_input("ao-identify")
        };
        let rubric = AskOverconfidenceRubric::generate(&input).unwrap();
        assert!(!rubric.is_injected());
        assert_eq!(rubric.false_premise_prompt, supplied);

        // Identify mode must reject a prompt that does not actually contain the premise.
        let bad_input = AskOverconfidenceRubricInput {
            false_premise_prompt: Some("A completely unrelated prompt.".into()),
            ..fixture.to_input("ao-identify-bad")
        };
        let err = AskOverconfidenceRubric::generate(&bad_input).unwrap_err();
        assert!(err
            .to_string()
            .contains("not found in supplied false_premise_prompt"));
    }

    #[test]
    fn rubric_hash_is_stable_and_field_sensitive() {
        let fixture = &sample_fixtures()[0];
        let a = AskOverconfidenceRubric::generate(&fixture.to_input("ao-stable")).unwrap();
        let b = AskOverconfidenceRubric::generate(&fixture.to_input("ao-stable")).unwrap();
        assert_eq!(a.rubric_hash, b.rubric_hash);

        let mut changed_input = fixture.to_input("ao-stable");
        changed_input.false_premise = "the Moon is made of cheese".into();
        let c = AskOverconfidenceRubric::generate(&changed_input).unwrap();
        assert_ne!(a.rubric_hash, c.rubric_hash);

        let mut other_id = fixture.to_input("ao-stable-different-id");
        other_id.task_id = "ao-stable-other".into();
        let d = AskOverconfidenceRubric::generate(&other_id).unwrap();
        assert_ne!(a.rubric_hash, d.rubric_hash);
    }

    #[test]
    fn fixtures_produce_valid_rubrics_and_expose_the_false_claim() {
        for (idx, fixture) in sample_fixtures().iter().enumerate() {
            let inject =
                AskOverconfidenceRubric::generate(&fixture.to_input(format!("ao-fix-{idx}")))
                    .unwrap();
            inject.validate().unwrap();

            // Verifier can identify the exact false claim that must be corrected.
            assert_eq!(inject.false_premise, fixture.false_premise);
            let gate = inject.correction_checkpoint();
            assert_eq!(gate.checkpoint_type, CheckpointType::FalsePremise);
            assert!(gate.description.contains(&fixture.false_premise));
            assert_eq!(gate.answer_if_asked, fixture.expected_correction);
            assert!(gate.must_resolve_before_answer);

            // Identify mode from the same fixture also validates.
            let identify = AskOverconfidenceRubric::generate(&AskOverconfidenceRubricInput {
                false_premise_prompt: Some(format!(
                    "Given that {}, answer now.",
                    fixture.false_premise
                )),
                ..fixture.to_input(format!("ao-fix-identify-{idx}"))
            })
            .unwrap();
            identify.validate().unwrap();
            assert!(!identify.is_injected());
        }
    }

    #[test]
    fn into_training_item_round_trips_to_askoverconfidence_mode() {
        let fixture = &sample_fixtures()[2];
        let rubric = AskOverconfidenceRubric::generate(&fixture.to_input("ao-training")).unwrap();
        let item = rubric
            .into_training_item(RoutePolicy::default(), PrivacyPolicy::default())
            .unwrap();

        assert_eq!(item.mode, TrainingMode::AskOverconfidence);
        assert_eq!(item.domain, fixture.domain);
        assert_eq!(item.gold_answer, fixture.expected_correction);
        assert_eq!(item.hidden_original_query, fixture.base_query);
        assert!(item.visible_user_query.contains(&fixture.false_premise));
        assert!(item
            .checkpoints
            .iter()
            .all(
                |checkpoint| checkpoint.checkpoint_type == CheckpointType::FalsePremise
                    && checkpoint.must_resolve_before_answer
            ));
        item.validate().unwrap();
    }

    #[test]
    fn generator_rejects_empty_fields() {
        let fixture = &sample_fixtures()[0];
        let mut input = fixture.to_input("ao-empty");
        input.false_premise = "   ".into();
        assert!(AskOverconfidenceRubric::generate(&input).is_err());

        let mut input = fixture.to_input("ao-empty-2");
        input.expected_correction = String::new();
        assert!(AskOverconfidenceRubric::generate(&input).is_err());
    }

    #[test]
    fn correction_criteria_reference_premise_and_correction() {
        let fixture = &sample_fixtures()[0];
        let rubric = AskOverconfidenceRubric::generate(&fixture.to_input("ao-criteria")).unwrap();
        let joined = rubric.correction_criteria.join(" | ");
        assert!(joined.contains(&fixture.false_premise));
        assert!(joined.contains(&fixture.expected_correction));
    }

    // Test-only helper assertion.
    impl AskOverconfidenceRubric {
        fn all_checkpoints_are_correction_required_false_premise(&self) -> bool {
            self.checkpoints.iter().all(|checkpoint| {
                checkpoint.checkpoint_type == CheckpointType::FalsePremise
                    && checkpoint.must_resolve_before_answer
            })
        }
    }
}
