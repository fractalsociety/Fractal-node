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
//! the same rules. Number formatting is ES6 `Number.prototype.toString`
//! (RFC 8785 JCS), implemented in [`format_f64_jcs`], so Rust and TS hash
//! float-bearing objects identically — including exponent-threshold edge cases
//! (`1e-7`, `1e21`). Cross-language parity is locked by a float corpus in the
//! `golden_hashes.json` conformance fixture.

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
        out.push_str(&format_f64_jcs(f));
    } else {
        return Err(Error::Serialization(
            "unsupported number kind in canonical JSON".to_string(),
        ));
    }
    Ok(())
}

/// Format a finite `f64` per ES6 `Number.prototype.toString` (ECMA-262
/// §6.1.6.1.20), which is the number serialization RFC 8785 JCS specifies and
/// the TypeScript `canonicalize` package implements. This makes Rust and TS
/// hash float-containing objects identically — Rust's default `Display` diverges
/// from ES6 at exponent thresholds (e.g. Rust prints `0.0000001`, ES6 prints
/// `1e-7`) and would otherwise produce different content hashes.
///
/// Both `+0` and `-0` render as `"0"`. The algorithm: take Rust's shortest
/// round-trip scientific form `{:e}` (`d.ddd e<exp>`), extract the significant
/// digits and exponent, then apply the four ES6 formatting cases.
fn format_f64_jcs(f: f64) -> String {
    if f == 0.0 {
        // Normalizes both +0 and -0 to "0".
        return "0".to_string();
    }
    let neg = f.is_sign_negative();
    let body = format_f64_abs_body(f.abs());
    if neg {
        format!("-{body}")
    } else {
        body
    }
}

/// Format the body (no sign) of a positive, non-zero `f64`. The caller passes
/// the absolute value so the `{:e}` output never carries a leading `-`.
fn format_f64_abs_body(abs: f64) -> String {
    let sci = format!("{:e}", abs);
    let (mantissa, exp_str) = sci
        .split_once('e')
        .expect("Rust {:e} output always contains 'e'");
    let exp: i32 = exp_str.parse().expect("exponent is a decimal integer");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exp + 1; // ES6 n: 1-based position of the decimal point.

    if k <= n && n <= 21 {
        // Case 1: integer, possibly with trailing zeros.
        let zeros = "0".repeat((n - k) as usize);
        format!("{digits}{zeros}")
    } else if 0 < n && n <= 21 {
        // Case 2: fixed point with a fractional part (n < k here).
        let (int_part, frac_part) = digits.split_at(n as usize);
        format!("{int_part}.{frac_part}")
    } else if -6 < n && n <= 0 {
        // Case 3: small magnitude, leading "0.".
        let zeros = "0".repeat((-n) as usize);
        format!("0.{zeros}{digits}")
    } else {
        // Case 4: exponential.
        let first = &digits[..1];
        let rest_digits = &digits[1..];
        let exp_val = n - 1;
        let sign = if exp_val >= 0 { "+" } else { "-" };
        let frac = if rest_digits.is_empty() {
            String::new()
        } else {
            format!(".{rest_digits}")
        };
        format!("{first}{frac}e{sign}{}", exp_val.abs())
    }
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

    #[test]
    fn floats_render_per_es6_jcs() {
        // Each (input, expected ES6 Number.prototype.toString output). These are
        // the cases where Rust's default `Display` diverges from ES6 (exponent
        // thresholds) plus canonical reference values.
        let cases: &[(f64, &str)] = &[
            (0.0, "0"),
            (-0.0, "0"),
            (5.0, "5"),
            (-5.0, "-5"),
            (2.5, "2.5"),
            (0.1 + 0.2, "0.30000000000000004"),
            (1.0 / 3.0, "0.3333333333333333"),
            (1e-6, "0.000001"),              // last fixed-point magnitude
            (1e-7, "1e-7"),                  // first exponential (negative)
            (1e21, "1e+21"),                 // first exponential (positive)
            (1e20, "100000000000000000000"), // last fixed integer
            (1.23456e30, "1.23456e+30"),
            (-0.00018429404999999998, "-0.00018429404999999998"),
        ];
        for (input, expected) in cases {
            assert_eq!(format_f64_jcs(*input), *expected, "input {input}");
        }
    }

    #[test]
    fn float_object_canonical_matches_es6() {
        // A float-bearing object canonicalizes with ES6 number tokens.
        let bytes = canonical_json(&json!({"a": 1e-7, "b": 1e21, "c": 0.1})).unwrap();
        assert_eq!(
            String::from_utf8(bytes).unwrap(),
            r#"{"a":1e-7,"b":1e+21,"c":0.1}"#
        );
    }
}
