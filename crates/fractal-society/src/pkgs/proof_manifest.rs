//! Proof-manifest builder package.
//!
//! Build and sign a `ProofManifest` from a run outcome and a scorecard, hashing
//! each referenced artifact canonically and attaching the author's Ed25519
//! signature.

use chrono::{DateTime, Utc};

use crate::kernel::RunOutcome;
use crate::protocol::{Hash, ProofManifest, Visibility};
use crate::signing::AuthorSigner;

/// Build a committed-private proof manifest for a run and scorecard, signed by
/// the supplied author key.
pub fn build(
    run: &RunOutcome,
    scorecard: &crate::verifier::Scorecard,
    signer: &AuthorSigner,
    timestamp: DateTime<Utc>,
) -> crate::Result<ProofManifest> {
    let mut manifest = ProofManifest {
        manifest_version: "1.0.0".to_string(),
        claim_id: run.manifest.run_id.clone(),
        protocol_hash: Hash::of(&run.manifest)?,
        agent_hash: Hash::of(&run.manifest.agent_id)?,
        dataset_hash: Hash::new(b"dataset"),
        environment_hash: Hash::new(b"environment"),
        trace_merkle_root: run.evidence_hash.clone(),
        verifier_set_hash: Hash::new(b"verifiers"),
        scorecard_hash: Hash::of(scorecard)?,
        disclosure: Visibility::CommittedPrivate,
        author_signature: String::new(),
        platform_attestation: None,
        chain_reference: None,
        timestamp,
    };
    manifest.author_signature = manifest.author_signature_hex(signer)?;
    Ok(manifest)
}
