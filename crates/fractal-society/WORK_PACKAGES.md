# Fractal Society — Parallel Work Packages

**Status:** Scaffolded & ready. 12 independent packages. Zero file-overlap.
**Crate:** `crates/fractal-society` (repo: `/Users/jamesstar/fractalchain`)
**This file:** `crates/fractal-society/WORK_PACKAGES.md`

> **Assign agents by number:** tell an agent *"do package N"*. They read package N
> below, implement it, and open a PR. Packages are fully independent — run as many
> in parallel as you have agents.

---

## How an agent works a package (read first)

Each package owns **exactly two files**, both pre-allocated:

1. `crates/fractal-society/src/pkgs/<name>.rs` — an architect-owned **stub**. Replace its contents with the implementation.
2. `crates/fractal-society/tests/wp_<name>.rs` — **create** this file (your tests).

**An agent edits ONLY those two files.** Everything else is forbidden (see each package's *Files Forbidden*). The crate already builds with the empty stubs, so an agent can run their test in isolation — other packages' empty stubs do not affect them.

**Workflow per agent:**
```sh
git checkout -b wp-<N>-<name>          # one branch per agent
# implement src/pkgs/<name>.rs, create tests/wp_<name>.rs
cargo fmt   -p fractal-society -- --check
cargo clippy -p fractal-society --all-targets -- -D warnings 2>&1 | grep fractal-society || echo clean
cargo test  -p fractal-society --test wp_<name> --nocapture
git add crates/fractal-society/src/pkgs/<name>.rs crates/fractal-society/tests/wp_<name>.rs
git commit -m "fractal-society: package <N> <name>"
```

**Global rules (all packages):**
- **No architecture decisions.** Implement exactly the interface in each package.
- **Deterministic only.** No `Utc::now()`, `OsRng`, `SystemTime`, or wall clocks. Set `VerifierReport.execution_time_seconds = 0.0` (do **not** measure time — it would break hash reproducibility).
- **No new dependencies.** Use only what `fractal-society` already depends on.
- **`#![deny(missing_docs)]`** is on — document every public item.
- **Do not edit** `src/pkgs/mod.rs`, `src/lib.rs`, any other `src/` file, `Cargo.toml`, `Cargo.lock`, or any other package's files.

---

## Dependency graph

All 12 packages depend **only on the stable public API of `fractal-society`**
(`protocol`, `verifier`, `kernel`, `simulation`, `signing`, `canonical`,
`adapters`). **No package depends on any other package.** The graph is flat —
fully parallelizable up to 12-wide.

```
                          ┌──────────────────────────┐
                          │  fractal-society base    │
                          │  (stable public API)     │
                          └───────────┬──────────────┘
        ┌────────┬────────┬──────────┼──────────┬────────┬────────┐
        ▼        ▼        ▼          ▼          ▼        ▼        ▼
  (1) acct  (2) cost  (3) risk  (4) repro  (5) data  (6) score  (7) proof
        ▼        ▼                                                        ▼
  (8) merkle  (9) disclosure  (10) reputation  (11) reward  (12) agent-freeze
```

**Edges between packages: none.** Each is a leaf against the base.

---

## Overlap analysis → **zero**

| File | Touched by |
|---|---|
| `src/pkgs/<name>.rs` (×12) | exactly one agent each |
| `tests/wp_<name>.rs` (×12) | exactly one agent each |
| `src/pkgs/mod.rs` | **architect only** (pre-declares all 12; forbidden to agents) |
| `src/lib.rs` | **architect only** (already adds `pub mod pkgs;`) |
| everything else | forbidden to all agents |

Because `src/pkgs/mod.rs` is pre-declared by the architect and the empty stubs
already compile, agents never touch a shared file → **literal zero merge
conflict surface**. (A later integration task may relocate finished modules into
their canonical homes.)

---

## Priority ranking

| Pri | Pkg | Gate / phase | Why first |
|---|---|---|---|
| 1 | (1) accounting_integrity | P06 | validation core; catches ledger/tamper bugs |
| 2 | (2) cost_completeness | P06 | validation core; fees+funding honesty |
| 3 | (4) reproducibility | P06 | proves runs replay; backbone of "prove in public" |
| 4 | (3) risk_policy | P06 | validation core; policy-violation consistency |
| 5 | (5) dataset_integrity | P06 | validation core; manifest validity |
| 6 | (6) scorecard_reproduction | P06 | validation core; scorecard honesty |
| 7 | (7) proof_manifest | P07 | the signed public artifact |
| 8 | (8) merkle_commitment | P07 | compact trace commitment |
| 9 | (9) disclosure_tiers | P07 | public-proof privacy gate |
| 10 | (12) agent_manifest_freeze | P05 | tamper-evident submission |
| 11 | (10) reputation_events | P10 | reputation from verified work |
| 12 | (11) reward_gate | P10 | reward release policy |

Suggested batch order for parallel agents: **Tier 1 = (1)(2)(3)(4)(5)(6)** first
(six verifiers, fully parallel), then **Tier 2 = (7)(8)(9)**, then **(12)**,
then **(10)(11)**.

---

# The packages

> Each `pub fn` below is the **exact interface** to implement. Types come from the
> paths shown. `VerifierReport` lives at `fractal_society::verifier::VerifierReport`
> (fields: `id, verifier_id, verifier_version, passed: bool, score: Option<f64>,
> details: serde_json::Value, warnings: Vec<String>, errors: Vec<String>,
> execution_time_seconds: f64, timestamp: DateTime<Utc>`).

---

## ☐ Package 1 — accounting_integrity

**Goal:** Re-verify per-step equity reconciliation over a run's evidence.

**Context:** Closes part of PHASE-06 (P06-N03 "accounting corruption … fail verified status"). Reads `EvidenceBundle.decision_traces[*].outcome` (JSON). For each step whose outcome has `equity`, `cash`, `position_notional`, assert `|equity − (cash + position_notional)| ≤ tolerance`.

**Interface:**
```rust
use fractal_society::protocol::EvidenceBundle;
use fractal_society::verifier::VerifierReport;

/// Verifier id for this package.
pub const VERIFIER_ID: &str = "accounting-integrity";

/// Return a VerifierReport; `passed` is true iff every step reconciles within `tolerance`.
pub fn verify(evidence: &EvidenceBundle, tolerance: f64) -> VerifierReport;
```

**Files allowed:** `src/pkgs/accounting_integrity.rs`, `tests/wp_accounting_integrity.rs`
**Files forbidden:** everything else (esp. `pkgs/mod.rs`, `lib.rs`, `ledger.rs`, `adapter.rs`).
**Dependencies:** none (only `EvidenceBundle`, `VerifierReport`).
**Acceptance tests:**
- `passes_for_clean_run`: build evidence from a `TradingAdapter` kernel run → `passed == true`.
- `fails_for_tampered_equity`: clone that evidence, set one `decision_traces[i].outcome["equity"]` to a wrong number → `passed == false` and `errors` non-empty.
- `verifier_id` equals `VERIFIER_ID`; `execution_time_seconds == 0.0`.
**Deliverables:** implemented module + 3 tests; clippy/fmt clean.
**Complexity:** ~1.5 h.

---

## ☐ Package 2 — cost_completeness

**Goal:** Re-verify the fees+funding PnL invariant per step.

**Context:** PHASE-06 cost-honesty. For each step outcome with `total_pnl`, `realized_pnl`, `unrealized_pnl`, `fees`, assert `|total_pnl − (realized_pnl + unrealized_pnl − fees)| ≤ tolerance`.

**Interface:**
```rust
use fractal_society::protocol::EvidenceBundle;
use fractal_society::verifier::VerifierReport;
pub const VERIFIER_ID: &str = "cost-completeness";
pub fn verify(evidence: &EvidenceBundle, tolerance: f64) -> VerifierReport;
```

**Files allowed:** `src/pkgs/cost_completeness.rs`, `tests/wp_cost_completeness.rs`
**Files forbidden:** everything else.
**Dependencies:** none.
**Acceptance tests:** `passes_for_clean_run`; `fails_when_fees_omitted` (tamper `fees` to 0 on a step that had fees → fail); id/zero-time checks.
**Deliverables:** module + 3 tests.
**Complexity:** ~1.5 h.

---

## ☐ Package 3 — risk_policy

**Goal:** Re-verify policy-violation consistency.

**Context:** PHASE-06. Count rejected steps (outcome JSON has a `"rejected"` field) and confirm the run's reported `policy_violations` metric matches that count. Detects hidden/under-reported violations.

**Interface:**
```rust
use fractal_society::protocol::EvidenceBundle;
use fractal_society::verifier::VerifierReport;
pub const VERIFIER_ID: &str = "risk-policy";
/// `evidence.metrics` must contain "policy_violations".
pub fn verify(evidence: &EvidenceBundle) -> VerifierReport;
```

**Files allowed:** `src/pkgs/risk_policy.rs`, `tests/wp_risk_policy.rs`
**Files forbidden:** everything else.
**Dependencies:** none.
**Acceptance tests:** `passes_when_consistent` (real run); `fails_when_count_mismatched` (set `evidence.metrics["policy_violations"]` to a wrong value → fail); id/zero-time.
**Deliverables:** module + 3 tests.
**Complexity:** ~1.5 h.

---

## ☐ Package 4 — reproducibility

**Goal:** Re-run an adapter+agent from a `RunManifest` and confirm the reproduced `evidence_hash` matches the original.

**Context:** PHASE-06 / P02-N10. Uses `fractal_society::kernel::replay`. Test with the `ReferenceAdapter` (bandit).

**Interface:**
```rust
use fractal_society::adapters::{ReferenceAdapter, ReferenceAgent};
use fractal_society::kernel::RunManifest;
use fractal_society::protocol::Hash;
use fractal_society::verifier::VerifierReport;
pub const VERIFIER_ID: &str = "reproducibility";
pub async fn verify(
    original_hash: &Hash,
    manifest: &RunManifest,
    rebuild: impl FnOnce() -> (ReferenceAdapter, ReferenceAgent) + Send,
) -> VerifierReport;
```

**Files allowed:** `src/pkgs/reproducibility.rs`, `tests/wp_reproducibility.rs`
**Files forbidden:** everything else.
**Dependencies:** none (uses `kernel::replay`, `ReferenceAdapter`).
**Acceptance tests:** `passes_when_replay_matches` (run once → capture hash+manifest; rebuild fresh adapter+agent; verify → `passed`); `fails_when_hash_differs` (pass a wrong `original_hash` → `!passed`); id/zero-time.
**Deliverables:** module + 3 tests (use `#[tokio::test]`).
**Complexity:** ~2 h.

---

## ☐ Package 5 — dataset_integrity

**Goal:** Validate `DatasetManifest` structure and hash well-formedness.

**Context:** PHASE-03/06. A manifest is valid iff `id` non-empty, `schema_version` non-empty, and `content_hash` parses as 64-hex (`Hash::from_hex`).

**Interface:**
```rust
use fractal_society::protocol::DatasetManifest;
use fractal_society::verifier::VerifierReport;
pub const VERIFIER_ID: &str = "dataset-integrity";
pub fn verify(manifest: &DatasetManifest) -> VerifierReport;
```

**Files allowed:** `src/pkgs/dataset_integrity.rs`, `tests/wp_dataset_integrity.rs`
**Files forbidden:** everything else.
**Dependencies:** none.
**Acceptance tests:** `passes_for_valid_manifest`; `fails_for_bad_hash` (content_hash = "zz" → fail); `fails_for_empty_id` (id = "" → fail); id/zero-time.
**Deliverables:** module + 4 tests.
**Complexity:** ~1 h.

---

## ☐ Package 6 — scorecard_reproduction

**Goal:** Re-derive the scorecard's `total_pnl` from the evidence and confirm it matches the `Scorecard`.

**Context:** PHASE-06 scorecard honesty. Derive `total_pnl` from the last parseable outcome in the evidence; compare to `scorecard.primary_metrics["total_pnl"].value`.

**Interface:**
```rust
use fractal_society::protocol::EvidenceBundle;
use fractal_society::verifier::{Scorecard, VerifierReport};
pub const VERIFIER_ID: &str = "scorecard-reproduction";
pub fn verify(evidence: &EvidenceBundle, scorecard: &Scorecard, tolerance: f64) -> VerifierReport;
```

**Files allowed:** `src/pkgs/scorecard_reproduction.rs`, `tests/wp_scorecard_reproduction.rs`
**Files forbidden:** everything else.
**Dependencies:** none (`EvidenceBundle`, `Scorecard`).
**Acceptance tests:** build evidence via a `TradingAdapter` run, build a `Scorecard` via `adapters::trading::build_scorecard` → `passes_when_consistent`; tamper the scorecard's `total_pnl` → `fails_when_mismatched`; id/zero-time.
**Deliverables:** module + 3 tests.
**Complexity:** ~2 h.

---

## ☐ Package 7 — proof_manifest

**Goal:** Build and Ed25519-sign a `ProofManifest` from a run outcome + scorecard.

**Context:** PHASE-07 (P07-N01). Hash each referenced artifact canonically; set `disclosure = Visibility::CommittedPrivate`; sign with `ProofManifest::author_signature_hex`.

**Interface:**
```rust
use chrono::{DateTime, Utc};
use fractal_society::kernel::RunOutcome;
use fractal_society::protocol::{Hash, ProofManifest, Visibility};
use fractal_society::signing::AuthorSigner;
pub fn build(
    run: &RunOutcome,
    scorecard: &fractal_society::verifier::Scorecard,
    signer: &AuthorSigner,
    timestamp: DateTime<Utc>,
) -> fractal_society::Result<ProofManifest>;
```
Set fields: `claim_id = run.manifest.run_id`, `protocol_hash = Hash::of(&run.manifest)`, `agent_hash = Hash::of(&run.manifest.agent_id)`, `dataset_hash = Hash::new(b"dataset")`, `environment_hash = Hash::new(b"environment")`, `trace_merkle_root = run.evidence_hash.clone()`, `verifier_set_hash = Hash::new(b"verifiers")`, `scorecard_hash = Hash::of(scorecard)`, `disclosure = Visibility::CommittedPrivate`, `author_signature` from `author_signature_hex(signer)`, `platform_attestation = None`, `chain_reference = None`, `manifest_version = "1.0.0"`.

**Files allowed:** `src/pkgs/proof_manifest.rs`, `tests/wp_proof_manifest.rs`
**Files forbidden:** everything else.
**Dependencies:** none (`RunOutcome`, `ProofManifest`, `AuthorSigner`, `Hash`).
**Acceptance tests:** build a manifest → `verify_author(&signer.public_key())` is `Ok`; mutate `claim_id` → `verify_author` `Err`; `disclosure == Visibility::CommittedPrivate`.
**Deliverables:** module + 2 tests.
**Complexity:** ~2 h.

---

## ☐ Package 8 — merkle_commitment

**Goal:** Merkle root + inclusion proof over decision-trace observation hashes.

**Context:** PHASE-07 (P07-N02). Leaves = `decision_traces[*].observation_hash` bytes (decode 64-hex). Pairwise SHA-256 (`fractal_crypto::sha256`); duplicate the last leaf on odd levels.

**Interface (define `InclusionProof` locally):**
```rust
use fractal_society::protocol::{EvidenceBundle, Hash};
pub struct InclusionProof { pub index: usize, pub siblings: Vec<Hash> }
pub fn root(evidence: &EvidenceBundle) -> Hash;
pub fn prove(evidence: &EvidenceBundle, index: usize) -> Option<InclusionProof>;
pub fn verify(leaf: &Hash, proof: &InclusionProof, root: &Hash) -> bool;
```

**Files allowed:** `src/pkgs/merkle_commitment.rs`, `tests/wp_merkle_commitment.rs`
**Files forbidden:** everything else.
**Dependencies:** none (`Hash`, `EvidenceBundle`, `fractal_crypto::sha256` via `fractal_crypto`).
**Acceptance tests:** `root_is_stable` (same evidence → same root); `valid_proof_verifies` (`prove(i)` → `verify(leaf_i, proof, root) == true`); `wrong_leaf_rejected` (different leaf → false); empty-evidence root is a fixed empty hash (no panic).
**Deliverables:** module + 4 tests.
**Complexity:** ~3 h.

---

## ☐ Package 9 — disclosure_tiers

**Goal:** Produce a tier-redacted copy of an `EvidenceBundle` so disclosure never leaks raw observations/actions.

**Context:** PHASE-07 (P07-N04). `Private` → empty `decision_traces`; `CommittedPrivate` → keep `step/observation_hash/timestamp`, blank `action`+`outcome` (e.g. `serde_json::Value::Null`); `PartialPublic` → keep `action`, blank `outcome`; `Open` → unchanged; `ReviewerAccess` → same as `CommittedPrivate`.

**Interface:**
```rust
use fractal_society::protocol::{EvidenceBundle, Visibility};
pub fn redact(evidence: &EvidenceBundle, tier: Visibility) -> EvidenceBundle;
```

**Files allowed:** `src/pkgs/disclosure_tiers.rs`, `tests/wp_disclosure_tiers.rs`
**Files forbidden:** everything else.
**Dependencies:** none (`EvidenceBundle`, `Visibility`).
**Acceptance tests:** `committed_private_hides_raw` (no step's action/outcome is a non-null object); `private_empties_traces`; `open_is_identity` (result serializes equal to input); `id_and_run_id_preserved`.
**Deliverables:** module + 4 tests.
**Complexity:** ~2 h.

---

## ☐ Package 10 — reputation_events

**Goal:** Derive reputation events from verifier/review outcomes.

**Context:** PHASE-10 (P10-N06). Define `ReputationEvent` + `ReputationKind` locally.

**Interface:**
```rust
use chrono::{DateTime, Utc};
use fractal_society::protocol::Hash;
use fractal_society::verifier::VerifierReport;
pub enum ReputationKind { VerifiedPass, VerifiedFail, ReviewApproved, ReviewRejected }
pub struct ReputationEvent {
    pub id: String,
    pub subject: String,
    pub kind: ReputationKind,
    pub delta: i64,
    pub evidence_ref: Hash,
    pub timestamp: DateTime<Utc>,
}
pub fn from_verifier(report: &VerifierReport, subject: &str, timestamp: DateTime<Utc>) -> ReputationEvent; // pass→+1, fail→-1
pub fn from_review(subject: &str, approved: bool, timestamp: DateTime<Utc>) -> ReputationEvent;            // approved→+2, rejected→-2
```

**Files allowed:** `src/pkgs/reputation_events.rs`, `tests/wp_reputation_events.rs`
**Files forbidden:** everything else.
**Dependencies:** none (`VerifierReport`, `Hash`).
**Acceptance tests:** passing report → `delta == 1` and `VerifiedPass`; failing report → `delta == -1`; approved review → `delta == 2`; events are deterministic given inputs.
**Deliverables:** module + 4 tests.
**Complexity:** ~1.5 h.

---

## ☐ Package 11 — reward_gate

**Goal:** Decide whether a reward may release.

**Context:** PHASE-10 (P10-N08). Release iff every verifier `passed` AND the challenge window is closed AND at least `min_required_pass` verifiers passed.

**Interface (define `RewardDecision` locally):**
```rust
use fractal_society::verifier::VerifierReport;
pub enum RewardDecision { Release, Withhold { reasons: Vec<String> } }
pub fn evaluate(
    verifier_reports: &[VerifierReport],
    challenge_window_open: bool,
    min_required_pass: usize,
) -> RewardDecision;
```

**Files allowed:** `src/pkgs/reward_gate.rs`, `tests/wp_reward_gate.rs`
**Files forbidden:** everything else.
**Dependencies:** none (`VerifierReport`).
**Acceptance tests:** all-pass + window-closed → `Release`; one-fail → `Withhold`; window-open → `Withhold`; too-few-pass → `Withhold`.
**Deliverables:** module + 4 tests.
**Complexity:** ~1.5 h.

---

## ☐ Package 12 — agent_manifest_freeze

**Goal:** Build a tamper-evident frozen `AgentManifest` from agent metadata.

**Context:** PHASE-05 (P05-N06). `code_hash = Hash::new(&code_bytes)`; freeze versions/ids.

**Interface (define `FreezeInput` locally):**
```rust
use fractal_society::protocol::AgentManifest;
pub struct FreezeInput {
    pub agent_id: String,
    pub author: String,
    pub version: String,
    pub code_bytes: Vec<u8>,
    pub tool_allowlist: Vec<String>,
    pub license: String,
}
pub fn freeze(input: FreezeInput) -> fractal_society::Result<AgentManifest>;
```
Fill remaining `AgentManifest` fields with safe defaults (`model_ref = None`, `system_prompt = None`, `skill_dependencies = vec![]`, `resource_limits` small defaults, `network_policy` deny-by-default).

**Files allowed:** `src/pkgs/agent_manifest_freeze.rs`, `tests/wp_agent_manifest_freeze.rs`
**Files forbidden:** everything else.
**Dependencies:** none (`AgentManifest`, `Hash`).
**Acceptance tests:** `freeze` is deterministic (same input → equal `code_hash`); one-byte change in `code_bytes` → different `code_hash`; `id`/`version`/`author` round-trip; network denied by default.
**Deliverables:** module + 3 tests.
**Complexity:** ~2 h.

---

## Integration follow-up (not part of any package)

After packages land, a single integration task (not parallel) may:
1. Promote `pkgs/*` modules into canonical homes (`verification/`, `protocol` helpers, etc.) and update `pub use` re-exports.
2. Add a `verify_all(evidence) -> Vec<VerifierReport>` aggregator once the verifier packages exist.
3. Wire `build_proof_manifest` + `merkle_commitment` + `disclosure_tiers` into a `ProofManifest` pipeline.

This is intentionally separate so the 12 packages stay conflict-free.

---

# Packages 13–25 (second batch)

Same rules as the first batch: each agent edits **only** `src/pkgs/<name>.rs`
(replace the stub) **and** creates `tests/wp_<name>.rs`. Everything else is
forbidden. The architect-owned `src/pkgs/mod.rs` already declares all 13; the
crate builds with the empty stubs, so each agent is fully isolated. Deterministic
only; `VerifierReport.execution_time_seconds = 0.0`; no new deps;
`#![deny(missing_docs)]`.

**Dependency graph (second batch):** still flat — every package depends only on
the stable `fractal-society` base API, none on each other, none on packages 1–12.
**Overlap with batch 1 and within batch 2:** zero (each owns a unique file pair).

**Priority (within this batch):** Tier 1b verifiers = **(13)(14)(15)**; Tier 2b
proof/commitment = **(16)(17)(18)**; Tier 3 arena = **(19)(20)(21)**; Tier 4
graph/reputation/fraud = **(22)(23)(24)**; utility = **(25)**.

---

## ☐ Package 13 — sandbox_policy

**Goal:** Verify a run's recorded actions stayed within a declared tool allowlist.

**Context:** PHASE-06 sandbox compliance (P06-N11). Scan each `decision_traces[*].action` JSON for a `"tool"` field; if present and not in `allowed_tools`, fail.

**Interface:**
```rust
use fractal_society::protocol::EvidenceBundle;
use fractal_society::verifier::VerifierReport;
pub const VERIFIER_ID: &str = "sandbox-policy";
pub fn verify(evidence: &EvidenceBundle, allowed_tools: &[String]) -> VerifierReport;
```

**Files allowed:** `src/pkgs/sandbox_policy.rs`, `tests/wp_sandbox_policy.rs`  •  **Forbidden:** everything else.
**Dependencies:** none.
**Acceptance tests:** `passes_when_no_tool_use`; `passes_when_tool_allowed` (action `{"tool":"calc"}` + allow `["calc"]`); `fails_when_tool_not_allowed`; id/zero-time.
**Complexity:** ~1.5 h.

---

## ☐ Package 14 — temporal_leakage

**Goal:** Detect non-monotonic / duplicate decision-trace steps (tamper or look-ahead signal).

**Context:** PHASE-06 (P06-N02). Steps must be strictly increasing.

**Interface:**
```rust
use fractal_society::protocol::EvidenceBundle;
use fractal_society::verifier::VerifierReport;
pub const VERIFIER_ID: &str = "temporal-leakage";
pub fn verify(evidence: &EvidenceBundle) -> VerifierReport;
```

**Files allowed:** `src/pkgs/temporal_leakage.rs`, `tests/wp_temporal_leakage.rs`  •  **Forbidden:** everything else.
**Dependencies:** none.
**Acceptance tests:** `passes_for_monotonic` (a real run); `fails_for_regressed_step` (swap two traces so a later step has a smaller index → fail); `fails_for_duplicate_step`; id/zero-time.
**Complexity:** ~1.5 h.

---

## ☐ Package 15 — baseline_correctness

**Goal:** Re-verify the baseline-comparison arithmetic stored in a `Scorecard`.

**Context:** PHASE-06 / P04-N08. For each `BaselineResult`, re-derive `difference = candidate_value − baseline_value`, `percent_difference`, `is_better`, and compare to the stored values.

**Interface:**
```rust
use fractal_society::verifier::{Scorecard, VerifierReport};
pub const VERIFIER_ID: &str = "baseline-correctness";
pub fn verify(scorecard: &Scorecard, tolerance: f64) -> VerifierReport;
```

**Files allowed:** `src/pkgs/baseline_correctness.rs`, `tests/wp_baseline_correctness.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`Scorecard`).
**Acceptance tests:** `passes_for_consistent_scorecard` (build one via `adapters::trading::build_scorecard`); `fails_when_difference_tampered` (mutate a `BaselineResult.difference`); id/zero-time.
**Complexity:** ~1.5 h.

---

## ☐ Package 16 — chain_commitment

**Goal:** Interface-first chain-commitment adapter plus a deterministic in-memory mock.

**Context:** PHASE-07 (P07-N03/N10). Define the `CommitmentAdapter` trait; the mock returns a `ChainReference` for a submitted proof hash.

**Interface:**
```rust
use fractal_society::protocol::{ChainReference, Hash};
pub trait CommitmentAdapter: Send + Sync {
    /// Commit `proof_hash`; return the on-chain reference.
    fn submit(&self, proof_hash: &Hash) -> fractal_society::Result<ChainReference>;
}
pub struct InMemoryCommitmentAdapter { /* network, next_block */ }
impl InMemoryCommitmentAdapter {
    pub fn new(network: impl Into<String>, starting_block: u64) -> Self;
}
impl CommitmentAdapter for InMemoryCommitmentAdapter { /* submit increments block, deterministic tx hash from proof_hash+block */ }
```

**Files allowed:** `src/pkgs/chain_commitment.rs`, `tests/wp_chain_commitment.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`Hash`, `ChainReference`).
**Acceptance tests:** `submit` returns a `ChainReference` with the configured network and `finalized == true`; two submits produce distinct increasing `block_number`; deterministic for a given `starting_block`.
**Complexity:** ~2 h.

---

## ☐ Package 17 — reviewer_grants

**Goal:** Issue, revoke, and validate reviewer access grants using logical (non-wall-clock) time.

**Context:** PHASE-07 (P07-N05).

**Interface:**
```rust
pub struct ReviewerGrant {
    pub proof_id: String,
    pub reviewer: String,
    pub granted_at: u64,
    pub expires_at: u64,
    pub revoked: bool,
}
pub fn issue(proof_id: &str, reviewer: &str, granted_at: u64, ttl_seconds: u64) -> ReviewerGrant;
pub fn revoke(grant: &mut ReviewerGrant);
pub fn is_valid(grant: &ReviewerGrant, now: u64) -> bool; // not revoked AND now < expires_at
```

**Files allowed:** `src/pkgs/reviewer_grants.rs`, `tests/wp_reviewer_grants.rs`  •  **Forbidden:** everything else.
**Dependencies:** none.
**Acceptance tests:** valid before expiry & not revoked; `revoke` → invalid; `now >= expires_at` → invalid; `expires_at == granted_at + ttl_seconds`.
**Complexity:** ~1 h.

---

## ☐ Package 18 — proof_level_resolver

**Goal:** Derive the `ProofLevel` for a proof from evidence + reviews + replications (not author-chosen).

**Context:** PHASE-07 (P07-N06). Monotone ladder: `PrivateDraft` → `Committed` (evidence non-empty) → `Auditable` (≥1 approved `Review`) → `Reproducible` (≥1 successful `Replication`); take the max reached.

**Interface:**
```rust
use fractal_society::protocol::EvidenceBundle;
use fractal_society::verifier::{ProofLevel, Replication, Review};
pub fn resolve(evidence: &EvidenceBundle, reviews: &[Review], replications: &[Replication]) -> ProofLevel;
```

**Files allowed:** `src/pkgs/proof_level_resolver.rs`, `tests/wp_proof_level_resolver.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`EvidenceBundle`, `Review`, `Replication`, `ProofLevel`).
**Acceptance tests:** empty inputs → `PrivateDraft`; ≥1 approved review (and non-empty evidence) → `Auditable`; ≥1 successful replication → `Reproducible`; deterministic.
**Complexity:** ~2 h.

---

## ☐ Package 19 — season_state_machine

**Goal:** Agent-Arena season lifecycle (`Draft→Open→Frozen→Final→Closed`) with rules frozen once a season opens.

**Context:** PHASE-08 (P08-N01).

**Interface:**
```rust
use fractal_society::protocol::Hash;
pub enum SeasonState { Draft, Open, Frozen, Final, Closed }
pub struct Season { pub id: String, pub state: SeasonState, pub rules_hash: Hash, pub rules_frozen: bool }
pub fn new_season(id: impl Into<String>, rules_hash: Hash) -> Season;
pub fn open(season: &mut Season) -> fractal_society::Result<()>;    // Draft→Open; sets rules_frozen=true
pub fn freeze(season: &mut Season) -> fractal_society::Result<()>;  // Open→Frozen
pub fn finalize(season: &mut Season) -> fractal_society::Result<()>;// Frozen→Final
pub fn close(season: &mut Season) -> fractal_society::Result<()>;   // Final→Closed
```

**Files allowed:** `src/pkgs/season_state_machine.rs`, `tests/wp_season_state_machine.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`Hash`).
**Acceptance tests:** legal transitions succeed; illegal transition (e.g. `Draft→Freeze`) returns `Err`; `rules_frozen` becomes true after `open` and `rules_hash` is unchanged; full lifecycle reaches `Closed`.
**Complexity:** ~2 h.

