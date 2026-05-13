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
6. **W6** — Reference wallet CLI + web stub, provider SDK surfaces, indexer hooks, native opcode wiring beyond anchor hash.
   - **W6-a** — `fractal-wallet-cli` + §15.2 builtins in `fractal-wallet` — done.
   - **W6-b** — `tools/wallet-web/` + `policy dump-builtins` + `serve-wallet-web.sh` — done (Executor 2026-05-12).
   - **W6-c** — `fractal_sdk::provider` + `packages/fractal-provider-ts/` + `provider_id_from_public_key` — done (Executor 2026-05-12).
   - **W6-d** (next / backlog) — Native opcode wiring beyond `wallet_anchor`, full indexer daemon, provider HTTP server sample.

## Project Status Board

- [x] Save PRD to `docs/prd.md`
- [x] M1-a: Workspace + crate skeleton + `node` binary stub
- [x] M1-b–d: `crypto` + `core` state machine + 10k-tx determinism test (this batch)
- [x] Wallet W1–W5: `crates/wallet` — 12 `cargo test -p fractal-wallet` tests; `fractal-core` optional `--features wallet` anchor
- [x] M2: PRD §18 — `consensus` + `mempool` + `rpc` + `network` (libp2p 0.56 QUIC + `/fractalchain/sync/1.0.0` req-resp) + `node` (producer + follower `FRACTAL_BOOTSTRAP`, `apply_synced_block` replay verify); integration test `crates/node/tests/quic_sync.rs`
- [x] M3 (initial slice): native opcodes + subtrie `State`, intrinsic gas, Merkle settle/claim, `m3_settle_claim` test, `fractal-evm` precompile scaffold; see Current Status for gaps vs full PRD §18 M3.
- [ ] M4-a (ready for Planner sign-off): add `TxBody::EvmCall` + `apply_block_with_evm` (core `EvmEngine` trait) + `fractal-evm` `revm` dependency + `RevmEngine` stub that routes `0xfc..` precompile calls into `State::apply_native_syscall`; add test `crates/evm/tests/m4_revm_precompile_dispatch.rs`. Manual `cargo test -q` on user machine: ✅.
- [x] M6-a (Executor slice, 2026-05-12): **PRD M6** — CORS; `fractal-faucet` + `DEVNET_FAUCET_TREASURY`; initial `tools/explorer`; `testnets/devnet` compose + `bootnodes.example.txt`; `docs/devnet.md`.
- [x] M6-b (Executor slice, 2026-05-12): explorer **blocks table**, **account** + **tx** JSON lookup; **`scripts/serve-explorer.sh`**. **Planner:** manual browser pass vs live node.
- [x] M6-c (Executor slice, 2026-05-12): explorer — **click block row** → tx hash list + jump to tx lookup; account shows **`eth_getCode`** (byte length / contract flag). **Planner:** browser smoke.
- [x] W6-a: `fractal-wallet-cli` + §15.2 builtins + `crates/cli/tests/wallet_cli.rs`
- [x] W6-b (Executor slice, 2026-05-12): **`tools/wallet-web/`** static reference UI + **`policy dump-builtins`** + **`scripts/serve-wallet-web.sh`** + `builtins.json` sync test. **Planner:** browser open stub vs `README`.
- [x] W6-c (Executor slice, 2026-05-12): **`fractal_sdk::provider`** (`crates/sdk-rust`) re-exports `fractal_wallet` market types + **`IndexerCursor`** / **`IntentPollFilter`**; **`packages/fractal-provider-ts/`** wire types (`npm run check`); **`provider_id_from_public_key`** on `fractal-wallet`; tests `crates/sdk-rust/tests/provider_sdk.rs`. **Planner:** `cargo test -p fractal-sdk`.
- [ ] W6-d (backlog): wallet native opcodes beyond anchor; long-running indexer binary; sample provider HTTP server.
- [ ] M6-d (next): Blockscout-style explorer / richer block UI (internal vs Ethereum tx hash docs), or ≥3 real bootnodes + status page.
- [ ] M5-a (Executor slice, 2026-05-12): **PRD `docs/prd.md` M5** (not `docs/wallet.md`) — `fractal-mvp-bridge` + `fractal_sdk::m5` + devnet Hardhat #1; smoke OK. **Next:** wire real off-chain receipts, post-bridge `eth_getBalance` check, CI `MVP_RECEIPT_COUNT=100` vs `fractal-node`.
- [ ] M4-b (ready for Planner sign-off): expand JSON-RPC toward MetaMask/ethers compatibility: add `eth_chainId`, `net_version`, `eth_getTransactionCount`. User confirmed via manual run.
- [ ] M4-c (in progress): expand JSON-RPC: add `web3_clientVersion`, `eth_getBlockByNumber`, `eth_getBlockByHash` (minimal block objects).
- [ ] M4-d (in progress): expand JSON-RPC: add `eth_getTransactionByHash`, `eth_getTransactionReceipt` with basic tx/receipt tracking (pending + mined).
- [ ] M4-e (in progress): expand JSON-RPC: implement `eth_call` + `eth_estimateGas` (devnet semantics; simulates via cloned state; supports `0xfc..` precompile calls deterministically).
- [ ] M4-f (in progress): expand JSON-RPC: add `eth_getLogs` (stub empty array for now) to satisfy MetaMask/ethers polling.
- [ ] M4-g (in progress): expand JSON-RPC: add `eth_syncing` (false), `eth_getCode` (empty), `eth_getStorageAt` (zero slot) stubs for MetaMask/ethers probing.
- [ ] M4-h (in progress): expand JSON-RPC: add fee APIs `eth_maxPriorityFeePerGas` and `eth_feeHistory` (devnet consistent stubs).
- [ ] M4-i (in progress): expand JSON-RPC: add block→tx lookup APIs `eth_getBlockTransactionCountByNumber`, `eth_getBlockTransactionCountByHash`, `eth_getTransactionByBlockHashAndIndex`.
- [ ] M4-j (in progress): expand JSON-RPC: add `eth_getTransactionByBlockNumberAndIndex`.
- [ ] M4-k (in progress): accept real Ethereum `eth_sendRawTransaction` EIP-1559 (type `0x02`) tx bytes: RLP decode + secp256k1 sender recovery + map into internal `Transaction`; add `crates/node/tests/eip1559_raw_tx.rs`.
- [ ] M4-l (in progress): support EIP-1559 contract creation (`to = ""`): map to `TxBody::EvmCreate`, store devnet code in `State.evm_code`, expose via `eth_getCode`, and return `contractAddress` in receipts.
- [ ] M4-m (in progress): real EVM CALL execution (devnet): add `State.evm_storage`, implement `revm` DB/commit bridge in `fractal-evm`, and wire `eth_getStorageAt` to return real storage values.
- [ ] M4-n (in progress): receipts gasUsed: record per-tx EVM gas used deterministically (`State.evm_tx_gas_used`) and expose via `eth_getTransactionReceipt.gasUsed`.
- [ ] M4-o (in progress): contract CALL correctness: add a bytecode execution test proving CALL can SSTORE + RETURN (foundation for `eth_call` on contract bytecode).
- [ ] M4-p (in progress): EVM logs/events: capture logs from `revm` execution, store deterministically per-tx, `eth_getLogs` (minimal filters), **`eth_getTransactionReceipt.logs`** with `blockHash` + block-scoped `logIndex`, shared `make_rpc_log`.
- [x] M4-q (Executor slice, 2026-05-12): PRD M4 **example contract + Solidity dev docs** — `contracts/examples/*`, `docs/solidity-dev.md`, borsh snapshot test for `NativeCall::NoOp`.
- [x] M4-r (Executor slice, 2026-05-12): in-repo **`contracts/` Hardhat** + **`scripts/deploy-fractal-contracts.sh`**; producer handles `PooledTx` + RPC tx hash / `eth_getTransactionByHash` EIP-1559 JSON (`r`/`s`/`yParity`) for ethers/Hardhat. **Planner:** confirm end-to-end deploy vs M4 exit criteria.
- [x] M4-s (Executor slice, 2026-05-12): **`contracts/scripts/deploy.js`** exercises **`openBounty` + `pingNativeNoOp`** (asserts `NativeCallResult(true)`); **`m4_contract_calls_precompile`** EVM test; fixed **double caller nonce bump** for top-level `EvmCall` through revm (`crates/evm/src/engine.rs`). **Planner:** confirm M4 exit line vs MetaMask balance display.

