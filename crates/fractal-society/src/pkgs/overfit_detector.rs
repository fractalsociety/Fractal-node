//! Overfit detector package.
//!
//! Flag overfitting: compare a candidate's public-training vs private-eval
//! scorecards and detect a suspicious train ≫ eval gap.

use crate::verifier::Scorecard;

/// Overfitting assessment derived from train and eval scorecards.
#[derive(Debug, Clone, PartialEq)]
pub struct OverfitAssessment {
    /// Whether the train/eval gap exceeds the supplied threshold.
    pub overfit: bool,
    /// Public-training net return.
    pub train_return: f64,
    /// Private-eval net return.
    pub eval_return: f64,
    /// `train_return - eval_return`.
    pub gap: f64,
}

/// Assess whether the train/eval net-return gap indicates overfitting.
pub fn assess(train: &Scorecard, eval: &Scorecard, gap_threshold: f64) -> OverfitAssessment {
    let train_return = net_return(train);
    let eval_return = net_return(eval);
    let gap = train_return - eval_return;
    OverfitAssessment {
        overfit: gap > gap_threshold,
        train_return,
        eval_return,
        gap,
    }
}

fn net_return(scorecard: &Scorecard) -> f64 {
    scorecard
        .primary_metrics
        .get("net_return")
        .map(|metric| metric.value)
        .unwrap_or(0.0)
}
