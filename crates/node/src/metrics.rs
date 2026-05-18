//! PRD §16.1 — Prometheus text exposition (opt-in via `FRACTAL_METRICS_ADDR`).

use std::collections::BTreeMap;
use std::fmt::Write;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

use crate::{NodeHandle, NodeInner};

const LATENCY_BUCKETS_MS: &[u64] = &[1, 5, 10, 25, 50, 100, 250, 500, 1_000, 5_000];

#[derive(Debug)]
pub struct Histogram {
    buckets: Vec<AtomicU64>,
    sum_micros: AtomicU64,
    count: AtomicU64,
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new()
    }
}

impl Histogram {
    pub fn new() -> Self {
        Self {
            buckets: LATENCY_BUCKETS_MS
                .iter()
                .map(|_| AtomicU64::new(0))
                .collect(),
            sum_micros: AtomicU64::new(0),
            count: AtomicU64::new(0),
        }
    }

    pub fn observe_ms(&self, ms: u64) {
        self.count.fetch_add(1, Ordering::Relaxed);
        self.sum_micros
            .fetch_add(ms.saturating_mul(1_000), Ordering::Relaxed);
        for (i, bucket) in LATENCY_BUCKETS_MS.iter().enumerate() {
            if ms <= *bucket {
                self.buckets[i].fetch_add(1, Ordering::Relaxed);
            }
        }
    }

    fn write(&self, out: &mut String, name: &str, help: &str, labels: &[(&str, String)]) {
        let _ = writeln!(out, "# HELP {name} {help}");
        let _ = writeln!(out, "# TYPE {name} histogram");
        let mut base = String::new();
        for (i, (k, v)) in labels.iter().enumerate() {
            if i > 0 {
                base.push(',');
            }
            let _ = write!(base, "{k}=\"{}\"", prometheus_escape_label(v));
        }
        let label_prefix = if base.is_empty() {
            "{".to_string()
        } else {
            format!("{{{base},")
        };
        for (i, le) in LATENCY_BUCKETS_MS.iter().enumerate() {
            let v = self.buckets[i].load(Ordering::Relaxed);
            let _ = writeln!(out, "{name}_bucket{label_prefix}le=\"{le}\"}} {v}");
        }
        let count = self.count.load(Ordering::Relaxed);
        let _ = writeln!(out, "{name}_bucket{label_prefix}le=\"+Inf\"}} {count}");
        let sum_seconds = (self.sum_micros.load(Ordering::Relaxed) as f64) / 1_000_000.0;
        let label_block = if base.is_empty() {
            String::new()
        } else {
            format!("{{{base}}}")
        };
        let _ = writeln!(out, "{name}_sum{label_block} {sum_seconds:.6}");
        let _ = writeln!(out, "{name}_count{label_block} {count}");
    }
}

#[derive(Debug, Default)]
pub struct P2pTopicStats {
    inner: Mutex<BTreeMap<(String, String), u64>>,
}

impl P2pTopicStats {
    pub fn record(&self, topic: &str, direction: &str) {
        let Ok(mut g) = self.inner.lock() else {
            return;
        };
        *g.entry((topic.to_owned(), direction.to_owned()))
            .or_insert(0) += 1;
    }

    fn write(&self, out: &mut String) {
        let Ok(g) = self.inner.lock() else {
            return;
        };
        let _ = writeln!(
            out,
            "# HELP fractal_p2p_messages_total libp2p gossipsub messages by topic and direction."
        );
        let _ = writeln!(out, "# TYPE fractal_p2p_messages_total counter");
        for ((topic, direction), count) in g.iter() {
            let _ = writeln!(
                out,
                "fractal_p2p_messages_total{{topic=\"{}\",direction=\"{}\"}} {}",
                prometheus_escape_label(topic),
                prometheus_escape_label(direction),
                count
            );
        }
    }
}

#[derive(Debug)]
pub struct MetricsState {
    pub proposal_latency_ms: Histogram,
    pub qc_formation_latency_ms: Histogram,
    pub state_root_computation_ms: Histogram,
    pub proof_jobs_enqueued_total: AtomicU64,
    pub proof_jobs_dropped_total: AtomicU64,
    pub p2p_topic_messages: P2pTopicStats,
}

