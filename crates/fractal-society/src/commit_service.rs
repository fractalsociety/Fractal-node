//! Generic research-package commitment service (AR-01, AR-02).
//!
//! Commits **any** research artifact — arbitrary bytes — to the chain *without*
//! running a simulation. This is the lighter sibling of the trading pipeline:
//!
//! ```text
//! any research package (bytes)
//!   -> canonical hash          (content_hash = SHA-256 of the bytes)
//!   -> signed manifest          (Ed25519 over signable manifest bytes)
//!   -> commit content_hash      (CommitmentAdapter::submit)
//!   -> verifiable receipt       (PublishedPackage)
//! ```
//!
//! The payload is content-addressed under `content_hash` (= `Hash::new(bytes)`,
//! which is exactly what gets committed to chain). The signed manifest is
//! content-addressed separately under its own `manifest_hash`. The two are
//! linked by an entry in the [`EventLog`] (`package_committed`), mirroring how
//! [`crate::persistence::persist_pipeline_result`] links pipeline artifacts.
//!
//! Retrieval ([`retrieve_research_package`]) recovers both the bytes and the
//! manifest from `content_hash` alone; [`verify_package`] then proves integrity
//! and authorship offline without re-running anything.
//!
//! No `DomainAdapter`, `run_pipeline`, or trading type is referenced anywhere
//! here — the service is domain-neutral by construction.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::artifact::{ArtifactManifest, ArtifactType};
use crate::canonical::{canonical_json, signable_bytes};
use crate::error::{Error, Result};
use crate::persistence::artifact_store::ArtifactStore;
use crate::persistence::event_log::{Event, EventLog};
use crate::pkgs::chain_commitment::CommitmentAdapter;
use crate::protocol::{ChainReference, Hash, Version, Visibility};
use crate::signing::{decode_signature_hex, verify_signature, AuthorSigner};

/// Event kind written to the [`EventLog`] when a package is committed.
pub const PACKAGE_COMMITTED_EVENT: &str = "package_committed";

/// What kind of research artifact a package is. Domain-neutral.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackageKind {
    /// A trading strategy (an agent policy package).
    TradingStrategy,
    /// A dataset artifact.
    Dataset,
    /// A packaged agent (non-trading or unspecified).
    AgentPackage,
    /// A science paper or ARA-style artifact directory.
    SciencePaper,
    /// A code artifact (scripts, configs, binaries).
    CodeArtifact,
    /// Any other research artifact, with a free-form label.
    Other(String),
}

/// Caller-supplied metadata for a package being committed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMetadata {
    /// Stable artifact identifier (caller-chosen; must be non-empty).
    pub id: String,
    /// What kind of research artifact this is.
    pub kind: PackageKind,
    /// Author label or DID.
    pub author: String,
    /// Disclosure policy for the package.
    pub visibility: Visibility,
    /// License identifier (e.g. "MIT", "CC-BY-4.0").
    pub license: String,
    /// Dependency artifact IDs and versions.
    pub dependencies: HashMap<String, Version>,
    /// Optional human-readable description.
    pub description: Option<String>,
}

/// Receipt returned by [`commit_research_package`]: everything needed to later
/// retrieve and verify the committed package.
#[derive(Debug, Clone)]
pub struct PublishedPackage {
    /// The signed, content-addressed artifact manifest.
    pub manifest: ArtifactManifest,
    /// `Hash::new(payload_bytes)` — committed to chain and stored.
    pub content_hash: Hash,
    /// `Hash::of(&manifest)` — storage key for the manifest.
    pub manifest_hash: Hash,
    /// On-chain commitment reference.
    pub chain_reference: ChainReference,
}

/// A package recovered from storage by its content hash.
#[derive(Debug, Clone)]
pub struct RetrievedPackage {
    /// The raw payload bytes.
    pub bytes: Vec<u8>,
    /// The signed manifest describing the payload.
    pub manifest: ArtifactManifest,
    /// Content hash of the manifest (storage key it was read from).
    pub manifest_hash: Hash,
}

