//! Canonical hash round-trip validation.
//!
//! Hash a value through a serialize → deserialize round-trip and confirm the
//! canonical hash is stable (catches non-canonical types).

use serde::{Serialize, de::DeserializeOwned};

use crate::protocol::Hash;

/// Hash `value`, deserialize it through JSON, and confirm the canonical hash is stable.
///
/// Returns [`None`] if serialization fails, deserialization fails, canonical
/// hashing fails, or the deserialized value hashes differently from the input.
pub fn roundtrip_hash<T: Serialize + DeserializeOwned>(value: &T) -> Option<Hash> {
    let before = Hash::of(value).ok()?;
    let bytes = serde_json::to_vec(value).ok()?;
    let decoded: T = serde_json::from_slice(&bytes).ok()?;
    let after = Hash::of(&decoded).ok()?;
    (before == after).then_some(before)
}