## Current Status / Progress Tracking

- PRD saved 2026-05-11.
- Implemented initial Rust workspace with `fractal-core` determinism integration test (10_000 txs × 10 runs).
- Local `cargo test` (user machine): all crates green; `ten_k_txs_state_root_is_identical_across_ten_runs` passed. Removed unused `Sim::idx` in determinism test to clear `dead_code` warning. Awaiting Planner sign-off on M1.
- **Wallet:** `fractal-wallet` implements W1–W5 (12 unit tests). `post_receipt` now takes `now_ms` and sets challenge deadlines (`DEFAULT_OPTIMISTIC_CHALLENGE_MS`). Use `cargo test -p fractal-core --features wallet` for `wallet_anchor` test.
- **Wallet:** `settle_trusted` remains as thin wrapper over `settle_after_window` using stored deadline (Trusted tier).
- **Chain M3 (2026-05-11, started):** `fractal-core` expanded `NativeCall` (13 PRD opcodes + `NoOp`), `State` subtries (`agents`, `receipts`, `batches`, `disputes`, `stakes`, `delegated`, …), `merkle.rs`, `native_gas.rs` / `intrinsic_gas`, `apply_block` returns total gas; `fractal-consensus` pre-checks `gas_limit` (`GasLimitExceeded`); `fractal-mempool` `drain_ready_gas_budget`; `fractal-evm` `precompile.rs` (`0xfc` prefix + borsh decode). Test `crates/core/tests/m3_settle_claim.rs` covers PRD M3 exit line (100 receipts + 100 Merkle claims). Still out of scope in this slice: `native_event_root` in block header, revm wiring, rich Solidity ABI, full stake/unbond/rewards economics.
- **Chain M4 (2026-05-12, started):** introduced core `EvmEngine` trait + `apply_block_with_evm`; added `TxBody::EvmCall` and `State::apply_native_syscall` (no nonce bump) for EVM→native bridging; `fractal-evm` now depends on `revm` and provides `RevmEngine` (initial stub: routes only `0xfc..` addresses). Workspace `cargo test` passed; awaiting manual validation before checking off M4-a.
- **Chain M4 (2026-05-12):** `eth_getTransactionReceipt` returns populated `logs` (same object shape as `eth_getLogs`); `RpcLog` includes `blockHash`; `logIndex` is block-scoped in both receipt and `eth_getLogs`. `cargo test -q`: ✅.
- **Chain M4 (2026-05-12, Executor):** `EvmCreate` runs init code through revm (`TxKind::Create`); runtime is deployed bytecode (RETURN data), `evm_tx_gas_used` / logs recorded, deployer nonce updated by revm (no extra `bump_nonce`). `create_contract_address` in `fractal-core`; `ExecError::EvmFailed`; CALL path commits only on `is_success` and sets `tx.nonce`. Tests: `crates/consensus/tests/m4_create_init_code.rs`. `cargo test -q`: ✅.
- **Chain M4 (2026-05-12, Executor):** Ethereum `logsBloom` on receipts + merged block bloom (`fractal-rpc`). `cargo test -q`: ✅.
- **Chain M4 (2026-05-12, Executor):** Receipt `status` (`0x1`/`0x0`) from `State.evm_tx_success` + `evm_receipt_success` default; `ExecError::EvmRevert` from revm `Revert`; `eth_call` / `eth_estimateGas` return JSON-RPC code `3` + hex `data` on revert; `eth_estimateGas` uses simulated `gas_used`. Tests: `m4_evm_revert.rs`, `m4_evm_receipt_success.rs`. `cargo test -q`: ✅.
- **Chain M4 (2026-05-12, Executor / PRD M4 deliverables):** Added `contracts/examples/FractalNative.sol`, `contracts/examples/AgentBountyEscrow.sol`, `docs/solidity-dev.md` (precompile addressing, NoOp borsh `0x0d`, Hardhat/MetaMask notes); `crates/core/tests/native_call_borsh_snapshots.rs` locks `NoOp` wire format. PRD §18 M4 bullets updated to reference paths. `cargo test -q`: ✅.
- **Chain M4 (2026-05-12, Executor):** `eth_getTransactionByHash` returns full EIP-1559 JSON from stored raw bytes when present; `eth_internal_to_rpc_tx_hash` so `eth_getLogs` uses the Ethereum tx hash in `transactionHash`; receipt logs already keyed via `internal_tx_hash_for_state`. Hardhat section added to `docs/solidity-dev.md`.
- **Chain M4 (2026-05-12, Executor):** Deploy script validates precompile-from-contract; `fractal-evm` resets caller nonce after revm `transact` for top-level `CALL` so `State::bump_nonce` remains the single increment. Full `cargo test`: ✅.

