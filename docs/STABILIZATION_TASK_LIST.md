# Fractal-node Stabilization Task List

Status legend: `[x]` implemented and locally verified · `[ ]` pending.

Verification commands (run at repo root):
```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo test -p fractal-node --test rlmf_attestation_rpc   # new suite
bash scripts/devnet-smoke.sh                             # single-command smoke
```

Verified locally after applying this patch:

- `cargo fmt --all -- --check`
- `cargo test -p fractal-node --test rlmf_attestation_rpc`
- `cargo test -p fractal-rpc`
- `cargo test -p fractal-node proof_commitment_tx`
- `bash scripts/devnet-smoke.sh`

Known verification caveat: clippy with `-D warnings` is currently blocked by pre-existing unrelated lint debt in `fractal-crypto`, `fractal-rlvr`, `fractal-rpc`, and `fractal-node`.

## Serial-required (order matters)

- [x] **S1. RLMF attestation RPC + index** — `fractal_submitRlmfAttestation`, `fractal_getRlmfAttestation`, `fractal_listRlmfAttestations`.
  - Files: `crates/rpc/src/module.rs` (record/response types, canonical-commitment + validation, trait methods with back-compat defaults, method registration), `crates/rpc/src/lib.rs` (re-exports), `crates/node/src/lib.rs` (`rlmf_attestations` index, `ChainInteraction` impl reusing the `ProofCommitmentV1` native-tx path), `crates/node/tests/rlmf_attestation_rpc.rs` (4 tests: mined-commitment round trip + index queries, idempotent resubmit + tamper rejection, RPC round trip + 6 invalid-input cases, canonical-hash determinism).
  - Design: on-chain footprint is the 32-byte canonical commitment (keccak256 over schema tag + length-prefixed fields); the full record is indexed off-chain — consistent with the existing `rlvr_chain_commit` raw-data-off-chain policy. `commitmentHash` must match the canonical commitment or the request is rejected (prevents garbage commitments). `promotionDecision ∈ {promote,reject,inconclusive,shadow,canary,rollback}`; evidence/lineage lists capped at 64; `deny_unknown_fields`.
  - Risks: persistence parity with `proof_commitments`: both are in-memory (see S3).
- [x] **S2. Devnet smoke command** — `scripts/devnet-smoke.sh`: build → start → `wait-for-jsonrpc.sh` → height check → invalid-attestation rejection check → height progression → clean exit; non-zero with log tail on failure.
- [ ] **S3. Attestation/commitment persistence across restart** — `proof_commitments` and `rlmf_attestations` are in-memory; both are reconstructible by chain replay (commitments are mined txs) but the attestation *envelopes* are not on-chain. Follow-up: persist `rlmf_attestations` in the chain snapshot (`crates/node/src/chain_snapshot.rs`) or project them in `crates/indexer`. Recommended next serial task.
- [ ] **S4. Replay determinism test** — genesis→N blocks replay asserting identical state roots and identical rebuilt attestation index (blocked on S3 decision).

## Parallel-safe

- [x] **P1. Compose hardening** — healthcheck (eth_blockNumber probe) + `fractalchain-node` network alias on the devnet node service, so ecosystem services can use `FRACTAL_CHAIN_RPC_URL=http://fractalchain-node:8545` without renaming the existing `node` service. File: `testnets/devnet/docker-compose.yml` (YAML validated).
- [x] **P2. Gitignore hardening** — local state dirs, p2p/node keys, RLVR trace logs, smoke logs. File: `.gitignore`.
- [x] **P3. Fractalwork integration doc** — `docs/fractalwork-attestation.md`: request/response schema, canonical-commitment spec, curl examples, compose wiring.
- [ ] **P4. Repo hygiene review (needs owner decision)** — committed items that look like local/generated state: `fractal_rlvr_ui_work/` (rollouts + eval reports), `m4_report.html`, `.cursor/scratchpad.md`, `docs/master jun 27th.md`. Not removed in this pass (may be intentional fixtures); decide keep-vs-purge, and if purge, also rewrite history if anything sensitive.
- [ ] **P5. RPC request-size/timeout limits** — jsonrpsee `ServerBuilder` in `crates/rpc` should set explicit `max_request_body_size` and connection limits; inventory shows none set at the `serve_http` call site. Small, isolated change.
- [ ] **P6. Prometheus additions** — `crates/node/src/metrics.rs` already exports peer count, RPC counters/latency; add `fractal_rlmf_attestation_submitted_total` and tx-pool gauges.
- [ ] **P7. Mempool restart-corruption tests** — extend existing mempool tests with a serialize→restart→state-equivalence case.
- [ ] **P8. macOS/Linux setup doc refresh** — `docs/devnet.md` exists; verify commands against a fresh checkout on both platforms (needs real machines).

## Verified by inspection this pass (no changes needed)

- RPC inventory: 22 `eth_*`, 21 `fractal_*` methods incl. `fractal_submitProofHash` / `fractal_submitProofUpdate` / `fractal_submitValidityProof`; standard JSON-RPC error objects (-32602/-32603) used consistently.
- CI: fmt + clippy `-D warnings` + workspace tests + `bash -n scripts/*.sh` already in `.github/workflows/ci.yml` — the new script and tests are covered by existing CI once merged.
- Health endpoints exist on indexer and faucet; node liveness is probed via RPC (compose healthcheck added in P1).
- No private keys/tokens found in tracked files (dev signers are the well-known Hardhat constants, correctly labeled `HARDHAT_DEFAULT_SIGNER_*`).