---

## ☐ Package 20 — leaderboard

**Goal:** Rank candidates by a risk/robustness-weighted score (never raw PnL alone), deterministic tie-break.

**Context:** PHASE-08 (P08-N04/N07).

**Interface:**
```rust
pub struct LeaderboardEntry { pub agent_id: String, pub net_return: f64, pub max_drawdown: f64, pub policy_violations: u64 }
pub struct RankedEntry { pub rank: u32, pub entry: LeaderboardEntry, pub score: f64 }
/// score = net_return − 0.5 * max_drawdown − 0.1 * policy_violations; sort desc, tie-break agent_id asc.
pub fn rank(entries: &[LeaderboardEntry]) -> Vec<RankedEntry>;
```

**Files allowed:** `src/pkgs/leaderboard.rs`, `tests/wp_leaderboard.rs`  •  **Forbidden:** everything else.
**Dependencies:** none.
**Acceptance tests:** higher score ranks first; drawdown and violations penalize rank vs raw return; equal scores tie-break by `agent_id`; ranks are contiguous from 1.
**Complexity:** ~1.5 h.

---

## ☐ Package 21 — submission_freeze

**Goal:** Freeze a candidate submission into a tamper-evident manifest hash.

**Context:** PHASE-08 (P08-N02). Submission = agent+protocol+dataset+env hashes + attempt.

