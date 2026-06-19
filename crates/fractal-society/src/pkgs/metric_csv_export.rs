//! Deterministic CSV export for simulation metrics.
//!
//! Export a `MetricSet` to CSV (name,value rows incl. primary metric) for
//! spreadsheet / downstream tooling.

use crate::simulation::MetricSet;

/// Export a metric set as CSV with header `name,value`.
///
/// The `primary_metric` row is always first. Additional metrics are sorted by
/// name for deterministic output.
pub fn to_csv(metrics: &MetricSet) -> String {
    let mut out = String::from("name,value\n");
    push_row(&mut out, "primary_metric", metrics.primary_metric);

    let mut names = metrics.metrics.keys().collect::<Vec<_>>();
    names.sort();
    for name in names {
        push_row(&mut out, name, metrics.metrics[name]);
    }

    out
}

fn push_row(out: &mut String, name: &str, value: f64) {
    out.push_str(&escape_csv_name(name));
    out.push(',');
    out.push_str(&value.to_string());
    out.push('\n');
}

fn escape_csv_name(name: &str) -> String {
    if name.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", name.replace('"', "\"\""))
    } else {
        name.to_string()
    }
}
