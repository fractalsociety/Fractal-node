use fractal_society::pkgs::risk_adjusted_metrics::compute;

fn approx_eq(left: f64, right: f64, tolerance: f64) {
    assert!(
        (left - right).abs() <= tolerance,
        "left={left}, right={right}, tolerance={tolerance}"
    );
}

#[test]
fn empty_and_single_element_return_zeroes() {
    let empty = compute(&[]);
    let single = compute(&[0.05]);

    assert_eq!(empty.sharpe, 0.0);
    assert_eq!(empty.sortino, 0.0);
    assert_eq!(empty.volatility, 0.0);
    assert_eq!(empty.max_drawdown, 0.0);
    assert_eq!(single.sharpe, 0.0);
    assert_eq!(single.sortino, 0.0);
    assert_eq!(single.volatility, 0.0);
    assert_eq!(single.max_drawdown, 0.0);
}

#[test]
fn known_series_has_hand_checked_volatility_and_drawdown() {
    let metrics = compute(&[0.10, -0.05, 0.02, -0.10]);

    approx_eq(metrics.volatility, 0.075_291_101_731_877_99, 1e-12);
    approx_eq(metrics.max_drawdown, 0.1279, 1e-12);
}

#[test]
fn sharpe_is_finite_for_non_constant_series() {
    let metrics = compute(&[0.10, -0.05, 0.02, -0.10]);

    assert!(metrics.sharpe.is_finite());
    assert!(metrics.sortino.is_finite());
}

#[test]
fn compute_is_deterministic() {
    let returns = [0.02, 0.01, -0.03, 0.04, -0.01];

    assert_eq!(compute(&returns), compute(&returns));
}
