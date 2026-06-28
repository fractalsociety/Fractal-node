//! Compile a research paper into a committed, verified research package.
//!
//! Usage:
//!   cargo run -p fractal-society --example compile_paper -- <digest.json> <repo-path> [--chain-url <rpc>]
//!
//! Reads a `PaperDigest` JSON (extracted from the paper by the reviewing
//! agent), assembles the ARA-style artifact directory, optionally commits the
//! root hash on-chain via `fractal_submitProofHash`, signs the manifest (binding
//! the chain reference), appends the concept index, and commits everything to
//! the packages repo. Prints a proof card: root hash, chain reference, review
//! verdict, commit SHA, concept + dead-end counts.
//!
//! On-chain submission requires the `live-chain` feature:
//!   cargo run -p fractal-society --features live-chain --example compile_paper -- <digest> <repo> --chain-url http://127.0.0.1:8545

use std::env;
use std::fs;
use std::path::Path;

use chrono::DateTime;

use fractal_society::concept_index;
use fractal_society::git_output;
use fractal_society::pkgs::chain_commitment::CommitmentAdapter;
use fractal_society::research_package::{assemble_package, PaperDigest};
use fractal_society::signing::AuthorSigner;

/// Build an on-chain commitment adapter from an optional RPC URL. Only the
/// `live-chain` feature can actually submit; without it, `--chain-url` is a
/// no-op (git-only) so the example still compiles by default.
#[cfg(feature = "live-chain")]
fn build_chain(url: Option<&str>) -> Option<Box<dyn CommitmentAdapter>> {
    let url = url?;
    let rpc = fractal_society::chain::fractalchain_adapter::JsonRpseeFractalChainRpc::connect(url)
        .expect("connect to FractalChain RPC");
    Some(Box::new(
        fractal_society::chain::fractalchain_adapter::FractalChainCommitmentAdapter::new(rpc),
    ))
}

#[cfg(not(feature = "live-chain"))]
fn build_chain(url: Option<&str>) -> Option<Box<dyn CommitmentAdapter>> {
    if url.is_some() {
        eprintln!(
            "note: --chain-url given but the crate was built without `live-chain`; \
             rebuilding with --features live-chain is required for on-chain submission. \
             Proceeding git-only."
        );
    }
    None
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mut positional: Vec<String> = Vec::new();
    let mut chain_url: Option<String> = None;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--chain-url" => {
                chain_url = Some(args.get(i + 1).cloned().unwrap_or_else(|| {
                    eprintln!("--chain-url requires a value");
                    std::process::exit(2)
                }));
                i += 2;
            }
            other => {
                positional.push(other.to_string());
                i += 1;
            }
        }
    }
    let (digest_path, repo_path) = match positional.as_slice() {
        [d, r] => (d.clone(), r.clone()),
        _ => {
            eprintln!("usage: compile_paper <digest.json> <repo-path> [--chain-url <rpc>]");
            std::process::exit(2);
        }
    };

    let digest_bytes = fs::read(&digest_path).unwrap_or_else(|e| panic!("read {digest_path}: {e}"));
    let digest: PaperDigest = serde_json::from_slice(&digest_bytes)
        .unwrap_or_else(|e| panic!("parse {digest_path} as PaperDigest: {e}"));

    let repo = Path::new(&repo_path).to_path_buf();
    git_output::ensure_repo(&repo, ("Fractal Society", "research@fractalsociety.org"))
        .expect("repo must be a git repo");

    // Deterministic signer + timestamp so the same digest yields the same proof.
    let signer = AuthorSigner::from_seed(&[0x42; 32]);
    let now = DateTime::from_timestamp(1_700_000_000, 0).unwrap();

    // Derive the package id (slug + source-hash prefix) to pick the target dir.
    let short = &digest.source.source_hash.0[..8];
    let slug: String = digest
        .source
        .title
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .filter(|c| !c.is_whitespace())
        .collect();
    let pkg_dir = repo.join("packages").join(format!("{slug}-{short}"));

    let chain = build_chain(chain_url.as_deref());
    let package = assemble_package(&digest, &signer, now, &pkg_dir, chain.as_deref())
        .expect("assemble must succeed");
    concept_index::append(&repo, &package).expect("append concept index");

    let chain_line = match &package.manifest.chain_reference {
        Some(c) => format!(
            "on-chain: {} @ block {} (tx {}, finalized={})",
            c.network, c.block_number, c.transaction_hash, c.finalized
        ),
        None => "on-chain: none (git-only)".to_string(),
    };

    // Commit the package + the updated concept index in one commit.
    let message = format!(
        "Add reviewed package: {}\n\n\
         root_hash (proof): {}\n\
         {}\n\
         review: {:?} ({}/100) by {}\n\
         source_hash: {}\n\
         claims: {}  concepts: {}  dead-ends: {}",
        digest.source.title,
        package.root_hash.0,
        chain_line,
        package.manifest.review.verdict,
        package.manifest.review.score,
        digest.reviewer,
        digest.source.source_hash.0,
        digest.claims.len(),
        digest.concepts.len(),
        package.manifest.review.dead_ends,
    );
    let commit = git_output::commit_package_to_repo(
        &repo,
        &message,
        ("Fractal Society", "research@fractalsociety.org"),
    )
    .expect("commit must succeed");

    // Verify the signed manifest (round-trip) — covers root_hash + chain ref.
    package
        .manifest
        .verify_author(&signer.public_key())
        .expect("signed manifest must verify");

    println!();
    println!("{}", "=".repeat(64));
    println!("  RESEARCH PACKAGE COMMITTED");
    println!("{}", "=".repeat(64));
    println!("title        : {}", digest.source.title);
    println!("package id   : {}-{}", slug, short);
    println!("reviewer     : {}", digest.reviewer);
    println!(
        "review       : {:?} ({}/100)",
        package.manifest.review.verdict, package.manifest.review.score
    );
    println!();
    println!("root hash    : {}", package.root_hash.0);
    println!("  (the proof; content-addressed, tamper-evident)");
    println!("source hash  : {}", digest.source.source_hash.0);
    println!("{}", chain_line);
    println!("commit       : {}", commit.commit_hash);
    println!("repo         : {}", repo.display());
    println!();
    println!(
        "concepts     : {} ({} dead-ends recorded)",
        digest.concepts.len(),
        package.manifest.review.dead_ends
    );
    println!("package dir  : {}", pkg_dir.display());
    println!("{}", "=".repeat(64));
}
