//! RLVR-039: MVP success metrics.
//!
//! Declares pass/fail of a trained adapter against the PRD's headline MVP
//! targets, comparing a baseline (base model) [`EvalMetricsReport`] against a
//! candidate (adapter) one:
//!
//! | Target | Metric | Bar |
//! |--------|--------|-----|
//! | Route correctness | `correct_route_rate` | improve by ≥ 15 pp |
//! | Clarification checkpoint | `checkpoint_coverage` | improve by ≥ 20 pp |
//! | False-premise correction | false-premise correction rate | improve by ≥ 20 pp |
//! | Redundant questions | `redundant_question_rate` | ≤ 15 % |
//! | Private-data leakage | `private_data_leakage_rate` | exactly 0 |
//! | Expensive-model escalation | `unnecessary_escalation_rate` | strictly decrease |
//!
//! The false-premise correction rate is not carried by [`EvalMetricsReport`]; it
//! is derived from dialogue-trace verifier outputs via
//! [`false_premise_correction_rate`] and supplied alongside the two reports.

use serde::{Deserialize, Serialize};

use crate::{DialogueTrace, EvalMetricsReport, RlvrError, VerifierOutput};

/// PRD headline MVP targets. Defaults match the MVP success bar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MvpSuccessTargets {
    /// Minimum `correct_route_rate` improvement (15 pp = 0.15).
    pub min_route_correctness_improvement: f64,
    /// Minimum `checkpoint_coverage` improvement (20 pp = 0.20).
    pub min_clarification_checkpoint_improvement: f64,
    /// Minimum false-premise correction-rate improvement (20 pp = 0.20).
    pub min_false_premise_correction_improvement: f64,
    /// Maximum tolerated `redundant_question_rate` (15 % = 0.15).
    pub max_redundant_question_rate: f64,
    /// Required `private_data_leakage_rate` (must equal zero).
    pub max_private_data_leakage_rate: f64,
    /// Require `unnecessary_escalation_rate` to strictly decrease vs baseline.
    pub require_escalation_decrease: bool,
}

impl Default for MvpSuccessTargets {
    fn default() -> Self {
        Self {
            min_route_correctness_improvement: 0.15,
            min_clarification_checkpoint_improvement: 0.20,
            min_false_premise_correction_improvement: 0.20,
            max_redundant_question_rate: 0.15,
            max_private_data_leakage_rate: 0.0,
            require_escalation_decrease: true,
        }
    }
}

impl MvpSuccessTargets {
    pub fn validate(&self) -> Result<(), RlvrError> {
        for (name, value) in [
            (
                "mvp.min_route_correctness_improvement",
                self.min_route_correctness_improvement,
            ),
            (
                "mvp.min_clarification_checkpoint_improvement",
                self.min_clarification_checkpoint_improvement,
            ),
            (
                "mvp.min_false_premise_correction_improvement",
                self.min_false_premise_correction_improvement,
            ),
            (
                "mvp.max_redundant_question_rate",
                self.max_redundant_question_rate,
            ),
            (
                "mvp.max_private_data_leakage_rate",
                self.max_private_data_leakage_rate,
            ),
        ] {
            require_rate(name, value)?;
        }
        Ok(())
    }
}

/// Baseline + candidate false-premise correction rates (derived from traces).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MvpFalsePremiseRates {
    pub baseline: f64,
    pub candidate: f64,
}

impl MvpFalsePremiseRates {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_rate("mvp.false_premise.baseline", self.baseline)?;
        require_rate("mvp.false_premise.candidate", self.candidate)?;
        Ok(())
    }
}

/// One MVP target verdict.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MvpTargetCheck {
    pub name: String,
    pub target: String,
    pub baseline_value: f64,
    pub candidate_value: f64,
    pub passed: bool,
    pub detail: String,
}