/// Commit any research package to the chain.
///
/// Steps:
/// 1. content-address the payload (`content_hash = Hash::new(bytes)`);
/// 2. build and Ed25519-sign an [`ArtifactManifest`] over its signable bytes
///    (the `signature` field blanked, so the signature never covers itself);
/// 3. persist the payload and the signed manifest to `store` under their
///    respective content hashes;
/// 4. append a `package_committed` event linking `content_hash -> manifest_hash`;
/// 5. submit `content_hash` to `chain`;
/// 6. return a [`PublishedPackage`] receipt.
///
/// Deterministic given `(bytes, metadata, signer_seed, chain, store, event_log, now)`.
/// The hash committed to chain is exactly `content_hash`, which is exactly the
/// manifest's `content_hash` field, which is exactly `Hash::new(bytes)`.
#[allow(clippy::too_many_arguments)]
pub fn commit_research_package(
    bytes: &[u8],
    meta: PackageMetadata,
    signer: &AuthorSigner,
    chain: &dyn CommitmentAdapter,
    store: &mut dyn ArtifactStore,
    event_log: &mut dyn EventLog,
    now: DateTime<Utc>,
) -> Result<PublishedPackage> {
    if bytes.is_empty() {
        return Err(Error::InvalidArtifact(
            "package payload must be non-empty".to_string(),
        ));
    }
    if meta.id.trim().is_empty() {
        return Err(Error::InvalidArtifact(
            "package id must be non-empty".to_string(),
        ));
    }

    let kind_value = serde_json::to_value(&meta.kind)?;
    let content_hash = Hash::new(bytes);

    // 1-2. Build the unsigned manifest, then sign it over signable bytes.
    let mut manifest = ArtifactManifest {
        id: meta.id.clone(),
        version: "1.0.0".to_string(),
        artifact_type: artifact_type_for(&meta.kind),
        content_hash: content_hash.clone(),
        size_bytes: bytes.len() as u64,
        author: meta.author.clone(),
        visibility: meta.visibility.clone(),
        license: meta.license.clone(),
        dependencies: meta.dependencies.clone(),
        metadata: serde_json::json!({
            "kind": kind_value.clone(),
            "description": meta.description,
        }),
        created_at: now,
        signature: None,
    };
    let signable = manifest_signable_bytes(&manifest)?;
    manifest.signature = Some(hex::encode(signer.sign_bytes(&signable)));

    // 3. Content-address the signed manifest and persist both blobs.
    let manifest_bytes = canonical_json(&manifest)?;
    let manifest_hash = Hash::new(&manifest_bytes);
    store.put(&content_hash, bytes)?;
    store.put(&manifest_hash, &manifest_bytes)?;

    // 4. Link content_hash -> manifest_hash in the event log.
    let event = Event::new(
        format!("{}:{}", PACKAGE_COMMITTED_EVENT, content_hash.0),
        PACKAGE_COMMITTED_EVENT,
        serde_json::json!({
            "content_hash": content_hash.0,
            "manifest_hash": manifest_hash.0,
            "author": meta.author,
            "kind": kind_value,
        }),
    );
    let _ = event_log.append(event)?;

    // 5. Commit the payload content hash to the chain.
    let chain_reference = chain.submit(&content_hash)?;

    Ok(PublishedPackage {
        manifest,
        content_hash,
        manifest_hash,
        chain_reference,
    })
}

