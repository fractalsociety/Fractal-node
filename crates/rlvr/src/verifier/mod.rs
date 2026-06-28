//! Strict JSON verifier interfaces and parser support.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use std::{collections::BTreeSet, iter::FromIterator};

use serde::{Deserialize, Serialize};

use crate::{hash_bytes, RlvrError, TrainingItem, VerifierOutput};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StrictVerifierOutput {
    pub is_final_answer: bool,
    pub is_clarification_question: bool,
    pub is_tool_call: bool,
    pub is_route_decision: bool,
    pub targeted_checkpoints: Vec<String>,
    pub resolved_checkpoints: Vec<String>,
    pub missed_checkpoints: Vec<String>,
    pub redundant_question: bool,
    pub premature_answer: bool,
    pub false_premise_corrected: Option<bool>,
    pub route_valid: bool,
    pub reward: f64,
}

impl StrictVerifierOutput {
    pub fn validate(&self) -> Result<(), RlvrError> {
        if self.is_final_answer && self.is_clarification_question {
            return Err(RlvrError::Config(
                "verifier output cannot be both final answer and clarification question".into(),
            ));
        }
        if !self.reward.is_finite() {
            return Err(RlvrError::Config(
                "verifier output reward must be finite".into(),
            ));
        }
        for checkpoint in self
            .targeted_checkpoints
            .iter()
            .chain(self.resolved_checkpoints.iter())
            .chain(self.missed_checkpoints.iter())
        {
            if checkpoint.trim().is_empty() {
                return Err(RlvrError::Config(
                    "verifier checkpoint ids cannot be empty".into(),
                ));
            }
        }
        Ok(())
    }

