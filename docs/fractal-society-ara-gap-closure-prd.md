# Fractal Society — ARA Gap-Closure PRD

## Upload any research package. Prove it. Commit to chain. Pull the original and verify.

**Document type:** Focused execution PRD (gap closure)
**Version:** 0.1
**Status:** Draft — Founder Review Required
**Date:** 2026-06-20
**Parent documents:** `docs/new doc/fractal_society_simulation_proof_deployment_prd_v0_2_reconciled.md`; `crates/fractal-society/WORK_PACKAGES.md`
**External reference:** ARA-Labs/Agent-Native-Research-Artifact (arXiv 2604.24658)
**Primary owner:** Founder

---

## 1. Why this PRD exists

The ARA protocol (Agent-Native Research Artifact) is the closest comparable design to Fractal Society's research layer. After analyzing their repo + paper, the picture is:

- **Where we are ahead:** cryptographic proof, signed manifests, chain commitment, offline trustless verification, and Rust↔TS hash/signature parity. **ARA has none of these.** It is a file-format protocol; we are a proof + settlement protocol.
- **Where we have gaps:** ARA solves problems our protocol does not yet address — most importantly the **founder's explicit ask**: *"I should be able to upload any research package and prove it and commit to chain or pull the original submitter package and hash on chain."*

Today our pipeline is one shape:

```
trading simulation → evidence → scorecard → verifiers → proof → commit
```

That requires running a simulation. The founder wants a **second, lighter shape** that works for *any* research artifact (an ML paper, a dataset, a non-trading agent) with no simulation:

```
any research package (arbitrary bytes)
  → canonical hash
  → sign
  → commit hash to chain
  → verifiable receipt
```

…and the inverse — given a hash, pull the original package and confirm it matches what landed on chain.

This PRD specifies the tasks to close that gap and three adjacent gaps ARA surfaced (dead-end preservation, navigable artifact format, epistemic rigor scoring). It deliberately reuses primitives that already exist in the crate so no work is duplicated.

---

## 2. Gap analysis

| # | Gap | ARA | Fractal Society today | Severity |
|---|---|---|---|---|
| G1 | **Upload any package → hash → commit** (no simulation) | N/A — ARA does no chain work | Pieces exist; no end-to-end flow | **Critical** (founder ask) |
| G2 | **Pull original package by on-chain hash → verify** | N/A | `ArtifactStore` + `offline_verify` exist but only cover pipeline proofs, not arbitrary packages | **Critical** (founder ask) |
| G3 | **Dead-end / exploration-graph preservation** | Headline feature (81.4% failure-knowledge recovery) | Not modeled — pipeline records successes only | High |
| G4 | **Navigable artifact directory format** | `PAPER.md` + `logic/` + `src/` + `trace/` + `evidence/` | Types describe research objects; no agent-browsable layout | Medium |
| G5 | **Epistemic rigor scoring** | `rigor-reviewer` scores falsifiability, evidence relevance, scope calibration | Verifiers check *integrity* (accounting/cost/risk), not *quality* | Medium |
| G6 | **Per-entry provenance** | Every entry tagged `user`/`ai-suggested`/`ai-executed`/`user-revised` | Author signature on manifest only; no per-entry tags | Low |

**What we are explicitly NOT adopting from ARA:** their agent-skill packaging model (`/research-manager`, `/compiler`, `/rigor-reviewer` as installable skills). We keep the Rust crate as the canonical protocol spec and the TS app as the runtime; skills are out of scope for this PRD.

---

## 3. Primitives that already exist (do not reinvent)

Every task below builds on these. Implementation agents must reuse, not duplicate:

| Primitive | Location | Purpose |
|---|---|---|
| `Hash::new(&[u8])` (SHA-256) | `protocol.rs:33` | raw-bytes content hash |
| `Hash::of::<T>()` / `content_hash` | `canonical.rs` (re-exported in `prelude`) | canonical JCS-JSON SHA-256 of any serializable value |
| `AuthorSigner` (Ed25519) | `signing.rs` | sign + verify author signatures, cross-compatible with TS `@noble/ed25519` |
| `ArtifactManifest` | `artifact.rs:22` | content-addressed artifact descriptor |
| `PackageDigest` (signed hash) | `artifact.rs` | signed hash of an artifact — the signed-hash primitive |
| `ArtifactStore` trait | `persistence/artifact_store.rs:14` | content-addressed put/get/contains (in-memory + fs impls) |
| `CommitmentAdapter` trait | `pkgs/chain_commitment.rs:8` | `submit(&Hash) -> ChainReference` |
| `FractalChainCommitmentAdapter` | `chain/fractalchain_adapter.rs:60` | live adapter calling `fractal_submitProofHash` JSON-RPC |
| `fractal_submitProofHash` RPC | `crates/rpc/src/module.rs` | node method accepting a hash, returning `{network, transaction_hash, block_number, finalized}` |
| `EvmCommitmentAdapter` | `chain/evm_adapter.rs` | EVM settlement alternative |
| `offline_verify::verify` | `offline_verify.rs` | trustless recompute + signature check for pipeline proofs |
| `ChainReference` / `ProofManifest` | `protocol.rs:443`, `protocol.rs:476` | commitment receipt types |
| `persist_pipeline_result` | `persistence/mod.rs` | store + event-log write pattern to mirror |

**Key reuse decision (G1/G2):** the existing `fractal_submitProofHash` RPC accepts any 32-byte hash. A "research package" hash is just the canonical hash of the package bytes. **No new RPC method is required** — package hashes reuse the existing commitment endpoint.

---

## 4. Scope and non-goals

**In scope:**
- A generic package-commitment service (hash → sign → commit → receipt) that does not require a simulation run.
- Retrieval + offline verification of an arbitrary committed package.
- An exploration-graph (dead-end) layer.
- A navigable artifact directory format with a reader/writer.
- An epistemic rigor reviewer.
- A second (non-trading) domain adapter to prove "any research" genuinely runs.
- TS port of the package commit/verify surface.

**Non-goals (this PRD):**
- Real-capital deployment (separately approved phase).
- A new L1 / new token.
- Agent-skill packaging (`/compiler`, `/research-manager` installable skills).
- Changing the existing trading pipeline or its 5 integrity verifiers.
- IPFS/S3 storage backends (the `StorageLocation` enum exists; wiring real backends is later).

---

## 5. Tasks

Each task is self-contained and specifies goal, gap closed, dependencies, files, acceptance criteria, and tests. Numbers are stable IDs (`AR-01`…`AR-10`) for dispatching to agents. Copy/paste-ready Codex prompts should be generated from these specs following the format established in `WORK_PACKAGES.md`.

### Implementation status

| Task | Status | Notes |
|---|---|---|
| AR-01 | ✅ Done | `src/commit_service.rs` — `commit_research_package`, `PackageKind`, `PackageMetadata`, `PublishedPackage`. Added `ArtifactType::ResearchPackage`. |
| AR-02 | ✅ Done | `retrieve_research_package`/`retrieve_payload` in `commit_service.rs`; `PackageVerifyVerdict`/`verify_package` in `offline_verify.rs`. |
| AR-03 | ✅ Done | Doc note in `rpc/src/module.rs`; test `submit_proof_hash_accepts_arbitrary_package_content_hash` in `rpc/tests/submit_proof_hash.rs`. No new RPC method. |
| AR-04 | ✅ Done | `examples/commit_arbitrary_package.rs`. |
| AR-05 | ✅ Done | `src/exploration.rs` — `ExplorationGraph`, `ExplorationNode`, `NodeKind/Status`, `ProvenanceTag` (defined in `protocol.rs`, re-exported). Deterministic sorted serialization + `content_hash`. |
| AR-06 | ✅ Done | `src/artifact_format/{mod,reader,writer}.rs` — ARA-style directory layout; `write_artifact_dir`/`read_artifact_dir`; directory root hash. |
| AR-07 | ✅ Done | `src/rigor.rs` — `review()`, `RigorDimension`, `Recommendation`, `RigorReport`, `Claim`. Mechanical/deterministic rubric. |
| AR-08 | ✅ Done | `Option<ProvenanceTag>` on `DecisionTrace`; set in `RunTrace::into_evidence`; default `Human`. |
| AR-09 | ✅ Done | `src/adapters/forecasting/{mod,types,adapter,scorecard}.rs` — full `DomainAdapter` (Brier-score forecasting). Drives `kernel::run` + generic `proof_manifest::build` (the trading-specific `run_pipeline` is bypassed). Determinism + architecture-boundary tests. |
| AR-10 | ✅ Done | `fractalwork/packages/society-schema`: `src/package_verify.ts` (`hashPackage`/`verifyPackage`), `test/golden_package.json` (Rust-emitted via `examples/emit_golden_package.rs`), `test/package_verify.test.ts` (5 tests, cross-language parity proven). |

