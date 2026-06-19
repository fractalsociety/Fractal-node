//! Reproducibility verifier package.
//!
//! Replays a frozen [`RunManifest`](crate::kernel::RunManifest) with freshly
//! rebuilt reference adapter/agent instances and verifies that the reproduced
//! evidence hash matches the originally recorded hash.

use chrono::{DateTime, Utc};
use serde_json::json;

use crate::adapters::{ReferenceAdapter, ReferenceAgent};
use crate::kernel::{self, RunManifest};
use crate::protocol::Hash;
use crate::verifier::VerifierReport;

/// Verifier id for this package.
pub const VERIFIER_ID: &str = "reproducibility";

const VERIFIER_VERSION: &str = "0.1.0";

/// Replay a reference adapter/agent run and verify its evidence hash.
pub async fn verify(
    original_hash: &Hash,
    manifest: &RunManifest,
    rebuild: impl FnOnce() -> (ReferenceAdapter, ReferenceAgent) + Send,
) -> VerifierReport {
    let (adapter, agent) = rebuild();
    match kernel::replay(adapter, agent, manifest).await {
        Ok(replayed) => {
            let reproduced_hash = replayed.evidence_hash;
            let passed = &reproduced_hash == original_hash;
            report(
                passed,
                Some(if passed { 1.0 } else { 0.0 }),
                json!({
                    "run_id": manifest.run_id,
                    "seed": manifest.seed,
                    "original_hash": original_hash.0,
                    "reproduced_hash": reproduced_hash.0,
                    "matches": passed,
                }),
                Vec::new(),
                if passed {
                    Vec::new()
                } else {
                    vec!["replayed evidence hash did not match original hash".to_string()]
                },
            )
        }
        Err(err) => report(
            false,
            Some(0.0),
            json!({
                "run_id": manifest.run_id,
                "seed": manifest.seed,
                "error": err.to_string(),
            }),
            Vec::new(),
            vec![format!("replay failed: {err}")],
        ),
    }
}

fn report(
    passed: bool,
    score: Option<f64>,
    details: serde_json::Value,
    warnings: Vec<String>,
    errors: Vec<String>,
) -> VerifierReport {
    VerifierReport {
        id: format!("{VERIFIER_ID}-report"),
        verifier_id: VERIFIER_ID.to_string(),
        verifier_version: VERIFIER_VERSION.to_string(),
        passed,
        score,
        details,
        warnings,
        errors,
        execution_time_seconds: 0.0,
        timestamp: DateTime::<Utc>::from_timestamp(0, 0).expect("epoch timestamp is valid"),
    }
}