/// Overall MVP success verdict against every target.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MvpSuccessReport {
    pub targets: MvpSuccessTargets,
    pub checks: Vec<MvpTargetCheck>,
    pub passed_count: usize,
    pub overall_passed: bool,
    pub summary: String,
}

/// Evaluate the candidate adapter against the MVP success targets.
///
/// `baseline` is the base-model report, `candidate` is the adapter report, and
/// `false_premise` supplies the false-premise correction rates for both
/// (computed via [`false_premise_correction_rate`]).
pub fn evaluate_mvp_success(
    baseline: &EvalMetricsReport,
    candidate: &EvalMetricsReport,
    false_premise: &MvpFalsePremiseRates,
    targets: &MvpSuccessTargets,
) -> Result<MvpSuccessReport, RlvrError> {
    targets.validate()?;
    baseline.validate()?;
    candidate.validate()?;
    false_premise.validate()?;

    let checks = vec![
        min_improvement_check(
            "route_correctness_improvement",
            "≥ 15 pp route correctness improvement",
            baseline.correct_route_rate,
            candidate.correct_route_rate,
            targets.min_route_correctness_improvement,
        ),
        min_improvement_check(
            "clarification_checkpoint_improvement",
            "≥ 20 pp clarification checkpoint improvement",
            baseline.checkpoint_coverage,
            candidate.checkpoint_coverage,
            targets.min_clarification_checkpoint_improvement,
        ),
        min_improvement_check(
            "false_premise_correction_improvement",
            "≥ 20 pp false-premise correction improvement",
            false_premise.baseline,
            false_premise.candidate,
            targets.min_false_premise_correction_improvement,
        ),
        max_value_check(
            "redundant_question_rate_under_limit",
            "redundant question rate ≤ 15 %",
            candidate.redundant_question_rate,
            targets.max_redundant_question_rate,
        ),
        max_value_check(
            "private_data_leakage_zero",
            "private-data leakage rate = 0",
            candidate.private_data_leakage_rate,
            targets.max_private_data_leakage_rate,
        ),
        escalation_decrease_check(
            "expensive_model_escalation_decrease",
            "expensive-model escalation must decrease vs baseline",
            baseline.unnecessary_escalation_rate,
            candidate.unnecessary_escalation_rate,
            targets.require_escalation_decrease,
        ),
    ];

    let passed_count = checks.iter().filter(|check| check.passed).count();
    let overall_passed = checks.iter().all(|check| check.passed);
    let missed: Vec<&str> = checks
        .iter()
        .filter(|check| !check.passed)
        .map(|check| check.name.as_str())
        .collect();
    let summary = if overall_passed {
        format!("MVP success: PASS ({}/{})", passed_count, checks.len())
    } else {
        format!(
            "MVP success: FAIL ({}/{}) — missed: {}",
            passed_count,
            checks.len(),
            missed.join(", ")
        )
    };

    Ok(MvpSuccessReport {
        targets: targets.clone(),
        checks,
        passed_count,
        overall_passed,
        summary,
    })
}

/// Fraction of dialogue traces whose verifier outputs corrected a false premise.
///
/// Among verifier outputs that report `false_premise_corrected = Some(_)`, this
/// is the share that are `Some(true)`; if no output reports one, the rate is
/// `1.0` (nothing to correct — matches the reward engine's convention).
pub fn false_premise_correction_rate(traces: &[DialogueTrace]) -> f64 {
    let outputs: Vec<&VerifierOutput> = traces
        .iter()
        .flat_map(|trace| trace.verifier_outputs.iter())
        .collect();
    let reported: Vec<&VerifierOutput> = outputs
        .iter()
        .copied()
        .filter(|output| output.false_premise_corrected.is_some())
        .collect();
    if reported.is_empty() {
        return 1.0;
    }
    let corrected = reported
        .iter()
        .filter(|output| output.false_premise_corrected == Some(true))
        .count();
    corrected as f64 / reported.len() as f64
}

