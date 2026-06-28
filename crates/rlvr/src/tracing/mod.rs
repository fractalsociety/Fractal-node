//! Node-facing route trace logger (RLVR-009).
//!
//! Captures one hash-only row per RLVR-enabled chat/route request. Raw prompts,
//! answers, and free-text corrections are never persisted — only their blake3
//! hashes plus non-private routing metadata (selected route, router reason,
//! route policy id/hash, latency, cost estimate, user rating, privacy tag
//! names). Rows are appended to a local JSONL file so they stay off-chain by
//! default; a later phase (RLVR-010) commits only `trace_hash`.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::{
    hash_bytes, route_policy_hash, scan_privacy_tags, stable_hash, RlvrError, RoutePolicy,
};

/// Raw request inputs captured at the node-facing route/proof API boundary.
///
/// Plaintext `prompt`, `answer`, and `user_correction` are hashed by the logger
/// and never written to disk, so this struct is safe to build at the request
/// edge even when the local trace store is on.
#[derive(Debug, Clone, Copy)]
pub struct RouteTraceInput<'a> {
    /// Raw user prompt (hashed before persistence).
    pub prompt: &'a str,
    /// Raw assistant/tool answer, when one was produced (hashed before persistence).
    pub answer: Option<&'a str>,
    /// Selected model / tool / agent route, e.g. `tiny-local-model` or
    /// `web-enabled model`.
    pub selected_route: &'a str,
    /// Human-readable router reason, e.g. `stable_knowledge; local model sufficient`.
    pub router_reason: &'a str,
    /// Route policy that produced the decision (id + hash are recorded).
    pub route_policy: &'a RoutePolicy,
    /// End-to-end request latency in milliseconds, when measured.
    pub latency_ms: Option<u64>,
    /// Cost estimate for the selected route, when known.
    pub cost_estimate: Option<f64>,
    /// Numeric user rating for the response, when collected.
    pub user_rating: Option<u32>,
    /// Raw free-text user correction, when collected (hashed before persistence).
    pub user_correction: Option<&'a str>,
}

/// One hash-only local trace row for an RLVR-enabled chat/route request.
///
/// No field here ever carries raw prompt/answer/correction content — only
/// hashes and non-private metadata — so a row is safe to reference from a
/// future chain proof object via [`RouteTraceRow::trace_hash`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RouteTraceRow {
    pub trace_id: String,
    pub timestamp_unix: u64,
    /// blake3 of the raw prompt bytes.
    pub prompt_hash: String,
    /// Selected model / tool / agent.
    pub selected_route: String,
    pub router_reason: String,
    pub route_policy_id: String,
    /// blake3 of the canonical route policy serialization.
    pub route_policy_hash: String,
    /// blake3 of the raw answer bytes, when an answer was produced.
    pub answer_hash: Option<String>,
    pub latency_ms: Option<u64>,
    pub cost_estimate: Option<f64>,
    pub user_rating: Option<u32>,
    /// blake3 of the raw user correction, when collected.
    pub user_correction_hash: Option<String>,
    /// Detected privacy tag *names* (never the matched content).
    pub privacy_tags: Vec<String>,
    pub local_only: bool,
    /// blake3 over every field above except `trace_hash` itself.
    pub trace_hash: String,
}

/// Serialization view used to derive `trace_hash` (excludes the hash itself to
/// avoid a circular dependency).
#[derive(Serialize)]
struct RouteTraceHashPayload<'a> {
    trace_id: &'a str,
    timestamp_unix: u64,
    prompt_hash: &'a str,
    selected_route: &'a str,
    router_reason: &'a str,
    route_policy_id: &'a str,
    route_policy_hash: &'a str,
    answer_hash: Option<&'a str>,
    latency_ms: Option<u64>,
    cost_estimate: Option<f64>,
    user_rating: Option<u32>,
    user_correction_hash: Option<&'a str>,
    privacy_tags: &'a [String],
    local_only: bool,
}

