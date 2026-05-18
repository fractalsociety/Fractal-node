# STWO (proof-condenser): how it is wired and how to verify it

This project does **not** use crates.io `stwo` 2.2.0 as-is. Upstream expects older nightly feature gates (`array_chunks`, iterator `into_remainder()` behavior, etc.) that break on current nightlies.

## Approach

1. **Vendored crate** at `third_party/stwo/` — a patched copy of STWO 2.2.0 aligned with this repo.
2. **Cargo patch** in the **workspace root** `Cargo.toml`:

   ```toml
   [patch.crates-io]
   stwo = { path = "third_party/stwo" }
   ```

   That forces `fractal-proof-condenser` and `stwo-constraint-framework` to resolve `stwo` from this path.

3. **`rust-toolchain.toml`** at the repo root pins **nightly** (STWO `prover` needs `portable_simd` and related unstable features). The exact date is chosen to stay compatible with the rest of the workspace (e.g. revm MSRV).

Integration code and tests live under `crates/proof-condenser/` (e.g. `checkpoint_stwo.rs`, `riscv_trace.rs`).

`fractal-proof-condenser` enables vendored **`stwo/parallel`** (Rayon). That is the main knob for **multi-core throughput** on servers, including **riscv64** hosts: there is no dependency on x86 SIMD for this checkpoint smoke prover (STWO’s **`CpuBackend`** path).

## Async condenser (HotStuff off-path)

- **`prove_checkpoint`**: `tokio::task::spawn_blocking` runs `prove_and_verify_checkpoint_stwo`, then returns a **blake3** digest of the serialized STARK proof (or falls back to `CheckpointJob::stwo_commitment_stub` on error). `CheckpointJob` now carries `[start_block..end_block]`; single-block checkpoints are represented as `[height..height]`, and `checkpoint_job_from_block_range` builds range-bound witnesses over contiguous finalized blocks.
- **`spawn_async_proof_condenser(rx, registry)`**: background loop; optional **`ProofArtifactRegistry`** with **`ProofPersistenceConfig`** (RocksDB path and/or filesystem dir).
- **RISC-V replay trace:** `checkpoint_job_from_block_range` now runs the deterministic trace harness in `crates/proof-condenser/src/riscv_trace.rs`. The harness validates contiguous block heights, parent-header links, and per-block transaction roots, emits canonical `BeginBlock` / `ApplyTx` / `EndBlock` rows, and stores `riscvTraceRoot` + `riscvTraceSteps` in `CheckpointJob`. STWO witness seeds mix those trace public inputs, so the proof is bound to the replay-derived trace rather than only the header hash or a padded job stub.

The current AIR remains the compact STWO checkpoint smoke circuit, not a full RISC-V CPU instruction verifier. The important production boundary now exists: finalized blocks are replayed into a versioned RISC-V trace object (`FRACRV02`) whose root is part of the STWO public statement.

## Prove, digest, verify-only, and v1 artifact (M9 hooks)

- **`prove_checkpoint_stwo(job)`** → `(serde_json proof bytes, blake3 digest)`. Proves, self-verifies, returns bytes you can store or gossip.
- **`verify_checkpoint_stwo_proof_json(json, job)`** → verifies a proof **without** re-running the prover; **Fiat–Shamir** mixes canonical **borsh(`job`)** before Merkle roots (same order as prove), so the blob is not valid for a different [`CheckpointJob`] even if the JSON digest matches.
- **`checkpoint_stwo_digest_from_json`** — stable `blake3(json)`; integrity of stored bytes (tamper detection). **Job binding** is enforced by `verify_checkpoint_stwo_proof_json(..., job)`, not by the digest alone.
- **`CheckpointStwoArtifactV1`** (`artifact.rs`) — `borsh` bundle: `version`, `CheckpointJob`, `stark_proof_json`; **`prove`** / **`verify(expected_digest)`** / **`to_bytes`** / **`from_bytes`**.

Binding a proof to a **specific** checkpoint: run **`verify_checkpoint_stwo_proof_json`** with the claimed **`CheckpointJob`** (and optional digest check on the JSON blob).

## Plonky2 aggregation binding