**As-built refinements vs. the spec (intentional):**
- **Storage layout (AR-01):** the payload is stored under `content_hash` and the signed manifest under its own `manifest_hash`; the two are linked by a `package_committed` entry in the `EventLog`, mirroring `persist_pipeline_result`. So `commit_research_package` and `retrieve_research_package` both take an `&mut dyn EventLog` / `&dyn EventLog` param (not in the original spec signature). This honors "on-chain hash = `content_hash` = `Hash::new(payload)`" exactly.
- **`verify_package` signature (AR-02):** takes `committed_hash: &Hash` as a parameter rather than deriving it from `ChainReference`, because `ChainReference` has no hash field. For packages committed via `commit_research_package`, `committed_hash` is the `content_hash`.
- **AR-09 pipeline path:** `run_pipeline`/`run_pipeline_default` are hardcoded to `TradingAdapter` (and `build_scorecard` takes a `TradingConfig`). The forecasting adapter therefore drives the generic `kernel::run` directly and builds its scorecard via `adapters::forecasting::build_forecasting_scorecard`, then signs via the generic `pkgs::proof_manifest::build`. This satisfies the PRD's "forecasting-aware variant." A future task could extract a truly generic `run_pipeline<A: DomainAdapter>`.
- **`ProvenanceTag` home:** defined in `protocol.rs` (canonical schema) and re-exported from `exploration.rs`, so decision traces and exploration nodes share one type without a module cycle.

**f64 canonical round-trip — diagnosed and fixed (Fix 1 + Fix 2):**
- *Root cause (corrected):* `serde_json`'s f64 **parser** is not correctly-rounded for some inputs (e.g. `-0.00018429404999999998`), so `serde_json::from_str(serde_json::to_string(x)) != x`. Rust `std` `Display`/`parse` ARE correctly-rounded; `canonical_json` (Rust `Display`) was never the problem.
- *Fix 1 (bytes-verified verification):* `offline_verify::verify` and `persistence::load_proof` now verify the scorecard by hashing its **stored canonical bytes** (`Hash::new(bytes)`), not a re-hash of a serde-deserialized object. `get_verified` reads bytes, asserts they hash to their key, then deserializes. No content hashes change; the latent `load_proof` bug is closed.
- *Fix 2 (JCS/ES6 float formatting + cross-language parity):* `canonical_json` now formats floats per ES6 `Number.prototype.toString` (`format_f64_jcs`), matching RFC 8785 JCS and the TS `canonicalize` package. Rust and TS now hash float-bearing objects identically (proven by a float corpus — `1e-7`, `1e21`, `0.1+0.2`, the AR-06 drift value — in `golden_hashes.json`, verified by `canonical.test.ts`). `golden_hashes.json` (no floats before) and `golden_proof.json` were re-emitted. If canonicalization ever changes again, add a `canonicalization` version field to `ProofManifest` and dispatch on it; not added now since no legacy proofs exist.

**Verification run:** AR-05..09 add 18 new tests; full `fractal-society` suite green (no regressions after Fix 1/Fix 2). AR-10 + Fix 2 cross-language parity: TS society-schema suite = 13 tests green. New files are `cargo fmt`/`clippy` clean. (Pre-existing, unrelated: `crates/crypto/src/bls.rs:112` clippy lint; pre-existing `wp_*.rs` fmt drift — both outside scope.)