- **Chain M5 (2026-05-12, Executor / PRD `docs/prd.md` only):** `fractal_core::HARDHAT_DEFAULT_SIGNER_1` + devnet account row; `fractal_sdk::m5` builders; binary `fractal-mvp-bridge` (`crates/mvp-backend`) posts borsh `SETTLE_BATCH` + `CLAIM_PAYOUT` via `eth_sendRawTransaction`; PRD M5 deliverables bullet updated. Smoke: `MVP_RECEIPT_COUNT=3` vs local node ✅.

- **Chain M6 (2026-05-12, Executor / PRD `docs/prd.md`):** CORS + `fractal-faucet` + `testnets/devnet` compose + `bootnodes.example.txt` + `docs/devnet.md`; explorer v2 (chain summary, **10-block** table, **account** + **tx** JSON) + **`scripts/serve-explorer.sh`**.
- **Chain M6-c (2026-05-12, Executor):** Explorer block row → **block detail** (`eth_getBlockByNumber` metadata + tx hash buttons); tx buttons fill hash + `lookupTx`; account lookup adds **`eth_getCode`** (`codeByteLength`, `isContract`).

- **Wallet W6-c (2026-05-12, Executor):** `fractal_sdk::provider` + TS package `packages/fractal-provider-ts/`; `market::provider_id_from_public_key`; `.gitignore` for `packages/**/node_modules` + `dist/`; `docs/wallet.md` §21.2 + §25.1 paths.