**Interface:**
```rust
use fractal_society::protocol::Hash;
pub struct Submission {
    pub agent_hash: Hash, pub protocol_hash: Hash, pub dataset_hash: Hash,
    pub env_hash: Hash, pub attempt: u32,
}
impl Submission {
    pub fn new(agent_hash: Hash, protocol_hash: Hash, dataset_hash: Hash, env_hash: Hash, attempt: u32) -> Self;
    pub fn manifest_hash(&self) -> fractal_society::Result<Hash>; // Hash::of(self)
}
```

**Files allowed:** `src/pkgs/submission_freeze.rs`, `tests/wp_submission_freeze.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`Hash`).
**Acceptance tests:** identical inputs → identical `manifest_hash`; changing any one hash or `attempt` → different hash; deterministic.
**Complexity:** ~1 h.

---

## ☐ Package 22 — graph_projection

**Goal:** Minimal relational research graph (nodes + edges) built from records.

**Context:** PHASE-10 (P10-N01/N02).

**Interface:**
```rust
use std::collections::HashSet;
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum GraphNode { Person(String), Agent(String), Run(String), Proof(String), Review(String) }
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum GraphEdge { Created, Used, VerifiedBy, ReviewedBy, ReplicatedBy }
pub struct ResearchGraph { /* nodes: HashSet<GraphNode>, edges: HashSet<(GraphNode, GraphNode, GraphEdge)> */ }
impl ResearchGraph {
    pub fn new() -> Self;
    pub fn add_node(&mut self, node: GraphNode);
    pub fn add_edge(&mut self, from: GraphNode, to: GraphNode, edge: GraphEdge);
    pub fn node_count(&self) -> usize;
    pub fn edge_count(&self) -> usize;
    pub fn has_edge(&self, from: &GraphNode, to: &GraphNode, edge: &GraphEdge) -> bool;
}
impl Default for ResearchGraph { fn default() -> Self { Self::new() } }
```

**Files allowed:** `src/pkgs/graph_projection.rs`, `tests/wp_graph_projection.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`std::collections::HashSet`).
**Acceptance tests:** counts correct; duplicate node/edge adds do not double-count; `has_edge` true/false correctly; `Default` works.
**Complexity:** ~2 h.

---

## ☐ Package 23 — review_conflicts

**Goal:** Apply review conflict-of-interest rules (reject self-review and direct financial-conflict reviews).

**Context:** PHASE-10 (P10-N04).

**Interface:**
```rust
pub struct ReviewRequest { pub reviewer: String, pub proof_author: String, pub financial_interests: Vec<String> }
pub enum ConflictOutcome { Accept, Reject { reason: String } }
pub fn check(request: &ReviewRequest) -> ConflictOutcome;
```

**Files allowed:** `src/pkgs/review_conflicts.rs`, `tests/wp_review_conflicts.rs`  •  **Forbidden:** everything else.
**Dependencies:** none.
**Acceptance tests:** reviewer == proof_author → `Reject`; non-empty `financial_interests` → `Reject`; otherwise `Accept`.
**Complexity:** ~1 h.

---

## ☐ Package 24 — sybil_detection

**Goal:** Flag suspicious review patterns (self-review, duplicate review, circular review).

**Context:** PHASE-10 (P10-N07).

**Interface:**
```rust
pub struct ReviewRecord { pub reviewer: String, pub subject: String }
pub enum SuspiciousPattern {
    SelfReview { reviewer: String },
    DuplicateReview { reviewer: String, subject: String },
    CircularReview { cycle: Vec<String> },
}
pub fn analyze(reviews: &[ReviewRecord]) -> Vec<SuspiciousPattern>;
```

**Files allowed:** `src/pkgs/sybil_detection.rs`, `tests/wp_sybil_detection.rs`  •  **Forbidden:** everything else.
**Dependencies:** none.
**Acceptance tests:** `reviewer==subject` → `SelfReview`; same `(reviewer,subject)` twice → `DuplicateReview`; `A→B` and `B→A` → `CircularReview`; clean set → empty.
**Complexity:** ~2.5 h.

---

## ☐ Package 25 — gap_detection

**Goal:** Detect gaps in an ordered integer sequence (e.g. bar timestamps / sequence numbers) and emit explicit `DataGap` records (never silently interpolate).

