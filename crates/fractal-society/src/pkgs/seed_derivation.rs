//! Deterministic seed-derivation package.
//!
//! Expands one parent seed into domain-labeled sub-seeds using SHA-256. This
//! module never reads OS randomness and is stable across runs for the same
//! inputs.

/// Derive a deterministic sub-seed from `parent` and a domain `label`.
pub fn sub_seed(parent: u64, label: &str) -> u64 {
    let mut input = Vec::with_capacity(16 + label.len());
    input.extend_from_slice(b"fractal-seed-v1");
    input.extend_from_slice(&parent.to_be_bytes());
    input.extend_from_slice(label.as_bytes());
    let digest = fractal_crypto::sha256(&input);
    u64::from_be_bytes([
        digest[0], digest[1], digest[2], digest[3], digest[4], digest[5], digest[6], digest[7],
    ])
}

/// Derive `count` ordered, distinct sub-seeds from `parent`.
pub fn expand(parent: u64, count: usize) -> Vec<u64> {
    (0..count)
        .map(|index| sub_seed(parent, &format!("expand:{index}")))
        .collect()
}