`fractal-proof-aggregator` accepts `borsh(CheckpointStwoArtifactV1)` through `verify_stwo_artifact_submission`, verifies the STWO artifact against the submitted digest/range, and converts the checkpoint public inputs into `VerifiedStwoStatementV1`. When every tier-1 submission in a masterchain seal has a verified statement, `MasterchainLedger` uses the verified-STWO Plonky2 circuit, which binds the range, chain id, final header/state roots, first parent root, aggregate tx root, gas, and proof digest. If artifacts are missing or invalid, the node falls back to the legacy digest-only circuit so older devnet flows keep working.

For the future mandatory-proof regime, `MasterchainLedger::set_proof_slashing_policy` enables an invalid-proof evidence journal. With `require_verified_stwo = true`, malformed submissions, out-of-anchor ranges, duplicates, missing verified STWO statements, and aggregator rejections produce `InvalidProofSlashEventV1` rows with deterministic `evidence_hash` values. Each unique row also attempts one registered-prover bond burn for `slash_amount_wei`; duplicate evidence is ignored so the same bad proof cannot double-slash.

## riscv64 builds (“RISC-V engine”)

- Use the **same** workspace root, patch, and nightly pin; add a `riscv64gc-unknown-linux-gnu` (or your board) target if you cross-compile.
- Example (after `rustup target add riscv64gc-unknown-linux-gnu` and a working linker/sysroot):

  ```bash
  cargo test -p fractal-proof-condenser --target riscv64gc-unknown-linux-gnu --no-run
  ```

  Run tests on-device or in QEMU the same way as other workspace crates.

- **Fastest practical setup on RISC-V today**: multi-core machine + **`parallel`** inside `stwo` + Tokio multi-thread runtime for `spawn_blocking`. A future step would be a dedicated trace recorder / zkVM guest feeding the same STWO pipeline.

## Run from the workspace root

All `cargo` commands below assume:

```bash
cd /path/to/Fractalchainz
```

If you run Cargo from a checkout or manifest where the root `[patch.crates-io]` is not loaded, resolution can fall back to **registry** `stwo`, which typically fails on new nightlies.

## Verify the patch is active

```bash
cargo tree -p fractal-proof-condenser -i stwo
```

You should see a **path** into this repo, for example:

```text
stwo v2.2.0 (.../Fractalchainz/third_party/stwo)
```

If you see `…/.cargo/registry/.../stwo-2.2.0`, the patch is **not** applied — fix the working directory or root `Cargo.toml` before debugging compiler errors.

## Run the STWO-related tests

```bash
cargo test -p fractal-proof-condenser
```

Expected: all tests pass, including:

- `checkpoint_stwo::tests::stwo_prove_verify_round_trip` — real prove + verify round-trip.
- `checkpoint_stwo::tests::verify_proof_json_round_trip` — standalone verify of serialized proof.
- `persist::tests::registry_rocksdb_reload` — RocksDB path survives registry reopen.
- `checkpoint_stwo::tests::verify_rejects_tampered_proof_json` — corrupted JSON fails verify.
- `artifact::tests::artifact_borsh_round_trip_and_verify` — v1 borsh bundle round-trip.
- `artifact::tests::artifact_rejects_swapped_job_same_proof_json` — same `stark_proof_json` + digest, wrong `job` in bundle → STWO verify fails.
- `checkpoint_stwo::tests::different_header_hash_different_digest` — digest changes when the checkpoint header hash changes.
- `riscv_trace::tests::replay_trace_has_begin_tx_end_rows_and_root_changes` — real transaction rows are emitted and trace roots change when replayed txs change.
- `riscv_trace::tests::replay_trace_rejects_bad_tx_root` — malformed block transaction roots are rejected before proving.
- `checkpoint_stwo::tests::different_riscv_guest_trace_same_header_changes_digest` — same `header_hash`, different trace public inputs → different digest.
- `tests::async_prove_digest_changes_when_riscv_trace_changes` — async `prove_checkpoint` reflects trace public-input changes when STWO succeeds.

## Optional: latest nightly instead of `rust-toolchain.toml`

If you intentionally want to try the **default** `nightly` toolchain (not the pinned file):

```bash
cargo +nightly test -p fractal-proof-condenser
```

The vendored tree has been adjusted so this often works when the patch is in effect; pinned nightly remains the primary supported configuration for reproducible CI and local builds.

## Full workspace sanity check

```bash
cargo test --workspace
```

Or with explicit nightly:

```bash
cargo +nightly test --workspace
```

## Maintenance note

When upgrading Rust or bumping the `stwo` version, expect to **re-apply or refresh patches** under `third_party/stwo/` until upstream ships a release that matches current nightly/stable APIs.