/// Retrieve a committed package by its (on-chain) `content_hash`.
///
/// Returns the payload bytes and the signed manifest. The manifest is located
/// via the `package_committed` event that links `content_hash -> manifest_hash`.
pub fn retrieve_research_package(
    content_hash: &Hash,
    store: &dyn ArtifactStore,
    event_log: &dyn EventLog,
) -> Result<RetrievedPackage> {
    let bytes = retrieve_payload(content_hash, store)?;

    let manifest_hash = find_manifest_hash(event_log, content_hash)?.ok_or_else(|| {
        Error::ArtifactNotFound(format!(
            "no package_committed link for content hash {}",
            content_hash.0
        ))
    })?;
    let manifest_bytes = store.get(&manifest_hash)?.ok_or_else(|| {
        Error::ArtifactNotFound(format!(
            "package manifest {} for content hash {}",
            manifest_hash.0, content_hash.0
        ))
    })?;
    let manifest: ArtifactManifest =
        serde_json::from_slice(&manifest_bytes).map_err(Error::Json)?;

    Ok(RetrievedPackage {
        bytes,
        manifest,
        manifest_hash,
    })
}

/// Retrieve only the payload bytes for `content_hash` (store-only, no event log).
///
/// This is the minimal "pull the package and recompute its hash" primitive:
/// `Hash::new(&retrieve_payload(h)?) == h` iff the bytes are intact.
pub fn retrieve_payload(content_hash: &Hash, store: &dyn ArtifactStore) -> Result<Vec<u8>> {
    store
        .get(content_hash)?
        .ok_or_else(|| Error::ArtifactNotFound(format!("package payload {}", content_hash.0)))
}

/// Canonical signable bytes of a manifest with its `signature` field blanked.
pub(crate) fn manifest_signable_bytes(manifest: &ArtifactManifest) -> Result<Vec<u8>> {
    let mut copy = manifest.clone();
    copy.signature = None;
    signable_bytes(&copy)
}

/// Verify a manifest's author signature against a 32-byte public key.
///
/// Returns `false` (rather than erroring) when the manifest is unsigned or the
/// signature is malformed, so callers can treat it as a single boolean check.
pub(crate) fn verify_manifest_signature(
    manifest: &ArtifactManifest,
    public_key: &[u8; 32],
) -> bool {
    let Some(sig_hex) = manifest.signature.as_ref() else {
        return false;
    };
    let Ok(signature) = decode_signature_hex(sig_hex) else {
        return false;
    };
    let Ok(bytes) = manifest_signable_bytes(manifest) else {
        return false;
    };
    verify_signature(public_key, &bytes, &signature).is_ok()
}

/// Map a [`PackageKind`] to the closest [`ArtifactType`] bucket.
fn artifact_type_for(kind: &PackageKind) -> ArtifactType {
    match kind {
        PackageKind::Dataset => ArtifactType::Dataset,
        PackageKind::AgentPackage => ArtifactType::AgentPackage,
        PackageKind::TradingStrategy
        | PackageKind::SciencePaper
        | PackageKind::CodeArtifact
        | PackageKind::Other(_) => ArtifactType::ResearchPackage,
    }
}

