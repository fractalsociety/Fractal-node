//! PRD §16.1 — JSON-RPC request counters and latency histograms for Prometheus scraping.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::sync::{Arc, Mutex};

const LATENCY_BUCKETS_MS: &[u64] = &[1, 5, 10, 25, 50, 100, 250, 500, 1_000, 5_000];

/// Thread-safe rolling counters since process start (reset only on restart).
#[derive(Clone, Default)]
pub struct RpcCallStats {
    inner: Arc<Mutex<BTreeMap<String, (u64, u64)>>>,
    latency: Arc<Mutex<BTreeMap<String, RpcLatencyHistogram>>>,
}

#[derive(Clone, Debug)]
struct RpcLatencyHistogram {
    buckets: Vec<u64>,
    count: u64,
    sum_micros: u64,
}

impl Default for RpcLatencyHistogram {
    fn default() -> Self {
        Self {
            buckets: vec![0; LATENCY_BUCKETS_MS.len()],
            count: 0,
            sum_micros: 0,
        }
    }
}

impl RpcCallStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn record(&self, method: &str, success: bool) {
        self.record_with_latency_ms(method, success, 0);
    }

    pub fn record_with_latency_ms(&self, method: &str, success: bool, latency_ms: u64) {
        let Ok(mut m) = self.inner.lock() else {
            return;
        };
        let e = m.entry(method.to_owned()).or_insert((0, 0));
        if success {
            e.0 = e.0.saturating_add(1);
        } else {
            e.1 = e.1.saturating_add(1);
        }
        drop(m);
        let Ok(mut l) = self.latency.lock() else {
            return;
        };
        let hist = l.entry(method.to_owned()).or_default();
        hist.count = hist.count.saturating_add(1);
        hist.sum_micros = hist
            .sum_micros
            .saturating_add(latency_ms.saturating_mul(1_000));
        for (i, bucket) in LATENCY_BUCKETS_MS.iter().enumerate() {
            if latency_ms <= *bucket {
                hist.buckets[i] = hist.buckets[i].saturating_add(1);
            }
        }
    }

    /// OpenMetrics counter lines (`fractal_rpc_requests_total{method,status}`).
    pub fn prometheus_counters(&self) -> String {
        let Ok(m) = self.inner.lock() else {
            return String::new();
        };
        let mut s = String::new();
        let _ = writeln!(
            &mut s,
            "# HELP fractal_rpc_requests_total JSON-RPC method calls since process start."
        );
        let _ = writeln!(&mut s, "# TYPE fractal_rpc_requests_total counter");
        for (method, (ok, err)) in m.iter() {
            let esc = prometheus_escape_label(method);
            if *ok > 0 {
                let _ = writeln!(
                    &mut s,
                    "fractal_rpc_requests_total{{method=\"{esc}\",status=\"ok\"}} {ok}"
                );
            }
            if *err > 0 {
                let _ = writeln!(
                    &mut s,
                    "fractal_rpc_requests_total{{method=\"{esc}\",status=\"err\"}} {err}"
                );
            }
        }
        s
    }

    pub fn prometheus_latency_histograms(&self) -> String {
        let Ok(m) = self.latency.lock() else {
            return String::new();
        };
        let mut s = String::new();
        let _ = writeln!(
            &mut s,
            "# HELP fractal_rpc_latency_ms JSON-RPC request latency by method."
        );
        let _ = writeln!(&mut s, "# TYPE fractal_rpc_latency_ms histogram");
        for (method, hist) in m.iter() {
            let esc = prometheus_escape_label(method);
            for (i, le) in LATENCY_BUCKETS_MS.iter().enumerate() {
                let _ = writeln!(
                    &mut s,
                    "fractal_rpc_latency_ms_bucket{{method=\"{esc}\",le=\"{le}\"}} {}",
                    hist.buckets[i]
                );
            }
            let _ = writeln!(
                &mut s,
                "fractal_rpc_latency_ms_bucket{{method=\"{esc}\",le=\"+Inf\"}} {}",
                hist.count
            );
            let _ = writeln!(
                &mut s,
                "fractal_rpc_latency_ms_sum{{method=\"{esc}\"}} {:.6}",
                (hist.sum_micros as f64) / 1_000_000.0
            );
            let _ = writeln!(
                &mut s,
                "fractal_rpc_latency_ms_count{{method=\"{esc}\"}} {}",
                hist.count
            );
        }
        s
    }
}

fn prometheus_escape_label(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\\' => "\\\\".to_string(),
            '"' => "\\\"".to_string(),
            '\n' => "\\n".to_string(),
            _ => c.to_string(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_ok_and_err() {
        let st = RpcCallStats::new();
        st.record("eth_chainId", true);
        st.record("eth_chainId", false);
        let txt = st.prometheus_counters();
        assert!(txt.contains("method=\"eth_chainId\",status=\"ok\"} 1"));
        assert!(txt.contains("method=\"eth_chainId\",status=\"err\"} 1"));
        let hist = st.prometheus_latency_histograms();
        assert!(
            hist.contains("fractal_rpc_latency_ms_bucket{method=\"eth_chainId\",le=\"+Inf\"} 2")
        );
        assert!(hist.contains("fractal_rpc_latency_ms_count{method=\"eth_chainId\"} 2"));
    }
}
