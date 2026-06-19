//! Dataset-integrity verifier package.
//!
//! Validates required [`DatasetManifest`](crate::protocol::DatasetManifest)
//! fields and confirms the content hash is a well-formed 64-character hex
//! string.

use chrono::{DateTime, Utc};
use serde_json::json;

use crate::protocol::{DatasetManifest, Hash};
use crate::verifier::VerifierReport;

/// Verifier id for this package.
pub const VERIFIER_ID: &str = "dataset-integrity";

const VERIFIER_VERSION: &str = "0.1.0";

/// Validate a dataset manifest's required fields and content hash format.
pub fn verify(manifest: &DatasetManifest) -> VerifierReport {
    let mut errors = Vec::new();

    if manifest.id.is_empty() {
        errors.push("dataset id must be non-empty".to_string());
    }
    if manifest.schema_version.is_empty() {
        errors.push("schema_version must be non-empty".to_string());
    }
    if let Err(err) = validate_hash(&manifest.content_hash) {
        errors.push(err);
    }

    let passed = errors.is_empty();
    report(
        passed,
        Some(if passed { 1.0 } else { 0.0 }),
        json!({
            "id": manifest.id,
            "schema_version": manifest.schema_version,
            "content_hash": manifest.content_hash.0,
            "hash_length": manifest.content_hash.0.len(),
        }),
        Vec::new(),
        errors,
    )
}

fn validate_hash(hash: &Hash) -> Result<(), String> {
    Hash::from_hex(&hash.0).map_err(|err| err.to_string())?;
    if !hash.0.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err("content_hash must be 64 hex characters".to_string());
    }
    Ok(())
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
