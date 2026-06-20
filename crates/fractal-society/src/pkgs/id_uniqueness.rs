//! Identifier-uniqueness package.
//!
//! Detects duplicate identifiers in ordered lists.

use std::collections::HashSet;

/// Return true iff every identifier appears at most once.
pub fn unique(ids: &[String]) -> bool {
    duplicates(ids).is_empty()
}

/// Return each duplicated identifier once, in first duplicate encounter order.
pub fn duplicates(ids: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut emitted = HashSet::new();
    let mut duplicates = Vec::new();

    for id in ids {
        if !seen.insert(id.as_str()) && emitted.insert(id.as_str()) {
            duplicates.push(id.clone());
        }
    }

    duplicates
}