---

### AR-01 — Generic package-commitment service (the core ask)

**Closes:** G1
**Depends on:** none (pure composition of existing primitives)
**Goal:** a single function that commits *any* research package (arbitrary bytes) to chain without running a simulation.

**New file:** `crates/fractal-society/src/commit_service.rs`

**New types:**
```rust
/// What kind of research artifact this is. Domain-neutral.
pub enum PackageKind {
    TradingStrategy,
    Dataset,
    AgentPackage,
    SciencePaper,      // e.g., an ARA-style artifact directory
    CodeArtifact,
    Other(String),
}

/// Lightweight, caller-supplied metadata for a package being committed.
pub struct PackageMetadata {
    pub id: String,            // stable artifact id (may be caller-generated)
    pub kind: PackageKind,
    pub author: String,        // author label / DID
    pub visibility: Visibility,
    pub license: String,
    pub dependencies: HashMap<String, Version>,
    pub description: Option<String>,
}

/// Result of committing a package: everything needed to later retrieve + verify.
pub struct PublishedPackage {
    pub manifest: ArtifactManifest,   // content-addressed descriptor
    pub signature: String,            // Ed25519 over signable manifest bytes
    pub content_hash: Hash,           // == manifest.content_hash == on-chain hash
    pub chain_reference: ChainReference,
}
```

**New functions:**
```rust
/// Commit any research package to the chain.
/// 1. hash the raw bytes (content_hash)
/// 2. build + sign an ArtifactManifest
/// 3. persist bytes + manifest to `store` under content_hash
/// 4. submit content_hash to `chain`
/// 5. return a PublishedPackage receipt
pub fn commit_research_package(
    bytes: &[u8],
    meta: PackageMetadata,
    signer: &AuthorSigner,
    chain: &dyn CommitmentAdapter,
    store: &mut dyn ArtifactStore,
    now: chrono::DateTime<chrono::Utc>,
) -> crate::Result<PublishedPackage>;
```

**Acceptance criteria:**
- No `DomainAdapter`, no `run_pipeline`, no trading types referenced anywhere in the module.
- `content_hash` = `Hash::new(bytes)`; the same `content_hash` is what gets committed and stored.
- The manifest is signed via the existing `AuthorSigner`/`signing.rs` over signable canonical bytes (signature field blanked before hashing — reuse the same convention as `ProofManifest`).
- Package bytes are persisted via `ArtifactStore::put(&content_hash, bytes)`.
- `CommitmentAdapter::submit(&content_hash)` is the only chain call.
- Deterministic given `(bytes, meta, signer_seed, chain, now)`.

**Tests (`commit_service.rs` inline + `tests/commit_service.rs`):**
- Commit 3 distinct byte payloads → 3 distinct `content_hash` + 3 distinct `chain_reference`.
- Commit the same bytes twice → same `content_hash` (idempotent content addressing).
- Tampering one byte of the payload changes `content_hash`.
- The committed hash equals the manifest's `content_hash` equals the on-chain-submitted hash (all three identical).
- Signature verifies with `AuthorSigner` public key.

---

### AR-02 — Retrieve + offline-verify an arbitrary committed package

**Closes:** G2
**Depends on:** AR-01
**Goal:** "pull the original submitter package and hash on chain" — given a hash, recover the package and prove integrity + authorship without re-running anything.

**Edit:** `crates/fractal-society/src/commit_service.rs` (add retrieve); `crates/fractal-society/src/offline_verify.rs` (add package verifier).