/// Scan the event log for the `manifest_hash` linked to `content_hash`.
fn find_manifest_hash(event_log: &dyn EventLog, content_hash: &Hash) -> Result<Option<Hash>> {
    for event in event_log.replay()? {
        if event.kind != PACKAGE_COMMITTED_EVENT {
            continue;
        }
        let matches_content = event
            .payload
            .get("content_hash")
            .and_then(|v| v.as_str())
            .is_some_and(|h| h == content_hash.0);
        if !matches_content {
            continue;
        }
        if let Some(hex) = event.payload.get("manifest_hash").and_then(|v| v.as_str()) {
            return Ok(Some(Hash::from_hex(hex)?));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offline_verify::verify_package;
    use crate::persistence::artifact_store::InMemoryArtifactStore;
    use crate::persistence::event_log::InMemoryEventLog;
    use crate::pkgs::chain_commitment::InMemoryCommitmentAdapter;
    use crate::signing::AuthorSigner;
    use std::sync::Mutex;

    fn signer() -> AuthorSigner {
        AuthorSigner::from_seed(&[0x42; 32])
    }

    fn meta(id: &str, kind: PackageKind) -> PackageMetadata {
        PackageMetadata {
            id: id.to_string(),
            kind,
            author: "founder".to_string(),
            visibility: Visibility::Open,
            license: "MIT".to_string(),
            dependencies: HashMap::new(),
            description: Some("a test package".to_string()),
        }
    }

    fn now() -> DateTime<Utc> {
        DateTime::from_timestamp(1_700_000_000, 0).unwrap()
    }

    /// Commitment adapter that records every submitted hash (interior-mutable).
    #[derive(Default)]
    struct RecordingChain {
        submitted: Mutex<Vec<Hash>>,
        block: u64,
    }

    impl CommitmentAdapter for RecordingChain {
        fn submit(&self, proof_hash: &Hash) -> Result<ChainReference> {
            self.submitted
                .lock()
                .expect("recording chain mutex")
                .push(proof_hash.clone());
            let block = self.block;
            Ok(ChainReference {
                network: "fractalchain-test".to_string(),
                transaction_hash: format!("0x{}", proof_hash.0),
                block_number: block,
                finalized: true,
            })
        }
    }

    #[test]
    fn distinct_payloads_get_distinct_hashes() {
        let signer = signer();
        let chain = RecordingChain::default();
        let mut store = InMemoryArtifactStore::new();
        let mut log = InMemoryEventLog::new();

        let a = commit_research_package(
            b"package A bytes",
            meta("a", PackageKind::Dataset),
            &signer,
            &chain,
            &mut store,
            &mut log,
            now(),
        )
        .unwrap();
        let b = commit_research_package(
            b"package B bytes",
            meta("b", PackageKind::SciencePaper),
            &signer,
            &chain,
            &mut store,
            &mut log,
            now(),
        )
        .unwrap();
        let c = commit_research_package(
            b"package C bytes",
            meta("c", PackageKind::Other("custom".into())),
            &signer,
            &chain,
            &mut store,
            &mut log,
            now(),
        )
        .unwrap();

        let content_hashes: Vec<_> = [&a, &b, &c]
            .iter()
            .map(|p| p.content_hash.0.clone())
            .collect();
        let manifest_hashes: Vec<_> = [&a, &b, &c]
            .iter()
            .map(|p| p.manifest_hash.0.clone())
            .collect();
        let txs: Vec<_> = [&a, &b, &c]
            .iter()
            .map(|p| p.chain_reference.transaction_hash.clone())
            .collect();
        let unique =
            |v: &Vec<String>| v.iter().collect::<std::collections::HashSet<_>>().len() == v.len();
        assert!(unique(&content_hashes), "content hashes must be distinct");
        assert!(unique(&manifest_hashes), "manifest hashes must be distinct");
        assert!(unique(&txs), "chain tx hashes must be distinct");
    }

    #[test]
    fn same_bytes_produce_same_content_and_manifest_hash() {
        let signer = signer();
        let chain = RecordingChain::default();
        let mut store = InMemoryArtifactStore::new();
        let mut log = InMemoryEventLog::new();

        let first = commit_research_package(
            b"identical bytes",
            meta("dup", PackageKind::CodeArtifact),
            &signer,
            &chain,
            &mut store,
            &mut log,
            now(),
        )
        .unwrap();
        let second = commit_research_package(
            b"identical bytes",
            meta("dup", PackageKind::CodeArtifact),
            &signer,
            &chain,
            &mut store,
            &mut log,
            now(),
        )
        .unwrap();

        assert_eq!(first.content_hash, second.content_hash);
        assert_eq!(first.manifest_hash, second.manifest_hash);
    }

    #[test]
    fn tampering_one_byte_changes_content_hash() {
        let signer = signer();
        let chain = RecordingChain::default();
        let mut store = InMemoryArtifactStore::new();
        let mut log = InMemoryEventLog::new();

        let original = commit_research_package(
            b"original payload",
            meta("o", PackageKind::Dataset),
            &signer,
            &chain,
            &mut store,
            &mut log,
            now(),
        )
        .unwrap();
        let tampered = commit_research_package(
            b"original payloaX",
            meta("t", PackageKind::Dataset),
            &signer,
            &chain,
            &mut store,
            &mut log,
            now(),
        )
        .unwrap();

        assert_ne!(original.content_hash, tampered.content_hash);
    }

    #[test]
    fn committed_hash_equals_manifest_content_hash_equals_submitted_hash() {
        let signer = signer();
        let chain = RecordingChain::default();
        let mut store = InMemoryArtifactStore::new();
        let mut log = InMemoryEventLog::new();

        let published = commit_research_package(
            b"chain-binding payload",
            meta("cb", PackageKind::AgentPackage),
            &signer,
            &chain,
            &mut store,
            &mut log,
            now(),
        )
        .unwrap();

        // All three hashes are identical, by construction.
        assert_eq!(published.content_hash, published.manifest.content_hash);
        let submitted = chain
            .submitted
            .lock()
            .expect("recording chain mutex")
            .clone();
        assert_eq!(submitted.len(), 1);
        assert_eq!(submitted[0], published.content_hash);
    }

    #[test]
    fn signature_verifies_under_author_public_key() {
        let signer = signer();
        let pk = signer.public_key();
        let chain = RecordingChain::default();
        let mut store = InMemoryArtifactStore::new();
        let mut log = InMemoryEventLog::new();

        let published = commit_research_package(
            b"signed payload",
            meta("sig", PackageKind::Dataset),
            &signer,
            &chain,
            &mut store,
            &mut log,
            now(),
        )
        .unwrap();

        assert!(verify_manifest_signature(&published.manifest, &pk));
    }

    #[test]
    fn retrieve_recovers_bytes_and_manifest() {
        let signer = signer();
        let chain = InMemoryCommitmentAdapter::new("fractalchain-test", 100);
        let mut store = InMemoryArtifactStore::new();
        let mut log = InMemoryEventLog::new();

        let payload = b"the original submitter package bytes";
        let published = commit_research_package(
            payload,
            meta("ret", PackageKind::SciencePaper),
            &signer,
            &chain,
            &mut store,
            &mut log,
            now(),
        )
        .unwrap();

        let retrieved = retrieve_research_package(&published.content_hash, &store, &log).unwrap();
        assert_eq!(retrieved.bytes, payload);
        assert_eq!(retrieved.manifest.content_hash, published.content_hash);
        assert_eq!(retrieved.manifest_hash, published.manifest_hash);
    }

    #[test]
    fn retrieve_payload_recomputes_equal_hash() {
        let signer = signer();
        let chain = InMemoryCommitmentAdapter::new("fractalchain-test", 1);
        let mut store = InMemoryArtifactStore::new();
        let mut log = InMemoryEventLog::new();

        let payload = b"payload-only retrieval";
        let published = commit_research_package(
            payload,
            meta("po", PackageKind::CodeArtifact),
            &signer,
            &chain,
            &mut store,
            &mut log,
            now(),
        )
        .unwrap();

        let bytes = retrieve_payload(&published.content_hash, &store).unwrap();
        assert_eq!(Hash::new(&bytes), published.content_hash);
    }

    #[test]
    fn empty_payload_and_empty_id_are_rejected() {
        let signer = signer();
        let chain = InMemoryCommitmentAdapter::new("x", 1);
        let mut store = InMemoryArtifactStore::new();
        let mut log = InMemoryEventLog::new();

        let err = commit_research_package(
            b"",
            meta("ok", PackageKind::Dataset),
            &signer,
            &chain,
            &mut store,
            &mut log,
            now(),
        )
        .unwrap_err();
        assert!(matches!(err, Error::InvalidArtifact(_)));

        let err = commit_research_package(
            b"bytes",
            PackageMetadata {
                id: "  ".to_string(),
                kind: PackageKind::Dataset,
                author: "a".to_string(),
                visibility: Visibility::Open,
                license: "MIT".to_string(),
                dependencies: HashMap::new(),
                description: None,
            },
            &signer,
            &chain,
            &mut store,
            &mut log,
            now(),
        )
        .unwrap_err();
        assert!(matches!(err, Error::InvalidArtifact(_)));
    }

    // ---- AR-02: retrieve + offline-verify a committed package ----

    fn commit_one(
        payload: &[u8],
        signer: &AuthorSigner,
        store: &mut dyn ArtifactStore,
        log: &mut dyn EventLog,
    ) -> PublishedPackage {
        commit_research_package(
            payload,
            meta("verify-target", PackageKind::SciencePaper),
            signer,
            &InMemoryCommitmentAdapter::new("fractalchain-test", 1),
            store,
            log,
            now(),
        )
        .unwrap()
    }

    #[test]
    fn golden_commit_then_retrieve_then_verify_passes() {
        let signer = signer();
        let pk = signer.public_key();
        let mut store = InMemoryArtifactStore::new();
        let mut log = InMemoryEventLog::new();

        let published = commit_one(b"golden package bytes", &signer, &mut store, &mut log);
        let retrieved = retrieve_research_package(&published.content_hash, &store, &log).unwrap();

        let verdict = verify_package(&retrieved, &published.content_hash, &pk).unwrap();
        assert!(verdict.valid, "reasons: {:?}", verdict.reasons);
        assert!(verdict.content_hash_matches);
        assert!(verdict.manifest_intact);
        assert!(verdict.signature_valid);
        assert!(verdict.on_chain_hash_matches);
    }

    #[test]
    fn tampered_payload_fails_verification() {
        let signer = signer();
        let pk = signer.public_key();
        let mut store = InMemoryArtifactStore::new();
        let mut log = InMemoryEventLog::new();

        let published = commit_one(b"original golden bytes", &signer, &mut store, &mut log);
        let mut retrieved =
            retrieve_research_package(&published.content_hash, &store, &log).unwrap();

        // Flip one byte of the retrieved payload.
        retrieved.bytes[0] ^= 0xff;

        let verdict = verify_package(&retrieved, &published.content_hash, &pk).unwrap();
        assert!(!verdict.valid);
        assert!(!verdict.content_hash_matches);
        assert!(!verdict.reasons.is_empty());
    }

    #[test]
    fn wrong_author_key_fails_signature_check() {
        let signer = signer();
        let mut store = InMemoryArtifactStore::new();
        let mut log = InMemoryEventLog::new();

        let published = commit_one(b"signed by the real author", &signer, &mut store, &mut log);
        let retrieved = retrieve_research_package(&published.content_hash, &store, &log).unwrap();

        let wrong_key = AuthorSigner::from_seed(&[0x11; 32]).public_key();
        let verdict = verify_package(&retrieved, &published.content_hash, &wrong_key).unwrap();
        assert!(!verdict.valid);
        assert!(!verdict.signature_valid);
    }

    #[test]
    fn mismatched_committed_hash_fails_on_chain_check() {
        let signer = signer();
        let pk = signer.public_key();
        let mut store = InMemoryArtifactStore::new();
        let mut log = InMemoryEventLog::new();

        let published = commit_one(b"chain-binding bytes", &signer, &mut store, &mut log);
        let retrieved = retrieve_research_package(&published.content_hash, &store, &log).unwrap();

        // A different hash than what was committed on chain.
        let wrong_committed = Hash::new(b"some other hash that is not the content hash");
        let verdict = verify_package(&retrieved, &wrong_committed, &pk).unwrap();
        assert!(!verdict.valid);
        assert!(!verdict.on_chain_hash_matches);
    }
}
