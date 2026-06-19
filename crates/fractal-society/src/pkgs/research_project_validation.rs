//! Research project manifest validation.
//!
//! Validate a `ResearchProject` (non-empty id/question/claim, valid domain
//! adapter reference).

use crate::protocol::ResearchProject;

/// Validate required identity fields for a research project.
///
/// Returns [`Ok`] when the project has a non-empty id, question, claim, and
/// domain adapter id/version. Otherwise returns all detected validation errors
/// in deterministic order.
pub fn validate(project: &ResearchProject) -> std::result::Result<(), Vec<String>> {
    let mut errors = Vec::new();

    if project.id.trim().is_empty() {
        errors.push("id must be non-empty".to_string());
    }
    if project.question.trim().is_empty() {
        errors.push("question must be non-empty".to_string());
    }
    if project.claim.trim().is_empty() {
        errors.push("claim must be non-empty".to_string());
    }
    if project.domain_adapter.id.trim().is_empty() {
        errors.push("domain_adapter.id must be non-empty".to_string());
    }
    if project.domain_adapter.version.trim().is_empty() {
        errors.push("domain_adapter.version must be non-empty".to_string());
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}
