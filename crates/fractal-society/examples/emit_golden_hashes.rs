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
    // Golden objects use only strings, integers, bools, null, nested objects,
    // and arrays — the subset where Rust's `canonical_json` and JS JCS
    // (`canonicalize`) are guaranteed to agree. Float parity is a separate,
    // harder case (see TODO in the TS test).
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

    let pairs = vec![
        pair(&run_manifest),
        pair(&nested_out_of_order),
        pair(&simple),
        pair(&empty),
    ];

    println!("{}", serde_json::to_string_pretty(&pairs).unwrap());
}
