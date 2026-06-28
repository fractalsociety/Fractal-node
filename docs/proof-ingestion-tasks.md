# Proof-Ingestion Decoupling — Parallel Task Breakdown (Tasks 1–27)

> Source PRD: [`proof-ingestion-decoupling-prd.md`](./proof-ingestion-decoupling-prd.md)
> Machine-readable companion: [`proof-ingestion-tasks.json`](./proof-ingestion-tasks.json) — read this to dispatch tasks to agents.
> Created: 2026-06-21 · Scope: Workstreams **A–H** (full PRD, tasks 1–22).

## Key finding

Most primitives **already exist** in the codebase — owned-object certificates (countersign / aggregate / verify / slashing), cross-zone messages + forced-inclusion types, DA sidecar from `borsh(transactions)`, the replay apply path, block-level finality RPC, and the benchmark harness. These tasks are **decoupling / wiring extensions**, which is why they parallelize well.

## Global invariants (apply to every task)

- Do **NOT** remove the current full-transaction block path — legacy mode stays.
- Do **NOT** weaken proof verification into trusted-sequencer assertions — fail closed.
- Unsupported production proof systems **MUST fail closed**; dev-digest mode only for local benchmarking.
- Legacy block encoding and `tx_root` must remain byte-for-byte stable.

## How to run a task

1. Pick a task whose `blocked_by` are all **completed**.
2. Read its **anchor** files to understand current code.
3. Implement the **build** spec, satisfying **every** acceptance criterion with tests.
4. Mark it done.
5. If you changed a shared interface (`BlockPayload` enum, header fields, apply seam), update the tasks in your **blocks** list.

## Parallel waves