- **Wallet W6-a:** `fractal-wallet-cli` + §15.2 builtins in `fractal-wallet` (see `crates/cli`, `crates/wallet/src/policy.rs`).
- **Wallet W6-b (2026-05-12, Executor):** `tools/wallet-web/` loads committed `builtins.json`; `fractal-wallet-cli policy dump-builtins` regenerates it; `wallet_web_builtins_json_matches_cli_dump` test locks sync; `./scripts/serve-wallet-web.sh` (port `WALLET_WEB_PORT`, default 3344). `docs/wallet.md` §25.1 bullet updated.

- **Chain M2:** remains as delivered earlier (QUIC sync, follower, `quic_sync` test).

## Executor's Feedback or Assistance Requests

- **BLS**: `fractal-crypto::bls` is a type-safe placeholder until M7 wiring; avoids `blst` native build in early CI.
- **M4 slice (receipt status / revert RPC):** Implementation complete; `cargo test -q` passed locally. **Planner:** please cross-check behavior vs MetaMask/ethers expectations (error `data` as plain hex string) and confirm whether to mark the corresponding status-board item done.
- **M4 Hardhat + deploy script:** Landed `contracts/` Hardhat package and `./scripts/deploy-fractal-contracts.sh`; fixed ethers `eth_getTransactionByHash` shape for EIP-1559. **Planner:** please run `./scripts/deploy-fractal-contracts.sh` with a live `fractal-node` and confirm M4-q/M4-r/M4-s sign-off.
- **M6 slice (PRD `docs/prd.md`):** Devnet pack (faucet, compose, CORS) + **explorer v2** (blocks/account/tx) + `serve-explorer.sh`. **M6-c:** block drill-down + `eth_getCode` on account. **Planner:** browser smoke on `tools/explorer` + faucet; Docker compose optional; Discord/status remain ops-owned.
- **M5 bridge (PRD `docs/prd.md`, not `docs/wallet.md`):** First slice landed (`fractal-mvp-bridge`, `fractal_sdk::m5`). **Planner:** run `MVP_RECEIPT_COUNT=100` against devnet and optionally `eth_getBalance` on agent to confirm M5 exit path numerically.
- **Next PRD milestone after M4 closure:** **M5** (Bridge to Core MVP) — off-chain backend submits `SETTLE_BATCH`, SDK claims, E2E receipts → settlement → MetaMask tFRAC. **Remaining M4 polish (Executor backlog):** MetaMask “tFRAC balance” doc / import for devnet prefund key; follower replication of `eth_signed_raw` / RPC hash maps if explorers must match leader on synced nodes.
- **Next chain milestone:** PRD §18 **M4** — `revm`, full JSON-RPC EVM surface, MetaMask path, real precompile dispatch from EVM execution.
- **Wallet W6**: **W6-a–c** landed (CLI, web stub, provider SDK types + `fractal_sdk::provider`). **W6-d** backlog: native opcodes beyond anchor, indexer service, sample provider HTTP.
- **Wallet W6-a (Executor, 2026-05-12):** First W6 slice — built-in §15.2 policy templates + `fractal-wallet-cli`. **Planner:** run `cargo test -p fractal-cli -p fractal-wallet` if not already signed off.

