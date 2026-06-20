use fractal_society::pkgs::confidence_intervals::{mean_ci, percentile};

#[test]
fn percentile_median_matches_expected_value() {
    assert_eq!(percentile(&[1.0, 2.0, 3.0], 50.0), Some(2.0));
}

#[test]
fn mean_ci_is_deterministic_for_fixed_seed() {
    let sample = [1.0, 2.0, 3.0, 4.0, 5.0];
    let first = mean_ci(&sample, 0.95, 500, 42);
    let second = mean_ci(&sample, 0.95, 500, 42);

    assert_eq!(first, second);
}

#[test]
fn symmetric_sample_ci_brackets_sample_mean() {
    let sample = [-2.0, -1.0, 0.0, 1.0, 2.0];
    let (lower, upper) = mean_ci(&sample, 0.95, 1000, 7).unwrap();
    let mean = sample.iter().sum::<f64>() / sample.len() as f64;

    assert!(lower <= mean, "lower={lower} mean={mean}");
    assert!(upper >= mean, "upper={upper} mean={mean}");
}

#[test]
fn empty_sample_returns_none() {
    assert_eq!(percentile(&[], 50.0), None);
    assert_eq!(mean_ci(&[], 0.95, 100, 1), None);
}