fn min_improvement_check(
    name: &str,
    target: &str,
    baseline: f64,
    candidate: f64,
    required_delta: f64,
) -> MvpTargetCheck {
    let delta = candidate - baseline;
    let passed = delta >= required_delta;
    MvpTargetCheck {
        name: name.into(),
        target: target.into(),
        baseline_value: baseline,
        candidate_value: candidate,
        passed,
        detail: format!(
            "improvement {delta:.3} (baseline {baseline:.3} -> candidate {candidate:.3}); required ≥ {required_delta:.3}"
        ),
    }
}

fn max_value_check(name: &str, target: &str, value: f64, limit: f64) -> MvpTargetCheck {
    let passed = value <= limit + f64::EPSILON;
    MvpTargetCheck {
        name: name.into(),
        target: target.into(),
        baseline_value: value,
        candidate_value: value,
        passed,
        detail: format!("value {value:.3}; required ≤ {limit:.3}"),
    }
}

fn escalation_decrease_check(
    name: &str,
    target: &str,
    baseline: f64,
    candidate: f64,
    required: bool,
) -> MvpTargetCheck {
    // Pass if escalation strictly decreased, or baseline was already zero and
    // the candidate stayed at zero (no regression).
    let passed = !required
        || candidate < baseline - f64::EPSILON
        || (baseline.abs() < f64::EPSILON && candidate.abs() < f64::EPSILON);
    MvpTargetCheck {
        name: name.into(),
        target: target.into(),
        baseline_value: baseline,
        candidate_value: candidate,
        passed,
        detail: format!("escalation {baseline:.3} -> {candidate:.3}; required strict decrease"),
    }
}