| Wave | Tasks | Gate |
|------|-------|------|
| **0 — run now** | 1, 3, 4, 7, 9, 11, 12, 13, 14, 16, 17, 20 | none |
| **1** | 2, 5, 10, 15, 18, 19 | after #1 (and #4 for 5, #14 for 15) |
| **2** | 6, 25, 27 | after #2 (+ wave-1 roots for 25/27) |
| **3** | 8, 23, 24 | after #7 and #6 (24 needs all roots) |
| **4** | 21, 26 | after proof-ingestion path (#5, #8, #15) |
| **5 — final** | 22 | after #20 and #21 |

**Critical path:** `#1 → #2 → #6 → #8` (payload enum → proof-update root → proof-input verify → state-root advance). Start #1 first; the rest of wave 0 runs alongside it.

#1 (A1) is the **shared interface** — the `BlockPayload` enum contract is embedded in its spec so the dependent tasks (2, 5, 10, 15) all build against the same shape without coordinating.

---

## Wave 0 — run now (no blockers)

### #1 · A1 — Add BlockPayload variants (legacy vs proof-ingestion) `blocks: 2, 5, 10, 15`
- **Status:** Completed. Implemented as a compatibility-safe payload contract in `crates/consensus/src/payload.rs`; legacy `Block`/`BlockHeader` serialization is unchanged, and versioned payload roots are available through the payload module for non-legacy payloads.
- **Anchor:** `Block` struct at `crates/consensus/src/lib.rs:235-243` carries `transactions`, `eth_signed_raw`, `da_sidecar` directly.
- **Build:** New `crates/consensus/src/payload.rs` with the shared payload contract; wrap existing block body behind the enum so legacy blocks encode byte-identically and `tx_root` stays deterministic; add a versioned payload-root slot in the header; add an `RpcBlock` payload-type field.
- **Contract:** `BlockPayload { FullTransactions{transactions, eth_signed_raw}, ProofUpdates(Vec<ZoneProofUpdateV1>), CertificateBatches(Vec<OwnedObjectCertificateBatchV1>), Mixed(Vec<BlockPayloadItem>) }` + `BlockPayloadItem { Transaction, ProofUpdate, CertificateBatch }`.
- **Acceptance:** legacy encode/decode unchanged · `tx_root` deterministic (regression test) · new variants committable via versioned header root · RPC reports payload type.

### #3 · A3 — Compatibility mode env flag
- **Anchor:** node init at `crates/node/src/lib.rs`.
- **Build:** Read `FRACTAL_BLOCK_PAYLOAD_MODE=legacy|proof_ingestion|mixed` at init; gate proposal payload selection. Default `legacy`.
- **Acceptance:** legacy = current · proof_ingestion emits proof/cert payloads when available · mixed = shared-state txs + proof updates · mode logged + queryable.

### #4 · B1 — Add ProofPool separate from tx mempool `blocks: 5`
- **Status:** Completed. Implemented `crates/mempool/src/proof_pool.rs` with `(zone_id, height)` keying, independent metrics, conflict rejection, and optional retained conflict evidence.
- **Anchor:** tx mempool `crates/mempool/src/lib.rs:19-49` (drain 126-165); proof hashes enter mempool at `crates/node/src/lib.rs:1584`.
- **Build:** New `crates/mempool/src/proof_pool.rs` — `ProofPool` keyed by `(zone_id, height)`; conflicts rejected or retained as evidence; own `ProofPoolMetrics`.
- **Acceptance:** stores/evicts by `(zone_id, height)` · conflict handling · metrics independent of tx mempool.

### #7 · C1 — Split block application into BlockApplyMode `blocks: 8`
- **Anchor:** `apply_synced_block` at `crates/node/src/lib.rs:1018-1062`; execution `apply_block_with_evm` at `crates/core/src/lib.rs:78-90`.
- **Build:** `BlockApplyMode { ReplayFullTransactions, VerifyProofAndDa, HeaderOnlyAfterProofFinal }` + dispatch skeleton selecting by payload type; introduce a trait/seam for C2; keep all three paths callable.
- **Acceptance:** legacy → replay · proof-ingestion → verify · replay stays for archive nodes · validators can skip replay for proof-covered updates · selection testable in isolation.

### #9 · C3 — Add proof-finality indexing (per-zone/update)
- **Status:** Completed. Added per-zone/update proof-finality indexes in `NodeInner`, persisted zone records in `ProofFinalityStore`, and exposed `fractal_getProofFinalHeight` plus `fractal_getZoneUpdateFinality`.
- **Anchor:** `settlement_finality_for_block_hash` at `crates/rpc/src/module.rs:575-584`; `fractal_getSettlementBlock` at 843-897.
- **Build:** Extend finality from block-level to zone/update-level; new RPC (e.g. `fractal_getProofFinalHeight(zone_id)`); persist a small index so it survives restart.
- **Acceptance:** per-update soft/proof finality · latest proof-final height per zone queryable · survives restart · no regression to block finality RPC.

### #11 · D2 — Add DA sampling receipt / certificate
- **Build:** `DaSamplingReceipt` type + verifier; self-contained (operates over commitments/indices).
- **Acceptance:** binds sampled indexes + commitments · DA verify passes on receipt alone (no full reconstruction) · full reconstruction stays for archive/debug · tests for valid / tampered / insufficient.

### #12 · D3 — Split DA fee accounting from execution gas
- **Build:** Separate DA fee from EVM/native gas; fee-policy module exposing three cost categories (DA bytes, proof verify, shared-state execution).
- **Acceptance:** DA fee metrics separate from gas · proof updates pay proof-verify cost separately · shared-state txs keep execution gas · fee-policy readable by benchmark.

### #13 · E1 — Certificate request + countersign RPC
- **Status:** Completed. Added owned-object precheck, countersign, and certificate aggregation RPCs with a node-backed round-trip test.
- **Anchor:** `OwnedObjectCertificate` countersign/aggregate/verify at `crates/core/src/tx.rs:286/298/314`; conflict evidence at 256-266.
- **Build:** Network/RPC path around existing cert types — client requests precheck data, validators countersign eligible owned txs, client aggregates `2f+1`.
- **Acceptance:** precheck request RPC · validator countersign handler · client aggregates `2f+1` · round-trip tests.

### #14 · E2 — CertificatePool + direct finality `blocks: 15`
- **Anchor:** cert at `crates/core/src/tx.rs:244`; slashing evidence at 256-266 (already implemented).
- **Build:** New `CertificatePool` giving certs direct object-local finality without global ordering; hook point for block cert-batch roots (root math is E3).
- **Acceptance:** accepts valid certs · conflicting versions rejected as slashable evidence · RPC reports object finality · blocks include cert-batch root hook.

---

## Wave 1 (after #1; #5 also needs #4, #15 also needs #14)

### #2 · A2 — Add proof-update payload root `blocked_by: 1` · `blocks: 6`
- **Status:** Completed. Added `proof_updates_root` and `proof_update_leaf_hash` in `crates/consensus/src/payload.rs`, wired `BlockPayload::ProofUpdates` to the explicit root, and covered empty/single/multi/order/tamper cases.
- **Build:** Deterministic `proof_updates_root(&[ZoneProofUpdateV1])` binding per leaf: `zone_id, parent_root, new_root, da_root, message_root, circuit_version, proof_digest`. Implement standalone; wire into `ProofUpdates` variant once #1 lands.
- **Acceptance:** stable across nodes · binds all fields · header hash changes on any field change · unit tests (single/multi/empty/tamper).

### #5 · B2 — Add fractal_submitProofUpdate RPC `blocked_by: 1, 4`
- **Status:** Completed. Added `fractal_submitProofUpdate` for Borsh-encoded `ZoneProofUpdateV1` submissions into `ProofPool`, wired node proposal to commit direct proof updates through the proof-ingestion payload root without transaction gas, and retained `fractal_submitProofHash` as the legacy compatibility wrapper.
- **Anchor:** `submit_proof_hash` at `crates/node/src/lib.rs:1584-1613`; native call at `crates/core/src/tx.rs:83`.
- **Build:** New RPC `fractal_submitProofUpdate` pushing `ZoneProofUpdateV1` directly into `ProofPool` (#4), includable in a proof-ingestion block without consuming gas; keep `fractal_submitProofHash` as compat wrapper.
- **Acceptance:** new RPC registered · legacy retained · direct submission includable without gas · tests for both paths.

### #10 · D1 — Replace tx-list DA with zone-blob DA `blocked_by: 1`
- **Status:** Completed. Added standalone `ZoneBlobDaV1` encoding/commitment helpers with sampling-parameter binding, wired proof-ingestion block production to use a zone-blob DA sidecar committed through the header extension slot, and kept the legacy tx-list DA path intact.
- **Anchor:** `build_da_sidecar` at `crates/consensus/src/lib.rs:1861-1863` (builder 1317-1374) uses `borsh(transactions)`; `da_root`, `reconstruct_da_payload`.
- **Build:** Zone-blob DA path submitted independently of the base-chain tx list; header commits namespace, DA root, byte count, share count, sampling params; proof finality requires DA verify; legacy tx-list DA still works. Implement zone-blob encoding/commitment standalone; wire header once #1 lands.
- **Acceptance:** zone blob DA independent of tx list · header commits all listed fields · proof finality requires DA verify · legacy path intact.

### #15 · E3 — Add certificate batch payload root `blocked_by: 1, 14`
- **Status:** Completed. Added deterministic certificate batch roots that bind certificate hashes and object versions, reject duplicate object versions, and are used by proof-ingestion block production to commit accepted certificates without replaying their transactions.
- **Build:** Deterministic batch root for `OwnedObjectCertificateBatchV1` (the `CertificateBatches` variant from #1); commit certs in proof-ingestion blocks without replaying txs. Implement standalone; wire once #1/#14 land.
- **Acceptance:** deterministic · binds all cert hashes + object versions · conflicting certs cannot appear in one accepted batch · unit tests (empty/single/multi/duplicate-version rejection).

---

## Wave 2 (after #2)

### #6 · B3 — Verify proof public inputs against block payload `blocked_by: 2` · `blocks: 8`
- **Build:** Verification binding a proof's public inputs to the committed payload root (#2) + DA commitment; implement standalone, wire once #2 lands.
- **Acceptance:** fails on mismatch of `parent_state_root, state_root, tx_root, da_root, message_root`, circuit metadata · unsupported production proofs fail closed · dev-digest only behind non-default flag · unit tests per field + digest-mode gating.

---

## Wave 3 (after #7 and #6)

### #8 · C2 — Update state roots from verified proof updates `blocked_by: 7, 6`
- **Build:** Implement the `VerifyProofAndDa` branch of `BlockApplyMode` (#7): state transition comes from accepted proof updates (#6), not local execution.
- **Acceptance:** zone root advances only on proof verify success · masterchain root deterministic from accepted zone roots · rejected proofs don't mutate state · tests for accepted / rejected / deterministic derivation.

---

## Workstream F — Scope-Aware Routing (16–17)

### #16 · F1 — Route by execution scope, not only signer
- **Anchor:** signer routing `crates/shard/src/lib.rs:75-114` (`home_shard_for_address`/`home_shard_for_signer`/`accepts_transaction`); gateway `crates/rpc/src/gateway.rs:105-143`; scope `TxExecutionScope::Owned` + `OwnedObjectId` at `crates/core/src/tx.rs:131,112-119`.
- **Build:** Add scope-aware routing alongside signer routing. Owned `Agent(agent_id)` → by agent id; owned `Receipt(receipt_id)` → by receipt id; wallet anchors → by commitment; shared/EVM → consensus. Deterministic `route_key` from `TxExecutionScope` + `OwnedObjectId`.
- **Acceptance:** Agent ops route by agent id · Receipt ops by receipt id · wallet anchors by commitment · shared/EVM keep consensus · route key deterministic + unit-tested.
- **Status:** Completed. Added scope route keys in `fractal-shard`, routed borsh gateway submissions/diagnostics by execution scope, and covered agent/receipt/wallet/shared/EVM route behavior with unit tests. Verified with `cargo check -p fractal-rpc -p fractal-node` and `cargo test -p fractal-shard route --lib`.

### #17 · F2 — Add routing diagnostics
- **Status:** Completed. Added route diagnostics with source shard, expected shard, shard count, and route key; exposed `fractal_debugTxRouting`; made multi-shard raw-tx submission reject wrong-shard transactions with route details; and extended the load tool to print per-shard submit imbalance.
- **Anchor:** wrong-shard check `crates/shard/src/lib.rs:147`; gateway `crates/rpc/src/gateway.rs:105-143`.
- **Build:** RPC reports computed home shard + route key; mempool rejects wrong-shard submissions with route details; load tests report per-shard imbalance. Build on existing signer routing; add scope-key reporting when #16 lands.
- **Acceptance:** RPC reports shard + route key · wrong-shard rejection includes route details · load tests report per-shard imbalance · observability only (no behavior change).

---

## Workstream G — Cross-Zone Messages & Forced Inclusion (18–19)

### #18 · G1 — Add message root payloads `blocked_by: 1`
- **Anchor:** `AsyncCrossZoneMessageV1` `crates/shard/src/lib.rs:317-323`; `CrossZoneMessageInclusionProofV1` 326-338; `ExecutionZoneRegistryV1` 422-428; `submit_cross_zone_message` 496; `drain_cross_zone_messages_for` 510.
- **Build:** Async cross-zone via message roots. Zone proof update includes outbound message root (merkle over queued messages); base chain orders/commits message roots (via A1's versioned payload root); destination consumes by `CrossZoneMessageInclusionProofV1`. Wires the existing `message_root` field bound by A2.
- **Acceptance:** zone proof update includes outbound message root · base chain orders message roots · destination consumes by inclusion proof · round-trip test (submit → root → order → consume).
- **Status:** Completed. Added outbound message selection and destination consume-by-proof APIs in `fractal-shard`, replay protection for consumed message leaves, and a submit → root → proof-update payload root → consume round-trip test. Verified with `cargo test -p fractal-shard --lib` and `cargo test -p fractal-consensus payload --lib`.

### #19 · G2 — Add forced-inclusion queue root `blocked_by: 1`
- **Anchor:** `ForcedInclusionRequestV1`/`ForcedInclusionEventV1` `crates/shard/src/lib.rs:341-361`; `ExecutionZoneRegistryV1` 422-428; `submit_forced_inclusion` (uses `deadline_masterchain_height`) 532.
- **Build:** Censorship escape hatch. Base chain accepts forced-inclusion requests and commits `forced_inclusion_queue_root` (via A1). Zone proof MUST include required items after `deadline_masterchain_height` elapses; missing items reject zone proof finality (hook into B3/C3).
- **Acceptance:** base chain accepts requests · commits `forced_inclusion_queue_root` · zone proof must include items after timeout · missing items reject finality · timeout/inclusion tests.
- **Status:** Completed. Bound forced-inclusion roots into proof-update payload commitments, enforced timed-out request roots during shard and dedicated-masterchain proof finality, tracked proven request ids, and covered queue-root/timeout/missing/satisfied inclusion cases. Brought `fractal-masterchain` and `fractal-proof-aggregator` into workspace checks with STWO/proof-condenser gated behind opt-in features. Verified with `cargo check --workspace`, `cargo test -p fractal-masterchain`, `cargo test -p fractal-shard forced_inclusion --lib`, `cargo test -p fractal-consensus payload --lib`, and `cargo test -p fractal-proof-aggregator --lib`.

---

## Workstream H — Benchmark Harness (20–22)

### #20 · H1 — Baseline benchmark path `blocks: 22`
- **Anchor:** `crates/benchmarks/src/lib.rs` (535-614 cert, 616-659 DA, 661-739 proof, 741 mixed SLO); `crates/benchmarks/src/bin/protocol.rs:1-81`; `tools/load-tps/src/main.rs:1-269`; `scripts/load-tps-paced.sh`.
- **Build:** Run the BASELINE (current fractalchain, full tx execution) across all H1 scenarios: native NoOp, owned-object tx, proof commitment, mixed EVM/native, BFT-7. Capture submitted/committed tx/sec, block p50/p95 latency, CPU/mem, block bytes, DA bytes, replay time. Emit a stable JSON summary for H3.
- **Acceptance:** all five scenarios one-command · full metric set · JSON summary file · BFT-7 lab included.
- **Status:** Completed. Added `fractal-baseline-bench` plus a stable `BaselineBenchReport` JSON schema covering native NoOp, owned-object tx, proof commitment, mixed EVM/native, and BFT-7 validator lab scenarios. Metrics include submitted/committed tx/sec, block p50/p95 latency, `cpuNanos`, `peakWorkingSetBytes`, block/DA bytes, replay time, and BFT quorum counts. Generate a summary with `cargo run -q -p fractal-bench --bin fractal-baseline-bench -- --output bench-results/baseline-summary.json`. Verified with `cargo test -p fractal-bench` and a JSON output smoke run.

### #21 · H2 — Proof-ingestion benchmark path `blocked_by: 5, 8, 15` · `blocks: 22`
- **Anchor:** extend `crates/benchmarks/src/lib.rs` + `tools/load-tps/src/main.rs`.
- **Build:** Benchmark fractalchain2 proof-ingestion payloads: accepted proof updates/sec, accepted cert updates/sec, mixed proof updates + shared-state txs, DA sampling enabled, BFT-7. Capture accepted proof/cert updates/sec, block p50/p95, proof verify time, DA sampling time, CPU/mem, payload bytes. Emit JSON summary in the SAME shape as H1.
- **Acceptance:** all proof-ingestion scenarios · full metric set · JSON schema matches H1 · comparison shows throughput/latency gains without weakening quorum safety.
- **Status:** Completed. Added `fractal-proof-ingestion-bench` using the same `BaselineBenchReport` schema as H1. Scenarios cover proof updates/sec, certificate updates/sec, mixed proof updates plus shared-state tx payloads, DA-sampling proof updates, and BFT-7 proof-ingestion quorum formation. Metrics include accepted proof/cert rates, block p50/p95, proof verify time, DA sampling time, `cpuNanos`, `peakWorkingSetBytes`, payload bytes, replay time, and BFT quorum counts. Generate a summary with `cargo run -q -p fractal-bench --bin fractal-proof-ingestion-bench -- --output bench-results/proof-ingestion-summary.json`. Verified with `cargo test -p fractal-bench` and a JSON output smoke run.

### #22 · H3 — Comparison report `blocked_by: 20, 21`
- **Status:** Completed. Added `scripts/compare-proof-ingestion-bench.py`, which ingests H1 baseline and H2 proof-ingestion JSON summaries, canonicalizes comparable scenario names, emits per-metric deltas and bottleneck classifications, and writes JSON, Markdown, and HTML reports. Verified with small H1/H2 benchmark runs plus generated comparison JSON/Markdown/HTML.
- **Build:** Script ingesting H1 (baseline) + H2 (proof-ingestion) JSON summaries; emits per-metric deltas + bottleneck-category classification per scenario (consensus, proof verification, DA sampling, network, storage). Human-readable markdown/HTML.
- **Acceptance:** reads both JSON summaries · per-metric deltas · bottleneck category per scenario · human-readable report.

---

## Dependency graph

```
#1 (A1 BlockPayload) ─┬─► #2 (A2 root) ────────────► #6 (B3 verify) ──┐
                      ├─► #5 (B2 RPC) ◄─ #4 (B1 pool)                   ├─► #8 (C2 state roots)
                      ├─► #10 (D1 zone DA)                              │
                      └─► #15 (E3 batch) ◄─ #14 (E2 cert pool)          │
                                                                        │
#7 (C1 apply modes) ────────────────────────────────────────────────────┘

Independent (wave 0): #3 (A3 flag), #9 (C3 finality), #11 (D2 receipt), #12 (D3 fees), #13 (E1 RPC), #16 (F1 scope routing), #17 (F2 routing diag), #20 (H1 baseline bench)

Also: #1 ─► #18 (G1) & #19 (G2)  |  #20 + #21 ─► #22 (H3)  |  #21 (H2) blocked_by #5, #8, #15
```

## Workstream S — Safety, Hardening & Adversarial Testing (23–27)

Hardening / liveness goals layered on top of the feature tasks (beyond the PRD).

### #23 · S1 — Make DevDigest impossible in production builds/configs `blocked_by: 6`
- **Status:** Completed. Gated `ValidityProofSystem::DevDigest` behind a non-default `fractal-consensus/dev-digest` feature, preserved explicit Borsh discriminants so default builds reject DevDigest tag `0`, added a runtime production/mainnet guard for opt-in dev builds, moved benchmarks to production fixture proofs, and kept node proof-finality tests on production proof fixtures.
- **Anchor:** `ValidityProofSystem::DevDigest` at `crates/consensus/src/lib.rs:322`; verify arm 2037-2040; `BadDevDigest` at 136-137.
- **Build:** Compile-time + runtime hard-gate: Cargo feature `dev-digest` (off by default, excluded from release/mainnet profiles); runtime guard hard-rejects DevDigest at submission AND verification under prod config; remove ambient defaults.
- **Acceptance:** release build cannot construct/verify a DevDigest proof · usable only under explicit dev feature + local bench · prod-config test rejects at both boundaries · no benchmark regression.

### #24 · S2 — Adversarial proof public-input mismatch tests across every root `blocked_by: 6, 10, 15, 18, 19`
- **Anchor:** `verify_zone_update_public_inputs` `crates/shard/src/lib.rs:742`; `zone_proof_public_input_digest` 1041; `payload_root()` `crates/consensus/src/payload.rs:98`; reject pattern `crates/consensus/src/lib.rs:3398`.
- **Build:** For every root (proof-update #2, cert-batch #15, message #18, DA #10, forced-inclusion #19, legacy tx_root) mutate each bound field and assert verify FAILS; cover swap / bit-flip / truncation / stale-replay / cross-root confusion.
- **Acceptance:** one test per (root × field) · cross-root confusion rejects · tampered/stale rejects · CI suite, no mutation slips through.
- **Status:** Completed. Added adversarial tests for proof-update roots, certificate-batch roots, consensus block validity public inputs, shard zone proof public-input digests, message/DA/forced-inclusion cross-root confusion, stale forced-inclusion replay, and zone-proof commitment `proof_digest`/`prover` tampering. Gated DevDigest-only tests behind `dev-digest` so default CI remains green while the DevDigest adversarial suite runs with `--features dev-digest`. Verified with `cargo test -p fractal-consensus --lib`, `cargo test -p fractal-consensus --features dev-digest --lib`, `cargo test -p fractal-shard --lib`, `cargo check --workspace`, and `cargo fmt --all --check`.

### #25 · S3 — Property/fuzz tests for payload root ordering + mutation resistance `blocked_by: 2, 10, 15, 18, 19`
- **Anchor:** `payload_root()` `crates/consensus/src/payload.rs:98`; existing fuzz target `fuzz/fuzz_targets/proof_envelope.rs`.
- **Build:** proptest + cargo-fuzz over every root fn — ordering sensitivity, single-bit mutation resistance, determinism, ordered/non-commutative, empty/single boundaries, no trivial collisions. New targets under `fuzz/fuzz_targets/`.
- **Acceptance:** targets per root fn · properties hold over generated inputs with reproducible shrinks · property tests in CI, fuzz targets for nightly.
- **Status:** Completed. Added `proptest` coverage for proof-update roots, certificate-batch roots, full/mixed payload roots, DA commitment hashes, ordering sensitivity, single-bit mutation resistance, determinism, and small-space collision checks. Added cargo-fuzz targets for payload roots and DA commitment/header roots under `fuzz/fuzz_targets/`. Verified with `cargo test -p fractal-consensus --lib payload --no-fail-fast`, `cargo check --manifest-path fuzz/Cargo.toml --bins`, and `cargo fmt --all --check`.

### #26 · S4 — Stress forced-inclusion liveness (multi-zone, delayed proofs) `blocked_by: 8, 19`
- **Anchor:** forced-inclusion types `crates/masterchain/src/ledger.rs:69-91,290-291`; `InvalidForcedInclusionTimeout` 332; `forced_inclusion_queue_root` `crates/masterchain/src/bft.rs:148`.
- **Build:** Multi-zone stress harness with delayed/withheld proofs asserting liveness (forced request eventually included after `deadline_masterchain_height`; report worst-case latency) AND safety (zone proof omitting a due item is rejected until included).
- **Acceptance:** bounded-block liveness after deadline · missing items reject finality until included · adversarial withholding overridden · latency reported.
- **Status:** Completed. Added a dedicated masterchain stress harness with four zones, staggered deadlines, eight concurrent forced-inclusion requests, delayed/withheld proofs, queue-root commitment checks, omitted-root finality rejection, satisfied-root finality acceptance, and a `ForcedInclusionStressReport` carrying `worst_inclusion_latency_blocks`. Verified with `cargo test -p fractal-masterchain` and `cargo check --workspace`.

### #27 · S5 — Define minimum DA sampling parameters, reject undersampled receipts `blocked_by: 10, 11`
- **Anchor:** `DaSamplingReceipt` + `min_samples` `crates/consensus/src/lib.rs:254,267`; `build_da_sampling_receipt` 1408; `verify_da_sampling_receipt` 1451-1463; commitment check 1648; literal `min_samples: 4` at 3990.
- **Build:** Single `DaSamplingPolicy` source of truth (min samples vs share count + RS ratio + collision target; min cols/rows; confidence), enforced at BOTH verify (1451) and build (1648) boundaries; eliminate literals like `min_samples: 4`.
- **Acceptance:** one policy module, no caller literals · undersampled rejected at both boundaries · soundness rationale documented · min-1 rejects / min+ passes / scales with share count.

---

## Status

All 27 tasks written (Workstreams A–H + S hardening). The PRD's Definition of Done is reached when 1–22 are complete and the H3 report shows whether proof ingestion improves throughput/latency without reducing validator quorum safety; tasks 23–27 then harden safety, liveness, and test coverage.

**Order:** wave 0 → wave 1 → 6 (then 25, 27) → 8 (then 23, 24) → 21, 26 → 22. Critical path: `#1 → #2 → #6 → #8 → #21 → #22`.
