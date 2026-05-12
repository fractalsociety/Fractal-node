# FractalChain — scratchpad

## Background and Motivation

FractalChain L1 testnet (PRD v0.1) is an AI-agent-first chain: HotStuff-2 consensus, hybrid Native VM + EVM (revm), unified state, FractalWork primitives as native precompiles. The PRD lives at `docs/prd.md`. Execution started with **M1: Core crate layout and determinism spike**.

**FractalWork Agent Wallet spec v2.0** lives at `docs/wallet.md` (capability tokens, budgets, tool market, receipts, staking). Phase 1 (§25.1) and §29 checklist items are being implemented in **`fractal-wallet`** in parallel with chain milestones.

## Key Challenges and Analysis

- **Determinism** is the execution layer’s highest constraint: same parent state + same ordered txs must yield identical state roots on every node.
- **M1 scope**: full Merkle Patricia Trie (EVM-compatible) is deferred to `storage`; M1 uses a **canonical commitment** over structured state (BTreeMap + borsh + keccak) so the determinism harness is meaningful before MPT lands.
- **Wallet spec**: Revocation proofs use a **sorted-leaf Merkle tree** (BLAKE3) in Phase 1 code; full sparse Merkle trie per §4.6 can replace `merkle` internals later without changing `RevocationSet` API surface much.

## High-level Task Breakdown

1. **M1-a**: Cargo workspace + crate skeleton per PRD §19 — success: `cargo build -p fractal-core` succeeds.
2. **M1-b**: `crypto` primitives (hash, canonical encoding; Ed25519 verify/sign helpers; BLS placeholder) — success: unit tests for hash/encode round-trip.
3. **M1-c**: `core` mocked native opcodes + `apply_block` — success: processes 10k txs.
4. **M1-d**: Determinism harness — success: 10 runs, identical `state_root` — **Planner must confirm M1 complete after review.**

### Wallet spec (docs/wallet.md) — execution slices

1. **W1** — `fractal-wallet`: capability token (first 6 caveats), sign/verify, attenuation vs parent — done + tests.
2. **W2** — Budget RESERVE/SETTLE/REFUND/PARTIAL + per-tool caps — done + tests.
3. **W3** — Token-bucket + revocation set + Merkle root/proof + cascade rule — done + tests.
4. **W4** — Tool intent / quote / match / receipt / trusted settle + stake lock — done + tests.
5. **W5** — Policy template registry, optimistic challenge window + dispute resolution, emergency stop registry, TaskReceipt + `tool_receipt_root`, optional `fractal-core` `wallet` feature + `wallet_anchor` — **done** (see `crates/wallet`, `crates/core`).
6. **W6** (next) — Reference wallet CLI/web stubs, provider SDK surfaces, indexer hooks, native opcode wiring beyond anchor hash.

## Project Status Board

- [x] Save PRD to `docs/prd.md`
- [x] M1-a: Workspace + crate skeleton + `node` binary stub
- [x] M1-b–d: `crypto` + `core` state machine + 10k-tx determinism test (this batch)
- [x] Wallet W1–W5: `crates/wallet` — 12 `cargo test -p fractal-wallet` tests; `fractal-core` optional `--features wallet` anchor
- [x] M2: PRD §18 — `consensus` + `mempool` + `rpc` + `network` (libp2p 0.56 QUIC + `/fractalchain/sync/1.0.0` req-resp) + `node` (producer + follower `FRACTAL_BOOTSTRAP`, `apply_synced_block` replay verify); integration test `crates/node/tests/quic_sync.rs`
- [x] M3 (initial slice): native opcodes + subtrie `State`, intrinsic gas, Merkle settle/claim, `m3_settle_claim` test, `fractal-evm` precompile scaffold; see Current Status for gaps vs full PRD §18 M3.

## Current Status / Progress Tracking

- PRD saved 2026-05-11.
- Implemented initial Rust workspace with `fractal-core` determinism integration test (10_000 txs × 10 runs).
- Local `cargo test` (user machine): all crates green; `ten_k_txs_state_root_is_identical_across_ten_runs` passed. Removed unused `Sim::idx` in determinism test to clear `dead_code` warning. Awaiting Planner sign-off on M1.
- **Wallet:** `fractal-wallet` implements W1–W5 (12 unit tests). `post_receipt` now takes `now_ms` and sets challenge deadlines (`DEFAULT_OPTIMISTIC_CHALLENGE_MS`). Use `cargo test -p fractal-core --features wallet` for `wallet_anchor` test.
- **Wallet:** `settle_trusted` remains as thin wrapper over `settle_after_window` using stored deadline (Trusted tier).
- **Chain M3 (2026-05-11, started):** `fractal-core` expanded `NativeCall` (13 PRD opcodes + `NoOp`), `State` subtries (`agents`, `receipts`, `batches`, `disputes`, `stakes`, `delegated`, …), `merkle.rs`, `native_gas.rs` / `intrinsic_gas`, `apply_block` returns total gas; `fractal-consensus` pre-checks `gas_limit` (`GasLimitExceeded`); `fractal-mempool` `drain_ready_gas_budget`; `fractal-evm` `precompile.rs` (`0xfc` prefix + borsh decode). Test `crates/core/tests/m3_settle_claim.rs` covers PRD M3 exit line (100 receipts + 100 Merkle claims). Still out of scope in this slice: `native_event_root` in block header, revm wiring, rich Solidity ABI, full stake/unbond/rewards economics.
- **Chain M2:** remains as delivered earlier (QUIC sync, follower, `quic_sync` test).

## Executor's Feedback or Assistance Requests

- **BLS**: `fractal-crypto::bls` is a type-safe placeholder until M7 wiring; avoids `blst` native build in early CI.
- **Next chain milestone:** PRD §18 **M4** — `revm`, full JSON-RPC EVM surface, MetaMask path, real precompile dispatch from EVM execution.
- **Wallet W6**: off-chain clients / SDK packaging not started; `fractal-sdk` still re-exports `fractal-core` only.

## Lessons

- Read `Cargo.toml` before editing workspace members; hyphenated crate dirs map to underscored package names in Rust 2021.
- Agent sandbox may lack `cargo`; user-run `cargo test` is the source of truth for compile/test until CI exists.
- **Borsh 1.5**: enums with `#[repr(u8)]` explicit discriminants require `#[borsh(use_discriminant = true)]` (or `false`) on the enum.
- `BTreeSet<(ToolClass, TeeType)>` requires `TeeType: Ord` (derive `PartialOrd, Ord` on field-less enums).
- **jsonrpsee 0.24:** there is no `http-abi` feature; use `features = ["server", "macros"]` in workspace `Cargo.toml`.
- **Producer vs RPC:** `producer_loop` must hold `Arc<Mutex<NodeInner>>` so it can read `mempool` / `state`; RPC uses `CoerceUnsized` to `SharedChain` from the same `Arc`. Do not type the producer as `SharedChain` (`dyn` loses fields).
- **libp2p request-response:** overlapping `GetTip`/`GetBlocks` requests can deliver responses out of order and break followers (e.g. duplicate height-1 apply). Serialize with an `outstanding` flag or single-pending RPC.
- **`NativeCall::try_from_slice` outside `fractal-core`:** import `borsh::BorshDeserialize` (methods are on the trait).