impl RouteTraceRow {
    /// Build a hash-only row from raw request inputs. Pure: callers (and tests)
    /// supply `trace_id` and `timestamp_unix` so the result is deterministic for
    /// a fixed input; [`RouteTraceLogger::record`] supplies live values.
    pub fn build(
        input: &RouteTraceInput,
        trace_id: String,
        timestamp_unix: u64,
        local_only: bool,
    ) -> Result<Self, RlvrError> {
        if trace_id.trim().is_empty() {
            return Err(RlvrError::Config("trace_id cannot be empty".into()));
        }
        if input.selected_route.trim().is_empty() {
            return Err(RlvrError::Config("selected_route cannot be empty".into()));
        }
        require_non_empty("router_reason", input.router_reason)?;
        if let Some(cost) = input.cost_estimate {
            require_finite_non_negative("cost_estimate", cost)?;
        }

        let prompt_hash = hash_bytes(input.prompt.as_bytes());
        let answer_hash = input.answer.map(|a| hash_bytes(a.as_bytes()));
        let user_correction_hash = input.user_correction.map(|c| hash_bytes(c.as_bytes()));
        let route_policy_hash = route_policy_hash(input.route_policy)?;
        let privacy_tags = privacy_tag_names(input.prompt, input.answer);

        let payload = RouteTraceHashPayload {
            trace_id: &trace_id,
            timestamp_unix,
            prompt_hash: &prompt_hash,
            selected_route: input.selected_route,
            router_reason: input.router_reason,
            route_policy_id: &input.route_policy.policy_id,
            route_policy_hash: &route_policy_hash,
            answer_hash: answer_hash.as_deref(),
            latency_ms: input.latency_ms,
            cost_estimate: input.cost_estimate,
            user_rating: input.user_rating,
            user_correction_hash: user_correction_hash.as_deref(),
            privacy_tags: &privacy_tags,
            local_only,
        };
        let trace_hash = stable_hash(&payload)?;

        let row = Self {
            trace_id,
            timestamp_unix,
            prompt_hash,
            selected_route: input.selected_route.to_string(),
            router_reason: input.router_reason.to_string(),
            route_policy_id: input.route_policy.policy_id.clone(),
            route_policy_hash,
            answer_hash,
            latency_ms: input.latency_ms,
            cost_estimate: input.cost_estimate,
            user_rating: input.user_rating,
            user_correction_hash,
            privacy_tags,
            local_only,
            trace_hash,
        };
        row.validate()?;
        Ok(row)
    }

    pub fn validate(&self) -> Result<(), RlvrError> {
        require_non_empty("trace_id", &self.trace_id)?;
        require_non_empty("route_trace.prompt_hash", &self.prompt_hash)?;
        require_non_empty("route_trace.selected_route", &self.selected_route)?;
        require_non_empty("route_trace.router_reason", &self.router_reason)?;
        require_non_empty("route_trace.route_policy_id", &self.route_policy_id)?;
        require_non_empty("route_trace.route_policy_hash", &self.route_policy_hash)?;
        require_non_empty("route_trace.trace_hash", &self.trace_hash)?;
        if let Some(cost) = self.cost_estimate {
            require_finite_non_negative("route_trace.cost_estimate", cost)?;
        }
        Ok(())
    }
}

/// Append-only local JSONL trace logger. `None` at the node level means RLVR
/// tracing is disabled and no rows are produced.
pub struct RouteTraceLogger {
    path: PathBuf,
    local_only: bool,
    counter: AtomicU64,
}

impl RouteTraceLogger {
    /// Open (or create) an append-only JSONL trace log at `path`, ensuring the
    /// parent directory exists. `local_only` is stamped onto every recorded row.
    pub fn open(path: impl Into<PathBuf>, local_only: bool) -> Result<Self, RlvrError> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }
        if !path.exists() {
            fs::File::create(&path)?;
        }
        Ok(Self {
            path,
            local_only,
            counter: AtomicU64::new(0),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn local_only(&self) -> bool {
        self.local_only
    }

    /// Build a hash-only row from `input` and append it as one JSON line.
    /// Returns the persisted row. Raw prompt/answer/correction never reach disk.
    pub fn record(&self, input: &RouteTraceInput) -> Result<RouteTraceRow, RlvrError> {
        let timestamp_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| RlvrError::Config("system clock is before the unix epoch".into()))?
            .as_secs();
        let seq = self.counter.fetch_add(1, Ordering::SeqCst);
        let trace_id = format!("rt-{timestamp_unix}-{seq}");
        let row = RouteTraceRow::build(input, trace_id, timestamp_unix, self.local_only)?;

        let mut line = serde_json::to_string(&row)?;
        line.push('\n');
        let mut file = OpenOptions::new().append(true).open(&self.path)?;
        file.write_all(line.as_bytes())?;
        Ok(row)
    }
}

/// Union of privacy tag *names* detected in the prompt and (optionally) the
/// answer. Only tag names are returned; matched content stays out of the row.
fn privacy_tag_names(prompt: &str, answer: Option<&str>) -> Vec<String> {
    let mut scan = scan_privacy_tags(prompt);
    if let Some(answer) = answer {
        let answer_scan = scan_privacy_tags(answer);
        scan.tags.extend(answer_scan.tags);
    }
    scan.tags.sort();
    scan.tags.dedup();
    scan.tags
        .into_iter()
        .map(|tag| tag.as_str().to_string())
        .collect()
}

