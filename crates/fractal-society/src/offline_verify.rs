//! Offline (trustless) proof verification — package 71, AR-02.
//!
//! Re-verify a published proof WITHOUT re-running the pipeline: recompute the
//! scorecard hash, confirm the manifest's author signature, and check that the
//! run bundle's hashes are internally consistent. Implements PRD P07-N09 ("a
//! proof can be exported and verified without trusting the web UI or hosted
//! API").
//!
//! AR-02 adds [`verify_package`] for arbitrary research packages committed via
//! the generic commit service — "pull the original submitter package and hash
//! on chain", verified without re-running anything.

use crate::commit_service::{verify_manifest_signature, RetrievedPackage};
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

/// Trustless verdict for an arbitrary committed research package (AR-02).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageVerifyVerdict {
    /// `Hash::new(payload_bytes) == manifest.content_hash`.
    pub content_hash_matches: bool,
    /// `Hash::of(&manifest) == stored manifest_hash` (manifest not tampered).
    pub manifest_intact: bool,
    /// The manifest's Ed25519 author signature verifies under the public key.
    pub signature_valid: bool,
    /// The hash committed on chain equals the package `content_hash`.
    pub on_chain_hash_matches: bool,
    /// `true` iff every check above passed.
    pub valid: bool,
    /// Human-readable failure reasons (empty when `valid`).
    pub reasons: Vec<String>,
}

/// Verify an arbitrary committed research package offline (AR-02).
///
/// Given a [`RetrievedPackage`] (pulled by its content hash) and the hash that
/// was committed on chain, prove — without re-running anything — that:
/// - the payload bytes hash to the manifest's `content_hash`;
/// - the manifest is intact (recomputed manifest hash matches the stored one);
/// - the author signature is valid under `author_pubkey`;
/// - the on-chain `committed_hash` equals the package `content_hash`.
///
/// For packages committed via [`crate::commit_service::commit_research_package`],
/// `committed_hash` is the package `content_hash` (what was submitted to chain).
pub fn verify_package(
    pkg: &RetrievedPackage,
    committed_hash: &Hash,
    author_pubkey: &[u8; 32],
) -> crate::Result<PackageVerifyVerdict> {
    let mut reasons = Vec::new();

    let content_hash_matches = Hash::new(&pkg.bytes) == pkg.manifest.content_hash;
    if !content_hash_matches {
        reasons.push("payload content hash does not match manifest.content_hash".to_string());
    }

    let recomputed_manifest_hash = Hash::of(&pkg.manifest)?;
    let manifest_intact = recomputed_manifest_hash == pkg.manifest_hash;
    if !manifest_intact {
        reasons.push("manifest content hash does not match stored manifest_hash".to_string());
    }

    let signature_valid = verify_manifest_signature(&pkg.manifest, author_pubkey);
    if !signature_valid {
        reasons.push("manifest author signature is invalid".to_string());
    }

    let on_chain_hash_matches = committed_hash == &pkg.manifest.content_hash;
    if !on_chain_hash_matches {
        reasons.push("on-chain committed hash does not match package content_hash".to_string());
    }

    let valid = content_hash_matches && manifest_intact && signature_valid && on_chain_hash_matches;

    Ok(PackageVerifyVerdict {
        content_hash_matches,
        manifest_intact,
        signature_valid,
        on_chain_hash_matches,
        valid,
        reasons,
    })
}
