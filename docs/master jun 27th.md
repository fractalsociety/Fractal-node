# Master Jun 27th - FractalWork + FractalChain2 PRD

**Date:** June 27, 2026  
**Scope:** `/Users/jamesstar/fractalwork` and `/Users/jamesstar/fractalchain2`  
**Purpose:** Single status PRD for what exists, what is partially done, and what remains across the Fractal Society work stack and FractalChain2 settlement layer.

---

## 1. Executive Summary

FractalWork is the product/work graph layer: deterministic work events, agents, verifiers, dashboards, repository graphs, Forge local engine, Agent Olympics, and emerging research/benchmark receipts.

FractalChain2 is the chain layer: a sharded agent-first L1 with HyperBFT shards, masterchain anchoring, native FractalCore syscalls, EVM compatibility, proof ingestion, DA commitments, owned-object certificates, explorer/indexer tooling, and devnet/testnet runbooks.

As of June 27, 2026:

- **FractalWork has a broad MVP scaffold with many real vertical slices implemented.** The deterministic core, API adapter, Postgres/event replay path, dashboard, queue workers, repository graph work, Forge core, Agent Olympics core, and BioLatent benchmark slice are present. The major gap is turning these slices into production-ready, fully integrated user flows with hardened marketplace, router, graph DB, payments, and deployment operations.
- **FractalChain2 has a working local/dev chain surface and a mostly completed proof-ingestion experiment.** HyperBFT shard/node tooling, RPC, masterchain/proof plumbing, explorer/indexer/wallet tools, benchmark harnesses, and CI/testnet gate docs exist. The major gap is release-grade validation: BFT-7 liveness under real conditions, sustained-load p99 finality, partition recovery, proof-worker scope/sign-off, M10/M11 exits, fuzz corpus runs, audit, and public testnet gates.
- **The integration thesis is clear but not fully shipped:** FractalWork should emit deterministic receipts/batches and permissioned work/reputation events; FractalChain2 should settle hashes, signatures, commitments, reward state, reputation checkpoints, and proof roots without storing private graph content on-chain.

---

## 2. Unified Product Thesis

Fractal Society is a useful-work operating system:

```text
people -> wallets -> skills -> agents -> tasks -> evidence -> verification
       -> receipts -> settlement -> reputation -> rewards -> training data
```

FractalWork owns the off-chain workflow, graph, agent, verifier, training, and user-facing product experience.

FractalChain2 owns deterministic settlement, finality, validator security, proof ingestion, data availability commitments, light-client verifiability, and native low-cost agent primitives.

The product succeeds when a human or agent can complete useful work, have it verified, produce deterministic receipts, settle commitments on-chain, update reputation, and feed approved outcomes back into routing, training, benchmarks, and rewards.

---

## 3. FractalWork Status

### 3.1 Done / Implemented

Core work engine:

- Monorepo scaffold with `apps/*` and `packages/*`.
- `packages/core` deterministic TypeScript FractalCore engine.
- Canonical JSON hashing, state roots, replay determinism, and Ed25519 event signature payload verification.
- Accounts, key rotation/history, suspension, jobs, bids, bid withdrawal, timeout expiration, submissions, verifier assignment/claim, verification, ledger, receipts, reputation, replay, and batch export.
- Tests for happy-path job cycle, replay determinism, account lifecycle, bid withdrawal, signatures, timeouts, Sybil operator prevention, and deterministic batches.

API and persistence:

- NestJS HTTP API adapter around `packages/core`.
- In-memory local mode when `DATABASE_URL` is unset.
- Postgres event repository with append-only `fractal_events`, replay, and transactional event/state persistence.
- Signed agent request verification helpers and Nest guard.
- Idempotency store for POST retry dedupe.
- Rate-limit and production-readiness tests.
- Prometheus/OpenTelemetry-oriented metrics and telemetry scaffolding.

Artifact and event operations:

- Artifact presigned URL and server-side SHA-256 verification service.
- Worker/verifier HTTP endpoints protected by `AgentSignatureGuard`.
- Repository-backed public-key resolution.
- Postgres materialized projections for accounts, jobs, bids, submissions, verifications, receipts, ledger records, and reputation.
- Webhook payloads signed with Ed25519 system-key signatures.

Queues and workers:

- BullMQ timeout and webhook queue services.
- Retry/backoff, due-timeout processing, state-transition deadline enqueueing, dispute-window finalization, and worker env gates.
- Settlement worker configuration documented in the runbook.

Dashboard:

- Next.js dashboard app.
- Requester, agent, verifier, and admin surfaces.
- Role-specific queues, financial health, attention metrics, and server actions for requester/admin flows.
- Settlement UI component exists.
- Tests for view models, dashboard flows, repository input, graph mock/store editing, wallet permissions, task payout, NFT registration, Forge surfaces, and Olympics dashboard regression.

Repository graph:

- Repository feature graph PRD exists.
- Dashboard repository input and analysis job tests exist.
- Repository analysis worker exists.
- Public/private GitHub access handling and OAuth/PAT strategy are specified.
- Feature graph schema and source-store tests exist.

Forge core:

- `packages/forge-core` exists with CLI, vault, importers, datasets, hardware detection, model registry, training abstraction, eval/run records, Langfuse hooks, memory, skills, Codex ingestion, redaction, and benchmarks.
- Tests cover headless workflow, CLI goals, importers, hardware, trainers, prerequisites, Langfuse, Codex ingest, and employee memory.

Agent Olympics:

- `packages/olympics-core` deterministic core is implemented.
- Closed Fractal Credit economy boundary is enforced in code.
- No transfer/bridge/withdrawal/external exchange/price feed path in the core.
- `WalletTaskReceiptV2`, seasons, arenas, matches, scoring, bonds, disputes, service market, dividends, pause, finalization, conservation checks, and no-transfer invariant checks exist.
- CLI can scaffold and validate adapter protocol flows.
- Tests cover adapter protocol, adversarial cases, arena plugins, BioLatent hooks, CLI, training flywheel, and vertical slice.
- Status from local README: implementation tasks 1-40 complete; API/persistence, dashboard surfaces, open ecosystem, training flywheel, and launch hardening remain.

BioLatent vertical slice:

- BioLatent benchmark docs and audit bundle flow exist.
- SIRT1 computational benchmark slice packages campaigns, hashes artifacts, emits research receipts, captures replay metadata, validates RO-Crate exports, separates public/dev from true-hidden labels, emits reproducibility/evidence gates, and exports training-failure candidates without hidden leakage.
- Dashboard page `/olympics/biolatent` is documented as showing metrics, gates, artifacts, receipts, candidates, and audit-bundle state.

Runbook and local operations:

- Local dev runbook exists for install, Docker infra, tests, API, seeding, dashboard, queues, GitHub repo analysis worker, settlement config, Olympics checks, disputes, finalization, and critical alerts.

### 3.2 Partially Done / Scaffolded

Blockchain integration:

- PRD exists for deterministic settlement batches, contract submission, reconciliation, dashboard status, NFT milestone minting, master wallets, and delegated sub-wallet permissions.
- EVM contract placeholder `contracts/evm/BatchSettlement.sol` is referenced.
- Settlement worker env/config is documented.
- Dashboard settlement component exists.
- Full production confirmation/reconciliation flow is not proven end to end in the current docs.

Wallets and permissions:

- Master wallet/sub-wallet requirements are specified.
- Dashboard tests for wallet permissions exist.
- Runbook documents local Fractal EVM shard payout and user-number NFT mint configuration.
- Production-grade wallet onboarding, delegated permission UX, and full runtime enforcement still need hardening/sign-off.

Repository graph:

- PRD, worker, tests, and dashboard pieces exist.
- Full graph persistence, large-repo scaling, high-quality evidence graph generation, and task conversion appear incomplete or not signed off.

Forge dashboard parity:

