//! RLVR-016: checkpoint coverage scorer.
//!
//! The scorer lives in `crates/rlvr/src/verifier/mod.rs`
//! (`score_checkpoint_coverage`, `score_checkpoint_coverage_for_item`,
//! `CheckpointCoverageReport`). These integration tests prove it satisfies the
//! RLVR-016 contract: it emits `targeted_checkpoints`, `resolved_checkpoints`,
//! `missed_checkpoints`, `redundant_question`, and a deterministic
//! `coverage_score` for a fixed verifier output.

use fractal_rlvr::{
    score_checkpoint_coverage, score_checkpoint_coverage_for_item, Checkpoint, CheckpointType,
    Difficulty, PrivacyPolicy, RoutePolicy, StrictVerifierOutput, TrainingItem, TrainingMode,
};

fn ids(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

/// Build a clarification-question verifier output targeting/resolving/missing
/// the given checkpoint ids. `is_final_answer` is false and
/// `is_clarification_question` is true, which passes `StrictVerifierOutput::validate`.
fn verifier_output(
    targeted: &[&str],
    resolved: &[&str],
    missed: &[&str],
    redundant_question: bool,
) -> StrictVerifierOutput {
    StrictVerifierOutput {
        is_final_answer: false,
        is_clarification_question: true,
        is_tool_call: false,
        is_route_decision: false,
        targeted_checkpoints: ids(targeted),
        resolved_checkpoints: ids(resolved),
        missed_checkpoints: ids(missed),
        redundant_question,
        premature_answer: false,
        false_premise_corrected: None,
        route_valid: true,
        reward: 0.0,
    }
}

fn checkpoint(checkpoint_id: &str) -> Checkpoint {
    Checkpoint {
        checkpoint_id: checkpoint_id.into(),
        checkpoint_type: CheckpointType::MissingInfo,
        description: format!("{checkpoint_id} description"),
        must_resolve_before_answer: true,
        answer_if_asked: format!("{checkpoint_id} answer"),
        failure_penalty: 0.5,
    }
}

fn sample_item() -> TrainingItem {
    TrainingItem {
        task_id: "task-coverage".into(),
        mode: TrainingMode::RouteCorrectness,
        visible_user_query: "What capacitor do I need?".into(),
        hidden_original_query: "What capacitor for an Xbox board?".into(),
        gold_answer: "Ask for value, voltage, package.".into(),
        domain: "electronics".into(),
        difficulty: Difficulty::Medium,
        checkpoints: vec![checkpoint("c1"), checkpoint("c2"), checkpoint("c3")],
        route_policy: RoutePolicy::default(),
        privacy_policy: PrivacyPolicy::default(),
    }
}

#[test]
fn score_is_deterministic_for_a_fixed_verifier_output() {
    let checkpoint_ids = ids(&["c1", "c2", "c3"]);
    let outputs = vec![verifier_output(&["c1"], &["c1", "c2"], &["c3"], false)];

    let first = score_checkpoint_coverage(&checkpoint_ids, &outputs).unwrap();
    let second = score_checkpoint_coverage(&checkpoint_ids, &outputs).unwrap();

    // Deterministic: same inputs -> identical report (incl. ordering from BTreeSet).
    assert_eq!(first, second);
    // Coverage is 2 resolved of 3 total.
    assert_eq!(first.total_checkpoints, 3);
    assert_eq!(first.resolved_count, 2);
    assert!((first.coverage_score - 2.0 / 3.0).abs() < f64::EPSILON);
    first.validate().unwrap();
}

#[test]
fn score_reports_targeted_resolved_missed_and_redundant() {
    let checkpoint_ids = ids(&["c1", "c2", "c3"]);
    let outputs = vec![verifier_output(&["c1"], &["c1", "c2"], &["c3"], true)];

    let report = score_checkpoint_coverage(&checkpoint_ids, &outputs).unwrap();

    assert_eq!(report.targeted_checkpoints, ids(&["c1"]));
    assert_eq!(report.resolved_checkpoints, ids(&["c1", "c2"]));
    // c3 is unresolved -> missed; explicit missed c3 folds into the same set.
    assert_eq!(report.missed_checkpoints, ids(&["c3"]));
    assert!(report.redundant_question);
    assert!(report.unknown_checkpoints.is_empty());
    report.validate().unwrap();
}

#[test]
fn score_aggregates_targeted_resolved_and_redundant_across_outputs() {
    let checkpoint_ids = ids(&["c1", "c2", "c3", "c4"]);
    let outputs = vec![
        verifier_output(&["c1"], &["c1"], &[], false),
        verifier_output(&["c2", "c3"], &["c2", "c3", "c4"], &[], true),
    ];

    let report = score_checkpoint_coverage(&checkpoint_ids, &outputs).unwrap();

    assert_eq!(report.targeted_checkpoints, ids(&["c1", "c2", "c3"]));
    assert_eq!(report.resolved_checkpoints, ids(&["c1", "c2", "c3", "c4"]));
    assert!(report.missed_checkpoints.is_empty());
    assert!(report.redundant_question); // OR across outputs
    assert!((report.coverage_score - 1.0).abs() < f64::EPSILON);
}

#[test]
fn score_flags_unknown_checkpoint_ids_as_redundant_signal() {
    let checkpoint_ids = ids(&["c1", "c2"]);
    // "c9" is not part of the rubric.
    let outputs = vec![verifier_output(&["c1", "c9"], &["c1"], &[], false)];

    let report = score_checkpoint_coverage(&checkpoint_ids, &outputs).unwrap();

    assert_eq!(report.unknown_checkpoints, ids(&["c9"]));
    // Referencing a non-existent checkpoint signals a hallucinated/redundant question.
    assert!(report.redundant_question);
    // Unknown ids never inflate the denominator.
    assert_eq!(report.total_checkpoints, 2);
    assert_eq!(report.resolved_count, 1);
}

#[test]
fn score_bounds_are_zero_when_unresolved_and_one_when_all_resolved() {
    let checkpoint_ids = ids(&["c1", "c2"]);

    let none_resolved = score_checkpoint_coverage(
        &checkpoint_ids,
        &[verifier_output(&["c1"], &[], &[], false)],
    )
    .unwrap();
    assert_eq!(none_resolved.resolved_count, 0);
    assert!(none_resolved.coverage_score.abs() < f64::EPSILON);
    assert_eq!(none_resolved.missed_checkpoints, ids(&["c1", "c2"]));

    let all_resolved = score_checkpoint_coverage(
        &checkpoint_ids,
        &[verifier_output(&[], &["c1", "c2"], &[], false)],
    )
    .unwrap();
    assert_eq!(all_resolved.resolved_count, 2);
    assert!((all_resolved.coverage_score - 1.0).abs() < f64::EPSILON);
    assert!(all_resolved.missed_checkpoints.is_empty());
}

#[test]
fn score_rejects_empty_and_duplicate_checkpoint_ids() {
    let empty_err =
        score_checkpoint_coverage(&[], &[verifier_output(&[], &[], &[], false)]).unwrap_err();
    assert!(empty_err.to_string().contains("at least one checkpoint id"));

    let dup_err = score_checkpoint_coverage(
        &ids(&["c1", "c1"]),
        &[verifier_output(&[], &[], &[], false)],
    )
    .unwrap_err();
    assert!(dup_err.to_string().contains("unique"));
}

#[test]
fn score_for_training_item_uses_the_items_checkpoints() {
    let item = sample_item();
    let outputs = vec![verifier_output(&["c1", "c2"], &["c1"], &["c3"], false)];

    let report = score_checkpoint_coverage_for_item(&item, &outputs).unwrap();

    assert_eq!(report.total_checkpoints, 3);
    assert_eq!(report.targeted_checkpoints, ids(&["c1", "c2"]));
    assert_eq!(report.resolved_checkpoints, ids(&["c1"]));
    // c2 and c3 remain unresolved.
    assert_eq!(report.missed_checkpoints, ids(&["c2", "c3"]));
    assert!((report.coverage_score - 1.0 / 3.0).abs() < f64::EPSILON);
    report.validate().unwrap();
}

#[test]
fn score_is_invariant_to_verifier_output_order_for_aggregation() {
    let checkpoint_ids = ids(&["c1", "c2"]);
    let outputs_a = vec![
        verifier_output(&["c1"], &["c1"], &[], false),
        verifier_output(&["c2"], &["c2"], &[], false),
    ];
    let outputs_b: Vec<StrictVerifierOutput> = outputs_a.iter().cloned().rev().collect();

    let report_a = score_checkpoint_coverage(&checkpoint_ids, &outputs_a).unwrap();
    let report_b = score_checkpoint_coverage(&checkpoint_ids, &outputs_b).unwrap();

    assert_eq!(report_a, report_b);
    assert!((report_a.coverage_score - 1.0).abs() < f64::EPSILON);
}
