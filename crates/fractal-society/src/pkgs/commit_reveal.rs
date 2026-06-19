//! Canonical hash-based commit/reveal utilities.
//!
//! Hash-based commit/reveal: commit produces a canonical hash; reveal verifies
//! a value matches an earlier commitment.

use crate::protocol::Hash;

/// Commit to a JSON value by hashing its canonical representation.
pub fn commit(value: &serde_json::Value) -> Hash {
    Hash::of(value).expect("serde_json::Value should be canonically hashable")
}

/// Reveal whether `value` matches a previously claimed commitment.
pub fn reveal(value: &serde_json::Value, claimed: &Hash) -> bool {
    commit(value) == *claimed
}
