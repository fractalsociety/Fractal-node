use std::collections::HashMap;

use fractal_society::pkgs::metric_set_ops::merge;
use fractal_society::simulation::MetricSet;

fn metric_set(primary_metric: f64, metrics: &[(&str, f64)]) -> MetricSet {
    MetricSet {
        primary_metric,
        metrics: metrics
            .iter()
            .map(|(key, value)| ((*key).to_string(), *value))
            .collect(),
        confidence_intervals: HashMap::new(),
    }
}

#[test]
fn merge_of_disjoint_key_sets_contains_all_keys() {
    let merged = merge(&[
        metric_set(1.0, &[("alpha", 10.0)]),
        metric_set(3.0, &[("beta", 20.0)]),
    ]);

    assert_eq!(merged.metrics["alpha"], 10.0);
    assert_eq!(merged.metrics["beta"], 20.0);
}

#[test]
fn duplicate_key_keeps_last_value() {
    let merged = merge(&[
        metric_set(1.0, &[("shared", 10.0)]),
        metric_set(3.0, &[("shared", 20.0)]),
    ]);

    assert_eq!(merged.metrics["shared"], 20.0);
}

#[test]
fn primary_metric_is_mean_of_inputs() {
    let merged = merge(&[
        metric_set(1.0, &[("a", 1.0)]),
        metric_set(3.0, &[("b", 2.0)]),
        metric_set(5.0, &[("c", 3.0)]),
    ]);

    assert_eq!(merged.primary_metric, 3.0);
}

#[test]
fn empty_input_returns_empty_metric_set() {
    let merged = merge(&[]);

    assert_eq!(merged.primary_metric, 0.0);
    assert!(merged.metrics.is_empty());
    assert!(merged.confidence_intervals.is_empty());
}
