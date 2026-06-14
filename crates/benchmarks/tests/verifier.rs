use fractal_bench::{
    evaluate_verifier_judgment, generate_verifier_cases, run_verifier_bench,
    synthetic_verifier_judgments, VerifierJudgment,
};

#[test]
fn verifier_cases_include_positive_and_negative_labels() {
    let cases = generate_verifier_cases();
    assert!(cases.iter().any(|c| c.should_accept));
    assert!(cases.iter().any(|c| !c.should_accept));
}

#[test]
fn false_accept_prices_leakage() {
    let case = generate_verifier_cases()
        .into_iter()
        .find(|c| !c.should_accept)
        .expect("negative case");
    let judgment = VerifierJudgment {
        case_id: case.case_id.clone(),
        verifier: "unit".into(),
        accept: true,
        confidence_milli: 900,
        score_milli: 900,
        input_tokens: 1,
        output_tokens: 1,
        input_token_price_micro_frac_per_million: 1,
        output_token_price_micro_frac_per_million: 1,
    };
    let ev = evaluate_verifier_judgment(&case, &judgment);
    assert!(ev.false_accept);
    assert_eq!(ev.leakage_cost_micro_frac, case.payout_micro_frac);
}

#[test]
fn verifier_report_includes_auc_and_brier() {
    let cases = generate_verifier_cases();
    let judgments = synthetic_verifier_judgments(&cases);
    let report = run_verifier_bench(&cases, &judgments);
    let strict = &report.by_verifier["strict-calibrated"];
    assert!(strict.judgments > 0);
    assert!(strict.auc_milli <= 1_000);
    assert!(strict.brier_milli <= 1_000);
}