**New functions:**
```rust
/// Retrieve a committed package by its (on-chain) hash.
pub fn retrieve_research_package(
    content_hash: &Hash,
    store: &dyn ArtifactStore,
) -> crate::Result<RetrievedPackage>;

pub struct RetrievedPackage {
    pub bytes: Vec<u8>,
    pub manifest: ArtifactManifest,
}

/// Trustless verdict for an arbitrary committed package.
pub struct PackageVerifyVerdict {
    pub content_hash_matches: bool,   // Hash::new(bytes) == manifest.content_hash
    pub manifest_intact: bool,        // canonical(manifest) == stored manifest hash
    pub signature_valid: bool,        // Ed25519 over signable manifest
    pub on_chain_hash_matches: bool,  // content_hash == chain_reference's committed hash
    pub valid: bool,                  // AND of all four
    pub reasons: Vec<String>,
}

pub fn verify_package(
    pkg: &RetrievedPackage,
    chain_reference: &ChainReference,
    author_pubkey: &AuthorVerifyingKey,
) -> crate::Result<PackageVerifyVerdict>;
```

**Acceptance criteria:**
- `retrieve_research_package` reconstructs the original bytes byte-for-byte.
- `verify_package` recomputes `Hash::new(&bytes)` and asserts equality with `manifest.content_hash` (catches tampered payload).
- A tampered byte → `content_hash_matches == false` → `valid == false`.
- A wrong author key → `signature_valid == false`.
- `on_chain_hash_matches` is `true` when the package hash equals the hash that produced the `ChainReference` (the caller supplies the reference; this PRD does not require reading chain state).

**Tests:** golden commit → retrieve → verify passes; mutate one byte → verify fails with a reason; swap author key → verify fails.

---

### AR-03 — Confirm `fractal_submitProofHash` suffices for packages (no new RPC)

**Closes:** G1 (chain surface)
**Depends on:** none
**Goal:** confirm the existing RPC accepts a package content hash unchanged; add only documentation + an alias path, not a new method.

**Edit:** `crates/rpc/src/module.rs` (doc comment clarifying hash generality); `crates/fractal-society/src/chain/fractalchain_adapter.rs` (doc note).

**Acceptance criteria:**
- No new JSON-RPC method is added.
- The `fractal_submitProofHash` docstring states it accepts *any* content hash (proof hashes and package hashes alike).
- A unit/integration test submits a synthetic package hash (not a pipeline proof hash) and receives a valid `ProofCommitmentResponse`.
- If, on review, the team prefers a semantically named alias, add `fractal_commitPackage` that delegates to the same handler — but default to **reuse**.

**Tests:** submit a non-proof hash (e.g. `Hash::new(b"hello world")`) → response has network/tx/block/finalized fields, no error.

---

### AR-04 — Example binary: `commit_arbitrary_package.rs`

**Closes:** G1 + G2 (demonstration)
**Depends on:** AR-01, AR-02
**Goal:** an end-to-end runnable demo proving "upload any research package and prove it and commit to chain."

**New file:** `crates/fractal-society/examples/commit_arbitrary_package.rs`

**Behavior:**
1. Read any bytes (default: a small markdown "research package" string; accept a `--file <path>` arg if present).
2. `commit_research_package(...)` against `InMemoryCommitmentAdapter` + `InMemoryArtifactStore` (and, optionally, a live node via feature flag).
3. Print a **package card**: id, kind, author, content_hash, signature (truncated), chain_reference (network/tx/block/finalized).
4. `retrieve_research_package(content_hash)` → `verify_package(...)` → print the 4 booleans + overall `valid`.
5. Tamper the retrieved bytes in-memory and show `verify_package` now fails — proving the chain hash binds the exact bytes.

**Acceptance criteria:** runs with `cargo run -p fractal-society --example commit_arbitrary_package`; prints a clean card; ends with `valid: true` then a demonstration that tampering flips it.

---

### AR-05 — Exploration-graph (dead-end) layer

**Closes:** G3
**Depends on:** none (schema only; integration with EvidenceBundle is additive)
**Goal:** preserve what was tried and *failed*, so no agent rediscovers the same dead end.

**New file:** `crates/fractal-society/src/exploration.rs`

