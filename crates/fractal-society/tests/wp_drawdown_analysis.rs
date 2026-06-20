use fractal_society::pkgs::drawdown_analysis::analyze;

fn approx_eq(left: f64, right: f64) {
    assert!((left - right).abs() < 1e-12, "left={left}, right={right}");
}

#[test]
fn monotonic_up_curve_has_zero_drawdowns() {
    let analysis = analyze(&[100.0, 110.0, 120.0]);

    assert_eq!(analysis.series, vec![0.0, 0.0, 0.0]);
    assert_eq!(analysis.max_drawdown, 0.0);
    assert_eq!(analysis.max_drawdown_duration, 0);
}

#[test]
fn peak_then_drop_reports_max_and_duration() {
    let analysis = analyze(&[100.0, 120.0, 90.0, 96.0, 130.0, 117.0]);

    approx_eq(analysis.series[2], 0.25);
    approx_eq(analysis.series[3], 0.20);
    approx_eq(analysis.series[5], 0.10);
    approx_eq(analysis.max_drawdown, 0.25);
    assert_eq!(analysis.max_drawdown_duration, 2);
}

#[test]
fn empty_and_single_curve_return_zeroes() {
    let empty = analyze(&[]);
    let single = analyze(&[100.0]);

    assert!(empty.series.is_empty());
    assert_eq!(empty.max_drawdown, 0.0);
    assert_eq!(empty.max_drawdown_duration, 0);
    assert_eq!(single.series, vec![0.0]);
    assert_eq!(single.max_drawdown, 0.0);
    assert_eq!(single.max_drawdown_duration, 0);
}
