//! Drawdown-analysis package.
//!
//! Full drawdown analysis from an equity curve: per-step drawdown series, max
//! drawdown, and max drawdown duration (richer than the single max_drawdown
//! number in risk_adjusted_metrics).

/// Drawdown metrics derived from an equity curve.
#[derive(Debug, Clone, PartialEq)]
pub struct DrawdownAnalysis {
    /// Per-step drawdown series.
    pub series: Vec<f64>,
    /// Maximum drawdown observed in `series`.
    pub max_drawdown: f64,
    /// Longest run of consecutive positive drawdown values.
    pub max_drawdown_duration: usize,
}

/// Analyze drawdowns from an equity curve.
pub fn analyze(equity_curve: &[f64]) -> DrawdownAnalysis {
    if equity_curve.len() < 2 {
        return DrawdownAnalysis {
            series: vec![0.0; equity_curve.len()],
            max_drawdown: 0.0,
            max_drawdown_duration: 0,
        };
    }

    let mut peak = equity_curve[0];
    let mut max_drawdown = 0.0_f64;
    let mut current_duration = 0_usize;
    let mut max_drawdown_duration = 0_usize;
    let mut series = Vec::with_capacity(equity_curve.len());

    for &equity in equity_curve {
        peak = peak.max(equity);
        let drawdown = if peak > 0.0 {
            (peak - equity) / peak
        } else {
            0.0
        };
        max_drawdown = max_drawdown.max(drawdown);
        if drawdown > 0.0 {
            current_duration += 1;
            max_drawdown_duration = max_drawdown_duration.max(current_duration);
        } else {
            current_duration = 0;
        }
        series.push(drawdown);
    }

    DrawdownAnalysis {
        series,
        max_drawdown,
        max_drawdown_duration,
    }
}
