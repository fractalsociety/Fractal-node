//! Risk-adjusted metrics package.
//!
//! Compute Sharpe, Sortino, volatility, and max drawdown from a return series
//! (risk-free rate 0).

/// Risk-adjusted metrics derived from a return series.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RiskAdjusted {
    /// Mean excess return divided by volatility. Risk-free rate is zero.
    pub sharpe: f64,
    /// Mean excess return divided by downside deviation. Risk-free rate is zero.
    pub sortino: f64,
    /// Population standard deviation of returns.
    pub volatility: f64,
    /// Maximum drawdown of the cumulative return curve.
    pub max_drawdown: f64,
}

/// Compute risk-adjusted metrics from a return series.
pub fn compute(returns: &[f64]) -> RiskAdjusted {
    if returns.len() < 2 {
        return zero();
    }

    let mean = returns.iter().sum::<f64>() / returns.len() as f64;
    let volatility = variance(returns, mean).sqrt();
    let downside = downside_deviation(returns);
    let sharpe = ratio_or_zero(mean, volatility);
    let sortino = ratio_or_zero(mean, downside);
    let max_drawdown = max_drawdown(returns);

    RiskAdjusted {
        sharpe,
        sortino,
        volatility,
        max_drawdown,
    }
}

fn zero() -> RiskAdjusted {
    RiskAdjusted {
        sharpe: 0.0,
        sortino: 0.0,
        volatility: 0.0,
        max_drawdown: 0.0,
    }
}

fn variance(values: &[f64], mean: f64) -> f64 {
    values
        .iter()
        .map(|value| (value - mean).powi(2))
        .sum::<f64>()
        / values.len() as f64
}

fn downside_deviation(returns: &[f64]) -> f64 {
    let downside_sum = returns
        .iter()
        .filter(|value| **value < 0.0)
        .map(|value| value.powi(2))
        .sum::<f64>();
    (downside_sum / returns.len() as f64).sqrt()
}

fn ratio_or_zero(numerator: f64, denominator: f64) -> f64 {
    if denominator.abs() < f64::EPSILON {
        0.0
    } else {
        numerator / denominator
    }
}

fn max_drawdown(returns: &[f64]) -> f64 {
    let mut equity = 1.0_f64;
    let mut peak = 1.0_f64;
    let mut max_drawdown = 0.0_f64;

    for ret in returns {
        equity *= 1.0 + ret;
        peak = peak.max(equity);
        if peak > 0.0 {
            max_drawdown = max_drawdown.max((peak - equity) / peak);
        }
    }

    max_drawdown
}