## Lessons

- Read `Cargo.toml` before editing workspace members; hyphenated crate dirs map to underscored package names in Rust 2021.
- Agent sandbox may lack `cargo`; user-run `cargo test` is the source of truth for compile/test until CI exists.
- **Borsh 1.5**: enums with `#[repr(u8)]` explicit discriminants require `#[borsh(use_discriminant = true)]` (or `false`) on the enum.
- `BTreeSet<(ToolClass, TeeType)>` requires `TeeType: Ord` (derive `PartialOrd, Ord` on field-less enums).
- **jsonrpsee 0.24:** there is no `http-abi` feature; use `features = ["server", "macros"]` in workspace `Cargo.toml`.
- **Producer vs RPC:** `producer_loop` must hold `Arc<Mutex<NodeInner>>` so it can read `mempool` / `state`; RPC uses `CoerceUnsized` to `SharedChain` from the same `Arc`. Do not type the producer as `SharedChain` (`dyn` loses fields).
- **libp2p request-response:** overlapping `GetTip`/`GetBlocks` requests can deliver responses out of order and break followers (e.g. duplicate height-1 apply). Serialize with an `outstanding` flag or single-pending RPC.
- **`NativeCall::try_from_slice` outside `fractal-core`:** import `borsh::BorshDeserialize` (methods are on the trait).
- **jsonrpsee / MetaMask:** `eth_call` revert uses RPC error code `3` and `data` as a JSON string value holding `0x` + hex return data (common wallet pattern).
- **Revm + `State::bump_nonce`:** For top-level `EvmCall` into contract bytecode, revm’s `transact` commits `caller_nonce + 1`. Core still calls `bump_nonce` after `execute_call`; reset the caller’s nonce to the pre-`transact` value in `RevmEngine::execute_call` after `db.commit` so the net increment is exactly one. Precompile fast path does not use revm and still relies on `bump_nonce` only.
- **jsonrpsee 0.24 + `tower-http` CORS:** `ServerBuilder::set_http_middleware` expects **`tower` 0.4** `ServiceBuilder` (jsonrpsee’s dependency). Using workspace `tower` 0.5 causes a type mismatch.
- **`borsh::to_vec` (1.5):** free function with `T: BorshSerialize` bound on the value; the trait does **not** need to be in scope at the call site. Only `BorshDeserialize` must be imported (it provides `try_from_slice`).
- **`PolicyTemplate` has no `allowed_classes` field:** §15.2's "allowed: X, Y" list is realized at capability-mint time as `Scope.tool_class_mask`, not as a caveat. Use `policy_builtins::suggested_tool_class_mask(template_id)` to materialize the mask without a schema change.
- **`fractal_sdk::provider`:** depends on `fractal-wallet`; re-exports market + `provider_id_from_public_key` for provider operators (`crates/sdk-rust/tests/provider_sdk.rs`).