impl Default for MetricsState {
    fn default() -> Self {
        Self {
            proposal_latency_ms: Histogram::new(),
            qc_formation_latency_ms: Histogram::new(),
            state_root_computation_ms: Histogram::new(),
            proof_jobs_enqueued_total: AtomicU64::new(0),
            proof_jobs_dropped_total: AtomicU64::new(0),
            p2p_topic_messages: P2pTopicStats::default(),
        }
    }
}

/// OpenMetrics text for `GET /metrics` (Prometheus scraper compatible).
pub fn prometheus_text(n: &NodeInner) -> String {
    let mempool = n.mempool.len();
    let (last_gas, last_txs) = n
        .blocks
        .iter()
        .find(|b| b.header.height == n.height)
        .or_else(|| n.blocks.last())
        .map(|b| (b.header.gas_used, b.transactions.len() as u64))
        .unwrap_or((0u64, 0u64));

    let mut s = String::new();
    macro_rules! gauge {
        ($name:literal, $help:literal, $v:expr) => {
            let _ = writeln!(&mut s, "# HELP {} {}", $name, $help);
            let _ = writeln!(&mut s, "# TYPE {} gauge", $name);
            let _ = writeln!(&mut s, "{} {}", $name, $v);
        };
    }
    macro_rules! counter {
        ($name:literal, $help:literal, $v:expr) => {
            let _ = writeln!(&mut s, "# HELP {} {}", $name, $help);
            let _ = writeln!(&mut s, "# TYPE {} counter", $name);
            let _ = writeln!(&mut s, "{} {}", $name, $v);
        };
    }
    gauge!(
        "fractal_consensus_height",
        "Committed chain height (tip block number).",
        n.height
    );
    gauge!(
        "fractal_consensus_view_number",
        "Current HotStuff view (pacemaker / leader rotation).",
        n.view
    );
    gauge!(
        "fractal_mempool_size",
        "Transactions currently in the mempool.",
        mempool
    );
    gauge!(
        "fractal_mempool_transactions",
        "Deprecated alias for fractal_mempool_size.",
        mempool
    );
    gauge!(
        "fractal_last_block_gas_used",
        "Gas used by the last committed block (0 at genesis).",
        last_gas
    );
    gauge!(
        "fractal_last_block_tx_count",
        "Transaction count in the last committed block.",
        last_txs
    );
    let _ = write!(&mut s, "{}", n.rpc_call_stats.prometheus_counters());
    let _ = write!(
        &mut s,
        "{}",
        n.rpc_call_stats.prometheus_latency_histograms()
    );
    gauge!(
        "fractal_p2p_peer_count",
        "Established libp2p QUIC connections (best-effort; PRD §16.1).",
        n.p2p_connected_peers.load(Ordering::Relaxed)
    );
    n.metrics.p2p_topic_messages.write(&mut s);
    n.metrics.proposal_latency_ms.write(
        &mut s,
        "fractal_consensus_proposal_latency_ms",
        "Block proposal/build latency observed by this node.",
        &[],
    );
    n.metrics.qc_formation_latency_ms.write(
        &mut s,
        "fractal_consensus_qc_formation_latency_ms",
        "QC formation latency observed by this node.",
        &[],
    );
    n.metrics.state_root_computation_ms.write(
        &mut s,
        "fractal_state_root_computation_ms",
        "State-root computation latency observed by this node.",
        &[],
    );
    gauge!(
        "fractal_db_size_bytes",
        "Approximate on-disk RocksDB directory size in bytes.",
        n.rocksdb_path.as_deref().map(dir_size_bytes).unwrap_or(0)
    );
    gauge!(
        "fractal_proof_worker_enabled",
        "Whether the async proof worker is wired on this node.",
        if n.proof_job_tx.is_some() { 1 } else { 0 }
    );
    gauge!(
        "fractal_proof_artifacts_cached",
        "Checkpoint proof artifacts cached in the local registry.",
        n.proof_artifact_registry
            .as_ref()
            .map(|r| r.len())
            .unwrap_or(0)
    );
    counter!(
        "fractal_proof_jobs_enqueued_total",
        "Async proof checkpoint jobs enqueued since process start.",
        n.metrics.proof_jobs_enqueued_total.load(Ordering::Relaxed)
    );
    counter!(
        "fractal_proof_jobs_dropped_total",
        "Async proof checkpoint jobs dropped since process start.",
        n.metrics.proof_jobs_dropped_total.load(Ordering::Relaxed)
    );
    s
}

