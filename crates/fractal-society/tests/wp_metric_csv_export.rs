use std::collections::HashMap;

use fractal_society::pkgs::metric_csv_export::to_csv;
use fractal_society::simulation::MetricSet;

fn metrics() -> MetricSet {
    MetricSet {
        primary_metric: 0.25,
        metrics: HashMap::from([
            ("zeta".to_string(), 3.5),
            ("alpha".to_string(), -1.25),
            ("middle".to_string(), 0.0),
        ]),
        confidence_intervals: HashMap::new(),
    }
}

#[test]
fn csv_has_header_and_one_row_per_metric_with_primary_first() {
    let csv = to_csv(&metrics());
    let lines = csv.lines().collect::<Vec<_>>();

    assert_eq!(lines[0], "name,value");
    assert_eq!(lines[1], "primary_metric,0.25");
    assert_eq!(lines.len(), 5);
}

#[test]
fn values_are_round_trip_parseable() {
    let csv = to_csv(&metrics());

    for line in csv.lines().skip(1) {
        let (_, value) = line.rsplit_once(',').expect("row should contain comma");
        value.parse::<f64>().expect("metric value should parse");
    }
}

#[test]
fn additional_metrics_are_deterministically_sorted() {
    let csv = to_csv(&metrics());
    let lines = csv.lines().collect::<Vec<_>>();

    assert_eq!(lines[2], "alpha,-1.25");
    assert_eq!(lines[3], "middle,0");
    assert_eq!(lines[4], "zeta,3.5");
    assert_eq!(csv, to_csv(&metrics()));
}

#[test]
fn metric_names_are_csv_escaped() {
    let metric_set = MetricSet {
        primary_metric: 1.0,
        metrics: HashMap::from([("needs,\"escaping\"".to_string(), 2.0)]),
        confidence_intervals: HashMap::new(),
    };

    let csv = to_csv(&metric_set);

    assert!(csv.contains("\"needs,\"\"escaping\"\"\",2"));
}