**New types:**
```rust
pub enum NodeKind { Hypothesis, Strategy, Approach, Config, DeadEnd, Abandoned }
pub enum NodeStatus { Active, Proven, Disproven, Abandoned, Superseded }
pub enum ProvenanceTag { Human, AiSuggested, AiExecuted, HumanRevised }

pub struct ExplorationNode {
    pub id: String,
    pub kind: NodeKind,
    pub status: NodeStatus,
    pub description: String,
    pub outcome_summary: Option<String>,
    pub parent: Option<String>,           // forms a DAG
    pub children: Vec<String>,
    pub evidence_ref: Option<Hash>,       // link to EvidenceBundle / proof
    pub provenance: ProvenanceTag,
    pub dead_end_reason: Option<String>,  // populated when kind == DeadEnd/Abandoned
}

pub struct ExplorationGraph {
    pub nodes: Vec<ExplorationNode>,      // serialized deterministically (sorted by id)
}
```

**Functions:**
```rust
impl ExplorationGraph {
    pub fn new() -> Self;
    pub fn add_node(&mut self, node: ExplorationNode) -> crate::Result<()>;
    pub fn dead_ends(&self) -> Vec<&ExplorationNode>;
    pub fn content_hash(&self) -> crate::Result<Hash>;  // canonical JCS hash
}
```

**Acceptance criteria:**
- All types `Serialize`/`Deserialize`, derive nothing nondeterministic (no `HashMap` in the graph — use sorted `Vec`).
- `content_hash` is stable across runs (deterministic serialization).
- Optional: an `ExplorationGraph` can be attached to a `PipelineResult`/`EvidenceBundle` (additive `Option<ExplorationGraph>` field; do not break existing constructors).

**Tests:** build a 5-node graph (2 dead ends) → `dead_ends()` returns 2; serialize/deserialize round-trips; hash is byte-stable across two instances with the same nodes.

---

### AR-06 — Navigable artifact directory format (ARA-style layers)

**Closes:** G4
**Depends on:** AR-05 (trace/ layer uses the exploration graph)
**Goal:** a standard, agent-browsable directory layout so a Fractal Society artifact can be navigated like an ARA artifact.

**New files:** `crates/fractal-society/src/artifact_format/mod.rs`, `reader.rs`, `writer.rs`.

**Directory layout (spec):**
```
<artifact>/
  PAPER.md                 # root manifest + layer index (human + agent readable)
  logic/
    claims.md              # falsifiable assertions + proof refs
    experiments.md         # experiment/protocol description
    architecture.md        # system / agent design
  src/
    configs.md             # hyperparameters + rationale
    environment.md         # deps, seeds, versions
  trace/
    exploration.yaml       # AR-05 ExplorationGraph
  evidence/
    proof_card.md          # human-readable proof card
    scorecard.json         # machine-readable scorecard
    bundle.json            # RunBundle
    manifest.json          # ProofManifest / PublishedPackage
```

**Functions:**
```rust
pub fn write_artifact_dir(
    root: &Path,
    result: &PipelineResult,           // or PublishedPackage for non-sim packages
    graph: Option<&ExplorationGraph>,
) -> crate::Result<Hash>;              // returns the artifact root hash

pub fn read_artifact_dir(root: &Path) -> crate::Result<LoadedArtifact>;
```

**Acceptance criteria:**
- Writing then reading an artifact directory round-trips the scorecard, bundle, manifest, and exploration graph.
- `PAPER.md` is generated from the manifest and lists each layer with its file + one-line summary (progressive disclosure).
- The artifact root hash is `Hash::of` of a canonical manifest listing every file → its content hash (a tiny Merkle-ish directory root). Tampering any file changes the root hash.

**Tests:** write a synthetic artifact → read it back → assert equality of scorecard/bundle/manifest/graph; mutate one file → root hash changes.

---

### AR-07 — Epistemic rigor reviewer

**Closes:** G5
**Depends on:** none (operates on proof card + claim text)
**Goal:** a quality gate distinct from the 5 integrity verifiers — scores whether the *research claim* is well-formed and well-supported, not whether the accounting is correct.

**New file:** `crates/fractal-society/src/rigor.rs`

