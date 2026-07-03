//! RLVR-009: node trace logger integration. When RLVR is enabled, every
//! chat/route request produces exactly one local, hash-only JSONL trace row;
//! when disabled, no row is produced. Raw prompt/answer text never reaches disk.

use fractal_node::NodeInner;
use fractal_rlvr::{hash_bytes, RlvrNodeFlags, RoutePolicy, RouteTraceInput, RouteTraceLogger};

fn scratch_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "fractal-node-rlvr-trace-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn sample_input<'a>(policy: &'a RoutePolicy, prompt: &'a str) -> RouteTraceInput<'a> {
    RouteTraceInput {
        prompt,
        answer: Some("RAW-ANSWER-MARKER-Paris"),
        selected_route: "tiny-local-model",
        router_reason: "stable_knowledge; local model sufficient",
        route_policy: policy,
        latency_ms: Some(37),
        cost_estimate: Some(0.0),
        user_rating: Some(5),
        user_correction: Some("RAW-CORRECTION-MARKER-add-a-citation"),
    }
}

#[test]
fn rlvr_enabled_route_request_produces_one_local_hash_only_row() {
    let mut node = NodeInner::devnet();
    node.set_rlvr_node_flags(RlvrNodeFlags::from_values(Some("true"), None, None));

    let dir = scratch_dir("enabled");
    let path = dir.join("route_traces.jsonl");
    node.set_route_trace_logger(RouteTraceLogger::open(&path, true).unwrap());
    assert_eq!(node.route_trace_log_path(), Some(path.as_path()));

    let policy = RoutePolicy::default();
    let prompt = "RAW-PROMPT-MARKER-what-is-the-capital-of-france";
    let row = node
        .record_route_trace(sample_input(&policy, prompt))
        .expect("recording succeeds")
        .expect("a row is produced when RLVR is enabled");

    // Every required RLVR-009 field is captured as a hash/metadata.
    assert_eq!(row.prompt_hash, hash_bytes(prompt.as_bytes()));
    assert_eq!(
        row.answer_hash.as_deref(),
        Some(hash_bytes(b"RAW-ANSWER-MARKER-Paris").as_str())
    );
    assert_eq!(
        row.user_correction_hash.as_deref(),
        Some(hash_bytes(b"RAW-CORRECTION-MARKER-add-a-citation").as_str())
    );
    assert_eq!(row.selected_route, "tiny-local-model");
    assert_eq!(row.route_policy_id, policy.policy_id);
    assert_eq!(
        row.route_policy_hash,
        fractal_rlvr::route_policy_hash(&policy).unwrap()
    );
    assert_eq!(row.latency_ms, Some(37));
    assert_eq!(row.cost_estimate, Some(0.0));
    assert_eq!(row.user_rating, Some(5));
    assert!(row.local_only);

    let content = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = content.trim_end_matches('\n').lines().collect();
    assert_eq!(lines.len(), 1, "exactly one local trace row per request");

    // Raw plaintext must never reach disk.
    assert!(!content.contains("RAW-PROMPT-MARKER"));
    assert!(!content.contains("RAW-ANSWER-MARKER"));
    assert!(!content.contains("RAW-CORRECTION-MARKER"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rlvr_enabled_logs_each_request_as_a_separate_row() {
    let mut node = NodeInner::devnet();
    node.set_rlvr_node_flags(RlvrNodeFlags::from_values(Some("true"), None, None));

    let dir = scratch_dir("multi");
    let path = dir.join("route_traces.jsonl");
    node.set_route_trace_logger(RouteTraceLogger::open(&path, true).unwrap());

    let policy = RoutePolicy::default();
    for _ in 0..3 {
        node.record_route_trace(sample_input(&policy, "RAW-PROMPT-MARKER"))
            .unwrap()
            .expect("row produced");
    }

    let content = std::fs::read_to_string(&path).unwrap();
    assert_eq!(content.trim_end_matches('\n').lines().count(), 3);
    assert!(!content.contains("RAW-PROMPT-MARKER"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn rlvr_disabled_produces_no_trace_row_and_no_file() {
    let node = NodeInner::devnet();
    // RLVR disabled by default and no logger attached.
    assert!(node.route_trace_log_path().is_none());

    let policy = RoutePolicy::default();
    let outcome = node
        .record_route_trace(sample_input(&policy, "RAW-PROMPT-MARKER"))
        .unwrap();
    assert!(outcome.is_none(), "disabled RLVR produces no trace row");
}

#[test]
fn rlvr_enabled_but_logger_unset_is_a_noop() {
    let mut node = NodeInner::devnet();
    node.set_rlvr_node_flags(RlvrNodeFlags::from_values(Some("true"), None, None));
    // Enabled, but no logger path configured.
    assert!(node.route_trace_log_path().is_none());

    let policy = RoutePolicy::default();
    let outcome = node
        .record_route_trace(sample_input(&policy, "RAW-PROMPT-MARKER"))
        .unwrap();
    assert!(outcome.is_none());
}