- Forge core/headless pieces are implemented.
- Dashboard tests for model selector, training prep/jobs, deployment, marketplace, router model, request classifier, external agents, RL artifacts, Hugging Face models, Langfuse chat capture, and outcome privacy exist.
- Native training backend execution across NVIDIA, Apple Silicon, and remote runners is still a major remaining item.

Marketplace and router:

- Marketplace/router concepts and tests exist.
- Production buyer checkout, metering, payouts, sandbox trials, route explanations, learned ranking, reliability feedback, and full fallback operations remain.

Graph operating system:

- Society schema package exists with canonical/offline verification tests.
- Master PRD defines node/edge model for people, wallets, skills, agents, models, datasets, tasks, claims, evidence, papers, rewards, DAOs, disputes, and reputation checkpoints.
- Full graph database, graph analytics, fraud detection, centrality/community/bottleneck dashboards, and graph reputation production paths are not done.

### 3.3 Not Done / Remaining

- Production-grade deployment hardening across API, dashboard, workers, queues, telemetry, secrets, backups, and incident response.
- Complete settlement batch submission and confirmation loop against FractalChain2 with test artifacts.
- On-chain reward/reputation settlement integration.
- Real marketplace checkout, metering, payouts, sandbox trials, and buyer privacy controls.
- Complete router filters/ranker/API and buyer-facing route explanations.
- Full graph database implementation and graph analytics.
- Full dashboard parity with headless Forge Core.
- Real native training backend execution and hardware-specific reliability across NVIDIA, Apple Silicon, CPU, and remote runners.
- Production GitHub OAuth app/scopes and private repo token lifecycle.
- Agent Olympics API + persistence tasks 41-45.
- Agent Olympics dashboard surfaces tasks 46-52.
- Agent Olympics plug-and-play ecosystem tasks 53-57.
- Agent Olympics training flywheel and launch hardening tasks 58-60.
- External security review for wallet, settlement, private repo, marketplace, and agent-runtime permission boundaries.

---

## 4. FractalChain2 Status

### 4.1 Done / Implemented

Core chain and local dev:

- Rust workspace with node, consensus, core, crypto, mempool, network, storage, shard, masterchain, proof, EVM, RPC, wallet, indexer, light-client, faucet, CLI, benchmarks, and SDK crates.
- Pinned nightly toolchain via `rust-toolchain.toml`.
- Local Track B lab runner with singleton HyperBFT shard, JSON-RPC, masterchain anchors, and STWO -> Plonky2 proof pipeline on a short interval.
- Pilot shard runners, masterchain runners, BFT-7/BFT-21 smoke scripts, RPC gateway, wallet web, explorer, indexer, status page, faucet, provider sample, load tools, and Langfuse import tools.

Execution and RPC:

- Native FractalCore syscalls plus EVM/revm coexistence are documented in the PRD.
- JSON-RPC supports standard Ethereum-style methods and Fractal extensions.
- Shard metadata RPCs exist: `fractal_getShardId`, `fractal_getShardCount`, `fractal_getConsensusMode`.
- Masterchain/proof RPCs are documented: `fractal_getMasterchainHead`, `fractal_getGlobalZkRoot`, `fractal_getGlobalZkProof`, `fractal_getCheckpointProof`, and digest/status methods.

Proof-ingestion decoupling experiment:

- Existing `docs/master-prd.md` reports 25/27 tasks complete as of June 21, 2026.
- Workstreams A-H are done:
  - Block payload refactor.
  - Proof pool and ingestion.
  - Replay-free apply path.
  - DA decoupling.
  - Owned-object certificate fast path.
  - Scope-aware routing.
  - Cross-zone and forced inclusion.
  - Benchmark harness.
- Base-chain block target is a settlement envelope with proof updates, certificate batch roots, DA commitments, cross-zone roots, forced-inclusion root, and legacy optional full transactions.
- Proof-ingestion benchmark and baseline benchmark exist.
- H3 comparison script exists: `scripts/compare-proof-ingestion-bench.py`.

DA and certificates:

- `DaSamplingReceipt`, zone-blob DA, separate DA accounting, and sampling verification paths are documented as implemented.
- Owned-object countersign RPC, `CertificatePool`, and certificate batch root are documented as implemented.

Masterchain and finality:

- Masterchain anchor/finality architecture is present.
- Dedicated masterchain + shards runner exists.
- Light-client crate and tests are listed as verified commands.
- Masterchain crate tests are listed as verified commands.

Explorer, indexer, wallet, status:

- Static FractalScan explorer exists under `tools/explorer`.
- Indexer with GraphQL/explorer API tests exists.
- Wallet web tool and docs exist.
- Local status page exists under `tools/status`.
- Faucet and provider HTTP sample exist.

CI/testnet gate scaffolding:

- `.github/workflows` installed.
- CI policy documented.
- Pilot smoke CI and masterchain + pilot CI are documented as installed.
- Testnet release gates T0-T4 are documented.
- Panic-boundary audit exists.
- Fuzz targets exist for transaction decoding, proof envelopes, DA share handling, peer messages, payload roots, DA commitment roots, and related inputs.

### 4.2 Partially Done / Scaffolded

Release gates:

- T0 harness items are checked, but T0 exit sign-off still requires CI green and nightly jobs scheduled/confirmed.
- T1 hardening has panic-boundary audit, converted reachable panics, fuzz target scaffolding, and regression tests, but still needs timed fuzz corpus run and seed retention metadata.
- T2 is not signed off. Local validation found BFT-7 shard progress and M10 proof-worker blockers.
- T3 and T4 remain future gates.

BFT validation:

- Deterministic BFT-7 unit coverage passed for proposer rotation, vote pooling, and timeout-certificate formation.
- Multi-process BFT-7 local smoke stalled at height 1 in the June 14 report.
- Remediation PRD has diagnostics work partly complete: connected validator count, current view, current leader, height-2 votes, QC status/reason, logs, genesis hash, and validator-set hash diagnostics.
- Actual root cause and fix for height-1 stall are not signed off in the report.

Proof finality/M10:

- Two-shard smoke reached block production but `fractal_proofMetrics.proofsAccepted` stayed `0x0` after 61 blocks in the June 14 report.
- Remediation notes say this may be scope/wiring rather than a protocol bug if no proof worker/native circuit is enabled.
- M10 remains incomplete until proof criterion is either satisfied or formally split/deferred.

Load and p99:

- Sustained-load p50/p95/p99 finality was not measured in the T2 report.
- 900 ms p99 target is not confirmed.
- Local node availability under load was not proven in that session.

Proof-ingestion hardening:

- Workstream S is 3/5 done in `docs/master-prd.md`.
- Task #25 property/fuzz tests are nearly done but need final status, nightly/CI wiring, and run documentation.
- Task #27 DA sampling policy centralization is open.

Docs:

- `docs/prd.md` still contains broad planned/TBD language and needs an implementation-note sweep.
- Operator runbook consolidation is deferred until scripts stabilize.

### 4.3 Not Done / Remaining

Testnet release:

- T0 exit sign-off.
- T1 timed fuzz run with retained corpus metadata and clean sign-off.
- T2 deterministic BFT-7 torture with partitions/view changes.
- T2 sustained lab benchmark on target hardware.
- T2 published p50/p95/p99 finality and p99 <= 900 ms confirmation.
- T2 partition recovery safety confirmation.
- M10: two shards finalize independently through RPC gateway with no proof-worker latency regression.
- T3 tagged release artifact, checksums, build provenance, operator join flow, join smoke, M11 benchmark, closed testnet runbook.
- T4 external audit or signed release exception, critical/high remediation, public RPC approval, and validator registration approval.

Protocol hardening:

- Resolve BFT-7 height-1 stall root cause and prove sustained commits.
- Replace `nohup` lab supervision with real process supervision or deterministic test supervisor.
- Decide proof-finality scope for T2/M10; either run proof worker and accept proofs or formally split soft finality from proof finality.
- Centralize DA sampling policy into a single `DaSamplingPolicy` source of truth.
- Wire property/fuzz tests into nightly/CI and record run instructions.