**Context:** PHASE-03 (P03-N04).

**Interface:**
```rust
pub struct DataGap { pub after: i64, pub before: i64, pub missing_count: i64 }
/// Sorts a copy ascending, then emits one `DataGap` per consecutive pair with diff > 1.
pub fn detect(sequence: &[i64]) -> Vec<DataGap>;
```

**Files allowed:** `src/pkgs/gap_detection.rs`, `tests/wp_gap_detection.rs`  •  **Forbidden:** everything else.
**Dependencies:** none.
**Acceptance tests:** `[1,2,4,7]` → gaps `{after:2,before:4,missing:1}` and `{after:4,before:7,missing:2}`; contiguous sequence → none; empty → none; unsorted input is sorted first (same result as its sorted form).
**Complexity:** ~1 h.

---

## Batch summary (1–25)

- **25 packages total**, all flat against the base crate, zero inter-package file overlap.
- Combined estimated effort: ~38–45 h of fully parallelizable work.
- Suggested wave plan: **Wave A = 1–6** • **Wave B = 13–15 + 7–9** • **Wave C = 16–18 + 10–12** • **Wave D = 19–21 + 22–25**.

---

# Packages 26–37 (third batch)

Same rules: each agent edits **only** `src/pkgs/<name>.rs` (replace the stub) **and**
creates `tests/wp_<name>.rs`. Everything else forbidden. `pkgs/mod.rs` already
declares all 12; crate builds with empty stubs. Deterministic only;
`execution_time_seconds = 0.0`; no new deps; `#![deny(missing_docs)]`.

**This batch is deliberately integration-leaning:** packages **29 (verifier_summary)**,
**30 (pipeline_contract)**, and **36 (run_bundle)** are the interface-first data
model the future end-to-end orchestrator will compose. They depend only on the
stable base API, so they stay parallel-safe — but they are the prep for the
**serial** orchestrator + vertical-slice milestone that actually makes the
pipeline "work." (The orchestrator itself is intentionally NOT a parallel package
— it wires other packages together, so it's a dedicated serial task after these land.)

**Dependency graph (third batch):** flat — all depend only on the base crate.
**Overlap:** zero.

---

## ☐ Package 26 — seed_derivation

**Goal:** Deterministic sub-seed derivation (expand a parent seed into labeled/ordered sub-seeds via SHA-256, no OS randomness).

**Interface:**
```rust
/// Derive a deterministic sub-seed from `parent` and a domain `label`.
pub fn sub_seed(parent: u64, label: &str) -> u64;
/// Derive `count` ordered, distinct sub-seeds from `parent`.
pub fn expand(parent: u64, count: usize) -> Vec<u64>;
```

**Files allowed:** `src/pkgs/seed_derivation.rs`, `tests/wp_seed_derivation.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`Hash::new` / `fractal_crypto::sha256`).
**Acceptance tests:** same inputs → same output; distinct labels → distinct seeds; `expand(n)` yields `n` distinct values; deterministic across calls.
**Complexity:** ~1 h.

---

## ☐ Package 27 — confidence_intervals

**Goal:** Percentile + deterministic bootstrap mean CI over a numeric sample.

**Interface:**
```rust
/// Percentile (0..=100) of a sample; `None` if empty.
pub fn percentile(sample: &[f64], pct: f64) -> Option<f64>;
/// Bootstrap mean confidence interval (lower, upper) at `confidence` (0..1); deterministic given `seed`.
pub fn mean_ci(sample: &[f64], confidence: f64, trials: usize, seed: u64) -> Option<(f64, f64)>;
```

**Files allowed:** `src/pkgs/confidence_intervals.rs`, `tests/wp_confidence_intervals.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`rand` seeded `StdRng`).
**Acceptance tests:** `percentile([1,2,3], 50) == 2`; `mean_ci` deterministic for a fixed `seed`; CI of a symmetric sample brackets the sample mean; empty sample → `None`.
**Complexity:** ~2 h.

---

## ☐ Package 28 — risk_adjusted_metrics

**Goal:** Sharpe, Sortino, volatility, and max drawdown from a return series (risk-free = 0).

**Interface:**
```rust
pub struct RiskAdjusted { pub sharpe: f64, pub sortino: f64, pub volatility: f64, pub max_drawdown: f64 }
pub fn compute(returns: &[f64]) -> RiskAdjusted;
```

**Files allowed:** `src/pkgs/risk_adjusted_metrics.rs`, `tests/wp_risk_adjusted_metrics.rs`  •  **Forbidden:** everything else.
**Dependencies:** none.
**Acceptance tests:** empty / single-element → all zeros; a known series → hand-checked volatility and max drawdown; Sharpe finite for a non-constant series; deterministic.
**Complexity:** ~2 h.

---

## ☐ Package 29 — verifier_summary

**Goal:** Aggregate a set of `VerifierReport`s into a `VerifierSummary`. *(Integration-enabling.)*

**Interface:**
```rust
use crate::verifier::{VerifierReport, VerifierSummary};
pub fn summarize(reports: &[VerifierReport]) -> VerifierSummary;
```
Map: `total_verifiers = reports.len()`, `verifiers_passed/failed` by `passed`, `required_total = total`, `required_passed = passed`.

**Files allowed:** `src/pkgs/verifier_summary.rs`, `tests/wp_verifier_summary.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`VerifierReport`, `VerifierSummary`).
**Acceptance tests:** all-pass → `verifiers_failed == 0`; mixed → counts correct; empty → all zero.
**Complexity:** ~1 h.

---

## ☐ Package 30 — pipeline_contract

**Goal:** Interface-first data model for the future orchestrator (types + well-formedness only, no wiring). *(Integration-enabling.)*

**Interface:**
```rust
use crate::protocol::Hash;
use crate::verifier::VerifierReport;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipelineStage { Run, Score, Verify, Commit, Reward }
#[derive(Debug, Clone)]
pub struct PipelineOutcome {
    pub evidence_hash: Hash,
    pub scorecard_hash: Hash,
    pub verifier_reports: Vec<VerifierReport>,
    pub committed: bool,
    pub reward_released: bool,
}
impl PipelineOutcome {
    pub fn stage(&self) -> PipelineStage;          // highest reached stage
    pub fn all_verifiers_passed(&self) -> bool;
    pub fn is_complete(&self) -> bool;             // reward_released AND all_verifiers_passed
}
pub fn validate(outcome: &PipelineOutcome) -> std::result::Result<(), String>;
```
Stage ladder: empty `evidence_hash` → `Run`; non-empty → `Score`; ≥1 verifier → `Verify`; `committed` → `Commit`; `reward_released` → `Reward`.

**Files allowed:** `src/pkgs/pipeline_contract.rs`, `tests/wp_pipeline_contract.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`Hash`, `VerifierReport`).
**Acceptance tests:** stage ladder correct for hand-built outcomes; `is_complete` true only when `reward_released && all_verifiers_passed`; `validate` rejects an empty/zero `evidence_hash`.
**Complexity:** ~1.5 h.

---

## ☐ Package 31 — proof_card

**Goal:** Build a public-facing `ProofCard` from a signed `ProofManifest` + `Scorecard`.

**Interface:**
```rust
use crate::protocol::{Hash, ProofManifest};
use crate::verifier::{Scorecard, SimulationTier};
#[derive(Debug, Clone, PartialEq)]
pub struct ProofCard {
    pub claim: String,
    pub proof_level: String,
    pub simulation_tier: SimulationTier,
    pub net_return: f64,
    pub max_drawdown: f64,
    pub disclaimer: String,
    pub proof_hash: Hash,
}
pub fn build(manifest: &ProofManifest, scorecard: &Scorecard) -> ProofCard;
```

**Files allowed:** `src/pkgs/proof_card.rs`, `tests/wp_proof_card.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`ProofManifest`, `Scorecard`).
**Acceptance tests:** `disclaimer` non-empty and contains "SIMULATION"; `net_return` from `scorecard.primary_metrics["net_return"]`; `proof_hash == manifest.trace_merkle_root`; `simulation_tier == scorecard.simulation_tier`.
**Complexity:** ~1.5 h.

---

## ☐ Package 32 — replication_check

**Goal:** Re-derive replication success from tolerance + `actual_difference`.

**Interface:**
```rust
use crate::verifier::Replication;
pub enum ReplicationClass { Success, Fail, Indeterminate }
pub fn classify(replication: &Replication) -> ReplicationClass;
pub fn within_tolerance(replication: &Replication) -> bool;
```

**Files allowed:** `src/pkgs/replication_check.rs`, `tests/wp_replication_check.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`Replication`).
**Acceptance tests:** `actual_difference <= tolerance` (finite) → `Success`/`true`; `actual_difference > tolerance` → `Fail`/`false`; `actual_difference == None` → `Indeterminate`/`false`.
**Complexity:** ~1 h.

---

## ☐ Package 33 — protocol_validation

**Goal:** Validate a `Protocol` (required fields, finite policy).

**Interface:**
```rust
use crate::protocol::Protocol;
pub fn validate(protocol: &Protocol) -> std::result::Result<(), Vec<String>>;
```
Checks: `primary_metrics` non-empty; `allowed_tools` non-empty; `safety_policy` fields finite/non-negative; `cost_model.fee_schedule` non-empty.

**Files allowed:** `src/pkgs/protocol_validation.rs`, `tests/wp_protocol_validation.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`Protocol`).
**Acceptance tests:** valid protocol → `Ok`; empty `primary_metrics` → `Err`; empty `allowed_tools` → `Err`; negative `max_leverage` → `Err`.
**Complexity:** ~1.5 h.

---

## ☐ Package 34 — environment_validation

**Goal:** Validate an `EnvironmentManifest`.

**Interface:**
```rust
use crate::protocol::EnvironmentManifest;
pub fn validate(env: &EnvironmentManifest) -> std::result::Result<(), Vec<String>>;
```
Checks: `id` non-empty; `version_hash` parses as 64-hex; `config` not `Value::Null`.

**Files allowed:** `src/pkgs/environment_validation.rs`, `tests/wp_environment_validation.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`EnvironmentManifest`, `Hash`).
**Acceptance tests:** valid → `Ok`; empty `id` → `Err`; null `config` → `Err`; bad version-hash hex → `Err`.
**Complexity:** ~1 h.

---

## ☐ Package 35 — metric_set_ops

**Goal:** Merge multiple `MetricSet`s for cross-run aggregation.

**Interface:**
```rust
use crate::simulation::MetricSet;
/// Union the metric maps (duplicate keys: last wins); primary_metric = mean of primaries.
pub fn merge(sets: &[MetricSet]) -> MetricSet;
```

**Files allowed:** `src/pkgs/metric_set_ops.rs`, `tests/wp_metric_set_ops.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`MetricSet`).
**Acceptance tests:** merge of disjoint-key sets contains all keys; duplicate key keeps the last value; `primary_metric` is the mean of inputs; empty input → empty `MetricSet`.
**Complexity:** ~1.5 h.

---

## ☐ Package 36 — run_bundle

**Goal:** Assemble a portable run bundle (manifest/evidence/scorecard/proof hashes + agent id) with a tamper-evident bundle hash. *(Integration-enabling.)*

**Interface:**
```rust
use crate::protocol::Hash;
#[derive(Debug, Clone, PartialEq)]
pub struct RunBundle {
    pub run_manifest_hash: Hash,
    pub evidence_hash: Hash,
    pub scorecard_hash: Hash,
    pub proof_hash: Hash,
    pub agent_id: String,
}
impl RunBundle {
    pub fn new(run_manifest_hash: Hash, evidence_hash: Hash, scorecard_hash: Hash, proof_hash: Hash, agent_id: impl Into<String>) -> Self;
    pub fn bundle_hash(&self) -> fractal_society::Result<Hash>; // Hash::of(self)
}
```

**Files allowed:** `src/pkgs/run_bundle.rs`, `tests/wp_run_bundle.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`Hash`).
**Acceptance tests:** `bundle_hash` deterministic; identical inputs → identical hash; changing any one hash or the agent id → different hash.
**Complexity:** ~1 h.

