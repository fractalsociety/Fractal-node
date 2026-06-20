//! Demonstrate the generic research-package commitment flow (AR-04).
//!
//! "Upload any research package and prove it and commit to chain, then pull the
//! original submitter package and hash on chain." This binary commits arbitrary
//! bytes through [`fractal_society::commit_service`] against in-memory adapters,
//! prints a package card, retrieves the package by its on-chain hash, verifies
//! it offline, and finally proves that tampering flips the verdict.
//!
//! Run with:
//!   cargo run -p fractal-society --example commit_arbitrary_package
//!   cargo run -p fractal-society --example commit_arbitrary_package -- --file path/to/any-file

use std::env;
use std::fs;

use chrono::{DateTime, Utc};

use fractal_society::commit_service::{
    commit_research_package, retrieve_research_package, PackageKind, PackageMetadata,
    PublishedPackage,
};
use fractal_society::offline_verify::verify_package;
use fractal_society::persistence::artifact_store::InMemoryArtifactStore;
use fractal_society::persistence::event_log::InMemoryEventLog;
use fractal_society::pkgs::chain_commitment::InMemoryCommitmentAdapter;
use fractal_society::protocol::Visibility;
use fractal_society::signing::AuthorSigner;

/// Read the payload bytes: from `--file <path>` if given, else a built-in
/// research-package document.
fn payload_bytes() -> (Vec<u8>, &'static str) {
    let mut args = env::args().skip(1);
    if let Some(flag) = args.next() {
        if flag == "--file" {
            if let Some(path) = args.next() {
                let bytes =
                    fs::read(&path).unwrap_or_else(|e| panic!("failed to read {path}: {e}"));
                return (bytes, "uploaded file");
            }
        }
        eprintln!("usage: commit_arbitrary_package [--file <path>]");
        std::process::exit(2);
    }

    let doc = b"# Research Package\n\
                \n\
                Title: Synthetic-basket mean-reversion v0.1\n\
                Author: founder@fractalsociety\n\
                License: MIT\n\
                \n\
                ## Claim\n\
                A z-score reversion signal on the BTC/ETH spread holds out-of-sample\n\
                with max drawdown under 10% on 1-minute candles.\n\
                \n\
                ## Method\n\
                - Compute spread = log(BTC) - beta*log(ETH), beta from rolling OLS.\n\
                - Entry when |z| > 2, exit when |z| < 0.5.\n\
                \n\
                ## Evidence\n\
                (referenced off-chain; this package commits the claim + method only.)\n";
    (doc.to_vec(), "built-in research document")
}

/// Fixed timestamp so the demo is deterministic.
fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_700_000_000, 0).unwrap()
}

fn print_card(published: &PublishedPackage, source: &str) {
    println!();
    println!("{}", "=".repeat(64));
    println!("  PACKAGE CARD — generic research package commitment");
    println!("{}", "=".repeat(64));
    println!("source            : {source}");
    println!("package id        : {}", published.manifest.id);
    println!(
        "kind              : {}",
        published
            .manifest
            .metadata
            .get("kind")
            .map(|v| v.to_string())
            .unwrap_or_else(|| "?".into())
    );
    println!("author            : {}", published.manifest.author);
    println!(
        "size              : {} bytes",
        published.manifest.size_bytes
    );
    println!("visibility        : {:?}", published.manifest.visibility);
    println!("license           : {}", published.manifest.license);
    println!();
    println!("content hash      : {}", published.content_hash.0);
    println!("manifest hash     : {}", published.manifest_hash.0);
    println!(
        "signature         : {}…",
        published
            .manifest
            .signature
            .as_deref()
            .map(|s| &s[..16])
            .unwrap_or("(unsigned)")
    );
    println!();
    println!("--- on-chain commitment ---");
    println!("network           : {}", published.chain_reference.network);
    println!(
        "transaction hash  : {}",
        published.chain_reference.transaction_hash
    );
    println!(
        "block number      : {}",
        published.chain_reference.block_number
    );
    println!(
        "finalized         : {}",
        published.chain_reference.finalized
    );
    println!();
}

fn print_verdict(label: &str, verdict: &fractal_society::offline_verify::PackageVerifyVerdict) {
    println!("--- offline verification: {label} ---");
    println!("  content_hash_matches : {}", verdict.content_hash_matches);
    println!("  manifest_intact      : {}", verdict.manifest_intact);
    println!("  signature_valid      : {}", verdict.signature_valid);
    println!("  on_chain_hash_matches: {}", verdict.on_chain_hash_matches);
    println!("  => valid             : {}", verdict.valid);
    if !verdict.reasons.is_empty() {
        println!("  reasons              : {:?}", verdict.reasons);
    }
    println!();
}

fn main() {
    let (bytes, source) = payload_bytes();

    let signer = AuthorSigner::from_seed(&[0x42; 32]);
    let author_pubkey = signer.public_key();
    let chain = InMemoryCommitmentAdapter::new("fractalchain-41", 1);
    let mut store = InMemoryArtifactStore::new();
    let mut event_log = InMemoryEventLog::new();

    println!("Committing {} bytes as a research package…", bytes.len());

    let meta = PackageMetadata {
        id: "research/synthetic-basket-mr-v0.1".to_string(),
        kind: PackageKind::SciencePaper,
        author: "founder@fractalsociety".to_string(),
        visibility: Visibility::CommittedPrivate,
        license: "MIT".to_string(),
        dependencies: Default::default(),
        description: Some("z-score mean-reversion claim + method".to_string()),
    };

    // 1. Commit: hash -> sign -> commit -> receipt.
    let published = commit_research_package(
        &bytes,
        meta,
        &signer,
        &chain,
        &mut store,
        &mut event_log,
        now(),
    )
    .expect("commit must succeed");
    print_card(&published, source);

    // 2. Pull the original package by its on-chain (content) hash and verify.
    let retrieved = retrieve_research_package(&published.content_hash, &store, &event_log)
        .expect("retrieve must succeed");
    assert_eq!(
        retrieved.bytes, bytes,
        "retrieved bytes must match the original"
    );

    let verdict = verify_package(&retrieved, &published.content_hash, &author_pubkey)
        .expect("verify must compute");
    print_verdict("pulled package", &verdict);
    assert!(verdict.valid, "the committed package must verify as valid");

    // 3. Prove tamper-detection: flip one byte and re-verify.
    let mut tampered = retrieved.clone();
    tampered.bytes[0] ^= 0xff;
    let tampered_verdict = verify_package(&tampered, &published.content_hash, &author_pubkey)
        .expect("verify must compute");
    print_verdict("tampered package (1 byte flipped)", &tampered_verdict);
    assert!(
        !tampered_verdict.valid,
        "a tampered package must NOT verify as valid"
    );

    println!("{}", "=".repeat(64));
    println!("  RESULT: package committed, retrieved, and verified valid.");
    println!("          tampering is reliably detected (content hash flips).");
    println!("{}", "=".repeat(64));
}