Evaluation:

- Run full H1 baseline vs H2 proof-ingestion benchmarks across equivalent scenarios.
- Generate and review H3 comparison reports.
- Record verdict on throughput, latency, validator CPU, payload bytes, DA sampling cost, shared-state correctness, and decentralization.

Production proof system:

- Do not claim production STWO proof acceptance until concrete verifier wiring is complete.
- Dev-digest benchmark mode must remain gated and fail closed outside local benchmarking.

Public operations:

- External security audit and bug bounty.
- Docker compose 7-validator and 21-validator profiles if needed for repeatable soak.
- BFT-7/BFT-21 long-running stability runs.
- RISC-V CI smoke if toolchain/dependencies stabilize.
- Public status page only after local status tools and ops flow stabilize.

---

## 5. Integration PRD: FractalWork -> FractalChain2

### 5.1 Done

- FractalWork deterministic receipts and batch roots exist.
- FractalWork blockchain integration PRD defines settlement batches, submission, confirmations, dashboard status, and config.
- FractalChain2 exposes EVM-compatible RPC and native Fractal extensions.
- FractalChain2 has a local EVM shard target with `CHAIN_ID=41` documented in FractalWork runbook.
- FractalWork runbook documents graph task payouts using native tFRAC through `eth_sendRawTransaction`.
- FractalWork runbook documents user-number NFT local-chain minting against Fractal EVM shard.
- FractalChain2 has contracts examples and deployment scripts.

### 5.2 Partially Done

- FractalWork can produce deterministic receipts/batches; Chain2 can accept transactions/proof updates locally; the full receipt batch settlement loop needs a signed-off integration test.
- Settlement worker config exists; production worker behavior against Chain2 needs end-to-end proof.
- Dashboard can show settlement status conceptually; confirmed chain references for receipts/batches need full integration validation.
- Wallet permission model is specified and tested in parts; runtime gating across every tool/agent/settlement path needs audit.

### 5.3 Not Done

- End-to-end demo: FractalWork job -> verification -> receipt -> batch -> Chain2 transaction -> confirmation -> dashboard link.
- Contract deployment/runbook for BatchSettlement or successor contracts on Chain2 devnet/testnet.
- Reconciliation that verifies emitted chain events against local batch payloads.
- Chain-backed reputation checkpoint and reward release flow.
- Chain-backed Agent Olympics receipt settlement.
- Chain-backed Graph task payout/review/release policy.
- Light-client or proof-backed dashboard verification for settlement references.

---

## 6. Non-Goals For Current Phase

- Do not store raw private chats, private repository contents, hidden prompts, wallet secrets, or full graph content on-chain.
- Do not move FractalWork deterministic business logic into smart contracts.
- Do not make FractalChain2 depend on a trusted single sequencer for proof-covered updates.
- Do not remove the legacy full-transaction block path from Chain2 while proof ingestion is still being evaluated.
- Do not open public RPC or public validator registration before T4 or signed release exception.
- Do not claim medical, wet-lab, clinical, dosing, or safety conclusions from BioLatent computational benchmark slices.

---

## 7. Highest-Priority Next Work

### P0 - Integration Proof

Build and document one complete local flow:

```text
FractalWork seed job
-> verifier approves
-> receipt finalizes
-> deterministic batch created
-> settlement tx submitted to FractalChain2 local EVM shard
-> confirmation reconciled
-> dashboard shows tx hash, chain id, batch root, receipt link
```

Acceptance:

- Repeat run is idempotent.
- Event payloads are deterministic.
- Failed RPC submission is retryable.
- Chain event mismatch fails closed.

### P0 - Chain T2 Remediation

Complete the T2 remediation dependency order:

1. Resolve BFT-7 height-1 stall.
2. Replace temporary process supervision with a real supervised lab harness.
3. Decide proof-finality scope for M10.
4. Run sustained load and publish p50/p95/p99.
5. Prove partition recovery safety.

