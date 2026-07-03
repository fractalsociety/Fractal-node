//! RLVR-055: training report screen.
//!
//! A server-rendered HTML "screen" that explains — in human terms — *why an
//! adapter should or should not be used*. It composes the before (base model)
//! and after (adapter) [`EvalMetricsReport`]s into metric deltas, surfaces the
//! promotion-gate verdict ([`AdapterPromotionDecision`]) and MVP verdict
//! ([`MvpSuccessReport`]), privacy status, behavior examples, and the chain
//! proof status, then renders a single self-contained HTML page.

use serde::{Deserialize, Serialize};

use crate::evals::mvp_success::MvpSuccessReport;
use crate::{AdapterPromotionDecision, EvalMetricsReport, RlvrError};

/// Direction of a metric change from baseline to candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MetricDirection {
    Improved,
    Worsened,
    Unchanged,
}

impl MetricDirection {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Improved => "improved",
            Self::Worsened => "worsened",
            Self::Unchanged => "unchanged",
        }
    }
}

/// One before/after metric comparison.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MetricDelta {
    pub name: String,
    pub baseline_value: f64,
    pub candidate_value: f64,
    pub delta: f64,
    pub higher_is_better: bool,
    pub direction: MetricDirection,
}

/// Promotion-gate verdict summarized for the screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PromotionSummary {
    pub promoted: bool,
    pub passed_checks: usize,
    pub total_checks: usize,
    pub failed_checks: Vec<String>,
    pub rollback_reason: String,
}

/// MVP success verdict summarized for the screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MvpSummary {
    pub overall_passed: bool,
    pub passed_count: usize,
    pub total: usize,
    pub summary: String,
}

/// Privacy status shown on the screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PrivacyStatus {
    pub local_only: bool,
    pub private_data_leakage_rate: f64,
    pub passed: bool,
}

/// Chain proof status (hash-only; never raw trace data).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChainProofStatus {
    pub proof_hash: String,
    pub proof_type: String,
    pub committed: bool,
    pub block_height: Option<u64>,
    pub block_hash: Option<String>,
}

/// A before/after behavior example (better or failure).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BehaviorExample {
    pub trace_id: String,
    pub task_id: String,
    pub summary: String,
    pub candidate_passed: bool,
}

/// The full training report shown on the screen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrainingReport {
    pub adapter_id: String,
    pub base_model_id: String,
    pub metric_deltas: Vec<MetricDelta>,
    pub promotion: Option<PromotionSummary>,
    pub mvp: Option<MvpSummary>,
    pub privacy: PrivacyStatus,
    pub chain_proof: ChainProofStatus,
    pub improved_examples: Vec<BehaviorExample>,
    pub failure_examples: Vec<BehaviorExample>,
    /// Plain-language "use / do not use this adapter because …".
    pub recommendation: String,
}

const EPS: f64 = 1e-9;

