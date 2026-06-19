//! Canonical serialization and content hashing (PHASE-01, gate P01-N02).
//!
//! Produces a deterministic byte encoding for any `Serialize` value, so that
//! the *same logical value* always yields the *same hash* regardless of struct
//! field declaration order or `HashMap` insertion order. This is the integrity
//! backbone for artifact manifests, run manifests, and proof manifests.
//!
//! # Canonical form
//!
//! - JSON object keys sorted by UTF-8 byte order (JCS-style), with no
//!   insignificant whitespace.
//! - Finite floating point only (`NaN`/`Infinity` are rejected); `-0.0` is
//!   normalized to `0`.
//! - String escaping delegates to `serde_json` (standard JSON escaping).
//!
//! The content hash is **SHA-256** of the canonical bytes, hex-encoded. This
//! matches `fractalwork/packages/core`'s `hashObjectJcs` (SHA-256 + JCS) so a
//! manifest hashed in Rust verifies against the same manifest hashed in the
//! TypeScript app. Rust is the reference implementation; the JS twin must apply
//! the same rules. Number formatting uses Rust's shortest round-trip `Display`
//! (integer-valued floats render without a trailing `.0`, matching ES6
//! `String()`); the small set of ES6 exponential-threshold edge cases is
//! deferred to the cross-language conformance test.

use crate::error::{Error, Result};
use crate::protocol::Hash;
use serde::Serialize;
use serde_json::Value;

/// Canonical JSON bytes for any serializable value.
///
/// Deterministic regardless of struct field declaration order or `HashMap`
/// insertion order: object keys are sorted by UTF-8 byte order.
pub fn canonical_json<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>> {
    let value = serde_json::to_value(value)
        .map_err(|e| Error::Serialization(format!("canonical conversion failed: {e}")))?;
    let mut out = String::new();
    write_canonical(&value, &mut out)?;
    Ok(out.into_bytes())
}

/// SHA-256 content hash (hex-encoded [`Hash`]) of the canonical JSON encoding.
pub fn content_hash<T: Serialize + ?Sized>(value: &T) -> Result<Hash> {
    let bytes = canonical_json(value)?;
    Ok(Hash(hex::encode(fractal_crypto::sha256(&bytes))))
}

/// Signable canonical bytes for a value.
///
/// Semantically identical to [`canonical_json`]; callers must first remove any
/// signature fields from the value so a signature never covers itself. This
/// helper exists to make that intent explicit at call sites.
pub fn signable_bytes<T: Serialize + ?Sized>(value: &T) -> Result<Vec<u8>> {
    canonical_json(value)
}

fn write_canonical(value: &Value, out: &mut String) -> Result<()> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => write_number(n, out)?,
        Value::String(s) => {
            // Delegate escaping to serde_json (standard JSON string escaping).
            let escaped = serde_json::to_string(s)
                .map_err(|e| Error::Serialization(format!("string escape failed: {e}")))?;
            out.push_str(&escaped);
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            // Sort keys by UTF-8 byte order. For ASCII field names (all current
            // schema fields) this matches JCS's UTF-16 code-unit ordering.
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                let escaped = serde_json::to_string(key.as_str())
                    .map_err(|e| Error::Serialization(format!("key escape failed: {e}")))?;
                out.push_str(&escaped);
                out.push(':');
                write_canonical(&map[*key], out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

fn write_number(n: &serde_json::Number, out: &mut String) -> Result<()> {
    if let Some(u) = n.as_u64() {
        out.push_str(&u.to_string());
    } else if let Some(i) = n.as_i64() {
        out.push_str(&i.to_string());
    } else if let Some(f) = n.as_f64() {
        if !f.is_finite() {
            return Err(Error::Serialization(
                "canonical JSON rejects NaN/Infinity".to_string(),
            ));
        }
        // Normalize -0.0 (and +0.0) to "0".
        if f == 0.0 {
            out.push('0');
        } else {
            // Rust's float `Display` is shortest round-trip and renders
            // integer-valued floats without ".0" (e.g. 5.0 -> "5").
            out.push_str(&format!("{}", f));
        }
    } else {
        return Err(Error::Serialization(
            "unsupported number kind in canonical JSON".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn identical_values_hash_identically() {
        let a = json!({"z": 1, "a": 2, "m": [3, 4]});
        let b = json!({"a": 2, "m": [3, 4], "z": 1}); // same map, different order
        assert_eq!(content_hash(&a).unwrap(), content_hash(&b).unwrap());
    }

    #[test]
    fn canonical_is_key_sorted_and_compact() {
        let bytes = canonical_json(&json!({"b": 1, "a": {"y": 2, "x": 1}})).unwrap();
        // Keys sorted at every level, no whitespace.
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            r#"{"a":{"x":1,"y":2},"b":1}"#
        );
    }

    #[test]
    fn negative_zero_is_normalized() {
        let bytes = canonical_json(&json!({"v": -0.0_f64})).unwrap();
        assert_eq!(String::from_utf8(bytes).unwrap(), r#"{"v":0}"#);
    }

    #[test]
    fn integer_valued_floats_render_without_decimal() {
        // Matches ES6 String(5.0) === "5"; integer and integer-valued-float
        // canonicalize identically, which is required for cross-runtime parity.
        let as_float = String::from_utf8(canonical_json(&json!({"v": 5.0_f64})).unwrap()).unwrap();
        let as_int = String::from_utf8(canonical_json(&json!({"v": 5_u64})).unwrap()).unwrap();
        assert_eq!(as_float, r#"{"v":5}"#);
        assert_eq!(as_float, as_int);
    }
}