### P1 - Proof-Ingestion Hardening

- Close task #25 by wiring property/fuzz tests into scheduled CI/nightly and documenting commands.
- Close task #27 by implementing centralized `DaSamplingPolicy`.
- Run H1/H2/H3 benchmark evaluation and write the verdict.

### P1 - FractalWork Production Slice

- Pick one product path as the first production slice: repository graph task payout, Agent Olympics receipt settlement, or core job receipt settlement.
- Harden only that path end to end before expanding marketplace/router/graph scope.
- Add deployment runbook, secrets checklist, backup/replay procedure, and incident stop conditions.

### P2 - Marketplace / Router / Graph OS

- Implement production graph DB schema and task DAG persistence.
- Add router filter/ranker API with route explanations.
- Add marketplace listing/eval card/meters/payout records.
- Add fraud/reputation analytics after the first settlement slice is reliable.

---

## 8. Release Gates

### FractalWork MVP Gate

- `npm test` passes.
- `npm run build` passes.
- API boots in memory and Postgres modes.
- Dashboard build passes.
- Seeded job cycle completes and displays in dashboard.
- Settlement slice passes against local FractalChain2.
- Replay root and ledger conservation checks pass.
- Secrets are not exposed to browser clients.
- Critical runbook alerts have a documented halt/recover path.

### Agent Olympics Gate

- `npm run test:olympics` passes.
- API persistence and projections exist for Olympics events.
- Dashboard surfaces exist for season, arena, match, receipt, reconciliation, and pause/recovery operations.
- Reconciliation reports `fc.ok=true`, `ce.ok=true`, `rep.ok=true`, and `closedEconomy.ok=true`.
- Adapter conformance flow works from CLI through API.
- Launch acceptance docs are complete.

### FractalChain2 Closed Testnet Gate

- T0 signed off.
- T1 signed off.
- T2 signed off.
- T3 signed off.
- External audit may run in parallel and does not block closed/invite-only testnet.

### FractalChain2 Public Gate

- T4 signed off.
- External audit complete or signed release exception recorded.
- Critical/high audit findings fixed or explicitly accepted.
- Public RPC and validator registration approved.

---

## 9. Source Documents Used

FractalWork:

- `/Users/jamesstar/fractalwork/README.md`
- `/Users/jamesstar/fractalwork/docs/runbook.md`
- `/Users/jamesstar/fractalwork/docs/prd-blockchain-integration.md`
- `/Users/jamesstar/fractalwork/docs/prd-repository-feature-graph.md`
- `/Users/jamesstar/fractalwork/docs/plans/2026-06-19-fractal-society-master-prd.md`
- `/Users/jamesstar/fractalwork/packages/olympics-core/README.md`
- `/Users/jamesstar/fractalwork/docs/biolatent/README.md`
- `/Users/jamesstar/fractalwork/docs/biolatent/release-notes.md`

FractalChain2:

- `README.md`
- `docs/prd.md`
- `docs/master-prd.md`
- `docs/remaining-work.md`
- `docs/testnet-release-gates-prd.md`
- `docs/t2-validation-report.md`
- `docs/t2-remediation-prd.md`
- Repository file map under `crates/`, `tools/`, `scripts/`, `contracts/`, and `packages/`.

---

## 10. Bottom Line

FractalWork has moved from concept to a multi-slice MVP scaffold with deterministic core logic, API, persistence, dashboards, Forge, Olympics, and research receipts. It is not yet production-complete because the graph, router, marketplace, wallet permissions, settlement, and deployment paths need one hardened end-to-end slice.

FractalChain2 has moved from chain concept to a runnable local/dev chain with proof-ingestion architecture mostly implemented. It is not yet release-complete because BFT-7/T2 validation, proof-finality scope, fuzz/sign-off, benchmark verdicts, and audit gates remain.

The next best move is not more broad scaffolding. It is one signed-off integration: FractalWork receipt settlement on FractalChain2, plus Chain2 T2 remediation.
