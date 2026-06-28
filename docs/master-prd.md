# Master PRD — fractalchain2 (Proof-Ingestion Decoupling)

> **Status as of 2026-06-21:** 25 / 27 tasks complete (93%). All 8 PRD workstreams (A–H) are **done** — the proof-ingestion path, both benchmark harnesses, and the comparison report are built and runnable. Only the hardening tail remains (Workstream S: #25 fuzz tests nearly done, #27 DA-sampling policy centralization open).
>
> **Companion files:** original vision [`proof-ingestion-decoupling-prd.md`](./proof-ingestion-decoupling-prd.md) · task breakdown [`proof-ingestion-tasks.md`](./proof-ingestion-tasks.md) / [`proof-ingestion-tasks.json`](./proof-ingestion-tasks.json) (source of truth for per-task status).
>
> **This document is the canonical guide.** It supersedes status details in the task files where they disagree — re-read the JSON for live task status.

---

## 1. Purpose & Hypothesis

`fractalchain2` is an experiment: convert FractalChain from a block-production path that **executes and carries most transactions** into a base-chain path that primarily **ingests proofs, data-availability commitments, owned-object certificates, and cross-zone message roots**.

**Hypothesis:** decoupling execution from block production improves transaction throughput and latency **without reducing decentralization**.

The experiment is judged by benchmarking `fractalchain2` (proof-ingestion) against the `fractalchain` baseline (full-tx execution) across identical scenarios, then reading the H3 comparison report.

## 2. Baseline (what we started from)

The original design already had the right primitives: HyperBFT shard block production, DA sidecars + sampling hooks, proof-finality status, owned-object classification, owned-object certificate wire types, execution-zone / masterchain coordination sketches, signer-based shard routing, cross-zone message + forced-inclusion types, and a benchmark harness.

The coupling we set out to remove: block production drained the mempool, executed during block construction, carried full transaction bodies, built DA sidecars from the transaction list, required followers to reconstruct DA and replay transactions, and submitted proof hashes through the normal mempool.

## 3. Target Architecture (as now realized)

Base-chain blocks are **settlement envelopes**:

```text
Block
  header
  proof_updates[]
  certificate_batch_roots[]
  da_commitments[]
  cross_zone_message_roots[]
  forced_inclusion_queue_root
  optional_full_transactions[]   # compatibility lane only
```

Execution lives in independent lanes:

```text
Owned-object lane:   user tx -> validator precheck -> certificate -> object-local finality
Execution-zone lane: zone txs -> local execution -> DA blob -> validity proof -> base-chain proof update
Shared-state lane:   transfers, EVM, disputes, governance -> HyperBFT ordered execution
```

The base chain stays decentralized because validators still verify: quorum certificates, proof public inputs, validity proofs, DA commitments + sampling evidence, forced-inclusion constraints, and slashable evidence for conflicting owned-object certificates.

## 4. Implementation Status

**All PRD workstreams (A–H) are complete.** Hardening (S) is 3/5 done.

| WS | Theme | Tasks | Status |
|----|-------|-------|--------|
| **A** | Block Payload Refactor | A1 ✅ A2 ✅ A3 ✅ | **Done** — `BlockPayload` enum, versioned payload roots, `FRACTAL_BLOCK_PAYLOAD_MODE` |
| **B** | Proof Pool & Ingestion | B1 ✅ B2 ✅ B3 ✅ | **Done** — `ProofPool`, `fractal_submitProofUpdate`, public-input verification |
| **C** | Replay-Free Apply Path | C1 ✅ C2 ✅ C3 ✅ | **Done** — `BlockApplyMode`, proof-driven state roots, per-zone finality RPC |
| **D** | DA Decoupling | D1 ✅ D2 ✅ D3 ✅ | **Done** — zone-blob DA, `DaSamplingReceipt`, separate DA fee accounting |
| **E** | Owned-Object Cert Fast Path | E1 ✅ E2 ✅ E3 ✅ | **Done** — countersign RPC, `CertificatePool`, cert-batch root |
| **F** | Scope-Aware Routing | F1 ✅ F2 ✅ | **Done** — scope-based route key + diagnostics |
| **G** | Cross-Zone & Forced Inclusion | G1 ✅ G2 ✅ | **Done** — message roots, `forced_inclusion_queue_root` on `MasterchainBlockV1` |
| **H** | Benchmark Harness | H1 ✅ H2 ✅ H3 ✅ | **Done** — baseline + proof-ingestion benches + comparison report |
| **S** | Safety, Hardening & Adversarial Testing | S1 ✅ S2 ✅ S3 🟡 S4 ✅ S5 ⏳ | **In progress** — fuzz tests nearly done; DA sampling policy open |

Legend: ✅ complete · 🟡 work present but not finalized · ⏳ open.

## 5. Key Deliverables & Where They Live

| Capability | Location |
|------------|----------|
| Payload contract + roots (`BlockPayload`, `payload_root()`) | `crates/consensus/src/payload.rs` |
| Proof pool (separate from tx mempool) | `crates/mempool/src/proof_pool.rs` |
| Proof-update RPC + proof-input verification + apply modes + finality RPC | `crates/node/src/lib.rs` |
| Zone proof public-input verification + digests | `crates/shard/src/lib.rs` (`verify_zone_update_public_inputs`, `zone_proof_public_input_digest`) |
| DA sampling receipt + zone-blob DA + sampling | `crates/consensus/src/lib.rs` (`DaSamplingReceipt`, `build_da_sampling_receipt`, `verify_da_sampling_receipt`) |
| Forced-inclusion queue root + types | `crates/masterchain/src/ledger.rs`, `crates/masterchain/src/bft.rs` |
| DevDigest production gate | `crates/consensus/Cargo.toml` (`dev-digest` feature) + `crates/consensus/src/lib.rs` |
| Baseline benchmark (H1) | `crates/benchmarks/src/bin/baseline.rs` |
| Proof-ingestion benchmark (H2) | `crates/benchmarks/src/bin/proof_ingestion.rs` |
| Comparison report (H3) | `scripts/compare-proof-ingestion-bench.py` |
| Adversarial / property / fuzz tests | `crates/*/tests`, `fuzz/fuzz_targets/` |

## 6. How to Run & Verify

```bash
# 1. Baseline: current fractalchain, full-tx execution (H1) -> JSON summary
cargo run --release --bin baseline -- <scenario/flags>      # emits a BaselineBenchReport JSON

# 2. Experiment: fractalchain2 proof-ingestion (H2) -> JSON summary (same schema)
cargo run --release --bin proof_ingestion -- <scenario/flags>

# 3. Comparison report (H3): deltas + bottleneck classification
python3 scripts/compare-proof-ingestion-bench.py <baseline.json> <proof_ingestion.json>
#   -> emits JSON + Markdown + HTML; classifies bottleneck per scenario
#   (consensus / proof verification / DA sampling / network / storage)

# 4. Test suite (incl. adversarial + property tests)
cargo test --workspace

# 5. Fuzz targets (nightly / periodic)
cargo +nightly fuzz run <target>     # targets live in fuzz/fuzz_targets/
```

> Binaries accept scenario/flags; run with `--help` for exact options. The H1/H2 reports share the `BaselineBenchReport` schema so H3 can diff them directly.

## 7. Success Metrics (now measurable via H1/H2/H3)

| Metric | Goal |
|---|---|
| Block production latency | Lower p50 / p95 than baseline under equivalent load |
| Owned-object finality latency | Sub-block / one round-trip after quorum countersignature |
| Proof ingestion throughput | Higher accepted updates/sec than tx-execution blocks |
| Validator CPU per finalized state update | Lower than replay-based path |
| Block payload bytes | Lower for proof-covered workloads |
| DA sampling cost | Bounded independently of full tx volume |
| Shared-state correctness | No regression vs baseline tests |
| Decentralization | Same validator quorum assumptions; no trusted single-sequencer finality |

The H3 report is the instrument that answers whether these hold.

## 8. Remaining Work

1. **#25 (S3) — finalize property/fuzz tests.** proptest coverage and cargo-fuzz targets are already added (ordering sensitivity, single-bit mutation resistance, determinism, small-space collisions). **Close-out:** flip status to complete, wire fuzz targets into the nightly/CI schedule, document how to run them. *(Work present; just needs finalization.)*
2. **#27 (S5) — centralize DA sampling parameters.** `min_samples` is still a pass-by-parameter / hardcoded literal (`min_samples: 4` appears at `crates/consensus/src/lib.rs:4223,4243,4597`). **Build:** a single `DaSamplingPolicy` source of truth (min samples as a function of share count + Reed-Solomon ratio + collision target), enforced at **both** the verify boundary (`:1486`) and the commitment-build boundary (`:1671`); replace all literals.

After both close, the experiment enters its **evaluation phase**: run H1 vs H2 across all scenarios, read H3, and record the verdict against the success metrics.

## 9. Global Invariants (apply to all remaining work)

- Do **NOT** remove the current full-transaction block path — legacy mode stays.
- Do **NOT** weaken proof verification into trusted-sequencer assertions — fail closed.
- Unsupported production proof systems **MUST fail closed**; dev-digest mode only for local benchmarking (now enforced via the `dev-digest` feature gate).
- Legacy block encoding and `tx_root` must remain byte-for-byte stable.

## 10. Non-Goals (unchanged)

- Do not remove the full-transaction block path.
- Do not weaken proof verification into trusted-sequencer assertions.
- Do not make cross-zone calls synchronous.
- Do not benchmark only singleton mode — BFT-7 must be included before judging.
- Do not claim production STWO proof acceptance until the concrete verifier is wired.

## 11. Open Questions (updated)

- Single generalized payload root vs separate roots per lane? → *Resolved in practice: separate versioned roots per lane, unified via the header.*
- Owned-object certs signed by full validator set or home-shard committee? → *Still open; current path uses `2f+1` aggregation.*
- Mandatory DA sampling for every validator or delegated sampler committee with aggregate evidence? → *`DaSamplingReceipt` (#27) will pin the concrete policy.*
- How should base fees price proof verification vs DA bytes vs shared-state execution? → *Fee categories are split (#12); pricing curves still to be tuned from H1/H2 data.*
- Minimum proof-update public-input set for benchmark realism before production STWO? → *DevDigest covers benchmark realism today; production STWO wiring remains future work.*

## 12. Definition of Done

This experiment is complete when:

- [x] `fractalchain` and `fractalchain2` can run the same benchmark scenarios.
- [x] `fractalchain2` can produce blocks containing proof updates and certificate batch roots.
- [x] validators can accept proof-covered updates without replaying transaction bodies.
- [x] DA verification can be sampled rather than fully reconstructed on the fast path.
- [x] owned-object certificates can finalize object-local transactions before global ordering.
- [x] a benchmark report shows whether proof ingestion improves throughput and latency without reducing validator quorum safety. *(H3 report exists; final verdict pending a full H1-vs-H2 run.)*
- [ ] **#25 and #27 closed** so safety/liveness/test coverage is complete.

**Bottom line:** the build is done. What remains is finishing two hardening tasks and running the evaluation to record the verdict.