fn dir_size_bytes(path: &Path) -> u64 {
    let Ok(meta) = std::fs::metadata(path) else {
        return 0;
    };
    if meta.is_file() {
        return meta.len();
    }
    let Ok(read_dir) = std::fs::read_dir(path) else {
        return 0;
    };
    read_dir
        .filter_map(Result::ok)
        .map(|entry| dir_size_bytes(&entry.path()))
        .sum()
}

pub fn prometheus_escape_label(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\\' => "\\\\".to_string(),
            '"' => "\\\"".to_string(),
            '\n' => "\\n".to_string(),
            _ => c.to_string(),
        })
        .collect()
}

pub async fn serve_metrics(
    bind: SocketAddr,
    node: NodeHandle,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let listener = TcpListener::bind(bind).await?;
    loop {
        let (mut stream, _) = listener.accept().await?;
        let node = node.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_one_connection(&mut stream, node).await {
                eprintln!("fractal-node metrics: {e}");
            }
        });
    }
}

async fn handle_one_connection(
    stream: &mut TcpStream,
    node: NodeHandle,
) -> Result<(), std::io::Error> {
    let mut buf = [0_u8; 2048];
    let n = stream.read(&mut buf).await?;
    if n == 0 {
        return Ok(());
    }
    let head = String::from_utf8_lossy(&buf[..n]);
    let first = head.lines().next().unwrap_or("");
    let ok = first.starts_with("GET /metrics ")
        || first.starts_with("GET /metrics?")
        || first == "GET /metrics"
        || first == "GET /metrics HTTP/1.0"
        || first == "GET /metrics HTTP/1.1";
    if !ok {
        stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .await?;
        return Ok(());
    }
    let body = {
        let g = node.lock().await;
        prometheus_text(&*g)
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/plain; version=0.0.4\r\nConnection: close\r\nContent-Length: {}\r\n\r\n{}",
        body.len(),
        body
    );
    stream.write_all(response.as_bytes()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;

    #[test]
    fn prometheus_includes_height_and_mempool() {
        let n = NodeInner::devnet();
        let t = prometheus_text(&n);
        assert!(t.contains("fractal_consensus_height"));
        assert!(t.contains("fractal_consensus_view_number"));
        assert!(t.contains("fractal_mempool_size"));
        assert!(t.contains("fractal_last_block_gas_used"));
        assert!(t.contains("fractal_last_block_tx_count"));
        assert!(t.contains("fractal_p2p_peer_count"));
        assert!(t.contains("fractal_db_size_bytes"));
        assert!(t.contains("fractal_proof_worker_enabled"));
        assert!(t.contains("fractal_consensus_proposal_latency_ms_bucket"));
        assert!(t.contains("fractal_consensus_qc_formation_latency_ms_bucket"));
        assert!(t.contains("fractal_state_root_computation_ms_bucket"));
    }

    #[test]
    fn prometheus_includes_rpc_counters_after_record() {
        let n = NodeInner::devnet();
        n.rpc_call_stats.record("eth_chainId", true);
        n.rpc_call_stats.record("eth_chainId", false);
        let t = prometheus_text(&n);
        assert!(t.contains("fractal_rpc_requests_total"));
        assert!(t.contains("fractal_rpc_latency_ms_bucket"));
        assert!(t.contains("eth_chainId"));
    }

    #[test]
    fn prometheus_includes_p2p_topic_and_proof_worker_stats() {
        let n = NodeInner::devnet();
        n.metrics.p2p_topic_messages.record("fractal/votes/1", "in");
        n.metrics
            .proof_jobs_enqueued_total
            .fetch_add(2, Ordering::Relaxed);
        n.metrics
            .proof_jobs_dropped_total
            .fetch_add(1, Ordering::Relaxed);
        let t = prometheus_text(&n);
        assert!(
            t.contains("fractal_p2p_messages_total{topic=\"fractal/votes/1\",direction=\"in\"} 1")
        );
        assert!(t.contains("fractal_proof_jobs_enqueued_total 2"));
        assert!(t.contains("fractal_proof_jobs_dropped_total 1"));
    }
}
