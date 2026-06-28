use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Instant;

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

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnedObjectCertificateThroughputReport {
    pub certificate_count: usize,
    pub validator_count: usize,
    pub quorum_threshold: usize,
    pub signatures_per_certificate: usize,
    pub total_signatures: usize,
    pub elapsed_nanos: u128,
    pub certificates_per_second: f64,
    pub signatures_per_second: f64,
    pub verified_certificates: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DaSamplingBandwidthReport {
    pub payload_bytes: usize,
    pub encoded_bytes: u64,
    pub share_size: u32,
    pub share_count: usize,
    pub sample_count_per_round: usize,
    pub rounds: usize,
    pub sampled_bytes: u64,
    pub elapsed_nanos: u128,
    pub sampled_bytes_per_second: f64,
    pub sampled_mib_per_second: f64,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProofLatencyCostReport {
    pub proof_count: usize,
    pub covered_blocks_per_proof: u64,
    pub proof_bytes: usize,
    pub elapsed_nanos: u128,
    pub avg_verify_latency_micros: f64,
    pub proofs_per_second: f64,
    pub verified_proofs: usize,
    pub prover_cost_micro_frac_per_block: MicroFrac,
    pub estimated_cost_per_proof_micro_frac: MicroFrac,
    pub estimated_total_prover_cost_micro_frac: MicroFrac,
    pub proof_verify_fee_per_proof: u128,
    pub estimated_total_proof_verify_fee: u128,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MixedProofSloBenchReport {
    pub iterations: usize,
    pub tx_count: usize,
    pub proof_bytes: usize,
    pub witness_gen_latency_nanos: u128,
    pub native_component_latency_nanos: u128,
    pub evm_zkvm_fixture_latency_nanos: u128,
    pub aggregation_latency_nanos: u128,
    pub verification_latency_nanos: u128,
    pub avg_total_latency_micros: f64,
    pub verified_proofs: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeePolicyBenchReport {
    pub cost_categories: Vec<String>,
    pub da_fee_per_byte: u128,
    pub proof_verify_base_fee: u128,
    pub proof_verify_fee_per_byte: u128,
    pub shared_state_gas_price: u128,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolBenchReport {
    pub fee_policy: FeePolicyBenchReport,
    pub owned_object_certificates: OwnedObjectCertificateThroughputReport,
    pub da_sampling: DaSamplingBandwidthReport,
    pub proof_latency_cost: ProofLatencyCostReport,
    pub mixed_proof_slo: MixedProofSloBenchReport,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum BaselineScenarioKind {
    NativeNoOp,
    OwnedObjectTx,
    ProofCommitment,
    MixedEvmNative,
    Bft7ValidatorLab,
    ProofUpdates,
    CertificateUpdates,
    MixedProofSharedState,
    DaSamplingProofUpdates,
    Bft7ProofIngestion,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BaselineBenchConfig {
    pub blocks_per_scenario: usize,
    pub txs_per_block: usize,
    pub chain_id: u64,
    pub gas_limit: u64,
    pub seed: u64,
}

impl Default for BaselineBenchConfig {
    fn default() -> Self {
        Self {
            blocks_per_scenario: 16,
            txs_per_block: 64,
            chain_id: 41,
            gas_limit: 60_000_000,
            seed: 41,
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BftLabMetrics {
    pub validator_count: usize,
    pub quorum_threshold: usize,
    pub formed_qcs: usize,
    pub votes_recorded: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BaselineScenarioReport {
    pub name: String,
    pub kind: BaselineScenarioKind,
    pub blocks: usize,
    pub submitted_txs: usize,
    pub committed_txs: usize,
    pub elapsed_nanos: u128,
    pub submitted_tx_per_second: f64,
    pub committed_tx_per_second: f64,
    pub block_p50_latency_nanos: u128,
    pub block_p95_latency_nanos: u128,
    pub cpu_nanos: u128,
    pub peak_working_set_bytes: u64,
    pub total_block_bytes: u64,
    pub avg_block_bytes: f64,
    pub total_da_bytes: u64,
    pub avg_da_bytes: f64,
    pub replay_time_nanos: u128,
    pub replay_tx_per_second: f64,
    pub accepted_proof_updates: usize,
    pub accepted_certificate_updates: usize,
    pub accepted_proof_updates_per_second: f64,
    pub accepted_certificate_updates_per_second: f64,
    pub proof_verify_time_nanos: u128,
    pub da_sampling_time_nanos: u128,
    pub total_payload_bytes: u64,
    pub avg_payload_bytes: f64,
    pub bft: BftLabMetrics,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BaselineBenchReport {
    pub schema_version: u16,
    pub run_kind: String,
    pub config: BaselineBenchConfig,
    pub scenarios: Vec<BaselineScenarioReport>,
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

pub fn run_owned_object_certificate_throughput_bench(
    certificate_count: usize,
    validator_count: usize,
    quorum_threshold: usize,
) -> OwnedObjectCertificateThroughputReport {
    use fractal_core::{
        NativeCall, OwnedObjectCertificate, OwnedObjectId, OwnedObjectVersion, Transaction, TxBody,
        VmKind,
    };
    use fractal_crypto::BlsSecretKey;

    let validator_count = validator_count.max(1);
    let quorum_threshold = quorum_threshold.clamp(1, validator_count);
    let validators = (0..validator_count)
        .map(|i| BlsSecretKey::from_ikm(&[(i as u8).wrapping_add(1); 32]).unwrap())
        .collect::<Vec<_>>();
    let pubkeys = validators
        .iter()
        .map(BlsSecretKey::public_key)
        .collect::<Vec<_>>();
    let owner = [0xA7; 20];
    let started = Instant::now();
    let mut verified_certificates = 0usize;
    for i in 0..certificate_count {
        let tx = Transaction {
            signer: owner,
            nonce: i as u64,
            vm: VmKind::Native,
            body: TxBody::Native(NativeCall::UpdateAgent {
                agent_id: i as u64,
                new_metadata_uri: format!("bench://agent/{i}"),
                new_pubkey: None,
            }),
        };
        let object_versions = vec![
            OwnedObjectVersion {
                object_id: OwnedObjectId::AccountNonce(owner),
                version: i as u64,
            },
            OwnedObjectVersion {
                object_id: OwnedObjectId::Agent(i as u64),
                version: 0,
            },
        ];
        let unsigned = OwnedObjectCertificate::from_owned_transaction(
            &tx,
            object_versions.clone(),
            Vec::new(),
        )
        .unwrap();
        let sign_body = unsigned.sign_body();
        let signatures = validators
            .iter()
            .take(quorum_threshold)
            .enumerate()
            .map(|(idx, sk)| {
                OwnedObjectCertificate::countersign(&sign_body, idx as u32, sk).unwrap()
            })
            .collect::<Vec<_>>();
        let cert =
            OwnedObjectCertificate::aggregate(&tx, object_versions, signatures, quorum_threshold)
                .unwrap();
        cert.verify(&pubkeys, quorum_threshold).unwrap();
        verified_certificates += 1;
    }
    let elapsed_nanos = started.elapsed().as_nanos().max(1);
    let seconds = elapsed_nanos as f64 / 1_000_000_000.0;
    let total_signatures = certificate_count.saturating_mul(quorum_threshold);
    OwnedObjectCertificateThroughputReport {
        certificate_count,
        validator_count,
        quorum_threshold,
        signatures_per_certificate: quorum_threshold,
        total_signatures,
        elapsed_nanos,
        certificates_per_second: certificate_count as f64 / seconds,
        signatures_per_second: total_signatures as f64 / seconds,
        verified_certificates,
    }
}

pub fn run_da_sampling_bandwidth_bench(
    payload_bytes: usize,
    share_size: u32,
    sample_count_per_round: usize,
    rounds: usize,
    seed: u64,
) -> DaSamplingBandwidthReport {
    let payload = deterministic_payload(payload_bytes, seed);
    let sidecar = fractal_consensus::build_da_sidecar(
        &payload,
        fractal_consensus::DEFAULT_DA_NAMESPACE,
        share_size,
    )
    .expect("DA sidecar benchmark fixture");
    let root = fractal_consensus::da_root(&sidecar);
    let started = Instant::now();
    for round in 0..rounds {
        fractal_consensus::verify_da_samples(
            &sidecar,
            root,
            fractal_consensus::DEFAULT_DA_NAMESPACE,
            seed.wrapping_add(round as u64),
            sample_count_per_round,
        )
        .unwrap();
    }
    let elapsed_nanos = started.elapsed().as_nanos().max(1);
    let sampled_bytes =
        sample_count_per_round as u64 * rounds as u64 * u64::from(sidecar.share_size);
    let seconds = elapsed_nanos as f64 / 1_000_000_000.0;
    let sampled_bytes_per_second = sampled_bytes as f64 / seconds;
    DaSamplingBandwidthReport {
        payload_bytes,
        encoded_bytes: fractal_consensus::da_encoded_bytes(&sidecar),
        share_size: sidecar.share_size,
        share_count: sidecar.shares.len(),
        sample_count_per_round,
        rounds,
        sampled_bytes,
        elapsed_nanos,
        sampled_bytes_per_second,
        sampled_mib_per_second: sampled_bytes_per_second / (1024.0 * 1024.0),
    }
}

pub fn run_proof_latency_cost_bench(
    proof_count: usize,
    covered_blocks_per_proof: u64,
    prover_cost_micro_frac_per_block: MicroFrac,
    seed: u64,
) -> ProofLatencyCostReport {
    use fractal_consensus::{
        coverage_manifest_digest, coverage_manifest_for_circuit_version, eth_signed_raws_for_txs,
        execute_and_build_block, header_hash, mixed_execution_witness_from_replay,
        native_recursive_proof_envelope_v1, native_state_transition_statement_v1,
        verify_block_validity_proof, BlockValidityProof, CircuitVersion, ValidityProofSystem,
    };
    use fractal_core::{Account, NativeCall, State, Transaction, TxBody, VmKind};

    let signer = [seed as u8; 20];
    let mut state = State::default();
    state.accounts.insert(
        signer,
        Account {
            nonce: 0,
            balance: 1_000_000,
        },
    );
    let tx = Transaction {
        signer,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let pre_state = state.clone();
    let block = execute_and_build_block(
        41,
        1,
        0,
        [seed as u8; 32],
        [0u8; 32],
        [7u8; 32],
        1_000,
        60_000_000,
        &mut state,
        vec![tx],
        eth_signed_raws_for_txs(1),
    )
    .unwrap();
    let mut proof = BlockValidityProof {
        chain_id: block.header.chain_id,
        height: block.header.height,
        block_hash: header_hash(&block.header).unwrap(),
        timestamp_ms: block.header.timestamp_ms,
        parent_state_root: block.header.parent_state_root,
        state_root: block.header.state_root,
        tx_root: block.header.tx_root,
        receipt_root: block.header.receipt_root,
        native_event_root: block.header.native_event_root,
        evm_log_root: block.header.evm_log_root,
        gas_used: block.header.gas_used,
        zone_namespace: block.header.zone_namespace,
        da_root: block.header.da_root,
        circuit_version: CircuitVersion::NativeStateTransitionV1,
        coverage_manifest_digest: coverage_manifest_digest(&coverage_manifest_for_circuit_version(
            CircuitVersion::NativeStateTransitionV1,
        ))
        .unwrap(),
        feature_set: block.header.feature_set,
        proof_system: ValidityProofSystem::StwoPlonky2,
        proof_bytes: Vec::new(),
    };
    let mut witness = mixed_execution_witness_from_replay(&block, &pre_state).unwrap();
    witness.public_inputs.circuit_version = CircuitVersion::NativeStateTransitionV1;
    witness.public_inputs.coverage_manifest_digest = coverage_manifest_digest(
        &coverage_manifest_for_circuit_version(CircuitVersion::NativeStateTransitionV1),
    )
    .unwrap();
    let statement = native_state_transition_statement_v1(&witness).unwrap();
    proof.proof_bytes =
        borsh::to_vec(&native_recursive_proof_envelope_v1(statement, &proof, [0x44; 32]).unwrap())
            .unwrap();

    let started = Instant::now();
    let mut verified_proofs = 0usize;
    for _ in 0..proof_count {
        verify_block_validity_proof(&block, &proof).unwrap();
        verified_proofs += 1;
    }
    let elapsed_nanos = started.elapsed().as_nanos().max(1);
    let seconds = elapsed_nanos as f64 / 1_000_000_000.0;
    let estimated_cost_per_proof_micro_frac =
        (covered_blocks_per_proof as MicroFrac).saturating_mul(prover_cost_micro_frac_per_block);
    let estimated_total_prover_cost_micro_frac =
        (proof_count as MicroFrac).saturating_mul(estimated_cost_per_proof_micro_frac);
    let fee_policy = fractal_consensus::default_fee_policy();
    let proof_verify_fee_per_proof = fee_policy.proof_verify_fee(proof.proof_bytes.len());
    let estimated_total_proof_verify_fee =
        (proof_count as u128).saturating_mul(proof_verify_fee_per_proof);

    ProofLatencyCostReport {
        proof_count,
        covered_blocks_per_proof,
        proof_bytes: proof.proof_bytes.len(),
        elapsed_nanos,
        avg_verify_latency_micros: elapsed_nanos as f64 / proof_count.max(1) as f64 / 1_000.0,
        proofs_per_second: proof_count as f64 / seconds,
        verified_proofs,
        prover_cost_micro_frac_per_block,
        estimated_cost_per_proof_micro_frac,
        estimated_total_prover_cost_micro_frac,
        proof_verify_fee_per_proof,
        estimated_total_proof_verify_fee,
    }
}

pub fn run_mixed_proof_slo_bench(iterations: usize, seed: u64) -> MixedProofSloBenchReport {
    use fractal_consensus::{
        coverage_manifest_digest, coverage_manifest_for_circuit_version, eth_signed_raws_for_txs,
        evm_zkvm_proof_fixture_v1, execute_and_build_block, header_hash,
        mixed_execution_witness_from_replay, mixed_intrablock_aggregate_proof_envelope_v1,
        native_mixed_component_statement_v1, verify_block_validity_proof, BlockValidityProof,
        CircuitVersion, ValidityProofSystem,
    };
    use fractal_core::{Account, NativeCall, State, Transaction, TxBody, VmKind};

    let native_signer = [seed as u8; 20];
    let evm_signer = [seed.wrapping_add(1) as u8; 20];
    let mut pre_state = State::default();
    pre_state.accounts.insert(
        native_signer,
        Account {
            nonce: 0,
            balance: 1_000_000,
        },
    );
    pre_state.accounts.insert(
        evm_signer,
        Account {
            nonce: 0,
            balance: 1_000_000,
        },
    );
    let native_tx = Transaction {
        signer: native_signer,
        nonce: 0,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::NoOp),
    };
    let evm_tx = Transaction {
        signer: evm_signer,
        nonce: 0,
        vm: VmKind::Evm,
        body: TxBody::Transfer {
            to: [seed.wrapping_add(2) as u8; 20],
            amount: 10,
        },
    };
    let mut execution_state = pre_state.clone();
    let block = execute_and_build_block(
        41,
        1,
        0,
        [seed as u8; 32],
        [0u8; 32],
        [7u8; 32],
        1_000,
        60_000_000,
        &mut execution_state,
        vec![native_tx, evm_tx],
        eth_signed_raws_for_txs(2),
    )
    .unwrap();

    let iterations = iterations.max(1);
    let mut witness_gen_latency_nanos = 0u128;
    let mut native_component_latency_nanos = 0u128;
    let mut evm_zkvm_fixture_latency_nanos = 0u128;
    let mut aggregation_latency_nanos = 0u128;
    let mut verification_latency_nanos = 0u128;
    let mut verified_proofs = 0usize;
    let mut proof_bytes = 0usize;

    for _ in 0..iterations {
        let started = Instant::now();
        let mut witness = mixed_execution_witness_from_replay(&block, &pre_state).unwrap();
        witness.public_inputs.circuit_version = CircuitVersion::MixedStateTransitionV1;
        witness.public_inputs.coverage_manifest_digest = coverage_manifest_digest(
            &coverage_manifest_for_circuit_version(CircuitVersion::MixedStateTransitionV1),
        )
        .unwrap();
        witness_gen_latency_nanos += started.elapsed().as_nanos();

        let started = Instant::now();
        let _native = native_mixed_component_statement_v1(&witness).unwrap();
        native_component_latency_nanos += started.elapsed().as_nanos();

        let started = Instant::now();
        let _evm = evm_zkvm_proof_fixture_v1(&witness).unwrap();
        evm_zkvm_fixture_latency_nanos += started.elapsed().as_nanos();

        let started = Instant::now();
        let envelope = mixed_intrablock_aggregate_proof_envelope_v1(&witness).unwrap();
        let proof_bytes_vec = borsh::to_vec(&envelope).unwrap();
        aggregation_latency_nanos += started.elapsed().as_nanos();
        proof_bytes = proof_bytes_vec.len();

        let proof = BlockValidityProof {
            chain_id: block.header.chain_id,
            height: block.header.height,
            block_hash: header_hash(&block.header).unwrap(),
            timestamp_ms: block.header.timestamp_ms,
            parent_state_root: block.header.parent_state_root,
            state_root: block.header.state_root,
            tx_root: block.header.tx_root,
            receipt_root: block.header.receipt_root,
            native_event_root: block.header.native_event_root,
            evm_log_root: block.header.evm_log_root,
            gas_used: block.header.gas_used,
            zone_namespace: block.header.zone_namespace,
            da_root: block.header.da_root,
            circuit_version: CircuitVersion::MixedStateTransitionV1,
            coverage_manifest_digest: coverage_manifest_digest(
                &coverage_manifest_for_circuit_version(CircuitVersion::MixedStateTransitionV1),
            )
            .unwrap(),
            feature_set: block.header.feature_set,
            proof_system: ValidityProofSystem::StwoPlonky2,
            proof_bytes: proof_bytes_vec,
        };
        let started = Instant::now();
        verify_block_validity_proof(&block, &proof).unwrap();
        verification_latency_nanos += started.elapsed().as_nanos();
        verified_proofs += 1;
    }

    let total_latency_nanos = witness_gen_latency_nanos
        + native_component_latency_nanos
        + evm_zkvm_fixture_latency_nanos
        + aggregation_latency_nanos
        + verification_latency_nanos;
    MixedProofSloBenchReport {
        iterations,
        tx_count: block.transactions.len(),
        proof_bytes,
        witness_gen_latency_nanos,
        native_component_latency_nanos,
        evm_zkvm_fixture_latency_nanos,
        aggregation_latency_nanos,
        verification_latency_nanos,
        avg_total_latency_micros: total_latency_nanos as f64 / iterations as f64 / 1_000.0,
        verified_proofs,
    }
}

pub fn run_protocol_bench(
    certificate_count: usize,
    validator_count: usize,
    quorum_threshold: usize,
    da_payload_bytes: usize,
    da_share_size: u32,
    da_samples: usize,
    da_rounds: usize,
    proof_count: usize,
    covered_blocks_per_proof: u64,
    prover_cost_micro_frac_per_block: MicroFrac,
    seed: u64,
) -> ProtocolBenchReport {
    let fee_policy = fractal_consensus::default_fee_policy();
    ProtocolBenchReport {
        fee_policy: FeePolicyBenchReport {
            cost_categories: fractal_consensus::FeePolicyV1::cost_categories()
                .into_iter()
                .map(|category| category.as_str().to_owned())
                .collect(),
            da_fee_per_byte: fee_policy.da_fee_per_byte,
            proof_verify_base_fee: fee_policy.proof_verify_base_fee,
            proof_verify_fee_per_byte: fee_policy.proof_verify_fee_per_byte,
            shared_state_gas_price: fee_policy.shared_state_gas_price,
        },
        owned_object_certificates: run_owned_object_certificate_throughput_bench(
            certificate_count,
            validator_count,
            quorum_threshold,
        ),
        da_sampling: run_da_sampling_bandwidth_bench(
            da_payload_bytes,
            da_share_size,
            da_samples,
            da_rounds,
            seed,
        ),
        proof_latency_cost: run_proof_latency_cost_bench(
            proof_count,
            covered_blocks_per_proof,
            prover_cost_micro_frac_per_block,
            seed,
        ),
        mixed_proof_slo: run_mixed_proof_slo_bench(proof_count, seed),
    }
}

pub fn run_baseline_bench(config: BaselineBenchConfig) -> BaselineBenchReport {
    let scenarios = [
        ("native-noop", BaselineScenarioKind::NativeNoOp),
        ("owned-object-tx", BaselineScenarioKind::OwnedObjectTx),
        ("proof-commitment", BaselineScenarioKind::ProofCommitment),
        ("mixed-evm-native", BaselineScenarioKind::MixedEvmNative),
        (
            "bft-7-validator-lab",
            BaselineScenarioKind::Bft7ValidatorLab,
        ),
    ]
    .into_iter()
    .enumerate()
    .map(|(idx, (name, kind))| {
        run_baseline_scenario(name, kind, &config, config.seed.wrapping_add(idx as u64))
    })
    .collect();
    BaselineBenchReport {
        schema_version: 1,
        run_kind: "baseline".to_owned(),
        config,
        scenarios,
    }
}

pub fn run_proof_ingestion_bench(config: BaselineBenchConfig) -> BaselineBenchReport {
    let scenarios = [
        ("proof-updates", BaselineScenarioKind::ProofUpdates),
        (
            "certificate-updates",
            BaselineScenarioKind::CertificateUpdates,
        ),
        (
            "mixed-proof-shared-state",
            BaselineScenarioKind::MixedProofSharedState,
        ),
        (
            "da-sampling-proof-updates",
            BaselineScenarioKind::DaSamplingProofUpdates,
        ),
        (
            "bft-7-proof-ingestion",
            BaselineScenarioKind::Bft7ProofIngestion,
        ),
    ]
    .into_iter()
    .enumerate()
    .map(|(idx, (name, kind))| {
        run_proof_ingestion_scenario(name, kind, &config, config.seed.wrapping_add(idx as u64))
    })
    .collect();
    BaselineBenchReport {
        schema_version: 1,
        run_kind: "proof_ingestion".to_owned(),
        config,
        scenarios,
    }
}

fn run_proof_ingestion_scenario(
    name: &str,
    kind: BaselineScenarioKind,
    config: &BaselineBenchConfig,
    seed: u64,
) -> BaselineScenarioReport {
    use fractal_consensus::{
        build_da_sidecar, da_encoded_bytes, da_root, proof_update_leaf_hash, verify_da_samples,
        verify_formed_qc, BlockPayload, BlockPayloadItem, OwnedObjectCertificateBatchV1,
        ValidatorSet, Vote, VotePool, VoteSignBody,
    };

    let blocks = config.blocks_per_scenario.max(1);
    let items_per_block = config.txs_per_block.max(1);
    let bft_validators =
        (kind == BaselineScenarioKind::Bft7ProofIngestion).then(ValidatorSet::phase2_bft7_fixture);
    let mut bft = bft_validators.as_ref().map(|validators| BftLabMetrics {
        validator_count: validators.len(),
        quorum_threshold: validators.quorum_threshold(),
        ..BftLabMetrics::default()
    });

    let mut payloads = Vec::with_capacity(blocks);
    let mut latencies = Vec::with_capacity(blocks);
    let mut accepted_proof_updates = 0usize;
    let mut accepted_certificate_updates = 0usize;
    let mut proof_verify_time_nanos = 0u128;
    let mut da_sampling_time_nanos = 0u128;
    let mut total_payload_bytes = 0u64;
    let mut total_da_bytes = 0u64;
    let mut peak_working_set_bytes = 0u64;
    let started = Instant::now();

    for height in 1..=blocks {
        let block_started = Instant::now();
        let mut block_proof_updates = 0usize;
        let mut block_certificates = 0usize;
        let payload = match kind {
            BaselineScenarioKind::ProofUpdates | BaselineScenarioKind::Bft7ProofIngestion => {
                let updates = proof_updates_for_block(seed, height, items_per_block);
                let verify_started = Instant::now();
                for update in &updates {
                    proof_update_leaf_hash(update).expect("proof update leaf hash");
                }
                proof_verify_time_nanos =
                    proof_verify_time_nanos.saturating_add(verify_started.elapsed().as_nanos());
                block_proof_updates = updates.len();
                BlockPayload::ProofUpdates(updates)
            }
            BaselineScenarioKind::CertificateUpdates => {
                let certificates = certificates_for_block(seed, height, items_per_block);
                block_certificates = certificates.len();
                BlockPayload::CertificateBatches(vec![OwnedObjectCertificateBatchV1 {
                    certificates,
                }])
            }
            BaselineScenarioKind::MixedProofSharedState => {
                let mut items = Vec::with_capacity(items_per_block);
                let proof_count = items_per_block.div_ceil(2);
                let updates = proof_updates_for_block(seed, height, proof_count);
                for idx in 0..items_per_block {
                    if idx % 2 == 0 {
                        let update = updates[idx / 2].clone();
                        let verify_started = Instant::now();
                        proof_update_leaf_hash(&update).expect("proof update leaf hash");
                        proof_verify_time_nanos = proof_verify_time_nanos
                            .saturating_add(verify_started.elapsed().as_nanos());
                        block_proof_updates = block_proof_updates.saturating_add(1);
                        items.push(BlockPayloadItem::ProofUpdate(update));
                    } else {
                        items.push(BlockPayloadItem::Transaction {
                            transaction: shared_state_tx(seed, height, idx),
                            eth_signed_raw: None,
                        });
                    }
                }
                BlockPayload::Mixed(items)
            }
            BaselineScenarioKind::DaSamplingProofUpdates => {
                let updates = proof_updates_for_block(seed, height, items_per_block);
                let verify_started = Instant::now();
                for update in &updates {
                    proof_update_leaf_hash(update).expect("proof update leaf hash");
                }
                proof_verify_time_nanos =
                    proof_verify_time_nanos.saturating_add(verify_started.elapsed().as_nanos());
                block_proof_updates = updates.len();
                BlockPayload::ProofUpdates(updates)
            }
            _ => unreachable!("proof-ingestion scenario kind"),
        };
        let payload_root = payload
            .payload_root()
            .expect("proof-ingestion payload root");
        let payload_bytes = borsh_len(&payload) as u64;
        total_payload_bytes = total_payload_bytes.saturating_add(payload_bytes);
        peak_working_set_bytes = peak_working_set_bytes.max(payload_bytes);
        if kind == BaselineScenarioKind::DaSamplingProofUpdates {
            let payload_raw = borsh::to_vec(&payload).expect("payload borsh");
            let sidecar = build_da_sidecar(
                &payload_raw,
                fractal_consensus::DEFAULT_DA_NAMESPACE,
                fractal_consensus::DEFAULT_DA_SHARE_SIZE,
            )
            .expect("proof-ingestion DA sidecar");
            let root = da_root(&sidecar);
            let sampling_started = Instant::now();
            verify_da_samples(
                &sidecar,
                root,
                fractal_consensus::DEFAULT_DA_NAMESPACE,
                seed.wrapping_add(height as u64),
                8.min(sidecar.shares.len()),
            )
            .expect("proof-ingestion DA samples");
            da_sampling_time_nanos =
                da_sampling_time_nanos.saturating_add(sampling_started.elapsed().as_nanos());
            total_da_bytes = total_da_bytes.saturating_add(da_encoded_bytes(&sidecar));
            peak_working_set_bytes = peak_working_set_bytes
                .max(payload_bytes.saturating_add(da_encoded_bytes(&sidecar)));
        } else {
            total_da_bytes = total_da_bytes.saturating_add(payload_bytes);
        }
        if let Some(validators) = bft_validators.as_ref() {
            let mut pool = VotePool::new();
            for idx in 0..validators.quorum_threshold() {
                let body = VoteSignBody {
                    view: height as u64 - 1,
                    height: height as u64,
                    header_hash: payload_root,
                };
                let secret = validators
                    .dev_bls_secret(idx)
                    .expect("BFT-7 fixture dev secret");
                let vote = Vote::sign(body, idx as u32, &secret);
                pool.record(vote, validators);
                if let Some(metrics) = bft.as_mut() {
                    metrics.votes_recorded = metrics.votes_recorded.saturating_add(1);
                }
            }
            let qc = pool
                .try_form_qc(height as u64 - 1, height as u64, payload_root, validators)
                .expect("proof-ingestion BFT-7 QC");
            verify_formed_qc(&qc, validators).expect("proof-ingestion BFT-7 QC verifies");
            if let Some(metrics) = bft.as_mut() {
                metrics.formed_qcs = metrics.formed_qcs.saturating_add(1);
            }
        }
        accepted_proof_updates = accepted_proof_updates.saturating_add(block_proof_updates);
        accepted_certificate_updates =
            accepted_certificate_updates.saturating_add(block_certificates);
        latencies.push(block_started.elapsed().as_nanos().max(1));
        payloads.push(payload);
    }
    let elapsed_nanos = started.elapsed().as_nanos().max(1);

    let replay_started = Instant::now();
    for payload in &payloads {
        payload
            .payload_root()
            .expect("proof-ingestion replay payload root");
    }
    let replay_time_nanos = replay_started.elapsed().as_nanos().max(1);

    let committed_items = accepted_proof_updates
        .saturating_add(accepted_certificate_updates)
        .saturating_add(
            payloads
                .iter()
                .map(|payload| match payload {
                    BlockPayload::Mixed(items) => items
                        .iter()
                        .filter(|item| matches!(item, BlockPayloadItem::Transaction { .. }))
                        .count(),
                    _ => 0,
                })
                .sum::<usize>(),
        );
    let submitted_items = blocks.saturating_mul(items_per_block);
    let elapsed_secs = elapsed_nanos as f64 / 1_000_000_000.0;
    let replay_secs = replay_time_nanos as f64 / 1_000_000_000.0;
    BaselineScenarioReport {
        name: name.to_owned(),
        kind,
        blocks,
        submitted_txs: submitted_items,
        committed_txs: committed_items,
        elapsed_nanos,
        submitted_tx_per_second: submitted_items as f64 / elapsed_secs,
        committed_tx_per_second: committed_items as f64 / elapsed_secs,
        block_p50_latency_nanos: percentile_nanos(&latencies, 50),
        block_p95_latency_nanos: percentile_nanos(&latencies, 95),
        cpu_nanos: elapsed_nanos,
        peak_working_set_bytes,
        total_block_bytes: total_payload_bytes,
        avg_block_bytes: total_payload_bytes as f64 / blocks as f64,
        total_da_bytes,
        avg_da_bytes: total_da_bytes as f64 / blocks as f64,
        replay_time_nanos,
        replay_tx_per_second: committed_items as f64 / replay_secs,
        accepted_proof_updates,
        accepted_certificate_updates,
        accepted_proof_updates_per_second: accepted_proof_updates as f64 / elapsed_secs,
        accepted_certificate_updates_per_second: accepted_certificate_updates as f64 / elapsed_secs,
        proof_verify_time_nanos,
        da_sampling_time_nanos,
        total_payload_bytes,
        avg_payload_bytes: total_payload_bytes as f64 / blocks as f64,
        bft: bft.unwrap_or_default(),
    }
}

fn proof_updates_for_block(
    seed: u64,
    block_index: usize,
    count: usize,
) -> Vec<fractal_consensus::ZoneProofUpdateV1> {
    (0..count)
        .map(|idx| fractal_consensus::ZoneProofUpdateV1 {
            zone_id: 1 + (idx % 4) as u64,
            height: block_index as u64,
            parent_root: hash_from_seed(seed, block_index as u64, idx as u64),
            new_root: hash_from_seed(seed.wrapping_add(1), block_index as u64, idx as u64),
            tx_root: hash_from_seed(seed.wrapping_add(2), block_index as u64, idx as u64),
            da_root: hash_from_seed(seed.wrapping_add(3), block_index as u64, idx as u64),
            message_root: hash_from_seed(seed.wrapping_add(4), block_index as u64, idx as u64),
            forced_inclusion_root: hash_from_seed(
                seed.wrapping_add(5),
                block_index as u64,
                idx as u64,
            ),
            circuit_version: fractal_consensus::CircuitVersion::NativeStateTransitionV1,
            feature_set: fractal_consensus::ExecutionFeatureSetV1::empty(),
            proof_digest: hash_from_seed(seed.wrapping_add(6), block_index as u64, idx as u64),
        })
        .collect()
}

fn certificates_for_block(
    seed: u64,
    block_index: usize,
    count: usize,
) -> Vec<fractal_core::OwnedObjectCertificate> {
    use fractal_core::{OwnedObjectCertificate, OwnedObjectId, OwnedObjectVersion};

    (0..count)
        .map(|idx| {
            let global_idx = ((block_index - 1) * count + idx) as u64;
            OwnedObjectCertificate {
                tx_hash: hash_from_seed(seed, block_index as u64, idx as u64),
                owner: address_from_seed(seed, 0),
                signer_nonce: global_idx,
                object_versions: vec![OwnedObjectVersion {
                    object_id: OwnedObjectId::Agent(global_idx),
                    version: block_index as u64,
                }],
                signer_indices: vec![0, 1, 2, 3, 4],
                validator_signatures: Vec::new(),
            }
        })
        .collect()
}

fn shared_state_tx(seed: u64, block_index: usize, item_index: usize) -> fractal_core::Transaction {
    use fractal_core::{NativeCall, Transaction, TxBody, VmKind};

    let nonce = ((block_index - 1) * 1_000_000 + item_index) as u64;
    Transaction {
        signer: address_from_seed(seed, 3),
        nonce,
        vm: VmKind::Native,
        body: TxBody::Native(NativeCall::RegisterAgent {
            operator: address_from_seed(seed, 4),
            pubkey: hash_from_seed(seed, block_index as u64, item_index as u64),
            kind: 1,
            metadata_uri: format!("bench://shared/{block_index}/{item_index}"),
        }),
    }
}

fn run_baseline_scenario(
    name: &str,
    kind: BaselineScenarioKind,
    config: &BaselineBenchConfig,
    seed: u64,
) -> BaselineScenarioReport {
    use fractal_consensus::{
        eth_signed_raws_for_txs, execute_and_build_block, header_hash, verify_formed_qc,
        ValidatorSet, Vote, VotePool, VoteSignBody,
    };
    use fractal_core::{Account, AgentRecord, State};

    let blocks = config.blocks_per_scenario.max(1);
    let txs_per_block = config.txs_per_block.max(1);
    let signer = address_from_seed(seed, 0);
    let evm_signer = address_from_seed(seed, 1);
    let mut state = State::default();
    state.accounts.insert(
        signer,
        Account {
            nonce: 0,
            balance: 10_000_000_000,
        },
    );
    state.accounts.insert(
        evm_signer,
        Account {
            nonce: 0,
            balance: 10_000_000_000,
        },
    );
    if kind == BaselineScenarioKind::OwnedObjectTx {
        state.agents.insert(
            1,
            AgentRecord {
                agent_id: 1,
                address: signer,
                operator: signer,
                pubkey: [0xA7; 32],
                kind: 1,
                metadata_uri: "bench://agent/1".to_owned(),
                reputation_score: 0,
                completed_jobs: 0,
                status: 0,
                registered_at: 0,
                schema_version: 1,
            },
        );
        state.address_to_agent.insert(signer, 1);
        state.next_agent_id = 2;
    }
    let initial_state = state.clone();
    let bft_validators =
        (kind == BaselineScenarioKind::Bft7ValidatorLab).then(ValidatorSet::phase2_bft7_fixture);
    let mut bft = bft_validators.as_ref().map(|validators| BftLabMetrics {
        validator_count: validators.len(),
        quorum_threshold: validators.quorum_threshold(),
        ..BftLabMetrics::default()
    });

    let mut blocks_out = Vec::with_capacity(blocks);
    let mut latencies = Vec::with_capacity(blocks);
    let mut total_block_bytes = 0u64;
    let mut total_da_bytes = 0u64;
    let mut peak_working_set_bytes = borsh_len(&state) as u64;
    let started = Instant::now();
    let mut parent_hash = [0u8; 32];
    for height in 1..=blocks {
        let txs = baseline_txs(kind, seed, height, txs_per_block);
        let block_started = Instant::now();
        let block = execute_and_build_block(
            config.chain_id,
            height as u64,
            height as u64 - 1,
            parent_hash,
            [0u8; 32],
            [0xB7; 32],
            1_000 + height as u64,
            config.gas_limit,
            &mut state,
            txs,
            eth_signed_raws_for_txs(txs_per_block),
        )
        .expect("baseline block execution");
        if let Some(validators) = bft_validators.as_ref() {
            let block_hash = header_hash(&block.header).expect("baseline header hash");
            let mut pool = VotePool::new();
            for idx in 0..validators.quorum_threshold() {
                let body = VoteSignBody {
                    view: block.header.view,
                    height: block.header.height,
                    header_hash: block_hash,
                };
                let secret = validators
                    .dev_bls_secret(idx)
                    .expect("BFT-7 fixture dev secret");
                let vote = Vote::sign(body, idx as u32, &secret);
                pool.record(vote, validators);
                if let Some(metrics) = bft.as_mut() {
                    metrics.votes_recorded = metrics.votes_recorded.saturating_add(1);
                }
            }
            let qc = pool
                .try_form_qc(
                    block.header.view,
                    block.header.height,
                    block_hash,
                    validators,
                )
                .expect("BFT-7 quorum certificate");
            verify_formed_qc(&qc, validators).expect("BFT-7 QC verifies");
            if let Some(metrics) = bft.as_mut() {
                metrics.formed_qcs = metrics.formed_qcs.saturating_add(1);
            }
        }
        let block_latency = block_started.elapsed().as_nanos().max(1);
        parent_hash = header_hash(&block.header).expect("baseline parent hash");
        total_da_bytes = total_da_bytes.saturating_add(block.header.da_bytes);
        let block_bytes = borsh_len(&block) as u64;
        total_block_bytes = total_block_bytes.saturating_add(block_bytes);
        peak_working_set_bytes =
            peak_working_set_bytes.max(block_bytes.saturating_add(borsh_len(&state) as u64));
        latencies.push(block_latency);
        blocks_out.push(block);
    }
    let elapsed_nanos = started.elapsed().as_nanos().max(1);

    let replay_started = Instant::now();
    let mut replay_state = initial_state;
    for block in &blocks_out {
        let mut evm = fractal_evm::RevmEngine::default();
        fractal_core::apply_block_with_evm(&mut replay_state, &block.transactions, &mut evm)
            .expect("baseline validator replay");
    }
    let replay_time_nanos = replay_started.elapsed().as_nanos().max(1);

    let committed_txs = blocks_out
        .iter()
        .map(|block| block.transactions.len())
        .sum::<usize>();
    let submitted_txs = blocks.saturating_mul(txs_per_block);
    let elapsed_secs = elapsed_nanos as f64 / 1_000_000_000.0;
    let replay_secs = replay_time_nanos as f64 / 1_000_000_000.0;
    BaselineScenarioReport {
        name: name.to_owned(),
        kind,
        blocks,
        submitted_txs,
        committed_txs,
        elapsed_nanos,
        submitted_tx_per_second: submitted_txs as f64 / elapsed_secs,
        committed_tx_per_second: committed_txs as f64 / elapsed_secs,
        block_p50_latency_nanos: percentile_nanos(&latencies, 50),
        block_p95_latency_nanos: percentile_nanos(&latencies, 95),
        cpu_nanos: elapsed_nanos,
        peak_working_set_bytes,
        total_block_bytes,
        avg_block_bytes: total_block_bytes as f64 / blocks as f64,
        total_da_bytes,
        avg_da_bytes: total_da_bytes as f64 / blocks as f64,
        replay_time_nanos,
        replay_tx_per_second: committed_txs as f64 / replay_secs,
        accepted_proof_updates: 0,
        accepted_certificate_updates: 0,
        accepted_proof_updates_per_second: 0.0,
        accepted_certificate_updates_per_second: 0.0,
        proof_verify_time_nanos: 0,
        da_sampling_time_nanos: 0,
        total_payload_bytes: total_block_bytes,
        avg_payload_bytes: total_block_bytes as f64 / blocks as f64,
        bft: bft.unwrap_or_default(),
    }
}

fn baseline_txs(
    kind: BaselineScenarioKind,
    seed: u64,
    block_index: usize,
    txs_per_block: usize,
) -> Vec<fractal_core::Transaction> {
    use fractal_core::{NativeCall, Transaction, TxBody, VmKind};

    let base = ((block_index - 1) * txs_per_block) as u64;
    let native_per_mixed_block = txs_per_block.div_ceil(2) as u64;
    let evm_per_mixed_block = (txs_per_block / 2) as u64;
    let signer = address_from_seed(seed, 0);
    let evm_signer = address_from_seed(seed, 1);
    let mut native_nonce = if kind == BaselineScenarioKind::MixedEvmNative {
        (block_index as u64 - 1).saturating_mul(native_per_mixed_block)
    } else {
        base
    };
    let mut evm_nonce = (block_index as u64 - 1).saturating_mul(evm_per_mixed_block);
    (0..txs_per_block)
        .map(|idx| match kind {
            BaselineScenarioKind::NativeNoOp | BaselineScenarioKind::Bft7ValidatorLab => {
                let tx = Transaction {
                    signer,
                    nonce: base + idx as u64,
                    vm: VmKind::Native,
                    body: TxBody::Native(NativeCall::NoOp),
                };
                tx
            }
            BaselineScenarioKind::OwnedObjectTx => Transaction {
                signer,
                nonce: base + idx as u64,
                vm: VmKind::Native,
                body: TxBody::Native(NativeCall::UpdateAgent {
                    agent_id: 1,
                    new_metadata_uri: format!("bench://agent/1/{}", base + idx as u64),
                    new_pubkey: None,
                }),
            },
            BaselineScenarioKind::ProofCommitment => Transaction {
                signer,
                nonce: base + idx as u64,
                vm: VmKind::Native,
                body: TxBody::Native(NativeCall::ProofCommitmentV1 {
                    proof_hash: hash_from_seed(seed, block_index as u64, idx as u64),
                }),
            },
            BaselineScenarioKind::MixedEvmNative => {
                if idx % 2 == 0 {
                    let nonce = native_nonce;
                    native_nonce = native_nonce.saturating_add(1);
                    Transaction {
                        signer,
                        nonce,
                        vm: VmKind::Native,
                        body: TxBody::Native(NativeCall::NoOp),
                    }
                } else {
                    let nonce = evm_nonce;
                    evm_nonce = evm_nonce.saturating_add(1);
                    Transaction {
                        signer: evm_signer,
                        nonce,
                        vm: VmKind::Evm,
                        body: TxBody::Transfer {
                            to: address_from_seed(seed, 2),
                            amount: 1,
                        },
                    }
                }
            }
            _ => unreachable!("proof-ingestion-only scenario kind"),
        })
        .collect()
}

fn address_from_seed(seed: u64, lane: u8) -> [u8; 20] {
    let mut out = [0u8; 20];
    out[..8].copy_from_slice(&seed.to_be_bytes());
    out[8] = lane;
    out
}

fn hash_from_seed(seed: u64, block_index: u64, tx_index: u64) -> [u8; 32] {
    let mut out = [0u8; 32];
    out[..8].copy_from_slice(&seed.to_be_bytes());
    out[8..16].copy_from_slice(&block_index.to_be_bytes());
    out[16..24].copy_from_slice(&tx_index.to_be_bytes());
    out[24] = 0xC0;
    out
}

fn borsh_len<T: borsh::BorshSerialize>(value: &T) -> usize {
    borsh::to_vec(value).map(|bytes| bytes.len()).unwrap_or(0)
}

fn percentile_nanos(values: &[u128], percentile: u32) -> u128 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let pct = percentile.min(100) as usize;
    let index = ((sorted.len().saturating_sub(1)) * pct + 99) / 100;
    sorted[index.min(sorted.len() - 1)]
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

fn deterministic_payload(len: usize, seed: u64) -> Vec<u8> {
    let mut rng = Lcg::new(seed);
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        out.extend_from_slice(&rng.next().to_le_bytes());
    }
    out.truncate(len);
    out
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