/// Build a training report from the before/after metrics and optional verdicts.
///
/// `privacy_local_only` reflects whether the run stayed local-only; the leakage
/// rate is read from the candidate report. `improved_examples` / `failure_examples`
/// are caller-supplied behavior samples (e.g. traces that got better / still fail).
pub fn build_training_report(
    adapter_id: impl Into<String>,
    base_model_id: impl Into<String>,
    baseline: &EvalMetricsReport,
    candidate: &EvalMetricsReport,
    promotion: Option<&AdapterPromotionDecision>,
    mvp: Option<&MvpSuccessReport>,
    improved_examples: Vec<BehaviorExample>,
    failure_examples: Vec<BehaviorExample>,
    chain_proof: ChainProofStatus,
    privacy_local_only: bool,
) -> Result<TrainingReport, RlvrError> {
    baseline.validate()?;
    candidate.validate()?;
    let adapter_id = adapter_id.into();
    let base_model_id = base_model_id.into();
    if adapter_id.trim().is_empty() {
        return Err(RlvrError::Config(
            "training report adapter_id cannot be empty".into(),
        ));
    }
    if base_model_id.trim().is_empty() {
        return Err(RlvrError::Config(
            "training report base_model_id cannot be empty".into(),
        ));
    }
    for ex in improved_examples.iter().chain(failure_examples.iter()) {
        if ex.trace_id.trim().is_empty() || ex.summary.trim().is_empty() {
            return Err(RlvrError::Config(
                "training report example trace_id/summary cannot be empty".into(),
            ));
        }
    }

    let metric_deltas = compute_metric_deltas(baseline, candidate);
    let promotion_summary = promotion.map(|decision| PromotionSummary {
        promoted: decision.promoted,
        passed_checks: decision.checks.iter().filter(|c| c.passed).count(),
        total_checks: decision.checks.len(),
        failed_checks: decision
            .checks
            .iter()
            .filter(|c| !c.passed)
            .map(|c| c.name.clone())
            .collect(),
        rollback_reason: decision.rollback.rollback_reason.clone(),
    });
    let mvp_summary = mvp.map(|report| MvpSummary {
        overall_passed: report.overall_passed,
        passed_count: report.passed_count,
        total: report.checks.len(),
        summary: report.summary.clone(),
    });
    let privacy = PrivacyStatus {
        local_only: privacy_local_only,
        private_data_leakage_rate: candidate.private_data_leakage_rate,
        passed: candidate.private_data_leakage_rate.abs() < EPS,
    };
    let recommendation = build_recommendation(&privacy, &promotion_summary, &mvp_summary);

    Ok(TrainingReport {
        adapter_id,
        base_model_id,
        metric_deltas,
        promotion: promotion_summary,
        mvp: mvp_summary,
        privacy,
        chain_proof,
        improved_examples,
        failure_examples,
        recommendation,
    })
}

fn compute_metric_deltas(
    baseline: &EvalMetricsReport,
    candidate: &EvalMetricsReport,
) -> Vec<MetricDelta> {
    let entries: [(&str, f64, f64, bool); 9] = [
        (
            "Final answer accuracy",
            baseline.final_answer_accuracy,
            candidate.final_answer_accuracy,
            true,
        ),
        (
            "Checkpoint coverage",
            baseline.checkpoint_coverage,
            candidate.checkpoint_coverage,
            true,
        ),
        (
            "Correct route rate",
            baseline.correct_route_rate,
            candidate.correct_route_rate,
            true,
        ),
        (
            "Redundant question rate",
            baseline.redundant_question_rate,
            candidate.redundant_question_rate,
            false,
        ),
        (
            "Premature answer rate",
            baseline.premature_answer_rate,
            candidate.premature_answer_rate,
            false,
        ),
        (
            "Unnecessary escalation rate",
            baseline.unnecessary_escalation_rate,
            candidate.unnecessary_escalation_rate,
            false,
        ),
        (
            "Private-data leakage rate",
            baseline.private_data_leakage_rate,
            candidate.private_data_leakage_rate,
            false,
        ),
        (
            "Average cost",
            baseline.average_cost,
            candidate.average_cost,
            false,
        ),
        (
            "Average latency (ms)",
            baseline.average_latency_ms,
            candidate.average_latency_ms,
            false,
        ),
    ];
    entries
        .iter()
        .map(|(name, b, c, higher_is_better)| {
            let delta = c - b;
            let direction = if delta.abs() < EPS {
                MetricDirection::Unchanged
            } else if *higher_is_better {
                if delta > 0.0 {
                    MetricDirection::Improved
                } else {
                    MetricDirection::Worsened
                }
            } else if delta < 0.0 {
                MetricDirection::Improved
            } else {
                MetricDirection::Worsened
            };
            MetricDelta {
                name: (*name).to_string(),
                baseline_value: *b,
                candidate_value: *c,
                delta,
                higher_is_better: *higher_is_better,
                direction,
            }
        })
        .collect()
}

fn build_recommendation(
    privacy: &PrivacyStatus,
    promotion: &Option<PromotionSummary>,
    mvp: &Option<MvpSummary>,
) -> String {
    if !privacy.passed {
        return format!(
            "Do NOT use this adapter: private-data leakage detected (rate {:.3}).",
            privacy.private_data_leakage_rate
        );
    }
    match (promotion, mvp) {
        (Some(p), Some(m)) if p.promoted && m.overall_passed => {
            "USE this adapter: it passed the promotion gate and all MVP targets.".into()
        }
        (Some(p), _) if p.promoted => {
            "USE this adapter (promotion passed); note: not all MVP targets are met yet.".into()
        }
        (Some(p), _) => {
            let failed = if p.failed_checks.is_empty() {
                String::new()
            } else {
                format!(" ({})", p.failed_checks.join(", "))
            };
            format!("Do NOT use this adapter yet: promotion blocked by failed checks{failed}.")
        }
        (None, _) => "Promotion gate not run yet — review the metrics before deciding.".into(),
    }
}