---

## ☐ Package 37 — manifest_registry

**Goal:** In-memory artifact registry (insert/lookup/list `ArtifactManifest` by content hash) — a minimal stand-in for the PRD's Artifact Registry.

**Interface:**
```rust
use crate::artifact::{ArtifactHash, ArtifactManifest};
pub struct ArtifactRegistry { /* HashMap<ArtifactHash, ArtifactManifest> */ }
impl ArtifactRegistry {
    pub fn new() -> Self;
    /// Insert; returns false if the content hash is already present.
    pub fn insert(&mut self, manifest: ArtifactManifest) -> bool;
    pub fn get(&self, hash: &ArtifactHash) -> Option<&ArtifactManifest>;
    pub fn contains(&self, hash: &ArtifactHash) -> bool;
    pub fn len(&self) -> usize;
    pub fn is_empty(&self) -> bool;
    pub fn list(&self) -> Vec<&ArtifactManifest>;
}
impl Default for ArtifactRegistry { fn default() -> Self { Self::new() } }
```

**Files allowed:** `src/pkgs/manifest_registry.rs`, `tests/wp_manifest_registry.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`ArtifactManifest`, `ArtifactHash`).
**Acceptance tests:** insert + `get`/`contains`; duplicate-hash insert returns `false` and does not clobber; `len`/`is_empty`/`list` correct; `Default` works.
**Complexity:** ~1.5 h.

---

## Batch summary (1–37)

- **37 packages total**, all flat against the base crate, zero inter-package file overlap.
- This third batch adds the **integration-enabling spine types** (29 verifier_summary, 30 pipeline_contract, 36 run_bundle) plus stats utilities (26/27/28), more validation (32/33/34), and artifact plumbing (35/37).
- **After these land, the next milestone is the SERIAL orchestrator + end-to-end vertical-slice test** (not a parallel package): it composes `kernel::run → build_scorecard → verifier_summary → build_proof_manifest → pipeline_contract::PipelineOutcome → run_bundle`. That test passing is "the research pipeline works end-to-end" on synthetic data.

---

# Packages 38–51 (fourth batch)

Same rules: each agent edits **only** `src/pkgs/<name>.rs` (replace the stub) **and**
creates `tests/wp_<name>.rs`. Everything else forbidden. `pkgs/mod.rs` already
declares all 14; crate builds with empty stubs. Deterministic only;
`execution_time_seconds = 0.0`; no new deps; `#![deny(missing_docs)]`.

**Dependency graph (fourth batch):** flat — all depend only on the base crate.
**Overlap:** zero. These deepen verification (42 holdout, 43 overfit, 38
determinism_audit), arena/reward mechanics (44/45/46/47), agent/dataset policy
(48/49/50/51), and small utilities (39/40/41).

---

## ☐ Package 38 — determinism_audit

**Goal:** Diff two `EvidenceBundle`s (original vs claimed replay) step-by-step; emit divergences.

**Interface:**
```rust
use crate::protocol::EvidenceBundle;
#[derive(Debug, Clone, PartialEq)]
pub struct Divergence { pub step: u64, pub field: String, pub left: serde_json::Value, pub right: serde_json::Value }
pub fn diff(left: &EvidenceBundle, right: &EvidenceBundle) -> Vec<Divergence>;
```
Compare matching steps' `action` and `outcome` JSON.

**Files allowed:** `src/pkgs/determinism_audit.rs`, `tests/wp_determinism_audit.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`EvidenceBundle`).
**Acceptance tests:** identical bundles → empty; one mutated field → exactly one `Divergence` with the right step/field.
**Complexity:** ~1.5 h.

---

## ☐ Package 39 — canonical_roundtrip

**Goal:** Hash a value through serialize → deserialize and confirm the canonical hash is stable.

**Interface:**
```rust
use crate::protocol::Hash;
pub fn roundtrip_hash<T: serde::Serialize + serde::de::DeserializeOwned>(value: &T) -> Option<Hash>;
```
Returns `None` if the round-trip changes the canonical hash or fails to deserialize.

**Files allowed:** `src/pkgs/canonical_roundtrip.rs`, `tests/wp_canonical_roundtrip.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`Hash`, `canonical`).
**Acceptance tests:** a plain struct → `Some` deterministic hash; a value whose deserialized form differs → `None`.
**Complexity:** ~1 h.

---

## ☐ Package 40 — evidence_summary

**Goal:** Compact summary of an `EvidenceBundle` (step count, metric snapshot, action-type histogram).

**Interface:**
```rust
use std::collections::HashMap;
use crate::protocol::EvidenceBundle;
#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceSummary { pub step_count: usize, pub metrics: HashMap<String, f64>, pub action_type_counts: HashMap<String, u64> }
pub fn summarize(evidence: &EvidenceBundle) -> EvidenceSummary;
```
`action_type_counts` keyed by the action JSON's top-level variant name.

**Files allowed:** `src/pkgs/evidence_summary.rs`, `tests/wp_evidence_summary.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`EvidenceBundle`).
**Acceptance tests:** step_count and metric snapshot correct; action histogram counts a known run's actions.
**Complexity:** ~1.5 h.

---

## ☐ Package 41 — execution_budget

**Goal:** Deterministic resource budget (steps/calls) with consume/allow semantics.

**Interface:**
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionBudget { pub limit: u64, pub used: u64 }
impl ExecutionBudget {
    pub fn new(limit: u64) -> Self;
    pub fn consume(&mut self, n: u64) -> bool; // false if it would exceed the limit
    pub fn remaining(&self) -> u64;
    pub fn exhausted(&self) -> bool;
}
```

**Files allowed:** `src/pkgs/execution_budget.rs`, `tests/wp_execution_budget.rs`  •  **Forbidden:** everything else.
**Dependencies:** none.
**Acceptance tests:** consume within limit → `true`; over-limit → `false` and `used` unchanged; `remaining`/`exhausted` correct.
**Complexity:** ~1 h.

---

## ☐ Package 42 — holdout_isolation

**Goal:** Verify private holdout identifiers do not leak into a run's evidence (action/observation/outcome JSON).

**Interface:**
```rust
use crate::protocol::EvidenceBundle;
use crate::verifier::VerifierReport;
pub const VERIFIER_ID: &str = "holdout-isolation";
pub fn verify(evidence: &EvidenceBundle, private_ids: &[String]) -> VerifierReport;
```

**Files allowed:** `src/pkgs/holdout_isolation.rs`, `tests/wp_holdout_isolation.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`EvidenceBundle`, `VerifierReport`).
**Acceptance tests:** clean evidence → `passed`; evidence whose action contains a private id → `!passed`.
**Complexity:** ~1.5 h.

---

## ☐ Package 43 — overfit_detector

**Goal:** Flag overfitting by comparing a candidate's public-training vs private-eval scorecards.

**Interface:**
```rust
use crate::verifier::Scorecard;
#[derive(Debug, Clone, PartialEq)]
pub struct OverfitAssessment { pub overfit: bool, pub train_return: f64, pub eval_return: f64, pub gap: f64 }
pub fn assess(train: &Scorecard, eval: &Scorecard, gap_threshold: f64) -> OverfitAssessment;
```
`overfit = (train.net_return − eval.net_return) > gap_threshold`.

**Files allowed:** `src/pkgs/overfit_detector.rs`, `tests/wp_overfit_detector.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`Scorecard`).
**Acceptance tests:** small gap → `overfit == false`; large gap → `overfit == true`; `gap == train − eval`.
**Complexity:** ~1.5 h.

---

## ☐ Package 44 — review_aggregation

**Goal:** Aggregate `Review` records into a consensus decision with a quorum.

**Interface:**
```rust
use crate::verifier::Review;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Consensus { Approved, Rejected, NoQuorum }
pub fn aggregate(reviews: &[Review], quorum: usize) -> Consensus;
```
`NoQuorum` if `reviews.len() < quorum`; else majority of approve vs reject (tie → `Rejected`).

**Files allowed:** `src/pkgs/review_aggregation.rs`, `tests/wp_review_aggregation.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`Review`, `ReviewDecision`).
**Acceptance tests:** majority-approve → `Approved`; tie → `Rejected`; below quorum → `NoQuorum`.
**Complexity:** ~1.5 h.

---

## ☐ Package 45 — challenge_bond

**Goal:** Track a challenge/dispute bond: post, slash, or release (local types, logical settlement).

**Interface:**
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BondState { Posted, Slashed, Released }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChallengeBond { pub poster: String, pub amount: u64, pub state: BondState }
impl ChallengeBond {
    pub fn post(poster: impl Into<String>, amount: u64) -> Self;
    pub fn slash(&mut self) -> crate::Result<()>;   // Posted → Slashed
    pub fn release(&mut self) -> crate::Result<()>; // Posted → Released
}
```

**Files allowed:** `src/pkgs/challenge_bond.rs`, `tests/wp_challenge_bond.rs`  •  **Forbidden:** everything else.
**Dependencies:** none.
**Acceptance tests:** `post` → `Posted`; `slash`/`release` transition correctly; double-settle → `Err`.
**Complexity:** ~1.5 h.

---

## ☐ Package 46 — reward_split

**Goal:** Split a reward pool among winners proportionally to weights, deterministic, zero-sum.

**Interface:**
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewardShare { pub recipient: String, pub amount: u64 }
/// Largest-remainder proportional split; sum of amounts == pool. Zero-weight recipients excluded.
pub fn split(pool: u64, weights: &[(String, u64)]) -> Vec<RewardShare>;
```

**Files allowed:** `src/pkgs/reward_split.rs`, `tests/wp_reward_split.rs`  •  **Forbidden:** everything else.
**Dependencies:** none.
**Acceptance tests:** shares sum exactly to `pool`; proportional to weights; zero-total-weights → empty (or equal split, document which).
**Complexity:** ~2 h.

---

## ☐ Package 47 — appeals_flow

**Goal:** Appeal lifecycle state machine (`Filed → UnderReview → Resolved`) with guards.

**Interface:**
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppealState { Filed, UnderReview, Resolved { upheld: bool } }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Appeal { pub id: String, pub state: AppealState, pub reason: Option<String> }
impl Appeal {
    pub fn file(id: impl Into<String>) -> Self;
    pub fn begin_review(&mut self) -> crate::Result<()>;                        // Filed → UnderReview
    pub fn resolve(&mut self, upheld: bool, reason: impl Into<String>) -> crate::Result<()>; // UnderReview → Resolved
}
```

**Files allowed:** `src/pkgs/appeals_flow.rs`, `tests/wp_appeals_flow.rs`  •  **Forbidden:** everything else.
**Dependencies:** none.
**Acceptance tests:** legal transitions succeed; resolving from `Filed` (skipping review) → `Err`; `reason` stored on resolve.
**Complexity:** ~1.5 h.

---

## ☐ Package 48 — tool_allowlist

**Goal:** Check requested tools against an `AgentManifest`'s declared `tool_allowlist` (static manifest policy).

**Interface:**
```rust
use crate::protocol::AgentManifest;
pub fn allowed(manifest: &AgentManifest, tool: &str) -> bool;
pub fn disallowed_subset(manifest: &AgentManifest, requested: &[String]) -> Vec<String>;
```

**Files allowed:** `src/pkgs/tool_allowlist.rs`, `tests/wp_tool_allowlist.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`AgentManifest`).
**Acceptance tests:** allowed tool → `true`; disallowed → `false`; `disallowed_subset` returns exactly the violators.
**Complexity:** ~1 h.

---

## ☐ Package 49 — skill_graph

**Goal:** Build a dependency graph over skills, detect cycles, produce a topological load order.

**Interface:**
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillDep { pub id: String, pub depends_on: Vec<String> }
pub fn has_cycle(deps: &[SkillDep]) -> bool;
pub fn load_order(deps: &[SkillDep]) -> crate::Result<Vec<String>>; // Err on cycle
```

