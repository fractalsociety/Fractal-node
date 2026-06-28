//! Emit golden `(object, hash)` pairs for cross-language canonical-hash
//! conformance (package 77). Prints a JSON array of `{"object": <value>,
//! "hash": "<sha256-jcs-hex>"}` pairs. Capture this output into the TS
//! conformance fixture and the TS test re-hashes each object with the
//! fractalwork `canonicalize` + SHA-256 convention and asserts equality.
//!
//! Re-run whenever schemas change to regenerate the fixture:
//!   cargo run -p fractal-society --example emit_golden_hashes

use fractal_society::canonical::content_hash;
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Serialize)]
struct Pair<'a> {
    /// The object whose canonical hash was computed.
    object: &'a Value,
    /// SHA-256 of the canonical (JCS) JSON encoding (`content_hash` output).
    hash: String,
}

fn pair<'a>(object: &'a Value) -> Pair<'a> {
    let hash = content_hash(object).expect("canonical hash of a fixed value");
    Pair {
        object,
        hash: hash.0,
    }
}

fn main() {
    // Golden objects use strings, integers, bools, null, nested objects,
    // arrays, AND floats. Float parity is enforced by `canonical_json`'s
    // ES6/JCS number formatting (`format_f64_jcs`), so Rust and the JS
    // `canonicalize` package hash float-bearing objects identically — including
    // exponent-threshold edge cases (1e-7, 1e21) and the AR-06 drift value.
    let run_manifest = json!({
        "run_id": "run-42",
        "seed": 42,
        "adapter_id": "trading-portfolio-sim",
        "adapter_version": "0.1.0",
        "agent_id": "starter-trading-agent",
        "episodes": 1,
        "max_steps_per_episode": 12
    });
    let nested_out_of_order = json!({
        "z": 1,
        "a": 2,
        "m": { "y": 3, "x": 4 },
        "list": [3, 1, 2],
        "flag": true,
        "nil": null
    });
    let simple = json!({ "b": 2, "a": 1 });
    let empty = json!({});

    // Float corpus: the cases where Rust's default `Display` would have
    // diverged from ES6/JCS, plus canonical reference values.
    let third = 1.0_f64 / 3.0;
    let drift = -0.00018429404999999998_f64;
    let floats = json!({
        "small": 1e-7,            // first negative-exponential threshold
        "small_fixed": 1e-6,      // last fixed-point magnitude
        "big": 1e21,              // first positive-exponential threshold
        "big_fixed": 1e20,        // last fixed integer
        "third": third,           // shortest round-trip
        "sum": 0.1_f64 + 0.2_f64, // 0.30000000000000004
        "int_valued": 5.0_f64,    // integer-valued float -> "5"
        "neg": -2.5_f64,
        "zero": 0.0_f64,
        "drift": drift            // the AR-06 problem value
    });

    let pairs = vec![
        pair(&run_manifest),
        pair(&nested_out_of_order),
        pair(&simple),
        pair(&empty),
        pair(&floats),
    ];

    println!("{}", serde_json::to_string_pretty(&pairs).unwrap());
}