/// Render the report as a single self-contained HTML page (the "screen").
pub fn render_training_report_html(report: &TrainingReport) -> String {
    let mut html = String::new();
    html.push_str("<!doctype html><html><head><meta charset=\"utf-8\">");
    html.push_str("<title>RLVR Training Report</title>");
    html.push_str(concat!(
        "<style>body{font-family:system-ui,sans-serif;margin:32px;color:#111;max-width:980px}",
        "h1,h2{margin-top:28px}table{border-collapse:collapse;width:100%;margin-top:12px}",
        "th,td{border:1px solid #ddd;padding:8px;text-align:left}th{background:#f5f5f5}",
        ".badge{padding:2px 8px;border-radius:10px;font-size:12px;color:#fff}",
        ".improved{background:#1a7f37}.worsened{background:#cf222e}.unchanged{background:#6e7781}",
        ".rec{padding:16px;border-radius:8px;font-size:18px;font-weight:600}",
        ".rec-use{background:#dafbe1;color:#1a7f37}.rec-block{background:#ffebe9;color:#cf222e}",
        ".rec-review{background:#fff8c5;color:#9a6700}ul{line-height:1.6}</style></head><body>",
    ));
    html.push_str(&format!("<h1>RLVR Training Report</h1>"));
    html.push_str(&format!(
        "<p><strong>Adapter:</strong> {} &middot; <strong>Base model:</strong> {}</p>",
        escape_html(&report.adapter_id),
        escape_html(&report.base_model_id),
    ));

    let rec_class = if report.recommendation.starts_with("USE") {
        "rec-use"
    } else if report.recommendation.starts_with("Do NOT") {
        "rec-block"
    } else {
        "rec-review"
    };
    html.push_str(&format!(
        "<div class=\"rec {}\">{}</div>",
        rec_class,
        escape_html(&report.recommendation)
    ));

    // Before vs after.
    html.push_str(
        "<h2>Before vs After</h2><table><thead><tr><th>Metric</th>\
<th>Baseline</th><th>Candidate</th><th>Delta</th><th>Direction</th></tr></thead><tbody>",
    );
    for delta in &report.metric_deltas {
        html.push_str(&format!(
            "<tr><td>{}</td><td>{:.4}</td><td>{:.4}</td><td>{:+.4}</td>\
<td><span class=\"badge {}\">{}</span></td></tr>",
            escape_html(&delta.name),
            delta.baseline_value,
            delta.candidate_value,
            delta.delta,
            delta.direction.as_str(),
            delta.direction.as_str(),
        ));
    }
    html.push_str("</tbody></table>");

    // What improved / what got worse.
    let improved: Vec<&MetricDelta> = report
        .metric_deltas
        .iter()
        .filter(|d| d.direction == MetricDirection::Improved)
        .collect();
    let worsened: Vec<&MetricDelta> = report
        .metric_deltas
        .iter()
        .filter(|d| d.direction == MetricDirection::Worsened)
        .collect();
    html.push_str("<h2>What improved</h2><ul>");
    if improved.is_empty() {
        html.push_str("<li>No metrics improved.</li>");
    } else {
        for d in improved {
            html.push_str(&format!(
                "<li>{}: {:+.4}</li>",
                escape_html(&d.name),
                d.delta
            ));
        }
    }
    html.push_str("</ul><h2>What got worse</h2><ul>");
    if worsened.is_empty() {
        html.push_str("<li>No metrics worsened.</li>");
    } else {
        for d in worsened {
            html.push_str(&format!(
                "<li>{}: {:+.4}</li>",
                escape_html(&d.name),
                d.delta
            ));
        }
    }
    html.push_str("</ul>");

    // Behavior examples.
    html.push_str("<h2>Better behavior examples</h2><ul>");
    if report.improved_examples.is_empty() {
        html.push_str("<li>No improved examples recorded.</li>");
    } else {
        for ex in &report.improved_examples {
            html.push_str(&format!(
                "<li><strong>{}</strong> ({}): {}</li>",
                escape_html(&ex.trace_id),
                escape_html(&ex.task_id),
                escape_html(&ex.summary)
            ));
        }
    }
    html.push_str("</ul><h2>Failure examples</h2><ul>");
    if report.failure_examples.is_empty() {
        html.push_str("<li>No failure examples recorded.</li>");
    } else {
        for ex in &report.failure_examples {
            html.push_str(&format!(
                "<li><strong>{}</strong> ({}): {}</li>",
                escape_html(&ex.trace_id),
                escape_html(&ex.task_id),
                escape_html(&ex.summary)
            ));
        }
    }
    html.push_str("</ul>");

    // Privacy status.
    let privacy_label = if report.privacy.passed {
        "PASS"
    } else {
        "FAIL"
    };
    let privacy_class = if report.privacy.passed {
        "improved"
    } else {
        "worsened"
    };
    html.push_str(&format!(
        "<h2>Privacy status</h2><p>Local-only: {} &middot; Private-data leakage rate: {:.4} \
&middot; <span class=\"badge {}\">{}</span></p>",
        report.privacy.local_only,
        report.privacy.private_data_leakage_rate,
        privacy_class,
        privacy_label
    ));

    // Promotion result.
    html.push_str("<h2>Promotion result</h2>");
    if let Some(p) = &report.promotion {
        let label = if p.promoted { "PROMOTED" } else { "BLOCKED" };
        let class = if p.promoted { "improved" } else { "worsened" };
        html.push_str(&format!(
            "<p><span class=\"badge {}\">{}</span> &middot; {}/{} checks passed</p>",
            class, label, p.passed_checks, p.total_checks
        ));
        if !p.failed_checks.is_empty() {
            html.push_str(&format!(
                "<p>Failed checks: {}</p>",
                escape_html(&p.failed_checks.join(", "))
            ));
        }
        html.push_str(&format!("<p>{}</p>", escape_html(&p.rollback_reason)));
    } else {
        html.push_str("<p>Promotion gate not run.</p>");
    }

    // MVP verdict.
    if let Some(m) = &report.mvp {
        html.push_str(&format!(
            "<h2>MVP success</h2><p>{}</p>",
            escape_html(&m.summary)
        ));
    }

    // Chain proof status.
    let committed_label = if report.chain_proof.committed {
        "COMMITTED"
    } else {
        "PENDING"
    };
    let committed_class = if report.chain_proof.committed {
        "improved"
    } else {
        "unchanged"
    };
    let block_ref = match (
        report.chain_proof.block_height,
        &report.chain_proof.block_hash,
    ) {
        (Some(height), Some(hash)) => format!("block {} ({})", height, escape_html(hash)),
        (Some(height), None) => format!("block {}", height),
        _ => "not yet included".into(),
    };
    html.push_str(&format!(
        "<h2>Chain proof status</h2><p>Proof hash: <code>{}</code> &middot; type: {} \
&middot; <span class=\"badge {}\">{}</span> &middot; {}</p>",
        escape_html(&report.chain_proof.proof_hash),
        escape_html(&report.chain_proof.proof_type),
        committed_class,
        committed_label,
        block_ref
    ));

    html.push_str("</body></html>");
    html
}

