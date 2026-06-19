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
