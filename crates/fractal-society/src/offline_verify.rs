//! Offline (trustless) proof verification — package 71.
//!
//! Re-verify a published proof WITHOUT re-running the pipeline: recompute the
//! scorecard hash, confirm the manifest's author signature, and check that the
//! run bundle's hashes are internally consistent. Implements PRD P07-N09 ("a
//! proof can be exported and verified without trusting the web UI or hosted
//! API").

use crate::pkgs::run_bundle::RunBundle;
use crate::protocol::{Hash, ProofManifest};
use crate::verifier::Scorecard;

/// Outcome of offline verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyVerdict {
    /// Every check passed.
    Valid,
    /// One or more checks failed.
    Invalid {
        /// Human-readable failure reasons.
        reasons: Vec<String>,
    },
}

/// Verify a published proof offline against the author's public key.
///
/// Checks performed:
/// - `Hash::of(scorecard)` matches the bundle's `scorecard_hash`;
/// - the manifest's `scorecard_hash` matches the bundle's `scorecard_hash`;
/// - the manifest's `trace_merkle_root` matches the bundle's `evidence_hash`;
/// - the manifest's author signature verifies under `public_key`;
/// - `Hash::of(manifest)` matches the bundle's `proof_hash`.
///
/// Returns [`VerifyVerdict::Valid`] iff every check passes.
pub fn verify(
    bundle: &RunBundle,
    manifest: &ProofManifest,
    scorecard: &Scorecard,
    public_key: &[u8; 32],
) -> VerifyVerdict {
    let mut reasons: Vec<String> = Vec::new();

    // Recompute the scorecard hash and confirm it matches both the bundle and manifest.
    let scorecard_hash = match Hash::of(scorecard) {
        Ok(hash) => hash,
        Err(error) => {
            reasons.push(format!("scorecard hash computation failed: {error}"));
            Hash(String::new())
        }
    };
    if scorecard_hash != bundle.scorecard_hash {
        reasons.push("bundle scorecard_hash does not match recomputed scorecard hash".to_string());
    }
    if manifest.scorecard_hash != bundle.scorecard_hash {
        reasons.push("manifest scorecard_hash does not match bundle scorecard_hash".to_string());
    }

    // Evidence hash must be consistent between manifest and bundle.
    if manifest.trace_merkle_root != bundle.evidence_hash {
        reasons.push("manifest trace_merkle_root does not match bundle evidence_hash".to_string());
    }

    // Author signature must verify under the supplied public key.
    if let Err(error) = manifest.verify_author(public_key) {
        reasons.push(format!("author signature verification failed: {error}"));
    }

    // Manifest hash must match the bundle's proof hash.
    match Hash::of(manifest) {
        Ok(hash) if hash == bundle.proof_hash => {}
        Ok(_) => {
            reasons.push("bundle proof_hash does not match recomputed manifest hash".to_string())
        }
        Err(error) => reasons.push(format!("manifest hash computation failed: {error}")),
    }

    if reasons.is_empty() {
        VerifyVerdict::Valid
    } else {
        VerifyVerdict::Invalid { reasons }
    }
}
