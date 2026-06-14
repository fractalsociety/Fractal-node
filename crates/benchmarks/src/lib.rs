use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub type MicroFrac = i128;

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DifficultyTier {
    Easy,
    Medium,
    Hard,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum TaskKind {
    Math,
    DataExtraction,
    CodeHiddenTests,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchTask {
    pub task_id: String,
    pub tier: DifficultyTier,
    pub kind: TaskKind,
    pub prompt: String,
    pub expected_answer: String,
    pub payout_micro_frac: MicroFrac,
    pub gas_micro_frac: MicroFrac,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelAttempt {
    pub task_id: String,
    pub model: String,
    pub output: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub input_token_price_micro_frac_per_million: MicroFrac,
    pub output_token_price_micro_frac_per_million: MicroFrac,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AttemptEvaluation {
    pub task_id: String,
    pub model: String,
    pub tier: DifficultyTier,
    pub kind: TaskKind,
    pub passed: bool,
    pub quality_score_milli: u16,
    pub payout_earned_micro_frac: MicroFrac,
    pub inference_cost_micro_frac: MicroFrac,
    pub gas_micro_frac: MicroFrac,
    pub profit_micro_frac: MicroFrac,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfitSummary {
    pub attempts: u64,
    pub passes: u64,
    pub pass_rate_milli: u64,
    pub avg_quality_milli: u64,
    pub total_available_payout_micro_frac: MicroFrac,
    pub total_earned_payout_micro_frac: MicroFrac,
    pub total_inference_cost_micro_frac: MicroFrac,
    pub total_gas_micro_frac: MicroFrac,
    pub total_profit_micro_frac: MicroFrac,
    pub avg_profit_micro_frac: MicroFrac,
    pub profit_margin_milli: i128,
    pub break_even_payout_per_pass_micro_frac: Option<MicroFrac>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EconomicBenchReport {
    pub task_count: usize,
    pub attempt_count: usize,
    pub by_model: BTreeMap<String, ProfitSummary>,
    pub by_model_tier: BTreeMap<String, BTreeMap<DifficultyTier, ProfitSummary>>,
    pub evaluations: Vec<AttemptEvaluation>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VerifierFailureMode {
    Correct,
    EdgeCaseBug,
    ConfidentWrong,
    Plagiarized,
    SemanticMismatch,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifierCase {
    pub case_id: String,
    pub tier: DifficultyTier,
    pub kind: TaskKind,
    pub failure_mode: VerifierFailureMode,
    pub task_prompt: String,
    pub submission: String,
    pub should_accept: bool,
    pub ground_truth_quality_milli: u16,
    pub payout_micro_frac: MicroFrac,
    pub honest_worker_churn_cost_micro_frac: MicroFrac,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifierJudgment {
    pub case_id: String,
    pub verifier: String,
    pub accept: bool,
    pub confidence_milli: u16,
    pub score_milli: u16,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub input_token_price_micro_frac_per_million: MicroFrac,
    pub output_token_price_micro_frac_per_million: MicroFrac,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifierEvaluation {
    pub case_id: String,
    pub verifier: String,
    pub tier: DifficultyTier,
    pub kind: TaskKind,
    pub failure_mode: VerifierFailureMode,
    pub should_accept: bool,
    pub accepted: bool,
    pub confidence_milli: u16,
    pub score_milli: u16,
    pub false_accept: bool,
    pub false_reject: bool,
    pub leakage_cost_micro_frac: MicroFrac,
    pub churn_cost_micro_frac: MicroFrac,
    pub inference_cost_micro_frac: MicroFrac,
    pub brier_x1m: u64,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifierSummary {
    pub judgments: u64,
    pub positives: u64,
    pub negatives: u64,
    pub true_accepts: u64,
    pub true_rejects: u64,
    pub false_accepts: u64,
    pub false_rejects: u64,
    pub false_accept_rate_milli: u64,
    pub false_reject_rate_milli: u64,
    pub accuracy_milli: u64,
    pub auc_milli: u64,
    pub brier_milli: u64,
    pub total_leakage_cost_micro_frac: MicroFrac,
    pub total_churn_cost_micro_frac: MicroFrac,
    pub total_inference_cost_micro_frac: MicroFrac,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifierBenchReport {
    pub case_count: usize,
    pub judgment_count: usize,
    pub by_verifier: BTreeMap<String, VerifierSummary>,
    pub evaluations: Vec<VerifierEvaluation>,
}

#[derive(Clone, Copy, Debug)]
struct TierParams {
    payout_micro_frac: MicroFrac,
    gas_micro_frac: MicroFrac,
}

fn tier_params(tier: DifficultyTier) -> TierParams {
    match tier {
        DifficultyTier::Easy => TierParams {
            payout_micro_frac: 1_000,
            gas_micro_frac: 35,
        },
        DifficultyTier::Medium => TierParams {
            payout_micro_frac: 3_000,
            gas_micro_frac: 50,
        },
        DifficultyTier::Hard => TierParams {
            payout_micro_frac: 9_000,
            gas_micro_frac: 75,
        },
    }
}

pub fn generate_economic_tasks(count: usize, seed: u64) -> Vec<BenchTask> {
    let mut tasks = Vec::with_capacity(count);
    let mut rng = Lcg::new(seed);
    for i in 0..count {
        let tier = match i % 3 {
            0 => DifficultyTier::Easy,
            1 => DifficultyTier::Medium,
            _ => DifficultyTier::Hard,
        };
        let kind = match (i / 3) % 3 {
            0 => TaskKind::Math,
            1 => TaskKind::DataExtraction,
            _ => TaskKind::CodeHiddenTests,
        };
        let params = tier_params(tier);
        let (prompt, expected_answer) = generate_task_body(kind, tier, &mut rng);
        tasks.push(BenchTask {
            task_id: format!("task-{i:06}"),
            tier,
            kind,
            prompt,
            expected_answer,
            payout_micro_frac: params.payout_micro_frac,
            gas_micro_frac: params.gas_micro_frac,
        });
    }
    tasks
}

pub fn evaluate_attempt(task: &BenchTask, attempt: &ModelAttempt) -> AttemptEvaluation {
    let passed = normalize_answer(&attempt.output) == normalize_answer(&task.expected_answer);
    let quality_score_milli = if passed { 1_000 } else { 0 };
    let payout_earned_micro_frac = if passed { task.payout_micro_frac } else { 0 };
    let inference_cost_micro_frac = token_cost_micro_frac(attempt);
    let profit_micro_frac =
        payout_earned_micro_frac - inference_cost_micro_frac - task.gas_micro_frac;
    AttemptEvaluation {
        task_id: task.task_id.clone(),
        model: attempt.model.clone(),
        tier: task.tier,
        kind: task.kind,
        passed,
        quality_score_milli,
        payout_earned_micro_frac,
        inference_cost_micro_frac,
        gas_micro_frac: task.gas_micro_frac,
        profit_micro_frac,
    }
}

pub fn run_economic_bench(tasks: &[BenchTask], attempts: &[ModelAttempt]) -> EconomicBenchReport {
    let task_by_id: BTreeMap<&str, &BenchTask> =
        tasks.iter().map(|t| (t.task_id.as_str(), t)).collect();
    let mut evaluations = Vec::new();
    let mut by_model = BTreeMap::<String, SummaryAcc>::new();
    let mut by_model_tier = BTreeMap::<String, BTreeMap<DifficultyTier, SummaryAcc>>::new();

    for attempt in attempts {
        let Some(task) = task_by_id.get(attempt.task_id.as_str()) else {
            continue;
        };
        let ev = evaluate_attempt(task, attempt);
        by_model
            .entry(ev.model.clone())
            .or_default()
            .push(task, &ev);
        by_model_tier
            .entry(ev.model.clone())
            .or_default()
            .entry(ev.tier)
            .or_default()
            .push(task, &ev);
        evaluations.push(ev);
    }

    EconomicBenchReport {
        task_count: tasks.len(),
        attempt_count: attempts.len(),
        by_model: by_model
            .into_iter()
            .map(|(model, acc)| (model, acc.finish()))
            .collect(),
        by_model_tier: by_model_tier
            .into_iter()
            .map(|(model, tiers)| {
                (
                    model,
                    tiers
                        .into_iter()
                        .map(|(tier, acc)| (tier, acc.finish()))
                        .collect(),
                )
            })
            .collect(),
        evaluations,
    }
}

pub fn synthetic_attempts(tasks: &[BenchTask]) -> Vec<ModelAttempt> {
    let profiles = [
        SyntheticProfile {
            model: "cheap-70",
            solve_easy: 90,
            solve_medium: 70,
            solve_hard: 35,
            input_tokens: 450,
            output_tokens: 80,
            input_price: 120,
            output_price: 500,
        },
        SyntheticProfile {
            model: "strong-expensive-90",
            solve_easy: 98,
            solve_medium: 90,
            solve_hard: 78,
            input_tokens: 2_400,
            output_tokens: 650,
            input_price: 3_000,
            output_price: 12_000,
        },
        SyntheticProfile {
            model: "tiny-weak-45",
            solve_easy: 68,
            solve_medium: 42,
            solve_hard: 18,
            input_tokens: 220,
            output_tokens: 45,
            input_price: 40,
            output_price: 160,
        },
    ];
    let mut attempts = Vec::with_capacity(tasks.len() * profiles.len());
    for profile in profiles {
        for task in tasks {
            let pass_threshold = match task.tier {
                DifficultyTier::Easy => profile.solve_easy,
                DifficultyTier::Medium => profile.solve_medium,
                DifficultyTier::Hard => profile.solve_hard,
            };
            let score = deterministic_score(profile.model, &task.task_id);
            let output = if score < pass_threshold {
                task.expected_answer.clone()
            } else {
                wrong_answer(&task.expected_answer)
            };
            attempts.push(ModelAttempt {
                task_id: task.task_id.clone(),
                model: profile.model.into(),
                output,
                input_tokens: profile.input_tokens,
                output_tokens: profile.output_tokens,
                input_token_price_micro_frac_per_million: profile.input_price,
                output_token_price_micro_frac_per_million: profile.output_price,
            });
        }
    }
    attempts
}

pub fn generate_verifier_cases() -> Vec<VerifierCase> {
    let mut cases = Vec::new();
    let tasks = generate_economic_tasks(15, 77);
    for (i, task) in tasks.into_iter().enumerate() {
        let mode = match i % 5 {
            0 => VerifierFailureMode::Correct,
            1 => VerifierFailureMode::EdgeCaseBug,
            2 => VerifierFailureMode::ConfidentWrong,
            3 => VerifierFailureMode::Plagiarized,
            _ => VerifierFailureMode::SemanticMismatch,
        };
        let should_accept = mode == VerifierFailureMode::Correct;
        let quality = if should_accept { 1_000 } else { 0 };
        let submission = verifier_submission_for(&task.expected_answer, mode);
        cases.push(VerifierCase {
            case_id: format!("verify-{i:06}"),
            tier: task.tier,
            kind: task.kind,
            failure_mode: mode,
            task_prompt: task.prompt,
            submission,
            should_accept,
            ground_truth_quality_milli: quality,
            payout_micro_frac: task.payout_micro_frac,
            honest_worker_churn_cost_micro_frac: task.payout_micro_frac / 2,
        });
    }
    cases
}

pub fn evaluate_verifier_judgment(
    case: &VerifierCase,
    judgment: &VerifierJudgment,
) -> VerifierEvaluation {
    let false_accept = judgment.accept && !case.should_accept;
    let false_reject = !judgment.accept && case.should_accept;
    let leakage_cost_micro_frac = if false_accept {
        case.payout_micro_frac
    } else {
        0
    };
    let churn_cost_micro_frac = if false_reject {
        case.honest_worker_churn_cost_micro_frac
    } else {
        0
    };
    let inference_cost_micro_frac = verifier_token_cost_micro_frac(judgment);
    let p_accept = u64::from(judgment.score_milli.min(1_000));
    let target = if case.should_accept { 1_000u64 } else { 0u64 };
    let diff = p_accept.abs_diff(target);
    VerifierEvaluation {
        case_id: case.case_id.clone(),
        verifier: judgment.verifier.clone(),
        tier: case.tier,
        kind: case.kind,
        failure_mode: case.failure_mode,
        should_accept: case.should_accept,
        accepted: judgment.accept,
        confidence_milli: judgment.confidence_milli,
        score_milli: judgment.score_milli,
        false_accept,
        false_reject,
        leakage_cost_micro_frac,
        churn_cost_micro_frac,
        inference_cost_micro_frac,
        brier_x1m: diff * diff,
    }
}

pub fn run_verifier_bench(
    cases: &[VerifierCase],
    judgments: &[VerifierJudgment],
) -> VerifierBenchReport {
    let case_by_id: BTreeMap<&str, &VerifierCase> =
        cases.iter().map(|c| (c.case_id.as_str(), c)).collect();
    let mut evaluations = Vec::new();
    let mut by_verifier = BTreeMap::<String, VerifierAcc>::new();
    let mut scores = BTreeMap::<String, Vec<(u16, bool)>>::new();
    for judgment in judgments {
        let Some(case) = case_by_id.get(judgment.case_id.as_str()) else {
            continue;
        };
        let ev = evaluate_verifier_judgment(case, judgment);
        scores
            .entry(ev.verifier.clone())
            .or_default()
            .push((ev.score_milli, ev.should_accept));
        by_verifier
            .entry(ev.verifier.clone())
            .or_default()
            .push(&ev);
        evaluations.push(ev);
    }
    let by_verifier = by_verifier
        .into_iter()
        .map(|(verifier, acc)| {
            let auc_milli = auc_milli(scores.get(&verifier).map(Vec::as_slice).unwrap_or(&[]));
            (verifier, acc.finish(auc_milli))
        })
        .collect();
    VerifierBenchReport {
        case_count: cases.len(),
        judgment_count: judgments.len(),
        by_verifier,
        evaluations,
    }
}

pub fn synthetic_verifier_judgments(cases: &[VerifierCase]) -> Vec<VerifierJudgment> {
    let profiles = [
        VerifierProfile {
            verifier: "strict-calibrated",
            false_accept_pct: 6,
            false_reject_pct: 12,
            confidence_correct: 860,
            confidence_wrong: 380,
            input_tokens: 900,
            output_tokens: 70,
            input_price: 800,
            output_price: 2_400,
        },
        VerifierProfile {
            verifier: "lenient-leaky",
            false_accept_pct: 28,
            false_reject_pct: 4,
            confidence_correct: 780,
            confidence_wrong: 640,
            input_tokens: 650,
            output_tokens: 45,
            input_price: 300,
            output_price: 900,
        },
        VerifierProfile {
            verifier: "noisy-uncalibrated",
            false_accept_pct: 18,
            false_reject_pct: 22,
            confidence_correct: 960,
            confidence_wrong: 910,
            input_tokens: 1_300,
            output_tokens: 120,
            input_price: 1_500,
            output_price: 5_000,
        },
    ];
    let mut judgments = Vec::with_capacity(cases.len() * profiles.len());
    for profile in profiles {
        for case in cases {
            let score = deterministic_score(profile.verifier, &case.case_id);
            let accept = if case.should_accept {
                score >= profile.false_reject_pct
            } else {
                score < profile.false_accept_pct
            };
            let correct = accept == case.should_accept;
            let confidence = if correct {
                profile.confidence_correct
            } else {
                profile.confidence_wrong
            };
            judgments.push(VerifierJudgment {
                case_id: case.case_id.clone(),
                verifier: profile.verifier.into(),
                accept,
                confidence_milli: confidence,
                score_milli: if accept {
                    confidence
                } else {
                    1_000 - confidence
                },
                input_tokens: profile.input_tokens,
                output_tokens: profile.output_tokens,
                input_token_price_micro_frac_per_million: profile.input_price,
                output_token_price_micro_frac_per_million: profile.output_price,
            });
        }
    }
    judgments
}

fn generate_task_body(kind: TaskKind, tier: DifficultyTier, rng: &mut Lcg) -> (String, String) {
    match kind {
        TaskKind::Math => {
            let span = match tier {
                DifficultyTier::Easy => 100,
                DifficultyTier::Medium => 1_000,
                DifficultyTier::Hard => 10_000,
            };
            let a = (rng.next() % span) as i64 + 1;
            let b = (rng.next() % span) as i64 + 1;
            let c = (rng.next() % span) as i64 + 1;
            let answer = a * b + c;
            (
                format!("Compute exactly: ({a} * {b}) + {c}. Return only the integer."),
                answer.to_string(),
            )
        }
        TaskKind::DataExtraction => {
            let n = match tier {
                DifficultyTier::Easy => 4,
                DifficultyTier::Medium => 8,
                DifficultyTier::Hard => 14,
            };
            let target_idx = (rng.next() as usize) % n;
            let mut records = Vec::new();
            let mut answer = String::new();
            for i in 0..n {
                let id = 10_000 + (rng.next() % 90_000);
                let score = rng.next() % 1_000;
                if i == target_idx {
                    answer = id.to_string();
                }
                records.push(format!("{{name:user_{i},id:{id},score:{score}}}"));
            }
            (
                format!(
                    "From these records, return only the id for name=user_{target_idx}: {}",
                    records.join(";")
                ),
                answer,
            )
        }
        TaskKind::CodeHiddenTests => {
            let n = match tier {
                DifficultyTier::Easy => 5,
                DifficultyTier::Medium => 9,
                DifficultyTier::Hard => 13,
            };
            let mut values = Vec::new();
            let mut sum = 0i64;
            for _ in 0..n {
                let v = (rng.next() % 200) as i64 - 50;
                sum += v * v;
                values.push(v.to_string());
            }
            (
                format!(
                    "Hidden-test proxy: return the sum of squares for this generated input list: [{}]. Return only the integer.",
                    values.join(",")
                ),
                sum.to_string(),
            )
        }
    }
}

fn token_cost_micro_frac(attempt: &ModelAttempt) -> MicroFrac {
    let input = ceil_micro_cost(
        attempt.input_tokens,
        attempt.input_token_price_micro_frac_per_million,
    );
    let output = ceil_micro_cost(
        attempt.output_tokens,
        attempt.output_token_price_micro_frac_per_million,
    );
    input + output
}

fn verifier_token_cost_micro_frac(judgment: &VerifierJudgment) -> MicroFrac {
    ceil_micro_cost(
        judgment.input_tokens,
        judgment.input_token_price_micro_frac_per_million,
    ) + ceil_micro_cost(
        judgment.output_tokens,
        judgment.output_token_price_micro_frac_per_million,
    )
}

fn ceil_micro_cost(tokens: u64, price_micro_frac_per_million: MicroFrac) -> MicroFrac {
    let raw = tokens as MicroFrac * price_micro_frac_per_million;
    if raw <= 0 {
        0
    } else {
        (raw + 999_999) / 1_000_000
    }
}

fn normalize_answer(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

fn wrong_answer(answer: &str) -> String {
    match answer.trim().parse::<i128>() {
        Ok(n) => (n + 1).to_string(),
        Err(_) => "incorrect".into(),
    }
}

fn deterministic_score(model: &str, task_id: &str) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in model.bytes().chain(task_id.bytes()) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h % 100
}

fn verifier_submission_for(expected: &str, mode: VerifierFailureMode) -> String {
    match mode {
        VerifierFailureMode::Correct => expected.to_string(),
        VerifierFailureMode::EdgeCaseBug => {
            format!("{expected}\nNote: implementation omits empty-input and overflow cases.")
        }
        VerifierFailureMode::ConfidentWrong => {
            format!("The answer is definitely {}.", wrong_answer(expected))
        }
        VerifierFailureMode::Plagiarized => {
            format!("Copied prior solution verbatim; final answer: {expected}")
        }
        VerifierFailureMode::SemanticMismatch => {
            format!(
                "Valid format, wrong semantic target: {}",
                wrong_answer(expected)
            )
        }
    }
}

fn auc_milli(scores: &[(u16, bool)]) -> u64 {
    let positives: Vec<u16> = scores
        .iter()
        .filter_map(|(score, label)| label.then_some(*score))
        .collect();
    let negatives: Vec<u16> = scores
        .iter()
        .filter_map(|(score, label)| (!label).then_some(*score))
        .collect();
    if positives.is_empty() || negatives.is_empty() {
        return 0;
    }
    let mut wins = 0u64;
    let mut ties = 0u64;
    for p in &positives {
        for n in &negatives {
            if p > n {
                wins += 1;
            } else if p == n {
                ties += 1;
            }
        }
    }
    let total = (positives.len() * negatives.len()) as u64;
    (wins * 1_000 + ties * 500) / total
}

#[derive(Default)]
struct SummaryAcc {
    attempts: u64,
    passes: u64,
    quality_sum_milli: u64,
    total_available_payout_micro_frac: MicroFrac,
    total_earned_payout_micro_frac: MicroFrac,
    total_inference_cost_micro_frac: MicroFrac,
    total_gas_micro_frac: MicroFrac,
    total_profit_micro_frac: MicroFrac,
}

impl SummaryAcc {
    fn push(&mut self, task: &BenchTask, ev: &AttemptEvaluation) {
        self.attempts += 1;
        self.passes += u64::from(ev.passed);
        self.quality_sum_milli += u64::from(ev.quality_score_milli);
        self.total_available_payout_micro_frac += task.payout_micro_frac;
        self.total_earned_payout_micro_frac += ev.payout_earned_micro_frac;
        self.total_inference_cost_micro_frac += ev.inference_cost_micro_frac;
        self.total_gas_micro_frac += ev.gas_micro_frac;
        self.total_profit_micro_frac += ev.profit_micro_frac;
    }

    fn finish(self) -> ProfitSummary {
        let pass_rate_milli = if self.attempts == 0 {
            0
        } else {
            self.passes * 1_000 / self.attempts
        };
        let avg_quality_milli = if self.attempts == 0 {
            0
        } else {
            self.quality_sum_milli / self.attempts
        };
        let avg_profit_micro_frac = if self.attempts == 0 {
            0
        } else {
            self.total_profit_micro_frac / self.attempts as MicroFrac
        };
        let profit_margin_milli = if self.total_available_payout_micro_frac == 0 {
            0
        } else {
            self.total_profit_micro_frac * 1_000 / self.total_available_payout_micro_frac
        };
        let break_even_payout_per_pass_micro_frac = if self.passes == 0 {
            None
        } else {
            Some(
                (self.total_inference_cost_micro_frac + self.total_gas_micro_frac)
                    / self.passes as MicroFrac,
            )
        };
        ProfitSummary {
            attempts: self.attempts,
            passes: self.passes,
            pass_rate_milli,
            avg_quality_milli,
            total_available_payout_micro_frac: self.total_available_payout_micro_frac,
            total_earned_payout_micro_frac: self.total_earned_payout_micro_frac,
            total_inference_cost_micro_frac: self.total_inference_cost_micro_frac,
            total_gas_micro_frac: self.total_gas_micro_frac,
            total_profit_micro_frac: self.total_profit_micro_frac,
            avg_profit_micro_frac,
            profit_margin_milli,
            break_even_payout_per_pass_micro_frac,
        }
    }
}

struct SyntheticProfile {
    model: &'static str,
    solve_easy: u64,
    solve_medium: u64,
    solve_hard: u64,
    input_tokens: u64,
    output_tokens: u64,
    input_price: MicroFrac,
    output_price: MicroFrac,
}

#[derive(Default)]
struct VerifierAcc {
    judgments: u64,
    positives: u64,
    negatives: u64,
    true_accepts: u64,
    true_rejects: u64,
    false_accepts: u64,
    false_rejects: u64,
    brier_sum_x1m: u64,
    total_leakage_cost_micro_frac: MicroFrac,
    total_churn_cost_micro_frac: MicroFrac,
    total_inference_cost_micro_frac: MicroFrac,
}

impl VerifierAcc {
    fn push(&mut self, ev: &VerifierEvaluation) {
        self.judgments += 1;
        self.positives += u64::from(ev.should_accept);
        self.negatives += u64::from(!ev.should_accept);
        self.true_accepts += u64::from(ev.accepted && ev.should_accept);
        self.true_rejects += u64::from(!ev.accepted && !ev.should_accept);
        self.false_accepts += u64::from(ev.false_accept);
        self.false_rejects += u64::from(ev.false_reject);
        self.brier_sum_x1m += ev.brier_x1m;
        self.total_leakage_cost_micro_frac += ev.leakage_cost_micro_frac;
        self.total_churn_cost_micro_frac += ev.churn_cost_micro_frac;
        self.total_inference_cost_micro_frac += ev.inference_cost_micro_frac;
    }

    fn finish(self, auc_milli: u64) -> VerifierSummary {
        let false_accept_rate_milli = rate_milli(self.false_accepts, self.negatives);
        let false_reject_rate_milli = rate_milli(self.false_rejects, self.positives);
        let accuracy_milli = rate_milli(self.true_accepts + self.true_rejects, self.judgments);
        let brier_milli = if self.judgments == 0 {
            0
        } else {
            (self.brier_sum_x1m / self.judgments) / 1_000
        };
        VerifierSummary {
            judgments: self.judgments,
            positives: self.positives,
            negatives: self.negatives,
            true_accepts: self.true_accepts,
            true_rejects: self.true_rejects,
            false_accepts: self.false_accepts,
            false_rejects: self.false_rejects,
            false_accept_rate_milli,
            false_reject_rate_milli,
            accuracy_milli,
            auc_milli,
            brier_milli,
            total_leakage_cost_micro_frac: self.total_leakage_cost_micro_frac,
            total_churn_cost_micro_frac: self.total_churn_cost_micro_frac,
            total_inference_cost_micro_frac: self.total_inference_cost_micro_frac,
        }
    }
}

fn rate_milli(numerator: u64, denominator: u64) -> u64 {
    if denominator == 0 {
        0
    } else {
        numerator * 1_000 / denominator
    }
}

struct VerifierProfile {
    verifier: &'static str,
    false_accept_pct: u64,
    false_reject_pct: u64,
    confidence_correct: u16,
    confidence_wrong: u16,
    input_tokens: u64,
    output_tokens: u64,
    input_price: MicroFrac,
    output_price: MicroFrac,
}

struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }
}