**New types:**
```rust
pub enum RigorDimension {
    Falsifiability,       // is the claim testable?
    EvidenceRelevance,    // does the evidence actually support the claim?
    ScopeCalibration,     // does the claim match the tested scope?
    Reproducibility,      // are seeds/configs/env fully specified?
    ClaimEvidenceBinding, // is every claim linked to evidence?
    LimitationHonesty,    // are limitations disclosed?
}

pub struct DimensionScore {
    pub dimension: RigorDimension,
    pub score: u8,            // 0..=100
    pub findings: Vec<String>,
}

pub enum Recommendation { StrongAccept, Accept, WeakAccept, WeakReject, Reject }

pub struct RigorReport {
    pub dimensions: Vec<DimensionScore>,
    pub overall: u8,
    pub recommendation: Recommendation,
    pub summary: String,
}
```

**Function:**
```rust
/// Deterministic, rule-based rigor scoring. No LLM call in the crate —
/// the rubric is mechanical so scores are reproducible. (An LLM-assisted
/// variant can live in the TS app; the crate defines the rubric + schema.)
pub fn review(
    manifest: &ProofManifest,
    scorecard: &Scorecard,
    claim: &Claim,
    graph: Option<&ExplorationGraph>,
) -> crate::Result<RigorReport>;
```

**Acceptance criteria:**
- `review` is deterministic: identical inputs → identical report.
- The rubric is *mechanical* and documented (e.g., Falsifiability = 0 if no falsifiable claim text; EvidenceRelevance derives from claim↔evidence linkage; Reproducibility drops if `environment_hash` is missing/zero).
- A claim with no evidence and no reproducibility info scores low (WeakReject/Reject); a well-bound, reproducible claim scores higher.
- Report serializes deterministically and is content-hashable (so a rigor score can itself be committed).

**Tests:** a deliberately weak claim → `Recommendation::Reject`; a fully-specified strong claim → at least `Accept`; identical inputs → byte-identical `RigorReport` hash.

---

### AR-08 — Per-entry provenance tagging

**Closes:** G6
**Depends on:** AR-05 (shares the `ProvenanceTag` enum)
**Goal:** distinguish human-confirmed entries from AI-suggested/executed ones within a run.

**Edit:** `crates/fractal-society/src/protocol.rs` (DecisionTrace / EvidenceBundle entries); `crates/fractal-society/src/simulation.rs` (kernel records provenance).

