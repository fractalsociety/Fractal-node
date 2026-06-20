//! Emit a golden research package (AR-10) for cross-language (TS) conformance.
//!
//! Commits a fixed payload through `commit_research_package` and prints a JSON
//! object the TypeScript port (`fractalwork/packages/society-schema`) uses to
//! prove `hashPackage(bytes) == Rust content_hash` and that the manifest's
//! Ed25519 signature verifies under `@noble/ed25519`.
//!
//! Run:
//!   cargo run -p fractal-society --example emit_golden_package

use std::collections::HashMap;

use fractal_society::commit_service::{commit_research_package, PackageKind, PackageMetadata};
use fractal_society::persistence::artifact_store::InMemoryArtifactStore;
use fractal_society::persistence::event_log::InMemoryEventLog;
use fractal_society::pkgs::chain_commitment::InMemoryCommitmentAdapter;
use fractal_society::protocol::Visibility;
use fractal_society::signing::AuthorSigner;

/// The canonical golden payload (UTF-8). TS re-hashes these exact bytes.
const PAYLOAD: &str = "golden research package payload v1";

fn main() {
    let signer = AuthorSigner::from_seed(&[0x42; 32]);
    let public_key = hex::encode(signer.public_key());
    let chain = InMemoryCommitmentAdapter::new("fractalchain-41", 1);
    let mut store = InMemoryArtifactStore::new();
    let mut event_log = InMemoryEventLog::new();
    let now = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();

    let meta = PackageMetadata {
        id: "golden/research-package-v1".to_string(),
        kind: PackageKind::SciencePaper,
        author: "founder@fractalsociety".to_string(),
        visibility: Visibility::CommittedPrivate,
        license: "MIT".to_string(),
        dependencies: HashMap::new(),
        description: Some("golden package for TS conformance".to_string()),
    };

    let published = commit_research_package(
        PAYLOAD.as_bytes(),
        meta,
        &signer,
        &chain,
        &mut store,
        &mut event_log,
        now,
    )
    .expect("commit must succeed");

    let golden = serde_json::json!({
        "payload": PAYLOAD,
        "content_hash": published.content_hash.0,
        "manifest_hash": published.manifest_hash.0,
        "committed_hash": published.content_hash.0,
        "public_key": public_key,
        "manifest": published.manifest,
    });

    println!("{}", serde_json::to_string_pretty(&golden).unwrap());
}