fn require_non_empty(name: &str, value: &str) -> Result<(), RlvrError> {
    if value.trim().is_empty() {
        return Err(RlvrError::Config(format!("{name} cannot be empty")));
    }
    Ok(())
}

fn require_finite_non_negative(name: &str, value: f64) -> Result<(), RlvrError> {
    if !value.is_finite() {
        return Err(RlvrError::Config(format!("{name} must be finite")));
    }
    if value < 0.0 {
        return Err(RlvrError::Config(format!("{name} cannot be negative")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_input<'a>(policy: &'a RoutePolicy) -> RouteTraceInput<'a> {
        RouteTraceInput {
            prompt: "What is the capital of France?",
            answer: Some("Paris"),
            selected_route: "tiny-local-model",
            router_reason: "stable_knowledge; local model sufficient",
            route_policy: policy,
            latency_ms: Some(42),
            cost_estimate: Some(0.0),
            user_rating: Some(5),
            user_correction: None,
        }
    }

    fn scratch_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("fractal-rlvr-trace-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn build_row_captures_all_required_fields_as_hashes() {
        let policy = RoutePolicy::default();
        let input = sample_input(&policy);
        let row = RouteTraceRow::build(&input, "rt-1".into(), 1_700_000_000, true).unwrap();

        assert_eq!(row.trace_id, "rt-1");
        assert_eq!(row.timestamp_unix, 1_700_000_000);
        assert_eq!(
            row.prompt_hash,
            hash_bytes(b"What is the capital of France?")
        );
        assert_eq!(
            row.answer_hash.as_deref(),
            Some(hash_bytes(b"Paris").as_str())
        );
        assert!(row.user_correction_hash.is_none());
        assert_eq!(row.selected_route, "tiny-local-model");
        assert_eq!(row.router_reason, input.router_reason);
        assert_eq!(row.route_policy_id, policy.policy_id);
        assert_eq!(row.route_policy_hash, route_policy_hash(&policy).unwrap());
        assert_eq!(row.latency_ms, Some(42));
        assert_eq!(row.cost_estimate, Some(0.0));
        assert_eq!(row.user_rating, Some(5));
        assert!(row.local_only);
        assert!(row.privacy_tags.is_empty());
        assert_eq!(row.trace_hash.len(), 64);
    }

    #[test]
    fn prompt_and_answer_hashes_are_raw_blake3_bytes() {
        let policy = RoutePolicy::default();
        let input = RouteTraceInput {
            prompt: "abc",
            answer: Some("xyz"),
            selected_route: "tiny-local-model",
            router_reason: "stable_knowledge",
            route_policy: &policy,
            latency_ms: None,
            cost_estimate: None,
            user_rating: None,
            user_correction: Some("actually, give me a citation"),
        };
        let row = RouteTraceRow::build(&input, "rt-2".into(), 1, false).unwrap();
        assert_eq!(row.prompt_hash, hash_bytes(b"abc"));
        assert_eq!(
            row.answer_hash.as_deref(),
            Some(hash_bytes(b"xyz").as_str())
        );
        assert_eq!(
            row.user_correction_hash.as_deref(),
            Some(hash_bytes(b"actually, give me a citation").as_str())
        );
        assert!(!row.local_only);
    }

    #[test]
    fn trace_hash_is_stable_and_field_sensitive() {
        let policy = RoutePolicy::default();
        let input = sample_input(&policy);
        let a = RouteTraceRow::build(&input, "rt-1".into(), 1_700_000_000, true).unwrap();
        let b = RouteTraceRow::build(&input, "rt-1".into(), 1_700_000_000, true).unwrap();
        assert_eq!(a.trace_hash, b.trace_hash);

        let escalated = RouteTraceInput {
            selected_route: "web-enabled model",
            router_reason: "current_public_info; escalation required",
            ..input
        };
        let c = RouteTraceRow::build(&escalated, "rt-1".into(), 1_700_000_000, true).unwrap();
        assert_ne!(a.trace_hash, c.trace_hash);

        let later = RouteTraceRow::build(&input, "rt-1".into(), 1_700_000_001, true).unwrap();
        assert_ne!(a.trace_hash, later.trace_hash);
    }

    #[test]
    fn build_rejects_empty_route_reason_and_negative_cost() {
        let policy = RoutePolicy::default();
        let mut input = sample_input(&policy);
        input.router_reason = "  ";
        assert!(RouteTraceRow::build(&input, "rt-1".into(), 1, true).is_err());

        input.router_reason = "stable_knowledge";
        input.cost_estimate = Some(f64::NAN);
        assert!(RouteTraceRow::build(&input, "rt-1".into(), 1, true).is_err());

        input.cost_estimate = Some(-0.5);
        assert!(RouteTraceRow::build(&input, "rt-1".into(), 1, true).is_err());
    }

    #[test]
    fn record_appends_one_hash_only_jsonl_row() {
        let dir = scratch_dir("record");
        let path = dir.join("route_traces.jsonl");
        let logger = RouteTraceLogger::open(&path, true).unwrap();
        assert_eq!(logger.path(), &path);
        assert!(logger.local_only());

        let policy = RoutePolicy::default();
        let input = sample_input(&policy);
        let row = logger.record(&input).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.trim_end_matches('\n').lines().collect();
        assert_eq!(lines.len(), 1, "exactly one trace row per request");

        let parsed: RouteTraceRow = serde_json::from_str(lines[0]).unwrap();
        assert_eq!(parsed, row);

        // Raw plaintext must never reach disk.
        assert!(!content.contains("What is the capital of France?"));
        assert!(!content.contains("Paris"));
    }

    #[test]
    fn record_appends_one_row_per_request_in_order() {
        let dir = scratch_dir("order");
        let path = dir.join("route_traces.jsonl");
        let logger = RouteTraceLogger::open(&path, true).unwrap();

        let policy = RoutePolicy::default();
        for i in 0..3 {
            let input = RouteTraceInput {
                prompt: &format!("RAW-PROMPT-MARKER-{i}"),
                answer: Some("RAW-ANSWER-MARKER"),
                selected_route: "tiny-local-model",
                router_reason: "stable_knowledge",
                route_policy: &policy,
                latency_ms: Some(i as u64),
                cost_estimate: Some(0.0),
                user_rating: None,
                user_correction: None,
            };
            logger.record(&input).unwrap();
        }

        let content = fs::read_to_string(&path).unwrap();
        let lines: Vec<&str> = content.trim_end_matches('\n').lines().collect();
        assert_eq!(lines.len(), 3);
        for line in &lines {
            assert!(line.contains("tiny-local-model"));
        }
        // Distinctive raw markers must never reach disk (they can't collide with
        // JSON field names like `answer_hash`).
        assert!(!content.contains("RAW-PROMPT-MARKER"));
        assert!(!content.contains("RAW-ANSWER-MARKER"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn privacy_tags_recorded_without_raw_content() {
        let dir = scratch_dir("privacy");
        let path = dir.join("route_traces.jsonl");
        let logger = RouteTraceLogger::open(&path, true).unwrap();

        let policy = RoutePolicy::default();
        let input = RouteTraceInput {
            prompt:
                "Please summarize this: my API key is sk-or-v1-abcdef1234567890abcdef1234567890",
            answer: Some("Sure, ship to 123 Main Street as planned."),
            selected_route: "local-file-model",
            router_reason: "private_file_analysis; local only",
            route_policy: &policy,
            latency_ms: Some(120),
            cost_estimate: Some(0.0),
            user_rating: None,
            user_correction: None,
        };
        let row = logger.record(&input).unwrap();

        let content = fs::read_to_string(&path).unwrap();
        // Tag names are recorded ...
        assert!(row.privacy_tags.contains(&"api_key".to_string()));
        assert!(row.privacy_tags.contains(&"address".to_string()));
        // ... but the matched raw content is not.
        assert!(!content.contains("sk-or-v1-abcdef1234567890abcdef1234567890"));
        assert!(!content.contains("123 Main Street"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn open_creates_parent_directory_and_empty_file() {
        let dir = scratch_dir("open");
        let nested = dir.join("nested/deep/route_traces.jsonl");
        let logger = RouteTraceLogger::open(&nested, true).unwrap();
        assert!(nested.exists());
        assert_eq!(logger.path(), &nested);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn row_round_trips_through_json() {
        let policy = RoutePolicy::default();
        let input = sample_input(&policy);
        let row = RouteTraceRow::build(&input, "rt-json".into(), 123, true).unwrap();
        let json = serde_json::to_string(&row).unwrap();
        let parsed: RouteTraceRow = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, row);
        assert!(!json.contains("What is the capital of France?"));
        assert!(!json.contains("Paris"));
    }
}