fn escape_html(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(ch),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::evals::mvp_success::{
        evaluate_mvp_success, MvpFalsePremiseRates, MvpSuccessTargets,
    };
    use crate::evals::{AdapterPromotionGatePolicy, AdapterRollbackMetadata};
    use crate::{evaluate_adapter_promotion_gate, EvalTraceMetrics, PromotionGateCheck};

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

    fn chain_proof_pending() -> ChainProofStatus {
        ChainProofStatus {
            proof_hash: "abcdef0123456789".into(),
            proof_type: "proof_of_route".into(),
            committed: false,
            block_height: None,
            block_hash: None,
        }
    }

    fn promotion(promoted: bool) -> AdapterPromotionDecision {
        let checks = vec![
            PromotionGateCheck {
                name: "coverage_improvement".into(),
                passed: promoted,
                baseline_value: 0.4,
                candidate_value: 0.65,
                requirement: ">=0".into(),
            },
            PromotionGateCheck {
                name: "zero_privacy_violations".into(),
                passed: true,
                baseline_value: 0.0,
                candidate_value: 0.0,
                requirement: "=0".into(),
            },
        ];
        AdapterPromotionDecision {
            adapter_id: "adapter-1".into(),
            promoted,
            checks,
            rollback: AdapterRollbackMetadata {
                adapter_id: "adapter-1".into(),
                base_model_id: "base-1".into(),
                previous_adapter_id: None,
                rollback_reason: if promoted {
                    "promotion passed".into()
                } else {
                    "promotion blocked by failed checks: coverage_improvement".into()
                },
            },
        }
    }

    fn mvp(passed: bool) -> MvpSuccessReport {
        let baseline = metrics(0.5, 0.4, 0.2, 0.3, 0.0);
        let candidate = if passed {
            metrics(0.7, 0.65, 0.1, 0.1, 0.0)
        } else {
            metrics(0.55, 0.45, 0.18, 0.28, 0.0) // misses most targets
        };
        evaluate_mvp_success(
            &baseline,
            &candidate,
            &MvpFalsePremiseRates {
                baseline: 0.4,
                candidate: if passed { 0.65 } else { 0.45 },
            },
            &MvpSuccessTargets::default(),
        )
        .unwrap()
    }

    #[test]
    fn metric_deltas_classify_higher_and_lower_is_better_directions() {
        // accuracy/route up (higher better -> improved); redundant up (lower better -> worsened).
        let baseline = metrics(0.5, 0.4, 0.10, 0.3, 0.0);
        let candidate = metrics(0.7, 0.4, 0.20, 0.3, 0.0);
        let report = build_training_report(
            "adapter-1",
            "base-1",
            &baseline,
            &candidate,
            None,
            None,
            Vec::new(),
            Vec::new(),
            chain_proof_pending(),
            true,
        )
        .unwrap();

        let by_name = |n: &str| report.metric_deltas.iter().find(|d| d.name == n).unwrap();
        assert_eq!(
            by_name("Correct route rate").direction,
            MetricDirection::Improved
        );
        assert_eq!(
            by_name("Redundant question rate").direction,
            MetricDirection::Worsened
        );
        assert_eq!(
            by_name("Checkpoint coverage").direction,
            MetricDirection::Unchanged
        );
    }

    #[test]
    fn recommendation_uses_adapter_when_promoted_mvp_and_privacy_ok() {
        let baseline = metrics(0.5, 0.4, 0.2, 0.3, 0.0);
        let candidate = metrics(0.7, 0.65, 0.1, 0.1, 0.0);
        let promo = promotion(true);
        let verdict = mvp(true);
        let report = build_training_report(
            "adapter-1",
            "base-1",
            &baseline,
            &candidate,
            Some(&promo),
            Some(&verdict),
            Vec::new(),
            Vec::new(),
            chain_proof_pending(),
            true,
        )
        .unwrap();
        assert!(report.recommendation.starts_with("USE"));
        assert!(report.privacy.passed);
    }

    #[test]
    fn recommendation_blocks_when_promotion_failed() {
        let baseline = metrics(0.5, 0.4, 0.2, 0.3, 0.0);
        let candidate = metrics(0.55, 0.42, 0.19, 0.29, 0.0);
        let promo = promotion(false);
        let report = build_training_report(
            "adapter-1",
            "base-1",
            &baseline,
            &candidate,
            Some(&promo),
            None,
            Vec::new(),
            Vec::new(),
            chain_proof_pending(),
            true,
        )
        .unwrap();
        assert!(report.recommendation.starts_with("Do NOT"));
        assert!(report.recommendation.contains("coverage_improvement"));
    }

    #[test]
    fn recommendation_blocks_on_privacy_leakage_regardless_of_promotion() {
        let baseline = metrics(0.5, 0.4, 0.2, 0.3, 0.0);
        let candidate = metrics(0.7, 0.65, 0.1, 0.1, 0.02); // leakage
        let promo = promotion(true);
        let report = build_training_report(
            "adapter-1",
            "base-1",
            &baseline,
            &candidate,
            Some(&promo),
            None,
            Vec::new(),
            Vec::new(),
            chain_proof_pending(),
            true,
        )
        .unwrap();
        assert!(report.recommendation.starts_with("Do NOT"));
        assert!(report.recommendation.contains("leakage"));
        assert!(!report.privacy.passed);
    }

    #[test]
    fn html_report_contains_every_section_and_escapes_input() {
        let baseline = metrics(0.5, 0.4, 0.2, 0.3, 0.0);
        let candidate = metrics(0.7, 0.65, 0.1, 0.1, 0.0);
        let report = build_training_report(
            "adapter-1",
            "base-1",
            &baseline,
            &candidate,
            Some(&promotion(true)),
            Some(&mvp(true)),
            vec![BehaviorExample {
                trace_id: "trace-good".into(),
                task_id: "task-1".into(),
                summary: "Router now picks the cheap local model.".into(),
                candidate_passed: true,
            }],
            vec![BehaviorExample {
                trace_id: "trace-bad".into(),
                task_id: "task-2".into(),
                summary: "Still answers prematurely on <script> injection.".into(),
                candidate_passed: false,
            }],
            ChainProofStatus {
                proof_hash: "deadbeef".into(),
                proof_type: "proof_of_route".into(),
                committed: true,
                block_height: Some(42),
                block_hash: Some("blockhash".into()),
            },
            true,
        )
        .unwrap();

        let html = render_training_report_html(&report);
        for needle in [
            "RLVR Training Report",
            "Before vs After",
            "What improved",
            "What got worse",
            "Better behavior examples",
            "Failure examples",
            "Privacy status",
            "Promotion result",
            "Chain proof status",
            "block 42",
        ] {
            assert!(
                html.contains(needle),
                "training report HTML missing section {needle:?}"
            );
        }
        // Dangerous input must be escaped, not injected as raw markup.
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<script>"));
    }

    #[test]
    fn html_shows_pending_when_proof_not_committed() {
        let baseline = metrics(0.5, 0.4, 0.2, 0.3, 0.0);
        let candidate = metrics(0.7, 0.65, 0.1, 0.1, 0.0);
        let report = build_training_report(
            "adapter-1",
            "base-1",
            &baseline,
            &candidate,
            None,
            None,
            Vec::new(),
            Vec::new(),
            chain_proof_pending(),
            true,
        )
        .unwrap();
        let html = render_training_report_html(&report);
        assert!(html.contains("PENDING"));
        assert!(html.contains("not yet included"));
    }

    #[test]
    fn build_rejects_empty_adapter_and_example_fields() {
        let baseline = metrics(0.5, 0.4, 0.2, 0.3, 0.0);
        let candidate = metrics(0.7, 0.65, 0.1, 0.1, 0.0);
        assert!(build_training_report(
            "",
            "base-1",
            &baseline,
            &candidate,
            None,
            None,
            Vec::new(),
            Vec::new(),
            chain_proof_pending(),
            true,
        )
        .is_err());

        let bad_example = BehaviorExample {
            trace_id: "  ".into(),
            task_id: "t".into(),
            summary: "s".into(),
            candidate_passed: true,
        };
        assert!(build_training_report(
            "adapter-1",
            "base-1",
            &baseline,
            &candidate,
            None,
            None,
            vec![bad_example],
            Vec::new(),
            chain_proof_pending(),
            true,
        )
        .is_err());
    }

    #[test]
    fn promotion_gate_integration_matches_real_decision() {
        // The screen's summary should reflect a real promotion-gate evaluation.
        let baseline = metrics(0.5, 0.4, 0.2, 0.3, 0.0);
        let candidate = metrics(0.7, 0.65, 0.1, 0.1, 0.0);
        let decision = evaluate_adapter_promotion_gate(
            "adapter-1",
            "base-1",
            None,
            &baseline,
            &candidate,
            &AdapterPromotionGatePolicy::default(),
        )
        .unwrap();
        let report = build_training_report(
            "adapter-1",
            "base-1",
            &baseline,
            &candidate,
            Some(&decision),
            None,
            Vec::new(),
            Vec::new(),
            chain_proof_pending(),
            true,
        )
        .unwrap();
        let summary = report.promotion.as_ref().unwrap();
        assert_eq!(summary.promoted, decision.promoted);
        assert_eq!(summary.total_checks, decision.checks.len());
        // keep EvalTraceMetrics import used for future per-trace example surfacing
        let _ = EvalTraceMetrics {
            trace_id: String::new(),
            task_id: String::new(),
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