fn require_rate(name: &str, value: f64) -> Result<(), RlvrError> {
    if !value.is_finite() {
        return Err(RlvrError::Config(format!("{name} must be finite")));
    }
    if !(0.0..=1.0).contains(&value) {
        return Err(RlvrError::Config(format!("{name} must be in [0, 1]")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EvalTraceMetrics, RewardVector};

    fn metrics(
        correct_route_rate: f64,
        checkpoint_coverage: f64,
        redundant_question_rate: f64,
        unnecessary_escalation_rate: f64,
        private_data_leakage_rate: f64,
    ) -> EvalMetricsReport {
        EvalMetricsReport {
            schema_version: "eval-metrics-v1".into(),
            trace_count: 10,
            final_answer_accuracy: 0.8,
            checkpoint_coverage,
            redundant_question_rate,
            premature_answer_rate: 0.1,
            correct_route_rate,
            unnecessary_escalation_rate,
            private_data_leakage_rate,
            average_cost: 0.01,
            average_latency_ms: 500.0,
            traces: Vec::new(),
        }
    }

    fn passing_candidate() -> (EvalMetricsReport, EvalMetricsReport, MvpFalsePremiseRates) {
        let baseline = metrics(0.50, 0.40, 0.20, 0.30, 0.0);
        let candidate = metrics(0.70, 0.65, 0.10, 0.10, 0.0); // +20pp route, +25pp coverage, escalation down
        let false_premise = MvpFalsePremiseRates {
            baseline: 0.40,
            candidate: 0.65, // +25pp
        };
        (baseline, candidate, false_premise)
    }

    #[test]
    fn all_targets_pass_when_adapter_meets_every_bar() {
        let (baseline, candidate, fp) = passing_candidate();
        let report =
            evaluate_mvp_success(&baseline, &candidate, &fp, &MvpSuccessTargets::default())
                .unwrap();
        assert!(report.overall_passed);
        assert_eq!(report.passed_count, report.checks.len());
        assert!(report.summary.starts_with("MVP success: PASS"));
    }

    #[test]
    fn route_correctness_target_requires_15_percentage_points() {
        let (baseline, candidate, fp) = passing_candidate();
        // Only +14 pp route correctness -> fails that target.
        let mut weak = candidate.clone();
        weak.correct_route_rate = baseline.correct_route_rate + 0.14;
        let report =
            evaluate_mvp_success(&baseline, &weak, &fp, &MvpSuccessTargets::default()).unwrap();
        let route = report
            .checks
            .iter()
            .find(|c| c.name == "route_correctness_improvement")
            .unwrap();
        assert!(!route.passed);
        assert!(!report.overall_passed);
    }

    #[test]
    fn clarification_checkpoint_target_requires_20_percentage_points() {
        let (baseline, candidate, fp) = passing_candidate();
        let mut weak = candidate.clone();
        weak.checkpoint_coverage = baseline.checkpoint_coverage + 0.199;
        let report =
            evaluate_mvp_success(&baseline, &weak, &fp, &MvpSuccessTargets::default()).unwrap();
        let check = report
            .checks
            .iter()
            .find(|c| c.name == "clarification_checkpoint_improvement")
            .unwrap();
        assert!(!check.passed);
    }

    #[test]
    fn false_premise_target_requires_20_percentage_points() {
        let (baseline, candidate, _) = passing_candidate();
        let weak_fp = MvpFalsePremiseRates {
            baseline: 0.40,
            candidate: 0.599, // +19.9 pp
        };
        let report = evaluate_mvp_success(
            &baseline,
            &candidate,
            &weak_fp,
            &MvpSuccessTargets::default(),
        )
        .unwrap();
        let check = report
            .checks
            .iter()
            .find(|c| c.name == "false_premise_correction_improvement")
            .unwrap();
        assert!(!check.passed);
    }

    #[test]
    fn redundant_question_rate_must_be_at_or_below_15_percent() {
        let (baseline, candidate, fp) = passing_candidate();
        let mut weak = candidate.clone();
        weak.redundant_question_rate = 0.151; // just over 15%
        let report =
            evaluate_mvp_success(&baseline, &weak, &fp, &MvpSuccessTargets::default()).unwrap();
        let check = report
            .checks
            .iter()
            .find(|c| c.name == "redundant_question_rate_under_limit")
            .unwrap();
        assert!(!check.passed);
    }

    #[test]
    fn private_data_leakage_must_be_exactly_zero() {
        let (baseline, candidate, fp) = passing_candidate();
        let mut leaky = candidate.clone();
        leaky.private_data_leakage_rate = 0.01;
        let report =
            evaluate_mvp_success(&baseline, &leaky, &fp, &MvpSuccessTargets::default()).unwrap();
        let check = report
            .checks
            .iter()
            .find(|c| c.name == "private_data_leakage_zero")
            .unwrap();
        assert!(!check.passed);
    }

    #[test]
    fn expensive_escalation_must_decrease_and_zero_baseline_is_ok_if_held() {
        let (baseline, candidate, fp) = passing_candidate();

        // No decrease -> fail.
        let mut flat = candidate.clone();
        flat.unnecessary_escalation_rate = baseline.unnecessary_escalation_rate;
        let report =
            evaluate_mvp_success(&baseline, &flat, &fp, &MvpSuccessTargets::default()).unwrap();
        let check = report
            .checks
            .iter()
            .find(|c| c.name == "expensive_model_escalation_decrease")
            .unwrap();
        assert!(!check.passed);

        // Baseline already zero, candidate held at zero -> pass (no regression).
        let mut zero_baseline = baseline.clone();
        zero_baseline.unnecessary_escalation_rate = 0.0;
        let mut zero_candidate = candidate.clone();
        zero_candidate.unnecessary_escalation_rate = 0.0;
        let report = evaluate_mvp_success(
            &zero_baseline,
            &zero_candidate,
            &fp,
            &MvpSuccessTargets::default(),
        )
        .unwrap();
        let check = report
            .checks
            .iter()
            .find(|c| c.name == "expensive_model_escalation_decrease")
            .unwrap();
        assert!(check.passed);
    }

    #[test]
    fn summary_lists_missed_targets_on_failure() {
        let (baseline, candidate, fp) = passing_candidate();
        let mut weak = candidate.clone();
        weak.correct_route_rate = baseline.correct_route_rate; // miss route
        weak.private_data_leakage_rate = 0.5; // miss privacy
        let report =
            evaluate_mvp_success(&baseline, &weak, &fp, &MvpSuccessTargets::default()).unwrap();
        assert!(report.summary.starts_with("MVP success: FAIL"));
        assert!(report.summary.contains("route_correctness_improvement"));
        assert!(report.summary.contains("private_data_leakage_zero"));
    }

    #[test]
    fn default_targets_match_prd_bars() {
        let t = MvpSuccessTargets::default();
        assert!((t.min_route_correctness_improvement - 0.15).abs() < f64::EPSILON);
        assert!((t.min_clarification_checkpoint_improvement - 0.20).abs() < f64::EPSILON);
        assert!((t.min_false_premise_correction_improvement - 0.20).abs() < f64::EPSILON);
        assert!((t.max_redundant_question_rate - 0.15).abs() < f64::EPSILON);
        assert_eq!(t.max_private_data_leakage_rate, 0.0);
        assert!(t.require_escalation_decrease);
    }

    #[test]
    fn false_premise_correction_rate_reads_verifier_outputs() {
        // 2 traces: one corrected a false premise, one did not -> 0.5 rate.
        let traces = vec![
            trace_with_false_premise(Some(true)),
            trace_with_false_premise(Some(false)),
            trace_with_false_premise(None), // ignored (not a false-premise trace)
        ];
        assert!((false_premise_correction_rate(&traces) - 0.5).abs() < f64::EPSILON);

        // No false-premise outputs -> 1.0 (vacuous).
        let none = vec![trace_with_false_premise(None)];
        assert!((false_premise_correction_rate(&none) - 1.0).abs() < f64::EPSILON);
    }

    fn trace_with_false_premise(corrected: Option<bool>) -> DialogueTrace {
        DialogueTrace {
            trace_id: format!("trace-{:?}", corrected),
            task_id: "task-fp".into(),
            turns: vec![crate::DialogueTurn {
                role: "assistant".into(),
                content: "response".into(),
                model_id: None,
                route_decision: None,
                latency_ms: None,
                cost_estimate: None,
            }],
            verifier_outputs: vec![VerifierOutput {
                is_final_answer: true,
                is_clarification_question: false,
                targeted_checkpoints: Vec::new(),
                missed_checkpoints: Vec::new(),
                redundant_question: false,
                premature_answer: false,
                false_premise_corrected: corrected,
                route_valid: true,
                reward: 0.5,
            }],
            reward_vector: RewardVector {
                correctness: 0.5,
                checkpoint_coverage: 0.5,
                clarification_quality: 0.5,
                false_premise_detection: 0.5,
                route_correctness: 0.5,
                tool_use_correctness: 0.5,
                cost_efficiency: 0.5,
                latency_efficiency: 0.5,
                privacy_compliance: 1.0,
                non_redundancy: 1.0,
            },
            final_reward: 0.5,
        }
    }

    #[test]
    fn mvp_report_serializes_to_pass_fail_json_without_raw_prompt_data() {
        let (baseline, candidate, fp) = passing_candidate();
        let report =
            evaluate_mvp_success(&baseline, &candidate, &fp, &MvpSuccessTargets::default())
                .unwrap();
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"overall_passed\":true"));
        assert!(json.contains("MVP success: PASS"));
        // The report is a verdict over metrics, not raw trace data.
        let _ = EvalTraceMetrics {
            trace_id: "x".into(),
            task_id: "x".into(),
            final_answer_accuracy: 0.0,
            checkpoint_coverage: 0.0,
            redundant_question: false,
            premature_answer: false,
            correct_route: false,
            unnecessary_escalation: false,
            private_data_leakage: false,
            total_cost: 0.0,
            total_latency_ms: 0,
        };
    }
}
