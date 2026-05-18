# FractalChain — Remaining Work

Living backlog derived from `docs/prd.md`, the current codebase, and devnet docs.

**Last cleaned:** 2026-05-17. This file tracks remaining work only. Completed protocol slices that are already covered by tests/docs are intentionally omitted.

---

## Next Protocol / Product Coding

These are the remaining implementation gaps that still change protocol or product behavior.

_(None open at last review.)_

---

## Tests / Observability

These are engineering quality gaps that should be done, but they do not define new protocol behavior.

- [ ] **Masterchain RPC integration test** — restore or replace the removed HTTP `fractal_submitShardAnchor` e2e test. Prefer a bounded jsonrpsee server test with explicit shutdown so it cannot hang.

---

## CI, Benchmarks, And Ops Gates

These are important, but they are validation/operations work rather than protocol coding.

- [ ] **Install CI workflows** — `docs/ci/mvp-bridge-smoke.workflow.yml` exists but `.github/workflows/` is not present. Add the workflow and decide which long-running jobs are nightly vs PR-gated.

- [ ] **Pilot smoke in CI** — run `./scripts/run-pilot-shards.sh smoke-start` in CI with bounded runtime and log capture.

- [ ] **Masterchain + pilot CI** — run `./scripts/run-pilot-shards.sh start-with-masterchain` plus a focused anchor/proof smoke.

- [ ] **HyperBFT lab load sign-off** — deterministic BFT-7 torture exists. Still need lab-hardware load with partitions/view-change under sustained traffic and measured p99 finality ≤ 900 ms.

- [ ] **M10 exit sign-off** — two shards finalize independently under sustained load, through the RPC gateway, with no proof-worker latency regression.

- [ ] **M11 exit benchmark** — after proof-chain fast sync exists, compare new-node sync from pruned proof chain vs full replay at a realistic large height. Keep “1M blocks” as the target scale, not as a coding task by itself.

- [ ] **Docker compose validator profiles** — current compose is producer/follower devnet. Add documented 7-validator and 21-validator profiles only if they will be used for repeatable soak runs.

- [ ] **BFT-7 / BFT-21 soak** — long-running 7-validator and 21-validator stability runs are release gates, not normal development tasks. Track results outside this backlog once automation exists.

- [ ] **riscv64 CI smoke** — cross-target build/test for `riscv64gc-unknown-linux-gnu` once the toolchain/dependency setup is stable enough for CI.

- [ ] **External security audit / bug bounty** — keep as a pre-testnet/mainnet gate after core protocol surfaces stop changing rapidly.

---

## Docs Cleanup

- [ ] **PRD implementation-note sweep** — `docs/prd.md` has been updated for recent prover market, slashing, snapshot v2, and masterchain BFT slices, but still contains broad planned/TBD language. Keep architecture language, and update only statements that conflict with code that is already implemented and tested.

- [ ] **Operator runbook consolidation** — `docs/devnet.md` is the source of truth today. Split long-running ops, CI, and economics/prover-market knobs into smaller runbooks only after the scripts stabilize.

---

## Deferred / Not Next

These items are real product questions, but they should not block the next engineering passes.

- **Chain ID registration** — testnet uses proposed `41`; external registration is a launch/admin task.
- **Shard assignment policy** — current code uses deterministic home-shard routing. Explicit registry-based `home_shard` is a product/governance choice for later.
- **Masterchain validator set policy** — code supports singleton, BFT-7, and BFT-21 fixtures. “Union of shard validators vs dedicated committee” is a governance/topology decision, not a missing primitive.
- **Anchor priority fee curve** — useful once priority anchoring is productized; not needed for the current fixed-interval pilot.
- **Proof artifact gossip** — optional. RPC submission, persisted proof records, masterchain block sync, and light-client verification exist. Add artifact gossip only if prover/operator UX requires decentralized proof blob propagation.
- **Full public status page** — `tools/status/` and `./scripts/serve-status.sh` cover local liveness. A hosted status page is operational work.
- **Trustless LLM-output verification** — still research/product territory; current wallet design correctly treats it as challenge/attestation/reputation based rather than a near-term ZK task.

---

## Verified Commands

| Goal | Command |
|------|---------|
| Track B lab | `./scripts/run-track-b-lab.sh` + `./scripts/smoke-track-b-e2e.sh` |
| Two shards | `./scripts/run-pilot-shards.sh smoke-start` |
| Shard RPC gateway | `./scripts/run-rpc-gateway.sh` |
| Two shards + dedicated masterchain | `./scripts/run-pilot-shards.sh start-with-masterchain` |
| Masterchain only | `./scripts/run-masterchain-bft.sh` |
| Masterchain crate | `cargo test -p fractal-masterchain` |
| Proof pipeline | `cargo test -p fractal-node --test stwo_plonky2_pipeline` |
| Light client | `cargo test -p fractal-light-client` |
| Wallet provider stake/slash | `cargo test -p fractal-core --features wallet --test w14_wallet_provider_stake` |
| Indexer smoke | `cargo test -p fractal-indexer --test graphql_smoke --test explorer_api` |