**Acceptance criteria:**
- `ProvenanceTag` (defined in AR-05's `exploration.rs`) is reused, not duplicated.
- Each recorded `DecisionTrace` carries an `Option<ProvenanceTag>` (default `Human` when produced by a human-authored agent, `AiExecuted` when produced by an autonomous loop).
- Adding the field does not change existing canonical hashes (provenance is `Option` with a stable default and is excluded from the existing integrity hashes, OR deliberately included — **decision required, see §7**).

**Tests:** a trace tagged `AiExecuted` round-trips through serialization; default tag is sane.

---

### AR-09 — Second (non-trading) domain adapter

**Closes:** validates "any research" genuinely runs
**Depends on:** none (uses the existing `DomainAdapter` contract)
**Goal:** prove the generic kernel is domain-neutral by shipping a second adapter — a **probabilistic forecasting** adapter (predict a value, score by Brier/log-loss). Trading is no longer the only adapter.

**New files:** `crates/fractal-society/src/adapters/forecasting/{mod.rs,types.rs,adapter.rs,fixtures.rs,scorecard.rs}`.

**Acceptance criteria:**
- Implements the full `DomainAdapter` contract (`id`, `capability_manifest`, `validate_protocol`, `resolve_dataset`, `create_environment`, `normalize_observation`, `validate_action`, `step`, `score`, `build_public_evidence`, `terminal_conditions`).
- No trading types imported anywhere in the module (architecture-boundary test must pass).
- Runs through `run_pipeline_default` (or a forecasting-aware variant) on synthetic fixtures and produces a signed proof.
- Determinism test: same seed → byte-identical evidence hash (mirrors the trading determinism test).

**Tests:** a forecasting run produces a valid proof; architecture-boundary test bans `trading`/`order`/`position` tokens in this module.

---

### AR-10 — TS port of package commit + verify

**Closes:** cross-repo parity for G1/G2
**Depends on:** AR-01, AR-02
**Goal:** the `fractalwork` TS app can submit and verify arbitrary research packages, mirroring the existing golden-hash / golden-proof tests.

**Edit/extend:** `fractalwork/packages/society-schema/` — add `src/commit.ts` (package hash + sign + submit shape), `src/package_verify.ts` (offline package verification), `test/golden_package.json`, `test/package_verify.test.ts`.

**Acceptance criteria:**
- `hashPackage(bytes)` in TS == `Hash::new(bytes)` in Rust (proven on golden bytes).
- TS `verifyPackage` accepts the Rust-emitted golden package and passes.
- A tampered golden package is rejected.
- (Optional, if a node is reachable) a TS helper submits a package hash via the existing JSON-RPC method.

---

## 6. Dependency graph + recommended sequencing

```
Phase A (the core ask — ship first):
  AR-01 ─┬─► AR-02 ─► AR-04  (demo)
         └─► AR-10  (TS port)
  AR-03  (parallel; just confirmation + docs)

Phase B (knowledge structure):
  AR-05 ─┬─► AR-06  (artifact format uses the graph)
         └─► AR-08  (provenance shares the enum)

Phase C (quality + generality):
  AR-07  (rigor reviewer)
  AR-09  (second adapter)
```

**Recommended dispatch order:** AR-01, AR-02, AR-03, AR-04 (lands the founder ask, demo-able), then AR-05, AR-06, AR-08, then AR-07, AR-09, then AR-10.

All Phase A tasks are parallel-safe (distinct files). AR-05 is the keystone of Phase B (AR-06 and AR-08 both depend on it). Phase C is fully independent.

---

## 7. Open decisions (resolve before dispatching AR-05/AR-08)

1. **Does `ProvenanceTag` enter the canonical integrity hash?** Including it means a human-confirmed run and an AI-executed run with identical numerics have *different* proof hashes — arguably desirable (it's a real semantic difference) but it changes the hash contract. **Recommendation:** include it, document the change.
2. **Rigor reviewer: pure-rules now, LLM-assisted later?** Keep the crate's rubric mechanical (reproducible); let the TS app add an LLM-assisted overlay that targets the same `RigorReport` schema.
3. **Package receipt type:** reuse `ProofManifest` (heavy, trading-flavored fields) vs. a lighter `PublishedPackage` (recommended). **Recommendation:** `PublishedPackage` for arbitrary packages; a simulation-produced package may *also* carry a full `ProofManifest`.
4. **`fractal_commitPackage` alias?** Reuse `fractal_submitProofHash` unless the team wants semantic clarity. **Recommendation:** reuse; document generality.

---

## 8. Verification (definition of done for this PRD)

- `cargo test -p fractal-society` passes with all new task tests green; existing 316+ tests still pass.
- `cargo clippy -p fractal-society -- -D warnings` clean; `cargo fmt --check -p fractal-society` clean.
- `cargo run -p fractal-society --example commit_arbitrary_package` prints a package card ending in `valid: true` and demonstrates tamper-detection.
- `cargo run -p fractal-society --example run_real_strategy` still works (no regression to the trading pipeline).
- A package committed via AR-01 verifies offline in Rust (AR-02) **and** in TS (AR-10) — cross-language parity proven on golden bytes.
- The architecture-boundary test still bans domain tokens in generic modules (and now also in the forecasting adapter module).

---

## 9. Traceability

| Gap | Tasks | Founder ask? |
|---|---|---|
| G1 upload→hash→commit | AR-01, AR-03, AR-04 | **yes** |
| G2 pull→verify | AR-02, AR-04, AR-10 | **yes** |
| G3 dead-ends | AR-05, AR-06 | no |
| G4 navigable format | AR-06 | no |
| G5 rigor scoring | AR-07 | no |
| G6 provenance | AR-05, AR-08 | no |
| generality proof | AR-09 | no |