    pub fn training_output(&self) -> Result<VerifierOutput, RlvrError> {
        self.validate()?;
        Ok(VerifierOutput {
            is_final_answer: self.is_final_answer,
            is_clarification_question: self.is_clarification_question,
            targeted_checkpoints: self.targeted_checkpoints.clone(),
            missed_checkpoints: self.missed_checkpoints.clone(),
            redundant_question: self.redundant_question,
            premature_answer: self.premature_answer,
            false_premise_corrected: self.false_premise_corrected,
            route_valid: self.route_valid,
            reward: self.reward,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifierParseFailure {
    pub attempt: usize,
    pub raw_output_hash: String,
    pub error: String,
    pub used_for_training: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParsedVerifierOutput {
    pub strict_output: StrictVerifierOutput,
    pub training_output: VerifierOutput,
    pub retry_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerifierParseReport {
    pub parsed: Option<ParsedVerifierOutput>,
    pub failures: Vec<VerifierParseFailure>,
    pub excluded_from_training: bool,
}

impl VerifierParseReport {
    pub fn training_output(&self) -> Option<&VerifierOutput> {
        self.parsed
            .as_ref()
            .map(|parsed| &parsed.training_output)
            .filter(|_| !self.excluded_from_training)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnparseableVerifierLogRow {
    pub timestamp_unix: u64,
    pub verifier_id: String,
    pub task_id: String,
    pub trace_id: String,
    pub attempt: usize,
    pub raw_output_hash: String,
    pub error: String,
    pub used_for_training: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifierQaRecordInput {
    pub task_id: String,
    pub trace_hash: String,
    pub policy_hash: String,
    pub model_id: String,
    pub verifier_id: String,
    pub checkpoint_id: String,
    pub question: String,
    pub answer: String,
    pub evidence: String,
    pub raw_prompt: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifierQaRecord {
    pub record_id: String,
    pub timestamp_unix: u64,
    pub task_id: String,
    pub trace_hash: String,
    pub policy_hash: String,
    pub model_id: String,
    pub verifier_id: String,
    pub checkpoint_id: String,
    pub question: String,
    pub answer: String,
    pub evidence_hash: String,
    pub raw_prompt_hash: Option<String>,
    pub local_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifierQaExportRecord {
    pub record_id: String,
    pub timestamp_unix: u64,
    pub task_id: String,
    pub trace_hash: String,
    pub policy_hash: String,
    pub model_id: String,
    pub verifier_id: String,
    pub checkpoint_id: String,
    pub question: String,
    pub answer: String,
    pub evidence_hash: String,
    pub raw_prompt_hash: Option<String>,
    pub raw_prompt: Option<String>,
    pub local_only: bool,
}

impl VerifierQaRecordInput {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("verifier_qa.task_id", &self.task_id)?;
        require_hash("verifier_qa.trace_hash", &self.trace_hash)?;
        require_hash("verifier_qa.policy_hash", &self.policy_hash)?;
        require_non_empty("verifier_qa.model_id", &self.model_id)?;
        require_non_empty("verifier_qa.verifier_id", &self.verifier_id)?;
        require_non_empty("verifier_qa.checkpoint_id", &self.checkpoint_id)?;
        require_non_empty("verifier_qa.question", &self.question)?;
        require_non_empty("verifier_qa.answer", &self.answer)?;
        require_non_empty("verifier_qa.evidence", &self.evidence)
    }

    pub fn into_record(
        self,
        timestamp_unix: u64,
        local_only: bool,
    ) -> Result<VerifierQaRecord, RlvrError> {
        self.validate()?;
        let raw_prompt_hash = self
            .raw_prompt
            .as_ref()
            .map(|raw_prompt| hash_bytes(raw_prompt.as_bytes()));
        let evidence_hash = hash_bytes(self.evidence.as_bytes());
        let record_id = hash_bytes(
            format!(
                "{}:{}:{}:{}:{}:{}:{}",
                self.task_id,
                self.trace_hash,
                self.policy_hash,
                self.model_id,
                self.verifier_id,
                self.checkpoint_id,
                evidence_hash
            )
            .as_bytes(),
        );
        Ok(VerifierQaRecord {
            record_id,
            timestamp_unix,
            task_id: self.task_id,
            trace_hash: self.trace_hash,
            policy_hash: self.policy_hash,
            model_id: self.model_id,
            verifier_id: self.verifier_id,
            checkpoint_id: self.checkpoint_id,
            question: self.question,
            answer: self.answer,
            evidence_hash,
            raw_prompt_hash,
            local_only,
        })
    }
}

impl VerifierQaRecord {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_hash("verifier_qa.record_id", &self.record_id)?;
        if self.timestamp_unix == 0 {
            return Err(RlvrError::Config(
                "verifier_qa.timestamp_unix must be greater than zero".into(),
            ));
        }
        require_non_empty("verifier_qa.task_id", &self.task_id)?;
        require_hash("verifier_qa.trace_hash", &self.trace_hash)?;
        require_hash("verifier_qa.policy_hash", &self.policy_hash)?;
        require_non_empty("verifier_qa.model_id", &self.model_id)?;
        require_non_empty("verifier_qa.verifier_id", &self.verifier_id)?;
        require_non_empty("verifier_qa.checkpoint_id", &self.checkpoint_id)?;
        require_non_empty("verifier_qa.question", &self.question)?;
        require_non_empty("verifier_qa.answer", &self.answer)?;
        require_hash("verifier_qa.evidence_hash", &self.evidence_hash)?;
        if let Some(raw_prompt_hash) = &self.raw_prompt_hash {
            require_hash("verifier_qa.raw_prompt_hash", raw_prompt_hash)?;
        }
        Ok(())
    }

    pub fn export(&self, include_raw_prompt: bool) -> VerifierQaExportRecord {
        VerifierQaExportRecord {
            record_id: self.record_id.clone(),
            timestamp_unix: self.timestamp_unix,
            task_id: self.task_id.clone(),
            trace_hash: self.trace_hash.clone(),
            policy_hash: self.policy_hash.clone(),
            model_id: self.model_id.clone(),
            verifier_id: self.verifier_id.clone(),
            checkpoint_id: self.checkpoint_id.clone(),
            question: self.question.clone(),
            answer: self.answer.clone(),
            evidence_hash: self.evidence_hash.clone(),
            raw_prompt_hash: self.raw_prompt_hash.clone(),
            raw_prompt: include_raw_prompt
                .then_some("[raw prompt unavailable in stored record]".into()),
            local_only: self.local_only,
        }
    }
}

pub struct UnparseableVerifierLogger {
    path: PathBuf,
}

impl UnparseableVerifierLogger {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, RlvrError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        if !path.exists() {
            fs::File::create(&path)?;
        }
        Ok(Self { path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn record(&self, row: &UnparseableVerifierLogRow) -> Result<(), RlvrError> {
        let mut line = serde_json::to_string(row)?;
        line.push('\n');
        let mut file = OpenOptions::new().append(true).open(&self.path)?;
        file.write_all(line.as_bytes())?;
        Ok(())
    }
}

pub struct VerifierQaStore {
    path: PathBuf,
    local_only: bool,
}

impl VerifierQaStore {
    pub fn open(path: impl Into<PathBuf>, local_only: bool) -> Result<Self, RlvrError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        if !path.exists() {
            fs::File::create(&path)?;
        }
        Ok(Self { path, local_only })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn record(&self, input: VerifierQaRecordInput) -> Result<VerifierQaRecord, RlvrError> {
        let record = input.into_record(now_unix()?, self.local_only)?;
        record.validate()?;
        let mut line = serde_json::to_string(&record)?;
        line.push('\n');
        let mut file = OpenOptions::new().append(true).open(&self.path)?;
        file.write_all(line.as_bytes())?;
        Ok(record)
    }

    pub fn replay(&self) -> Result<Vec<VerifierQaRecord>, RlvrError> {
        let raw = fs::read_to_string(&self.path)?;
        let mut records = Vec::new();
        for (idx, line) in raw.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let record: VerifierQaRecord = serde_json::from_str(line).map_err(|err| {
                RlvrError::Config(format!("verifier QA line {} is invalid: {err}", idx + 1))
            })?;
            record.validate()?;
            records.push(record);
        }
        Ok(records)
    }

    pub fn export_jsonl(&self, include_raw_prompt: bool) -> Result<String, RlvrError> {
        let mut out = String::new();
        for record in self.replay()? {
            out.push_str(&serde_json::to_string(&record.export(include_raw_prompt))?);
            out.push('\n');
        }
        Ok(out)
    }
}

pub fn parse_strict_verifier_output(raw: &str) -> Result<StrictVerifierOutput, RlvrError> {
    let output: StrictVerifierOutput = serde_json::from_str(raw)?;
    output.validate()?;
    Ok(output)
}

pub fn parse_verifier_output_with_retries(
    attempts: &[&str],
    max_retries: usize,
) -> Result<VerifierParseReport, RlvrError> {
    parse_verifier_output_with_retries_and_logger(attempts, max_retries, None, None)
}

pub fn parse_verifier_output_with_retries_and_logger(
    attempts: &[&str],
    max_retries: usize,
    context: Option<&VerifierLogContext>,
    logger: Option<&UnparseableVerifierLogger>,
) -> Result<VerifierParseReport, RlvrError> {
    let max_attempts = max_retries.saturating_add(1);
    let mut failures = Vec::new();
    for (idx, raw) in attempts.iter().take(max_attempts).enumerate() {
        match parse_strict_verifier_output(raw) {
            Ok(strict_output) => {
                let training_output = strict_output.training_output()?;
                return Ok(VerifierParseReport {
                    parsed: Some(ParsedVerifierOutput {
                        strict_output,
                        training_output,
                        retry_count: idx,
                    }),
                    failures,
                    excluded_from_training: false,
                });
            }
            Err(err) => {
                let failure = VerifierParseFailure {
                    attempt: idx + 1,
                    raw_output_hash: hash_bytes(raw.as_bytes()),
                    error: err.to_string(),
                    used_for_training: false,
                };
                if let (Some(context), Some(logger)) = (context, logger) {
                    logger.record(&UnparseableVerifierLogRow {
                        timestamp_unix: now_unix()?,
                        verifier_id: context.verifier_id.clone(),
                        task_id: context.task_id.clone(),
                        trace_id: context.trace_id.clone(),
                        attempt: failure.attempt,
                        raw_output_hash: failure.raw_output_hash.clone(),
                        error: failure.error.clone(),
                        used_for_training: false,
                    })?;
                }
                failures.push(failure);
            }
        }
    }
    Ok(VerifierParseReport {
        parsed: None,
        failures,
        excluded_from_training: true,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifierLogContext {
    pub verifier_id: String,
    pub task_id: String,
    pub trace_id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CheckpointCoverageReport {
    pub targeted_checkpoints: Vec<String>,
    pub resolved_checkpoints: Vec<String>,
    pub missed_checkpoints: Vec<String>,
    pub unknown_checkpoints: Vec<String>,
    pub redundant_question: bool,
    pub total_checkpoints: usize,
    pub resolved_count: usize,
    pub coverage_score: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinalAnswerScoreReport {
    pub passed: bool,
    pub final_score: f64,
    pub answer_correctness: f64,
    pub rubric_completion: f64,
    pub reasoning_failure: bool,
    pub insufficient_information_failure: bool,
    pub route_failure: bool,
    pub tool_failure: bool,
    pub explanation: String,
    pub coverage: CheckpointCoverageReport,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerifierPanelJudge {
    pub verifier_id: String,
    pub output: StrictVerifierOutput,
}

impl VerifierPanelJudge {
    pub fn validate(&self) -> Result<(), RlvrError> {
        if self.verifier_id.trim().is_empty() {
            return Err(RlvrError::Config(
                "verifier panel judge verifier_id cannot be empty".into(),
            ));
        }
        self.output.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VerifierPanelReport {
    pub judge_count: usize,
    pub local_judge_id: Option<String>,
    pub stronger_judge_ids: Vec<String>,
    pub aggregated_output: StrictVerifierOutput,
    pub coverage: CheckpointCoverageReport,
    pub final_answer_score: Option<FinalAnswerScoreReport>,
    pub verifier_disagreement: bool,
    pub disagreement_reasons: Vec<String>,
}

impl VerifierPanelReport {
    pub fn validate(&self) -> Result<(), RlvrError> {
        if self.judge_count == 0 {
            return Err(RlvrError::Config(
                "verifier panel requires at least one judge".into(),
            ));
        }
        self.aggregated_output.validate()?;
        self.coverage.validate()?;
        if let Some(score) = &self.final_answer_score {
            score.validate()?;
        }
        Ok(())
    }
}

impl FinalAnswerScoreReport {
    pub fn validate(&self) -> Result<(), RlvrError> {
        require_score("final_answer.final_score", self.final_score)?;
        require_score("final_answer.answer_correctness", self.answer_correctness)?;
        require_score("final_answer.rubric_completion", self.rubric_completion)?;
        if self.explanation.trim().is_empty() {
            return Err(RlvrError::Config(
                "final answer score explanation cannot be empty".into(),
            ));
        }
        self.coverage.validate()
    }
}

pub fn evaluate_verifier_panel_for_item(
    item: &TrainingItem,
    judges: &[VerifierPanelJudge],
    local_judge_id: Option<&str>,
    stronger_judge_ids: &[&str],
) -> Result<VerifierPanelReport, RlvrError> {
    item.validate()?;
    if judges.is_empty() {
        return Err(RlvrError::Config(
            "verifier panel requires at least one judge".into(),
        ));
    }
    for judge in judges {
        judge.validate()?;
    }
    let outputs: Vec<StrictVerifierOutput> =
        judges.iter().map(|judge| judge.output.clone()).collect();
    let coverage = score_checkpoint_coverage_for_item(item, &outputs)?;
    let aggregated_output = aggregate_panel_outputs(&outputs)?;
    let final_answer_score = if aggregated_output.is_final_answer {
        Some(score_final_answer_from_coverage(
            std::slice::from_ref(&aggregated_output),
            coverage.clone(),
        )?)
    } else {
        None
    };
    let disagreement_reasons = panel_disagreement_reasons(judges, &final_answer_score)?;
    let report = VerifierPanelReport {
        judge_count: judges.len(),
        local_judge_id: local_judge_id.map(str::to_string),
        stronger_judge_ids: stronger_judge_ids
            .iter()
            .map(|judge_id| judge_id.to_string())
            .collect(),
        aggregated_output,
        coverage,
        final_answer_score,
        verifier_disagreement: !disagreement_reasons.is_empty(),
        disagreement_reasons,
    };
    report.validate()?;
    Ok(report)
}

pub fn evaluate_single_local_verifier_for_item(
    item: &TrainingItem,
    local_judge_id: impl Into<String>,
    output: StrictVerifierOutput,
) -> Result<VerifierPanelReport, RlvrError> {
    let local_judge_id = local_judge_id.into();
    let judges = vec![VerifierPanelJudge {
        verifier_id: local_judge_id.clone(),
        output,
    }];
    evaluate_verifier_panel_for_item(item, &judges, Some(&local_judge_id), &[])
}

impl CheckpointCoverageReport {
    pub fn validate(&self) -> Result<(), RlvrError> {
        if self.total_checkpoints == 0 {
            return Err(RlvrError::Config(
                "checkpoint coverage total_checkpoints must be greater than zero".into(),
            ));
        }
        if self.resolved_count > self.total_checkpoints {
            return Err(RlvrError::Config(
                "checkpoint coverage resolved_count cannot exceed total_checkpoints".into(),
            ));
        }
        if !self.coverage_score.is_finite()
            || self.coverage_score < 0.0
            || self.coverage_score > 1.0
        {
            return Err(RlvrError::Config(
                "checkpoint coverage score must be finite and in [0, 1]".into(),
            ));
        }
        Ok(())
    }
}

pub fn score_final_answer_for_item(
    item: &TrainingItem,
    verifier_outputs: &[StrictVerifierOutput],
) -> Result<FinalAnswerScoreReport, RlvrError> {
    item.validate()?;
    let coverage = score_checkpoint_coverage_for_item(item, verifier_outputs)?;
    score_final_answer_from_coverage(verifier_outputs, coverage)
}

pub fn score_final_answer_from_coverage(
    verifier_outputs: &[StrictVerifierOutput],
    coverage: CheckpointCoverageReport,
) -> Result<FinalAnswerScoreReport, RlvrError> {
    coverage.validate()?;
    let Some(final_output) = verifier_outputs
        .iter()
        .rev()
        .find(|output| output.is_final_answer)
    else {
        return Err(RlvrError::Config(
            "final answer scoring requires at least one final-answer verifier output".into(),
        ));
    };
    for output in verifier_outputs {
        output.validate()?;
    }

    let answer_correctness = final_output.reward.clamp(0.0, 1.0);
    let rubric_completion = coverage.coverage_score;
    let reasoning_failure = final_output.reward < 0.0;
    let insufficient_information_failure = final_output.premature_answer
        || (!coverage.missed_checkpoints.is_empty() && final_output.is_final_answer);
    let route_failure = !final_output.route_valid;
    let tool_failure = final_output.is_final_answer
        && coverage
            .missed_checkpoints
            .iter()
            .any(|checkpoint| checkpoint.contains("tool") || checkpoint.starts_with("tu-"));
    let mut final_score = (answer_correctness * 0.55) + (rubric_completion * 0.45);
    for failed in [
        reasoning_failure,
        insufficient_information_failure,
        route_failure,
        tool_failure,
        coverage.redundant_question,
    ] {
        if failed {
            final_score -= 0.15;
        }
    }
    final_score = final_score.clamp(0.0, 1.0);
    let passed = final_score >= 0.70
        && answer_correctness >= 0.70
        && rubric_completion >= 0.80
        && !reasoning_failure
        && !insufficient_information_failure
        && !route_failure
        && !tool_failure;
    let explanation = final_answer_explanation(
        passed,
        answer_correctness,
        rubric_completion,
        reasoning_failure,
        insufficient_information_failure,
        route_failure,
        tool_failure,
        &coverage,
    );
    let report = FinalAnswerScoreReport {
        passed,
        final_score,
        answer_correctness,
        rubric_completion,
        reasoning_failure,
        insufficient_information_failure,
        route_failure,
        tool_failure,
        explanation,
        coverage,
    };
    report.validate()?;
    Ok(report)
}

pub fn score_checkpoint_coverage_for_item(
    item: &TrainingItem,
    verifier_outputs: &[StrictVerifierOutput],
) -> Result<CheckpointCoverageReport, RlvrError> {
    item.validate()?;
    let checkpoint_ids: Vec<String> = item
        .checkpoints
        .iter()
        .map(|checkpoint| checkpoint.checkpoint_id.clone())
        .collect();
    score_checkpoint_coverage(&checkpoint_ids, verifier_outputs)
}

pub fn score_checkpoint_coverage(
    checkpoint_ids: &[String],
    verifier_outputs: &[StrictVerifierOutput],
) -> Result<CheckpointCoverageReport, RlvrError> {
    if checkpoint_ids.is_empty() {
        return Err(RlvrError::Config(
            "checkpoint coverage requires at least one checkpoint id".into(),
        ));
    }
    let known = BTreeSet::<String>::from_iter(checkpoint_ids.iter().cloned());
    if known.len() != checkpoint_ids.len() {
        return Err(RlvrError::Config(
            "checkpoint coverage checkpoint ids must be unique".into(),
        ));
    }

    let mut targeted = BTreeSet::new();
    let mut resolved = BTreeSet::new();
    let mut explicit_missed = BTreeSet::new();
    let mut unknown = BTreeSet::new();
    let mut redundant_question = false;

    for output in verifier_outputs {
        output.validate()?;
        redundant_question |= output.redundant_question;
        collect_checkpoint_ids(
            &known,
            &output.targeted_checkpoints,
            &mut targeted,
            &mut unknown,
        );
        collect_checkpoint_ids(
            &known,
            &output.resolved_checkpoints,
            &mut resolved,
            &mut unknown,
        );
        collect_checkpoint_ids(
            &known,
            &output.missed_checkpoints,
            &mut explicit_missed,
            &mut unknown,
        );
    }

    redundant_question |= !unknown.is_empty();
    let missed = known
        .difference(&resolved)
        .cloned()
        .chain(explicit_missed.into_iter())
        .collect::<BTreeSet<_>>();
    let resolved_count = resolved.len();
    let total_checkpoints = known.len();
    let report = CheckpointCoverageReport {
        targeted_checkpoints: targeted.into_iter().collect(),
        resolved_checkpoints: resolved.into_iter().collect(),
        missed_checkpoints: missed.into_iter().collect(),
        unknown_checkpoints: unknown.into_iter().collect(),
        redundant_question,
        total_checkpoints,
        resolved_count,
        coverage_score: resolved_count as f64 / total_checkpoints as f64,
    };
    report.validate()?;
    Ok(report)
}

fn collect_checkpoint_ids(
    known: &BTreeSet<String>,
    input: &[String],
    known_out: &mut BTreeSet<String>,
    unknown_out: &mut BTreeSet<String>,
) {
    for checkpoint_id in input {
        if known.contains(checkpoint_id) {
            known_out.insert(checkpoint_id.clone());
        } else {
            unknown_out.insert(checkpoint_id.clone());
        }
    }
}

fn aggregate_panel_outputs(
    outputs: &[StrictVerifierOutput],
) -> Result<StrictVerifierOutput, RlvrError> {
    if outputs.is_empty() {
        return Err(RlvrError::Config(
            "verifier panel aggregation requires at least one output".into(),
        ));
    }
    for output in outputs {
        output.validate()?;
    }
    let threshold = (outputs.len() / 2) + 1;
    let is_final_answer = majority(
        outputs
            .iter()
            .filter(|output| output.is_final_answer)
            .count(),
        threshold,
    );
    let is_clarification_question = majority(
        outputs
            .iter()
            .filter(|output| output.is_clarification_question)
            .count(),
        threshold,
    );
    let is_tool_call = majority(
        outputs.iter().filter(|output| output.is_tool_call).count(),
        threshold,
    );
    let is_route_decision = majority(
        outputs
            .iter()
            .filter(|output| output.is_route_decision)
            .count(),
        threshold,
    );
    let redundant_question = outputs.iter().any(|output| output.redundant_question);
    let premature_answer = outputs.iter().any(|output| output.premature_answer);
    let route_valid = majority(
        outputs.iter().filter(|output| output.route_valid).count(),
        threshold,
    );
    let false_premise_corrected = aggregate_optional_bool(
        outputs
            .iter()
            .filter_map(|output| output.false_premise_corrected),
        threshold,
    );
    let reward = outputs.iter().map(|output| output.reward).sum::<f64>() / outputs.len() as f64;
    let targeted_checkpoints = union_sorted(
        outputs
            .iter()
            .flat_map(|output| output.targeted_checkpoints.iter()),
    );
    let resolved_checkpoints = union_sorted(
        outputs
            .iter()
            .flat_map(|output| output.resolved_checkpoints.iter()),
    );
    let missed_checkpoints = union_sorted(
        outputs
            .iter()
            .flat_map(|output| output.missed_checkpoints.iter()),
    );
    let aggregated = StrictVerifierOutput {
        is_final_answer,
        is_clarification_question: is_clarification_question && !is_final_answer,
        is_tool_call,
        is_route_decision,
        targeted_checkpoints,
        resolved_checkpoints,
        missed_checkpoints,
        redundant_question,
        premature_answer,
        false_premise_corrected,
        route_valid,
        reward,
    };
    aggregated.validate()?;
    Ok(aggregated)
}

fn panel_disagreement_reasons(
    judges: &[VerifierPanelJudge],
    panel_final_score: &Option<FinalAnswerScoreReport>,
) -> Result<Vec<String>, RlvrError> {
    let mut reasons = Vec::new();
    if judges.len() <= 1 {
        return Ok(reasons);
    }
    let first = &judges[0].output;
    let fields = [
        (
            "final_answer",
            judges
                .iter()
                .any(|judge| judge.output.is_final_answer != first.is_final_answer),
        ),
        (
            "clarification_question",
            judges.iter().any(|judge| {
                judge.output.is_clarification_question != first.is_clarification_question
            }),
        ),
        (
            "tool_call",
            judges
                .iter()
                .any(|judge| judge.output.is_tool_call != first.is_tool_call),
        ),
        (
            "route_decision",
            judges
                .iter()
                .any(|judge| judge.output.is_route_decision != first.is_route_decision),
        ),
        (
            "route_valid",
            judges
                .iter()
                .any(|judge| judge.output.route_valid != first.route_valid),
        ),
        (
            "premature_answer",
            judges
                .iter()
                .any(|judge| judge.output.premature_answer != first.premature_answer),
        ),
        (
            "redundant_question",
            judges
                .iter()
                .any(|judge| judge.output.redundant_question != first.redundant_question),
        ),
        (
            "false_premise_corrected",
            judges
                .iter()
                .any(|judge| judge.output.false_premise_corrected != first.false_premise_corrected),
        ),
    ];
    for (field, disagreed) in fields {
        if disagreed {
            reasons.push(format!("binary disagreement on {field}"));
        }
    }
    if reward_range(judges) > 0.25 {
        reasons.push("reward disagreement exceeds 0.25".into());
    }
    if checkpoint_set_disagreement(judges, |output| &output.resolved_checkpoints) {
        reasons.push("resolved checkpoint disagreement".into());
    }
    if checkpoint_set_disagreement(judges, |output| &output.missed_checkpoints) {
        reasons.push("missed checkpoint disagreement".into());
    }
    if let Some(panel_score) = panel_final_score {
        let mut pass_values = BTreeSet::new();
        for judge in judges {
            if judge.output.is_final_answer {
                let ids = union_sorted(
                    judge
                        .output
                        .resolved_checkpoints
                        .iter()
                        .chain(judge.output.missed_checkpoints.iter()),
                );
                if !ids.is_empty() {
                    let coverage =
                        score_checkpoint_coverage(&ids, std::slice::from_ref(&judge.output))?;
                    let score = score_final_answer_from_coverage(
                        std::slice::from_ref(&judge.output),
                        coverage,
                    )?;
                    pass_values.insert(score.passed);
                }
            }
        }
        pass_values.insert(panel_score.passed);
        if pass_values.len() > 1 {
            reasons.push("final pass/fail disagreement".into());
        }
    }
    reasons.sort();
    reasons.dedup();
    Ok(reasons)
}

fn majority(count: usize, threshold: usize) -> bool {
    count >= threshold
}

fn aggregate_optional_bool(values: impl Iterator<Item = bool>, threshold: usize) -> Option<bool> {
    let mut total = 0usize;
    let mut true_count = 0usize;
    for value in values {
        total += 1;
        if value {
            true_count += 1;
        }
    }
    if total == 0 {
        None
    } else if true_count >= threshold.min(total) {
        Some(true)
    } else {
        Some(false)
    }
}

fn union_sorted<'a>(values: impl Iterator<Item = &'a String>) -> Vec<String> {
    values
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn reward_range(judges: &[VerifierPanelJudge]) -> f64 {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for judge in judges {
        min = min.min(judge.output.reward);
        max = max.max(judge.output.reward);
    }
    max - min
}

fn checkpoint_set_disagreement(
    judges: &[VerifierPanelJudge],
    selector: fn(&StrictVerifierOutput) -> &Vec<String>,
) -> bool {
    let Some(first) = judges.first() else {
        return false;
    };
    let first_set = selector(&first.output)
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    judges.iter().skip(1).any(|judge| {
        selector(&judge.output)
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            != first_set
    })
}

fn final_answer_explanation(
    passed: bool,
    answer_correctness: f64,
    rubric_completion: f64,
    reasoning_failure: bool,
    insufficient_information_failure: bool,
    route_failure: bool,
    tool_failure: bool,
    coverage: &CheckpointCoverageReport,
) -> String {
    let mut reasons = Vec::new();
    reasons.push(format!("answer_correctness={answer_correctness:.3}"));
    reasons.push(format!("rubric_completion={rubric_completion:.3}"));
    if reasoning_failure {
        reasons.push("reasoning failure: verifier reward is negative".into());
    }
    if insufficient_information_failure {
        reasons.push(format!(
            "insufficient information: unresolved checkpoints {:?}",
            coverage.missed_checkpoints
        ));
    }
    if route_failure {
        reasons.push("route failure: final verifier marked route invalid".into());
    }
    if tool_failure {
        reasons.push(format!(
            "tool failure: unresolved tool checkpoints {:?}",
            coverage
                .missed_checkpoints
                .iter()
                .filter(|checkpoint| checkpoint.contains("tool") || checkpoint.starts_with("tu-"))
                .collect::<Vec<_>>()
        ));
    }
    if coverage.redundant_question {
        reasons.push("redundant or unknown checkpoint reference present".into());
    }
    if passed {
        format!("PASS: {}", reasons.join("; "))
    } else {
        format!("FAIL: {}", reasons.join("; "))
    }
}

fn require_score(name: &str, value: f64) -> Result<(), RlvrError> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(RlvrError::Config(format!(
            "{name} must be finite and in [0, 1]"
        )));
    }
    Ok(())
}

fn require_non_empty(name: &str, value: &str) -> Result<(), RlvrError> {
    if value.trim().is_empty() {
        return Err(RlvrError::Config(format!("{name} cannot be empty")));
    }
    Ok(())
}

fn require_hash(name: &str, value: &str) -> Result<(), RlvrError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(RlvrError::Config(format!(
            "{name} must be a 64-character hex hash"
        )));
    }
    Ok(())
}

fn now_unix() -> Result<u64, RlvrError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| RlvrError::Config("system clock is before the unix epoch".into()))
        .map(|duration| duration.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        generate_tool_use_rubric, RoutePolicy, RouteTraceInput, RouteTraceRow, ToolUseRubricInput,
    };

    #[test]
    fn strict_verifier_parser_accepts_contract_and_converts_to_training_output() {
        let raw = r#"{
            "is_final_answer": false,
            "is_clarification_question": true,
            "is_tool_call": false,
            "is_route_decision": true,
            "targeted_checkpoints": ["c1"],
            "resolved_checkpoints": [],
            "missed_checkpoints": ["c2"],
            "redundant_question": false,
            "premature_answer": false,
            "false_premise_corrected": null,
            "route_valid": true,
            "reward": 0.45
        }"#;
        let strict = parse_strict_verifier_output(raw).unwrap();
        assert!(strict.is_route_decision);
        let training = strict.training_output().unwrap();
        assert!(training.is_clarification_question);
        assert_eq!(training.targeted_checkpoints, vec!["c1"]);
        assert_eq!(training.reward, 0.45);
    }

    #[test]
    fn strict_verifier_parser_rejects_missing_and_unknown_fields() {
        let missing = r#"{
            "is_final_answer": false,
            "is_clarification_question": true,
            "targeted_checkpoints": [],
            "resolved_checkpoints": [],
            "missed_checkpoints": [],
            "redundant_question": false,
            "premature_answer": false,
            "false_premise_corrected": null,
            "route_valid": true,
            "reward": 0.0
        }"#;
        assert!(parse_strict_verifier_output(missing).is_err());

        let unknown = r#"{
            "is_final_answer": false,
            "is_clarification_question": true,
            "is_tool_call": false,
            "is_route_decision": false,
            "targeted_checkpoints": [],
            "resolved_checkpoints": [],
            "missed_checkpoints": [],
            "redundant_question": false,
            "premature_answer": false,
            "false_premise_corrected": null,
            "route_valid": true,
            "reward": 0.0,
            "overall_score": 10
        }"#;
        assert!(parse_strict_verifier_output(unknown).is_err());
    }

    #[test]
    fn verifier_retry_report_uses_first_valid_attempt_and_excludes_failures() {
        let invalid = "not json";
        let valid = r#"{
            "is_final_answer": true,
            "is_clarification_question": false,
            "is_tool_call": false,
            "is_route_decision": false,
            "targeted_checkpoints": ["c1"],
            "resolved_checkpoints": ["c1"],
            "missed_checkpoints": [],
            "redundant_question": false,
            "premature_answer": false,
            "false_premise_corrected": true,
            "route_valid": true,
            "reward": 1.0
        }"#;
        let report = parse_verifier_output_with_retries(&[invalid, valid], 2).unwrap();
        assert!(!report.excluded_from_training);
        assert_eq!(report.failures.len(), 1);
        assert!(!report.failures[0].used_for_training);
        assert_eq!(report.parsed.as_ref().unwrap().retry_count, 1);
        assert_eq!(
            report.training_output().map(|output| output.reward),
            Some(1.0)
        );
    }

    #[test]
    fn verifier_retry_report_excludes_all_unparseable_outputs_from_training() {
        let report = parse_verifier_output_with_retries(&["not json", "{}"], 1).unwrap();
        assert!(report.excluded_from_training);
        assert!(report.parsed.is_none());
        assert_eq!(report.failures.len(), 2);
        assert!(report.training_output().is_none());
    }

    #[test]
    fn unparseable_verifier_logger_writes_hash_only_jsonl() {
        let dir =
            std::env::temp_dir().join(format!("fractal-rlvr-verifier-log-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("unparseable.jsonl");
        let logger = UnparseableVerifierLogger::open(&path).unwrap();
        let context = VerifierLogContext {
            verifier_id: "judge-local".into(),
            task_id: "task-1".into(),
            trace_id: "trace-1".into(),
        };
        let raw = "raw invalid verifier answer";
        let report =
            parse_verifier_output_with_retries_and_logger(&[raw], 0, Some(&context), Some(&logger))
                .unwrap();
        assert!(report.excluded_from_training);
        let written = fs::read_to_string(&path).unwrap();
        assert!(!written.contains(raw));
        assert!(written.contains(&hash_bytes(raw.as_bytes())));
        assert!(written.contains("\"used_for_training\":false"));
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn checkpoint_coverage_scorer_outputs_targeted_resolved_missed_and_score() {
        let checkpoint_ids = vec!["c1".to_string(), "c2".to_string(), "c3".to_string()];
        let output = StrictVerifierOutput {
            is_final_answer: false,
            is_clarification_question: true,
            is_tool_call: false,
            is_route_decision: false,
            targeted_checkpoints: vec!["c1".into(), "c2".into()],
            resolved_checkpoints: vec!["c1".into()],
            missed_checkpoints: vec!["c2".into()],
            redundant_question: false,
            premature_answer: false,
            false_premise_corrected: None,
            route_valid: true,
            reward: 0.25,
        };
        let report = score_checkpoint_coverage(&checkpoint_ids, &[output]).unwrap();
        assert_eq!(report.targeted_checkpoints, vec!["c1", "c2"]);
        assert_eq!(report.resolved_checkpoints, vec!["c1"]);
        assert_eq!(report.missed_checkpoints, vec!["c2", "c3"]);
        assert!(!report.redundant_question);
        assert_eq!(report.total_checkpoints, 3);
        assert_eq!(report.resolved_count, 1);
        assert_eq!(report.coverage_score, 1.0 / 3.0);
    }

    #[test]
    fn checkpoint_coverage_scorer_is_deterministic_for_fixed_verifier_output() {
        let checkpoint_ids = vec!["b".to_string(), "a".to_string(), "c".to_string()];
        let output = StrictVerifierOutput {
            is_final_answer: true,
            is_clarification_question: false,
            is_tool_call: false,
            is_route_decision: false,
            targeted_checkpoints: vec!["c".into(), "a".into(), "a".into()],
            resolved_checkpoints: vec!["c".into(), "a".into()],
            missed_checkpoints: vec!["b".into()],
            redundant_question: false,
            premature_answer: false,
            false_premise_corrected: Some(true),
            route_valid: true,
            reward: 0.8,
        };
        let first = score_checkpoint_coverage(&checkpoint_ids, &[output.clone()]).unwrap();
        let second = score_checkpoint_coverage(&checkpoint_ids, &[output]).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.targeted_checkpoints, vec!["a", "c"]);
        assert_eq!(first.resolved_checkpoints, vec!["a", "c"]);
        assert_eq!(first.missed_checkpoints, vec!["b"]);
        assert_eq!(first.coverage_score, 2.0 / 3.0);
    }

    #[test]
    fn checkpoint_coverage_flags_unknown_checkpoint_as_redundant() {
        let output = StrictVerifierOutput {
            is_final_answer: false,
            is_clarification_question: true,
            is_tool_call: false,
            is_route_decision: false,
            targeted_checkpoints: vec!["c1".into(), "not-in-rubric".into()],
            resolved_checkpoints: vec!["c1".into()],
            missed_checkpoints: Vec::new(),
            redundant_question: false,
            premature_answer: false,
            false_premise_corrected: None,
            route_valid: true,
            reward: 0.5,
        };
        let report = score_checkpoint_coverage(&["c1".into(), "c2".into()], &[output]).unwrap();
        assert!(report.redundant_question);
        assert_eq!(report.unknown_checkpoints, vec!["not-in-rubric"]);
        assert_eq!(report.missed_checkpoints, vec!["c2"]);
    }

    #[test]
    fn checkpoint_coverage_can_score_real_training_item_rubric() {
        let policy = RoutePolicy::default();
        let trace = RouteTraceRow::build(
            &RouteTraceInput {
                prompt: "What is the weather today?",
                answer: Some("Use weather lookup first."),
                selected_route: "web-enabled model",
                router_reason: "current_public_info; weather lookup required",
                route_policy: &policy,
                latency_ms: Some(500),
                cost_estimate: Some(0.001),
                user_rating: None,
                user_correction: None,
            },
            "rt-weather-coverage".into(),
            1,
            true,
        )
        .unwrap();
        let item = generate_tool_use_rubric(ToolUseRubricInput {
            trace,
            visible_prompt: Some("What is the weather today?".into()),
            tools: Vec::new(),
            route_policy: policy,
        })
        .unwrap();
        let output = StrictVerifierOutput {
            is_final_answer: false,
            is_clarification_question: false,
            is_tool_call: true,
            is_route_decision: false,
            targeted_checkpoints: vec!["tu-current-info".into(), "tu-weather".into()],
            resolved_checkpoints: vec!["tu-current-info".into()],
            missed_checkpoints: vec!["tu-weather".into()],
            redundant_question: false,
            premature_answer: false,
            false_premise_corrected: None,
            route_valid: true,
            reward: 0.4,
        };
        let report = score_checkpoint_coverage_for_item(&item, &[output]).unwrap();
        assert_eq!(report.total_checkpoints, item.checkpoints.len());
        assert_eq!(report.resolved_checkpoints, vec!["tu-current-info"]);
        assert!(report.missed_checkpoints.contains(&"tu-weather".into()));
    }

    #[test]
    fn final_answer_scorer_passes_correct_complete_answer() {
        let checkpoint_ids = vec!["c1".to_string(), "c2".to_string()];
        let output = StrictVerifierOutput {
            is_final_answer: true,
            is_clarification_question: false,
            is_tool_call: false,
            is_route_decision: false,
            targeted_checkpoints: checkpoint_ids.clone(),
            resolved_checkpoints: checkpoint_ids.clone(),
            missed_checkpoints: Vec::new(),
            redundant_question: false,
            premature_answer: false,
            false_premise_corrected: Some(true),
            route_valid: true,
            reward: 0.95,
        };
        let coverage =
            score_checkpoint_coverage(&checkpoint_ids, std::slice::from_ref(&output)).unwrap();
        let report = score_final_answer_from_coverage(&[output], coverage).unwrap();
        assert!(report.passed);
        assert_eq!(report.answer_correctness, 0.95);
        assert_eq!(report.rubric_completion, 1.0);
        assert!(report.explanation.starts_with("PASS:"));
        report.validate().unwrap();
    }

    #[test]
    fn final_answer_scorer_flags_reasoning_and_insufficient_information_failures() {
        let checkpoint_ids = vec!["c1".to_string(), "c2".to_string()];
        let output = StrictVerifierOutput {
            is_final_answer: true,
            is_clarification_question: false,
            is_tool_call: false,
            is_route_decision: false,
            targeted_checkpoints: vec!["c1".into()],
            resolved_checkpoints: vec!["c1".into()],
            missed_checkpoints: vec!["c2".into()],
            redundant_question: false,
            premature_answer: true,
            false_premise_corrected: None,
            route_valid: true,
            reward: -0.25,
        };
        let coverage =
            score_checkpoint_coverage(&checkpoint_ids, std::slice::from_ref(&output)).unwrap();
        let report = score_final_answer_from_coverage(&[output], coverage).unwrap();
        assert!(!report.passed);
        assert!(report.reasoning_failure);
        assert!(report.insufficient_information_failure);
        assert!(report.explanation.contains("reasoning failure"));
        assert!(report.explanation.contains("insufficient information"));
    }

    #[test]
    fn final_answer_scorer_flags_route_and_tool_failures_on_real_rubric() {
        let policy = RoutePolicy::default();
        let trace = RouteTraceRow::build(
            &RouteTraceInput {
                prompt: "What is the weather today?",
                answer: Some("It is sunny."),
                selected_route: "tiny-local-model",
                router_reason: "stable_knowledge; incorrectly skipped weather tool",
                route_policy: &policy,
                latency_ms: Some(30),
                cost_estimate: Some(0.0),
                user_rating: None,
                user_correction: None,
            },
            "rt-weather-final-score".into(),
            1,
            true,
        )
        .unwrap();
        let item = generate_tool_use_rubric(ToolUseRubricInput {
            trace,
            visible_prompt: Some("What is the weather today?".into()),
            tools: Vec::new(),
            route_policy: policy,
        })
        .unwrap();
        let output = StrictVerifierOutput {
            is_final_answer: true,
            is_clarification_question: false,
            is_tool_call: false,
            is_route_decision: false,
            targeted_checkpoints: vec!["tu-current-info".into()],
            resolved_checkpoints: Vec::new(),
            missed_checkpoints: vec!["tu-current-info".into(), "tu-weather".into()],
            redundant_question: false,
            premature_answer: false,
            false_premise_corrected: None,
            route_valid: false,
            reward: 0.4,
        };
        let report = score_final_answer_for_item(&item, &[output]).unwrap();
        assert!(!report.passed);
        assert!(report.route_failure);
        assert!(report.tool_failure);
        assert!(report.explanation.contains("route failure"));
        assert!(report.explanation.contains("tool failure"));
    }

    #[test]
    fn final_answer_scorer_requires_final_answer_output() {
        let checkpoint_ids = vec!["c1".to_string()];
        let output = StrictVerifierOutput {
            is_final_answer: false,
            is_clarification_question: true,
            is_tool_call: false,
            is_route_decision: false,
            targeted_checkpoints: vec!["c1".into()],
            resolved_checkpoints: Vec::new(),
            missed_checkpoints: vec!["c1".into()],
            redundant_question: false,
            premature_answer: false,
            false_premise_corrected: None,
            route_valid: true,
            reward: 0.1,
        };
        let coverage =
            score_checkpoint_coverage(&checkpoint_ids, std::slice::from_ref(&output)).unwrap();
        let err = score_final_answer_from_coverage(&[output], coverage).unwrap_err();
        assert!(err.to_string().contains("final-answer verifier output"));
    }

    #[test]
    fn verifier_panel_supports_one_local_judge_without_disagreement() {
        let item = tool_use_weather_item();
        let output = final_output(
            &["tu-current-info", "tu-weather"],
            &["tu-current-info", "tu-weather"],
            &[],
            true,
            0.95,
        );
        let report = evaluate_single_local_verifier_for_item(&item, "local-judge", output).unwrap();
        assert_eq!(report.judge_count, 1);
        assert_eq!(report.local_judge_id.as_deref(), Some("local-judge"));
        assert!(!report.verifier_disagreement);
        assert!(report.disagreement_reasons.is_empty());
        assert!(report.final_answer_score.as_ref().unwrap().passed);
        report.validate().unwrap();
    }

    #[test]
    fn verifier_panel_aggregates_multiple_matching_outputs() {
        let item = tool_use_weather_item();
        let local = VerifierPanelJudge {
            verifier_id: "local-judge".into(),
            output: final_output(
                &["tu-current-info", "tu-weather"],
                &["tu-current-info", "tu-weather"],
                &[],
                true,
                0.9,
            ),
        };
        let strong = VerifierPanelJudge {
            verifier_id: "strong-judge".into(),
            output: final_output(
                &["tu-current-info", "tu-weather"],
                &["tu-current-info", "tu-weather"],
                &[],
                true,
                0.95,
            ),
        };
        let report = evaluate_verifier_panel_for_item(
            &item,
            &[local, strong],
            Some("local-judge"),
            &["strong-judge"],
        )
        .unwrap();
        assert_eq!(report.judge_count, 2);
        assert_eq!(report.stronger_judge_ids, vec!["strong-judge"]);
        assert!(!report.verifier_disagreement);
        assert_eq!(report.aggregated_output.reward, 0.925);
        assert_eq!(
            report.aggregated_output.resolved_checkpoints,
            vec!["tu-current-info", "tu-weather"]
        );
    }

    #[test]
    fn verifier_panel_flags_disagreement_with_stronger_verifier() {
        let item = tool_use_weather_item();
        let local = VerifierPanelJudge {
            verifier_id: "local-judge".into(),
            output: final_output(&["tu-current-info"], &["tu-current-info"], &[], true, 0.9),
        };
        let strong = VerifierPanelJudge {
            verifier_id: "strong-judge".into(),
            output: final_output(
                &["tu-current-info", "tu-weather"],
                &[],
                &["tu-current-info", "tu-weather"],
                false,
                0.1,
            ),
        };
        let report = evaluate_verifier_panel_for_item(
            &item,
            &[local, strong],
            Some("local-judge"),
            &["strong-judge"],
        )
        .unwrap();
        assert!(report.verifier_disagreement);
        assert!(report
            .disagreement_reasons
            .iter()
            .any(|reason| reason.contains("route_valid")));
        assert!(report
            .disagreement_reasons
            .iter()
            .any(|reason| reason.contains("reward disagreement")));
        assert!(report
            .disagreement_reasons
            .iter()
            .any(|reason| reason.contains("resolved checkpoint")));
    }

    #[test]
    fn verifier_qa_store_records_questions_answers_evidence_and_metadata() {
        let dir = verifier_store_scratch("qa-record");
        let path = dir.join("verifier-qa.jsonl");
        let store = VerifierQaStore::open(&path, true).unwrap();
        let input = qa_input("Does the answer use the weather tool?");
        let record = store.record(input).unwrap();

        assert_eq!(record.task_id, "task-qa-1");
        assert_eq!(record.model_id, "candidate-model");
        assert_eq!(record.verifier_id, "local-verifier");
        assert_eq!(record.checkpoint_id, "tu-weather");
        assert_eq!(record.answer, "No");
        assert_eq!(
            record.evidence_hash,
            hash_bytes(b"Answer was produced from memory.")
        );
        assert_eq!(
            record.raw_prompt_hash.as_deref(),
            Some(hash_bytes(b"What is the weather today?").as_str())
        );
        assert!(record.local_only);
        record.validate().unwrap();

        let replayed = store.replay().unwrap();
        assert_eq!(replayed, vec![record]);
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn verifier_qa_export_excludes_raw_prompt_by_default() {
        let dir = verifier_store_scratch("qa-export");
        let path = dir.join("verifier-qa.jsonl");
        let store = VerifierQaStore::open(&path, true).unwrap();
        store
            .record(qa_input("Is the final answer correct?"))
            .unwrap();

        let export = store.export_jsonl(false).unwrap();
        assert!(!export.contains("What is the weather today?"));
        assert!(!export.contains("Answer was produced from memory."));
        assert!(export.contains("raw_prompt_hash"));
        assert!(export.contains("evidence_hash"));
        assert!(export.contains("\"raw_prompt\":null"));
        let parsed: VerifierQaExportRecord = serde_json::from_str(export.trim()).unwrap();
        assert_eq!(parsed.question, "Is the final answer correct?");
        assert!(parsed.raw_prompt.is_none());
        let _ = fs::remove_dir_all(dir);
    }

    #[test]
    fn verifier_qa_store_rejects_invalid_metadata_hashes() {
        let mut input = qa_input("Bad hash?");
        input.policy_hash = "not-a-hash".into();
        let err = input.into_record(1, true).unwrap_err();
        assert!(err.to_string().contains("policy_hash"));
    }

    fn tool_use_weather_item() -> TrainingItem {
        let policy = RoutePolicy::default();
        let trace = RouteTraceRow::build(
            &RouteTraceInput {
                prompt: "What is the weather today?",
                answer: Some("Use weather lookup first."),
                selected_route: "web-enabled model",
                router_reason: "current_public_info; weather lookup required",
                route_policy: &policy,
                latency_ms: Some(500),
                cost_estimate: Some(0.001),
                user_rating: None,
                user_correction: None,
            },
            "rt-weather-panel".into(),
            1,
            true,
        )
        .unwrap();
        generate_tool_use_rubric(ToolUseRubricInput {
            trace,
            visible_prompt: Some("What is the weather today?".into()),
            tools: Vec::new(),
            route_policy: policy,
        })
        .unwrap()
    }

    fn final_output(
        targeted: &[&str],
        resolved: &[&str],
        missed: &[&str],
        route_valid: bool,
        reward: f64,
    ) -> StrictVerifierOutput {
        StrictVerifierOutput {
            is_final_answer: true,
            is_clarification_question: false,
            is_tool_call: false,
            is_route_decision: false,
            targeted_checkpoints: targeted.iter().map(|id| id.to_string()).collect(),
            resolved_checkpoints: resolved.iter().map(|id| id.to_string()).collect(),
            missed_checkpoints: missed.iter().map(|id| id.to_string()).collect(),
            redundant_question: false,
            premature_answer: false,
            false_premise_corrected: None,
            route_valid,
            reward,
        }
    }

    fn qa_input(question: &str) -> VerifierQaRecordInput {
        VerifierQaRecordInput {
            task_id: "task-qa-1".into(),
            trace_hash: hash_bytes(b"trace-qa-1"),
            policy_hash: hash_bytes(b"policy-qa-1"),
            model_id: "candidate-model".into(),
            verifier_id: "local-verifier".into(),
            checkpoint_id: "tu-weather".into(),
            question: question.into(),
            answer: "No".into(),
            evidence: "Answer was produced from memory.".into(),
            raw_prompt: Some("What is the weather today?".into()),
        }
    }

    fn verifier_store_scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "fractal-rlvr-verifier-{name}-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }
}