**Files allowed:** `src/pkgs/skill_graph.rs`, `tests/wp_skill_graph.rs`  •  **Forbidden:** everything else.
**Dependencies:** none.
**Acceptance tests:** acyclic deps → a valid topological order; cyclic deps → `has_cycle == true` and `load_order` → `Err`.
**Complexity:** ~2 h.

---

## ☐ Package 50 — dataset_window

**Goal:** Validate `DatasetBoundaries` (development/validation/evaluation windows).

**Interface:**
```rust
use crate::protocol::DatasetBoundaries;
pub fn validate(boundaries: &DatasetBoundaries) -> std::result::Result<(), Vec<String>>;
```
Checks: each window `start < end`; windows ordered and non-overlapping (`dev.end ≤ val.start ≤ val.end ≤ eval.start`).

**Files allowed:** `src/pkgs/dataset_window.rs`, `tests/wp_dataset_window.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`DatasetBoundaries`).
**Acceptance tests:** valid ordered windows → `Ok`; overlapping windows → `Err`; reversed window → `Err`.
**Complexity:** ~1.5 h.

---

## ☐ Package 51 — data_quality_report

**Goal:** Summarize data quality from an observed sequence + expected range (completeness, gap/missing counts).

**Interface:**
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct DataQualityReport { pub observed: usize, pub expected: usize, pub completeness: f64, pub gap_count: usize, pub missing_count: i64 }
pub fn report(observed: &[i64], expected_min: i64, expected_max: i64) -> DataQualityReport;
```
`expected = max(0, expected_max − expected_min + 1)`; `completeness = observed_unique_in_range / expected`.

**Files allowed:** `src/pkgs/data_quality_report.rs`, `tests/wp_data_quality_report.rs`  •  **Forbidden:** everything else.
**Dependencies:** none.
**Acceptance tests:** a complete sequence → `completeness == 1.0`; a gappy sequence → `completeness < 1.0` and `missing_count > 0`; empty expected range → `completeness` defined (no division by zero).
**Complexity:** ~1.5 h.

---

## Batch summary (1–51)

- **51 packages total**, all flat against the base crate, zero inter-package file overlap.
- The library now spans: canonical integrity, kernel, trading adapter, 6+ verifiers, proof/commitment, disclosure, arena (season/leaderboard/submission/appeals/reward), reputation/reward/fraud (graph/review-conflict/sybil/reputation/reward-gate/challenge-bond), agent/skill policy, dataset/data-quality, and the integration-enabling spine types (verifier_summary, pipeline_contract, run_bundle).
- **The serial orchestrator + vertical-slice test remains the next milestone** once the desired subset of these 51 has landed — that is what turns the component library into a working research pipeline.

---

# Packages 52–66 (fifth batch)

Same rules: each agent edits **only** `src/pkgs/<name>.rs` (replace the stub) **and**
creates `tests/wp_<name>.rs`. Everything else forbidden. `pkgs/mod.rs` already
declares all 15; crate builds with empty stubs. Deterministic only;
`execution_time_seconds = 0.0`; no new deps; `#![deny(missing_docs)]`.

**Dependency graph (fifth batch):** flat — all depend only on the base crate
(a few read trading `MarketBar`/`EvidenceBundle`, which are stable public types;
no package depends on another package). **Overlap:** zero.

---

## ☐ Package 52 — signature_verification

**Goal:** Verify signatures attached to a `PackageDigest` against a set of public keys (multi-sig).

**Interface:**
```rust
use crate::artifact::PackageDigest;
pub fn verify_all(digest: &PackageDigest, public_keys: &[&[u8; 32]]) -> usize; // count that verify
pub fn all_valid(digest: &PackageDigest, public_keys: &[&[u8; 32]]) -> bool;    // every signature verifies
```

**Files allowed:** `src/pkgs/signature_verification.rs`, `tests/wp_signature_verification.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`PackageDigest`).
**Acceptance tests:** a digest signed by a known key → `all_valid == true` with that key present; wrong key set → `false`; counts correct.
**Complexity:** ~1.5 h.

---

## ☐ Package 53 — bar_validation

**Goal:** OHLCV sanity check for a `MarketBar`.

**Interface:**
```rust
use crate::adapters::trading::MarketBar;
pub fn validate(bar: &MarketBar) -> std::result::Result<(), Vec<String>>;
```
Checks: `high ≥ max(open, close)`; `low ≤ min(open, close)`; `high ≥ low`; all prices finite and `≥ 0`; `volume ≥ 0`.

**Files allowed:** `src/pkgs/bar_validation.rs`, `tests/wp_bar_validation.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`MarketBar`).
**Acceptance tests:** a well-formed bar → `Ok`; `high < close` → `Err`; negative price → `Err`.
**Complexity:** ~1.5 h.

---

## ☐ Package 54 — ohlc_aggregation

**Goal:** Resample a bar series into a higher timeframe (combine N consecutive bars).

**Interface:**
```rust
use crate::adapters::trading::MarketBar;
pub fn aggregate(bars: &[MarketBar], group_size: usize) -> Vec<MarketBar>;
```
Per group: `open = first.open`, `high = max`, `low = min`, `close = last.close`, `volume = sum`, `ts = first.ts`, `asset/stale/funding_rate = first`'s.

**Files allowed:** `src/pkgs/ohlc_aggregation.rs`, `tests/wp_ohlc_aggregation.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`MarketBar`).
**Acceptance tests:** 4 bars grouped by 2 → 2 bars with correct OHLCV; `group_size == 0` → empty (or identity, document); remaining bars (`len % group_size != 0`) form a final partial group.
**Complexity:** ~2 h.

---

## ☐ Package 55 — equity_curve

**Goal:** Extract the per-step equity series from an `EvidenceBundle`.

**Interface:**
```rust
use crate::protocol::EvidenceBundle;
pub fn extract(evidence: &EvidenceBundle) -> Vec<f64>;
```
Parse the `equity` field of each decision-trace outcome (skip non-matching).

**Files allowed:** `src/pkgs/equity_curve.rs`, `tests/wp_equity_curve.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`EvidenceBundle`).
**Acceptance tests:** a known evidence bundle → the expected equity series; steps without `equity` are skipped.
**Complexity:** ~1 h.

---

## ☐ Package 56 — field_redactor

**Goal:** Recursively redact JSON at given dot-paths (generic, field-level; distinct from bundle-level disclosure tiers).

**Interface:**
```rust
/// Return a copy of `value` with every node at a dot-`paths` location replaced by `"REDACTED"`.
pub fn redact(value: &serde_json::Value, paths: &[&str]) -> serde_json::Value;
```

**Files allowed:** `src/pkgs/field_redactor.rs`, `tests/wp_field_redactor.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`serde_json`).
**Acceptance tests:** nested object path redacted; sibling paths intact; missing path is a no-op; arrays handled (document index or skip).
**Complexity:** ~2 h.

---

## ☐ Package 57 — commit_reveal

**Goal:** Hash-based commit/reveal scheme.

**Interface:**
```rust
use crate::protocol::Hash;
pub fn commit(value: &serde_json::Value) -> Hash;
pub fn reveal(value: &serde_json::Value, claimed: &Hash) -> bool;
```

**Files allowed:** `src/pkgs/commit_reveal.rs`, `tests/wp_commit_reveal.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`Hash`, `canonical`).
**Acceptance tests:** `reveal(v, commit(v)) == true`; a tampered value → `false`; deterministic.
**Complexity:** ~1 h.

---

## ☐ Package 58 — id_uniqueness

**Goal:** Detect duplicate identifiers in a list.

**Interface:**
```rust
pub fn unique(ids: &[String]) -> bool;
pub fn duplicates(ids: &[String]) -> Vec<String>; // each duplicated id once
```

**Files allowed:** `src/pkgs/id_uniqueness.rs`, `tests/wp_id_uniqueness.rs`  •  **Forbidden:** everything else.
**Dependencies:** none.
**Acceptance tests:** all-distinct → `unique == true` and `duplicates` empty; a repeated id listed once.
**Complexity:** ~1 h.

---

## ☐ Package 59 — jsonl_export

**Goal:** Serialize records to newline-delimited JSON (JSONL).

**Interface:**
```rust
pub fn to_jsonl<T: serde::Serialize>(records: &[T]) -> crate::Result<String>;
```

**Files allowed:** `src/pkgs/jsonl_export.rs`, `tests/wp_jsonl_export.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`serde_json`).
**Acceptance tests:** N records → N newline-separated lines, each parses back to the record; empty input → empty string.
**Complexity:** ~1 h.

---

## ☐ Package 60 — metric_csv_export

**Goal:** Export a `MetricSet` to CSV.

**Interface:**
```rust
use crate::simulation::MetricSet;
pub fn to_csv(metrics: &MetricSet) -> String; // header `name,value` + one row per metric (primary first)
```

**Files allowed:** `src/pkgs/metric_csv_export.rs`, `tests/wp_metric_csv_export.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`MetricSet`).
**Acceptance tests:** output has a header and one row per metric incl. `primary_metric`; values round-trip parseable; deterministic ordering.
**Complexity:** ~1.5 h.

---

## ☐ Package 61 — verifier_registry

**Goal:** In-memory registry of `VerifierPackage`s keyed by id.

**Interface:**
```rust
use crate::verifier::VerifierPackage;
pub struct VerifierRegistry { /* HashMap<String, VerifierPackage> */ }
impl VerifierRegistry {
    pub fn new() -> Self;
    pub fn insert(&mut self, pkg: VerifierPackage) -> bool; // false if id already present
    pub fn get(&self, id: &str) -> Option<&VerifierPackage>;
    pub fn contains(&self, id: &str) -> bool;
    pub fn len(&self) -> usize;
    pub fn list(&self) -> Vec<&VerifierPackage>;
}
impl Default for VerifierRegistry { fn default() -> Self { Self::new() } }
```

**Files allowed:** `src/pkgs/verifier_registry.rs`, `tests/wp_verifier_registry.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`VerifierPackage`).
**Acceptance tests:** insert + `get`/`contains`; duplicate-id insert → `false`; `len`/`list` correct.
**Complexity:** ~1.5 h.

---

## ☐ Package 62 — replication_summary

**Goal:** Aggregate `Replication` records into a summary.

**Interface:**
```rust
use crate::verifier::Replication;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplicationSummary { pub total: usize, pub successful: usize, pub failed: usize, pub any_success: bool }
pub fn summarize(replications: &[Replication]) -> ReplicationSummary;
```

**Files allowed:** `src/pkgs/replication_summary.rs`, `tests/wp_replication_summary.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`Replication`).
**Acceptance tests:** counts by `success`; `any_success` true iff ≥1 success; empty → all zero.
**Complexity:** ~1 h.

---

## ☐ Package 63 — challenge_window

**Goal:** Logical-time challenge window (open/closed by deadline).

