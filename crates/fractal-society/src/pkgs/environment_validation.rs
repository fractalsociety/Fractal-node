//! Environment-validation package.
//!
//! Validates required `EnvironmentManifest` fields without mutating the manifest.

use crate::protocol::{EnvironmentManifest, Hash};

/// Validate an environment manifest and return all detected errors.
pub fn validate(env: &EnvironmentManifest) -> std::result::Result<(), Vec<String>> {
    let mut errors = Vec::new();

    if env.id.trim().is_empty() {
        errors.push("id must be non-empty".to_string());
    }
    if Hash::from_hex(&env.version_hash.0).is_err() {
        errors.push("version_hash must be 64 hex characters".to_string());
    }
    if env.config.is_null() {
        errors.push("config must not be null".to_string());
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
