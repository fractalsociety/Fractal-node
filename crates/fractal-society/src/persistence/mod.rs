//! Persistence primitives for durable pipeline state.

pub mod artifact_store;
pub mod event_log;

use serde::de::DeserializeOwned;
use serde::Serialize;

use crate::canonical::canonical_json;
use crate::offline_verify::{self, VerifyVerdict};
use crate::persistence::artifact_store::ArtifactStore;
use crate::persistence::event_log::{Event, EventLog};
use crate::pipeline::PipelineResult;
use crate::pkgs::run_bundle::RunBundle;
use crate::protocol::{EvidenceBundle, Hash, ProofManifest};
use crate::verifier::Scorecard;

/// Hashes written by [`persist_pipeline_result`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistedPipelineArtifacts {
    /// Evidence bundle hash.
    pub evidence_hash: Hash,
    /// Scorecard hash.
    pub scorecard_hash: Hash,
    /// Proof manifest hash.
    pub proof_hash: Hash,
    /// Run bundle hash.
    pub bundle_hash: Hash,
}

/// A proof reloaded from persisted artifacts.
#[derive(Debug, Clone)]
pub struct LoadedProof {
    /// Persisted run bundle.
    pub bundle: RunBundle,
    /// Persisted proof manifest.
    pub manifest: ProofManifest,
    /// Persisted scorecard.
    pub scorecard: Scorecard,
    /// Persisted evidence bundle.
    pub evidence: EvidenceBundle,
    /// Offline verification verdict for the loaded proof.
    pub verdict: VerifyVerdict,
}

/// Persist a completed pipeline result to the artifact store and event log.
pub fn persist_pipeline_result(
    result: &PipelineResult,
    store: &mut dyn ArtifactStore,
    event_log: &mut dyn EventLog,
) -> crate::Result<PersistedPipelineArtifacts> {
    put_canonical(store, &result.bundle.evidence_hash, &result.run.evidence)?;
    append_event(
        event_log,
        "evidence_persisted",
        &result.bundle.evidence_hash,
        &result.bundle.agent_id,
    )?;

    put_canonical(store, &result.bundle.scorecard_hash, &result.scorecard)?;
    append_event(
        event_log,
        "scorecard_persisted",
        &result.bundle.scorecard_hash,
        &result.bundle.agent_id,
    )?;

    put_canonical(store, &result.bundle.proof_hash, &result.proof_manifest)?;
    append_event(
        event_log,
        "proof_manifest_persisted",
        &result.bundle.proof_hash,
        &result.bundle.agent_id,
    )?;

    let bundle_hash = result.bundle.bundle_hash()?;
    put_canonical(store, &bundle_hash, &result.bundle)?;
    append_event(
        event_log,
        "run_bundle_persisted",
        &bundle_hash,
        &result.bundle.agent_id,
    )?;

    Ok(PersistedPipelineArtifacts {
        evidence_hash: result.bundle.evidence_hash.clone(),
        scorecard_hash: result.bundle.scorecard_hash.clone(),
        proof_hash: result.bundle.proof_hash.clone(),
        bundle_hash,
    })
}

/// Load a persisted proof and verify it without re-running the pipeline.
///
/// Verification is **bytes-based**: the scorecard is verified by hashing its
/// stored canonical bytes (`Hash::new(bytes) == bundle.scorecard_hash`), not by
/// re-hashing the deserialized object. This avoids a latent f64-precision bug —
/// `serde_json`'s number parser is not correctly-rounded for some values, so a
/// deserialized scorecard can drift and would otherwise fail verification
/// spuriously. See `docs/fractal-society-ara-gap-closure-prd.md`.
pub fn load_proof(
    store: &dyn ArtifactStore,
    bundle: &RunBundle,
    public_key: &[u8; 32],
) -> crate::Result<LoadedProof> {
    let (manifest, _manifest_bytes) = get_verified(store, &bundle.proof_hash, "proof manifest")?;
    let (scorecard, scorecard_bytes) = get_verified(store, &bundle.scorecard_hash, "scorecard")?;
    let (evidence, _evidence_bytes) = get_verified(store, &bundle.evidence_hash, "evidence")?;
    let verdict = offline_verify::verify(bundle, &manifest, &scorecard_bytes, public_key);

    Ok(LoadedProof {
        bundle: bundle.clone(),
        manifest,
        scorecard,
        evidence,
        verdict,
    })
}

fn put_canonical<T: Serialize + ?Sized>(
    store: &mut dyn ArtifactStore,
    hash: &Hash,
    value: &T,
) -> crate::Result<()> {
    let bytes = canonical_json(value)?;
    store.put(hash, &bytes)
}

/// Read a canonical artifact: fetch its bytes, assert they hash to the key they
/// are stored under (defense-in-depth tamper check), then deserialize.
///
/// Returns both the deserialized value and the canonical bytes, so callers that
/// need to verify-by-bytes (e.g. [`load_proof`]) can pass the bytes on without
/// re-hashing a (possibly f64-drifted) deserialized object.
fn get_verified<T: DeserializeOwned>(
    store: &dyn ArtifactStore,
    hash: &Hash,
    label: &str,
) -> crate::Result<(T, Vec<u8>)> {
    let bytes = store.get(hash)?.ok_or_else(|| {
        crate::error::Error::ArtifactNotFound(format!("{label} artifact {}", hash.0))
    })?;
    // The store validates Hash::new(bytes) == hash on `put`; re-check on read so
    // a corrupt/custom store cannot serve bytes that don't match their key.
    let actual = Hash::new(&bytes);
    if &actual != hash {
        return Err(crate::error::Error::InvalidArtifact(format!(
            "{label} bytes hash {} does not match stored key {}",
            actual.0, hash.0
        )));
    }
    let value: T = serde_json::from_slice(&bytes).map_err(crate::error::Error::Json)?;
    Ok((value, bytes))
}

fn append_event(
    event_log: &mut dyn EventLog,
    kind: &str,
    hash: &Hash,
    agent_id: &str,
) -> crate::Result<()> {
    event_log.append(Event::new(
        format!("{kind}:{}", hash.0),
        kind,
        serde_json::json!({
            "hash": hash.0,
            "agent_id": agent_id,
        }),
    ))?;
    Ok(())
}