**Interface:**
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChallengeWindow { pub opened_at: u64, pub duration: u64 }
impl ChallengeWindow {
    pub fn new(opened_at: u64, duration: u64) -> Self;
    pub fn deadline(&self) -> u64;             // opened_at + duration (saturating)
    pub fn is_open(&self, now: u64) -> bool;   // now < deadline
}
```

**Files allowed:** `src/pkgs/challenge_window.rs`, `tests/wp_challenge_window.rs`  •  **Forbidden:** everything else.
**Dependencies:** none.
**Acceptance tests:** `is_open` true before deadline; false at/after; `deadline == opened_at + duration`.
**Complexity:** ~1 h.

---

## ☐ Package 64 — streak_analysis

**Goal:** Max win/loss streaks from a return series.

**Interface:**
```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Streaks { pub max_win_streak: usize, pub max_loss_streak: usize }
pub fn analyze(returns: &[f64]) -> Streaks;
```
`> 0` = win step, `< 0` = loss step, `0` breaks both streaks.

**Files allowed:** `src/pkgs/streak_analysis.rs`, `tests/wp_streak_analysis.rs`  •  **Forbidden:** everything else.
**Dependencies:** none.
**Acceptance tests:** alternating signs → 1/1; a run of N wins → `max_win_streak == N`; empty → 0/0.
**Complexity:** ~1.5 h.

---

## ☐ Package 65 — drawdown_analysis

**Goal:** Full drawdown analysis from an equity curve (series + max + duration).

**Interface:**
```rust
#[derive(Debug, Clone, PartialEq)]
pub struct DrawdownAnalysis { pub series: Vec<f64>, pub max_drawdown: f64, pub max_drawdown_duration: usize }
pub fn analyze(equity_curve: &[f64]) -> DrawdownAnalysis;
```
`series[i]` = `(peak_i − equity_i)/peak_i`; `max_drawdown_duration` = longest run of consecutive `series[i] > 0`.

**Files allowed:** `src/pkgs/drawdown_analysis.rs`, `tests/wp_drawdown_analysis.rs`  •  **Forbidden:** everything else.
**Dependencies:** none.
**Acceptance tests:** monotonic-up curve → all-zero `series`, `max_drawdown == 0`; a peak-then-drop curve → correct max + duration; empty/single → zeros.
**Complexity:** ~2 h.

---

## ☐ Package 66 — research_project_validation

**Goal:** Validate a `ResearchProject`.

**Interface:**
```rust
use crate::protocol::ResearchProject;
pub fn validate(project: &ResearchProject) -> std::result::Result<(), Vec<String>>;
```
Checks: non-empty `id`, `question`, `claim`; non-empty `domain_adapter.id` / `domain_adapter.version`.

**Files allowed:** `src/pkgs/research_project_validation.rs`, `tests/wp_research_project_validation.rs`  •  **Forbidden:** everything else.
**Dependencies:** none (`ResearchProject`).
**Acceptance tests:** valid project → `Ok`; empty `id`/`question`/`claim` → `Err`; empty domain-adapter id → `Err`.
**Complexity:** ~1 h.

---

## Batch summary (1–66)

- **66 packages total**, all flat against the base crate, zero inter-package file overlap.
- This fifth batch adds market-data validation/resampling (53/54), evidence/stat extraction (55/64/65), generic privacy/export utilities (56/57/58/59/60), registries (61), and lifecycle/validation helpers (52/62/63/66).
- **The serial orchestrator + vertical-slice test remains the single milestone that converts this component library into a running research pipeline."""

---

# Serial integration track — Packages 67–72

> ⚠️ **These are NOT parallel packages.** They are **serial, ordered integration
> tasks** that wire the 66 components into a running pipeline. Rules:
> - **One agent, in order (67 → 68 → 69 → 70 → 71 → 72).** Do not assign them to
>   different agents simultaneously — each depends on the prior.
> - They are **exempt from the parallel zero-overlap rule**: they create/edit
>   real integration files (`src/pipeline.rs`, `lib.rs`, examples, tests) and
>   legitimately `use` many `pkgs::*` modules together.
> - Prerequisite: the spine packages they compose — **7 (proof_manifest), 29
>   (verifier_summary), 30 (pipeline_contract), 36 (run_bundle), 11 (reward_gate)**,
>   plus trading `build_scorecard` and the kernel — must be landed. (They are.)

**Dependency chain:**
```
67 orchestrator  →  68 vertical-slice test            ← "it works end-to-end" (Bar B)
                →  69 canonical verifier pack
                →  70 example/CLI binary
                →  71 offline proof verification
                →  72 pipeline determinism + replay test
```
**Milestone:** **67 + 68 landing = "the research pipeline works end-to-end" on
synthetic data.** 69–72 harden it (defaults, usability, trustless verify, replay).

---

## ☐ Package 67 — pipeline orchestrator (SERIAL)

**Goal:** Create `src/pipeline.rs` — the spine that chains the stages into one call: run → score → verify → proof → outcome/bundle.

**Files allowed:** create `src/pipeline.rs`; edit `src/lib.rs` (add `pub mod pipeline;`); create `tests/pipeline.rs`.
**Dependencies:** packages 7, 11, 29, 30, 36; trading `build_scorecard`; `kernel::run`.
**Interface (interface-first — implement to this):**
```rust
use std::sync::Arc;
use chrono::{DateTime, Utc};
use crate::adapters::trading::{TradingAdapter, TradingConfig, build_scorecard};
use crate::kernel::{KernelConfig, RunOutcome};
use crate::pkgs::pipeline_contract::PipelineOutcome;
use crate::pkgs::run_bundle::RunBundle;
use crate::protocol::{EvidenceBundle, ProofManifest};
use crate::signing::AuthorSigner;
use crate::simulation::Agent;
use crate::verifier::{Scorecard, VerifierReport};

/// A verifier function run over a run's evidence.
pub type VerifierFn = Arc<dyn Fn(&EvidenceBundle) -> VerifierReport + Send + Sync>;

/// Full output of a pipeline run.
pub struct PipelineResult {
    pub run: RunOutcome,
    pub scorecard: Scorecard,
    pub verifier_reports: Vec<VerifierReport>,
    pub proof_manifest: ProofManifest,
    pub outcome: PipelineOutcome,
    pub bundle: RunBundle,
}

