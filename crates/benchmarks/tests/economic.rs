use fractal_bench::{
    evaluate_attempt, generate_economic_tasks, run_economic_bench, synthetic_attempts, ModelAttempt,
};

#[test]
fn generated_tasks_are_deterministic() {
    let a = generate_economic_tasks(12, 41);
    let b = generate_economic_tasks(12, 41);
    assert_eq!(a.len(), 12);
    assert_eq!(a[0].prompt, b[0].prompt);
    assert_eq!(a[0].expected_answer, b[0].expected_answer);
}

#[test]
fn profit_accounts_for_payout_tokens_and_gas() {
    let task = generate_economic_tasks(1, 41).remove(0);
    let attempt = ModelAttempt {
        task_id: task.task_id.clone(),
        model: "unit".into(),
        output: task.expected_answer.clone(),
        input_tokens: 1_000_000,
        output_tokens: 0,
        input_token_price_micro_frac_per_million: 100,
        output_token_price_micro_frac_per_million: 0,
    };
    let ev = evaluate_attempt(&task, &attempt);
    assert!(ev.passed);
    assert_eq!(ev.inference_cost_micro_frac, 100);
    assert_eq!(
        ev.profit_micro_frac,
        task.payout_micro_frac - 100 - task.gas_micro_frac
    );
}

#[test]
fn report_groups_by_model_and_tier() {
    let tasks = generate_economic_tasks(9, 41);
    let attempts = synthetic_attempts(&tasks);
    let report = run_economic_bench(&tasks, &attempts);
    assert_eq!(report.task_count, 9);
    assert_eq!(report.by_model.len(), 3);
    assert!(report.by_model_tier["cheap-70"].contains_key(&tasks[0].tier));
}