/// Run the full research pipeline for one candidate.
pub async fn run_pipeline(
    adapter: TradingAdapter,
    agent: impl Agent<TradingAdapter>,
    seed: u64,
    kernel_config: KernelConfig,
    trading_config: TradingConfig,
    baselines: Vec<(String, RunOutcome)>,
    verifiers: Vec<VerifierFn>,
    signer: &AuthorSigner,
    timestamp: DateTime<Utc>,
) -> crate::Result<PipelineResult>;
```
Steps inside: `kernel::run` → `build_scorecard(&run, &baselines, &trading_config, ts)` → run each `verifier` over `&run.evidence` → `reward_gate::evaluate(&reports, false, 1)` sets `reward_released` → `proof_manifest::build(&run, &scorecard, signer, ts)` → `PipelineOutcome { evidence_hash: run.evidence_hash, scorecard_hash: Hash::of(&scorecard)?, verifier_reports, committed: true, reward_released }` → `RunBundle::new(run_manifest_hash, evidence_hash, scorecard_hash, proof_hash, agent_id)`.
**Acceptance tests:** `run_pipeline` returns a `PipelineResult` whose `proof_manifest` verifies (`verify_author(&signer.public_key())` ok), `outcome.stage() >= Verify`, and `bundle.bundle_hash()` is deterministic.
**Complexity:** ~3 h.

---

## ☐ Package 68 — vertical-slice end-to-end test (SERIAL)

**Goal:** One test that proves the whole pipeline works on synthetic trading data. **This test passing IS "the pipeline works" (Bar B).**

**Files allowed:** create `tests/pipeline_vertical_slice.rs`.
**Dependencies:** package 67; verifier packages 1 (accounting_integrity), 2 (cost_completeness), 3 (risk_policy), 14 (temporal_leakage); trading adapter + baselines.
**Steps:** build a `TradingAdapter` (synthetic bars) + `TradingAgent` + the 4 baselines + an `AuthorSigner`; assemble `verifiers` from the landed verifier functions; call `run_pipeline`; assert: `run.evidence` non-empty; `scorecard.baselines.len() == 4`; **all** `verifier_reports` `passed`; `proof_manifest.verify_author(pk)` is `Ok`; `outcome.stage() == Reward` and `outcome.is_complete()`.
**Acceptance test:** the single `pipeline_runs_end_to_end` test passes.
**Complexity:** ~2 h.

---

## ☐ Package 69 — canonical trading verifier pack (SERIAL)

**Goal:** A single `trading_verifier_pack()` returning the standard verifier set, wired as the orchestrator's default so callers don't hand-pick verifiers.

**Files allowed:** create `src/pipeline/verify_pack.rs` (and `src/pipeline/mod.rs` if converting `pipeline.rs` to a directory module) or extend `src/pipeline.rs`; add a `run_pipeline_default` convenience.
**Dependencies:** package 67; verifier packages 1, 2, 3, 13 (sandbox_policy), 14 (temporal_leakage), 15 (baseline_correctness).
**Interface:**
```rust
/// The canonical verifier set for a trading simulation run.
pub fn trading_verifier_pack(tolerance: f64, allowed_tools: Vec<String>) -> Vec<VerifierFn>;
```
**Acceptance tests:** the pack is non-empty; `run_pipeline_default` (which calls `run_pipeline` with this pack) succeeds end-to-end.
**Complexity:** ~1.5 h.

---

## ☐ Package 70 — example/CLI binary (SERIAL)

**Goal:** A runnable `examples/run_research.rs` that executes the pipeline on a fixture and prints a public `ProofCard` — the human-usable "it works" demo.

**Files allowed:** create `examples/run_research.rs`.
**Dependencies:** packages 67, 69, 31 (proof_card).
**Steps:** construct adapter/agent/baselines/signer with fixed seeds; call `run_pipeline_default`; build a `ProofCard` via `proof_card::build`; print claim, tier, net return, proof hash, disclaimer.
**Acceptance tests:** `cargo run --example run_research` exits 0 and prints a line containing the proof hash + "SIMULATION".
**Complexity:** ~1.5 h.

---

## ☐ Package 71 — offline proof verification (SERIAL)

**Goal:** Trustless verification — given a `RunBundle` + public `ProofManifest` + `Scorecard` + author pubkey, verify the proof **without re-running** the pipeline (recompute `scorecard_hash`, check the manifest signature, confirm the bundle's hashes are consistent). Implements PRD P07-N09.

**Files allowed:** create `src/offline_verify.rs` (register `pub mod offline_verify;` in `lib.rs`); create `tests/offline_verify.rs`.
**Dependencies:** packages 7, 30, 36; `ProofManifest::verify_author`, `Hash::of`.
**Interface:**
```rust
use crate::pkgs::run_bundle::RunBundle;
use crate::protocol::{ProofManifest, Hash};
use crate::verifier::Scorecard;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyVerdict { Valid, Invalid { reasons: Vec<String> } }
pub fn verify(bundle: &RunBundle, manifest: &ProofManifest, scorecard: &Scorecard, public_key: &[u8; 32]) -> VerifyVerdict;
```
Checks: `Hash::of(scorecard) == bundle.scorecard_hash == manifest.scorecard_hash`; `manifest.trace_merkle_root == bundle.evidence_hash`; `manifest.verify_author(public_key)` ok; `Hash::of(manifest) == bundle.proof_hash`. Collect failures into `Invalid`.
**Acceptance tests:** a genuine bundle/manifest/scorecard → `Valid`; a tampered scorecard → `Invalid`; wrong pubkey → `Invalid`.
**Complexity:** ~2 h.

---

## ☐ Package 72 — pipeline determinism + replay test (SERIAL)

**Goal:** Prove the integrated pipeline is deterministic and replayable: two `run_pipeline` calls with identical inputs produce identical proof/bundle hashes, and the pipeline can be replayed from a frozen seed.

**Files allowed:** create `tests/pipeline_determinism.rs`.
**Dependencies:** package 67 (and 69 for the default pack).
**Steps:** run the pipeline twice with identical (adapter, agent, seed, config, baselines, verifiers, signer, timestamp); assert `proof_manifest` hashes equal and `bundle.bundle_hash()` equal; run with a different seed → different hashes.
**Acceptance tests:** `pipeline_is_deterministic` (identical hashes) and `pipeline_varies_by_seed` (different hashes) both pass.
**Complexity:** ~1.5 h.

---

## Track summary (67–72)

- **6 serial integration tasks**, ~11–12 h total, **one agent in order**.
- **67 + 68 = Bar B: the research pipeline works end-to-end on synthetic data.**
- 69–72 add defaults, a runnable demo, trustless offline verification, and replay determinism — the credibility/usability layer before the cross-repo TS port (Bar C).
- After this track, the remaining work to a *product* is cross-repo (fractalwork TS schema port), infrastructure (persistence, live data recorder P03), and the arena launch — none of which is more Rust packages.

---

# Cross-repo / infrastructure track — Packages 73–84

> These are **NOT** the zero-overlap parallel `pkgs/` leaves, and not the pure-Rust
> serial track. They are the work that turns the working synthetic pipeline into a
> real system: **real data, a TS app that consumes it, persistence, and live chain
> commitment.** Read the flags per task:
> - **Repo:** `fractalchain` (Rust) or `fractalwork` (TS).
> - **Live/CI:** most are deterministic/CI-testable; the genuinely-live pieces
>   (75, 82/83) are **feature-gated** and **cannot be fully CI-tested** — they
>   need real infra/credentials and are verified manually + with mocks.
> - Dependencies are listed; some tasks are serial within a priority, the four
>   priorities are largely independent of each other.

## Priority 1 — real market data (PHASE-03 recorder)

### ☐ Package 73 — market_data_schema (Rust, fractalchain)
**Goal:** A normalized market-data layer: define `MarketRecord`/raw-payload types and a `normalize(...)` that converts a raw exchange payload (Hyperliquid trade + L2 snapshot + funding) into the trading adapter's existing `MarketBar` (populating `stale`, `funding_rate`, ohlcv). The deterministic core of the recorder.
**Repo/files:** `crates/fractal-society/src/market_data.rs` (+ register in `lib.rs`); `tests/wp_market_data.rs`.
**Dependencies:** `adapters::trading::MarketBar`.
**Acceptance:** normalize a fixture raw Hyperliquid payload → correct `MarketBar`s; ohlcv aggregation correct; `bar_validation` (pkg 53) passes on outputs.
**Live/CI:** fully CI-testable (fixture payloads). **Complexity:** ~2 h.

### ☐ Package 74 — bar_dataset_store (Rust, fractalchain)
**Goal:** Append-only recording of normalized bars to disk (JSONL or borsh) + a reader yielding `Vec<BarSet>` consumable by `TradingAdapter::with_bars`. Lets the pipeline run on a *recorded* dataset.
**Repo/files:** `crates/fractal-society/src/market_data/store.rs`; `tests/wp_bar_dataset_store.rs`.
**Dependencies:** package 73; `adapters::trading::{BarSet, TradingAdapter}`.
**Acceptance:** write fixture bars → read back → feed `TradingAdapter::with_bars` → `run_pipeline_default` deterministic; round-trip is byte-stable.
**Live/CI:** CI-testable (temp files). **Complexity:** ~2 h.

### ☐ Package 75 — hyperliquid_source (Rust, fractalchain, FEATURE-GATED, LIVE)
**Goal:** A `HyperliquidSource` adapter (REST snapshot + WS subscription) that emits raw events into the normalizer (73) → store (74). The genuinely-live piece.
**Repo/files:** `crates/fractal-society/src/market_data/hyperliquid.rs`; gate behind a new `live-data` cargo feature.
**Dependencies:** packages 73, 74; a working Hyperliquid client (the optional `hyperliquid` crate or a hand-written WS client — **verify the client lib works before scoping**).
**Acceptance:** with the feature off, the crate builds and existing tests pass; with a **mock** source, record→normalize→store works; a manual run against live Hyperliquid (founder-supplied endpoint) produces a real dataset. **Not CI-verifiable end-to-end.**
**Live/CI:** live infra + creds required for real use; CI uses mocks only. **Complexity:** ~4 h + live-integration risk.

## Priority 2 — fractalwork TS schema port (Bar C)

### ☐ Package 76 — ts_schema_port (TS, fractalwork)
**Goal:** Port the canonical Rust schemas to TypeScript (interfaces + zod/valibot validators) matching Rust field names **exactly**: `ResearchProject`, `Protocol`, `DatasetManifest`, `EnvironmentManifest`, `AgentManifest`, `ExperimentRun`, `EvidenceBundle`, `ProofManifest`, `RunManifest`, `Scorecard`, `Hash`, `Visibility`.
**Repo/files:** `~/fractalwork/packages/society-schema/` (new package) — `src/index.ts` types + `src/schemas.ts` validators + `package.json`.
**Dependencies:** none (mirrors `crates/fractal-society/src/protocol.rs`).
**Acceptance:** `pnpm test` passes; types round-trip a Rust-exported JSON fixture (names/shape match).
**Live/CI:** CI-testable. **Complexity:** ~3 h.

### ☐ Package 77 — ts_canonical_conformance (TS + Rust golden, fractalwork)
**Goal:** The cross-language hash lock. A conformance test asserting `hashObjectJcs(obj)` (TS) == `Hash::of(obj)` (Rust) for a golden set of objects.
**Repo/files:** `~/fractalwork/packages/society-schema/test/canonical.test.ts`; a Rust helper (or `examples/emit_golden_hashes.rs`) that emits `(json, sha256-jcs-hash)` pairs; the TS test imports those pairs and checks `hashObjectJcs` reproduces each hash.
**Dependencies:** package 76; Rust `canonical::content_hash` + `Hash::of`.
**Acceptance:** every golden object's TS hash == the Rust-emitted hash, byte-for-byte; a one-field mutation changes the hash.
**Live/CI:** CI-testable (the lock that catches future drift). **Complexity:** ~2 h.

### ☐ Package 78 — ts_offline_verifier (TS, fractalwork)
**Goal:** A TS port of `offline_verify.rs` so the TS app can trustlessly verify a Rust-produced proof using `@noble/ed25519` + `hashObjectJcs`.
**Repo/files:** `~/fractalwork/packages/society-schema/src/offline_verify.ts`; `test/offline_verify.test.ts`.
**Dependencies:** packages 76, 77; a Rust-produced golden proof (from `examples/run_research` or a test fixture).
**Acceptance:** `verify(bundle, manifest, scorecard, pubkey)` returns `Valid` for the golden proof; `Invalid` for a tampered scorecard / wrong key — mirroring the Rust tests.
**Live/CI:** CI-testable. **Complexity:** ~3 h.

## Priority 3 — persistence layer

### ☐ Package 79 — event_log_append (Rust, fractalchain)
**Goal:** An append-only event-log trait (`append(event)`, `replay()`) + an in-memory and a file-backed implementation. Events = the pipeline's durable writes (run recorded, proof committed, etc.).
**Repo/files:** `crates/fractal-society/src/persistence/event_log.rs` (+ register); `tests/wp_event_log.rs`.
**Dependencies:** none (define a local `Event` type or reuse protocol events).
**Acceptance:** append N events to the file log → reopen → replay yields the same N in order; idempotent append (same event id not duplicated).
**Live/CI:** CI-testable (temp files). **Complexity:** ~2 h.

### ☐ Package 80 — artifact_store (Rust, fractalchain)
**Goal:** A content-addressed store trait (`put(hash, bytes)`, `get(hash) -> bytes`, `contains`) + a filesystem impl. Stores serialized evidence/manifests/scorecards.
**Repo/files:** `crates/fractal-society/src/persistence/artifact_store.rs`; `tests/wp_artifact_store.rs`.
**Dependencies:** none.
**Acceptance:** `put(h, b)` then `get(h) == b`; content-addressed (same bytes → same hash key); missing hash → `None`.
**Live/CI:** CI-testable (temp dir). **Complexity:** ~2 h.

### ☐ Package 81 — pipeline_persistence (Rust, fractalchain, SERIAL)
**Goal:** Wire the pipeline to persist. After `run_pipeline`, write evidence/manifest/scorecard/bundle to the artifact store (80) and record events (79); add a `load_proof(store, bundle)` that reloads and re-verifies.
**Repo/files:** extend `src/pipeline.rs` (add `run_pipeline_persisted`) or `src/persistence/mod.rs`; `tests/wp_pipeline_persistence.rs`.
**Dependencies:** packages 67, 79, 80, 71 (offline_verify).
**Acceptance:** run → persist → reload → `offline_verify::verify(...)` still `Valid`; reloading a tampered artifact → `Invalid`.
**Live/CI:** CI-testable. **Complexity:** ~3 h.

## Priority 4 — real chain commitment (PHASE-07 — "posts to a blockchain")

> **Decision needed:** which chain? (a) FractalChain's own RPC, or (b) an EVM
> chain via the existing `BatchSettlement.sol` (fractalwork). The PRD prefers an
> established low-cost chain via adapter. 82 and 83 are **alternatives** — pick
> one; 84 wires the chosen one.

### ☐ Package 82 — chain_adapter_fractalchain (Rust, FEATURE-GATED, LIVE)
**Goal:** A real `CommitmentAdapter` (the trait from pkg 16) that submits a proof hash to a running **FractalChain** node via jsonrpsee RPC, returning a real `ChainReference`.
**Repo/files:** `crates/fractal-society/src/chain/fractalchain_adapter.rs`; gate behind `live-chain`; `tests/wp_chain_adapter_mock.rs`.
**Dependencies:** pkg 16 (`CommitmentAdapter`); `crates/rpc` jsonrpsee client types; a running FractalChain node for real use.
**Acceptance:** against an **in-process jsonrpsee mock server**, `submit(hash)` returns a `ChainReference` with the node's network/tx/block; signature of submission matches the node's expected call. **Not CI-verifiable against a real node.**
**Live/CI:** live node required for real use; CI uses a mock RPC. **Complexity:** ~4 h + node-integration risk.

### ☐ Package 83 — chain_adapter_evm (Rust, FEATURE-GATED, LIVE) — alternative to 82
**Goal:** A real `CommitmentAdapter` that submits the proof hash as a batch root to `BatchSettlement.sol` (already in fractalwork) via an ethers/alloy client on an EVM chain.
**Repo/files:** `crates/fractal-society/src/chain/evm_adapter.rs`; gate behind `live-chain`; `tests/wp_evm_adapter_mock.rs`.
**Dependencies:** pkg 16; `BatchSettlement.sol` ABI; an EVM client crate (ethers/alloy — **add to workspace deps**).
**Acceptance:** against a local **anvil** devnet (or a mock), `submit(hash)` calls `submitBatchRoot` and returns a `ChainReference` with the tx hash + block.
**Live/CI:** anvil devnet for CI-able smoke; real EVM needs RPC + funded wallet. **Complexity:** ~4 h.

### ☐ Package 84 — pipeline_commitment (Rust, SERIAL)
**Goal:** Wire the chosen chain adapter into `run_pipeline`: after building the proof manifest, optionally submit its hash and populate `manifest.chain_reference` (today it's `None`). Adapter is `Option<&dyn CommitmentAdapter>` — `None` keeps current (off-chain) behavior.
**Repo/files:** extend `src/pipeline.rs`; `tests/wp_pipeline_commitment.rs`.
**Dependencies:** packages 67, (82 **or** 83); 71.
**Acceptance:** with a mock adapter, `run_pipeline` populates `chain_reference` (non-`None`), the manifest still verifies, and `offline_verify` passes including the chain ref; with `adapter = None`, behavior is unchanged from today.
**Live/CI:** CI-testable with a mock; a real commit is a manual, founder-authorized step. **Complexity:** ~2 h.

---

## Dependency graph (73–84)

```
P1 real data:     73 ─▶ 74 ─▶ 75(live)
P2 TS port:       76 ─▶ 77   ;  76 ─▶ 78
P3 persistence:   79 ─┐
                   80 ─┴─▶ 81 ─▶ (uses 71)
P4 chain:     82 | 83 (alt) ─▶ 84 ─▶ (uses 71)
```

- The **four priorities are largely independent** — P1, P2, P3, P4 can proceed in parallel (different files/repos).
- Within each priority, tasks are **ordered** (arrows).
- **Genuinely serial across everything:** nothing — 81 and 84 both consume the already-built pipeline (67) and offline_verify (71), which exist.
- **Live-infra / not-CI tasks:** 75 (Hyperliquid), 82/83 (real chain). Everything else is deterministic and CI-testable.

## Honest notes
- **P4 needs a founder decision** (FractalChain RPC vs EVM) and, per the PRD, on-chain commitment of *real value* is gated; this track only commits a *proof hash* (tamper-evidence), not funds.
- **P2 (TS port) is the unlock for "a product"** — until schemas are mirrored in fractalwork, no user-facing app can consume these proofs.
- **P1 (real data) is the unlock for "credible trading proof"** — synthetic S0 bars can't support a public claim better than "preliminary."
- This track is what stands between "working synthetic pipeline" (Bar B, done) and "a product + real proofs" (Bar C).
