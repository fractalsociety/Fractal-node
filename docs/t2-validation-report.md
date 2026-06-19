# T2 Validation Report

**Date:** 2026-06-14
**Gate:** T2 - Proven
**Environment:** local macOS development machine, not target lab hardware.

## Summary

T2 is **not signed off**.

Deterministic BFT-7 unit coverage passed for proposer rotation, vote pooling, and
timeout-certificate formation. Local multi-process and lab smoke runs exposed
release blockers:

- BFT-7 shard smoke started seven validators but stalled at height 1.
- Two-shard M10 smoke reached block production but proof-worker metrics stayed at
  zero accepted proofs after an extended wait.
- The single-node Track B lab process did not remain available long enough for
  smoke/load measurements in this local session.
- Sustained-load p50/p95/p99 finality latency was therefore **not measured** and
  the 900 ms p99 target is **not confirmed**.

## Commands Run

| Command | Result |
| --- | --- |
| `cargo test -p fractal-node --test m7_c_validator_index -- --nocapture` | Pass: 5/5 tests. |
| `cargo test -p fractal-node --test m7_d4_vote_pool -- --nocapture` | Pass: 5/5 tests. |
| `cargo test -p fractal-bft-wire timeout -- --nocapture` | Pass: 5/5 filtered timeout/misbehavior tests. |
| `HYPERBFT_SMOKE_MIN_HEIGHT=3 ./scripts/run-hyperbft-bft7-shard.sh smoke-start` | Fail: all seven validators started, all RPCs reported height 1, smoke timed out waiting for height 3. |
| `FRACTAL_ANCHOR_INTERVAL=4 PILOT_SMOKE_MIN_HEIGHT=8 PILOT_PROOF_WAIT_SECS=30 ./scripts/run-pilot-shards.sh smoke-start` | Fail: shard 0 reached height 61, but `fractal_proofMetrics.proofsAccepted` remained `0x0`. |
| `LOAD_DURATION_SECS=20 LOAD_WORKERS=4 FRACTAL_RPC_URL=http://127.0.0.1:8545 ./scripts/load-tps-smoke.sh` | Blocked: RPC unavailable during local lab run; load tool now returns a normal error instead of panicking. |
| `cargo check --workspace` | Pass after wiring `crates/bft-wire`, `crates/shard`, and `tools/load-tps` into the workspace. |

## Fixes Made During Validation

- Added shard metadata RPCs to node RPC:
  - `fractal_getShardId`
  - `fractal_getShardCount`
  - `fractal_getConsensusMode`
- Wired node startup to expose `FRACTAL_SHARD_ID`, `FRACTAL_SHARD_COUNT`, and
  `FRACTAL_CONSENSUS_MODE`.
- Updated `run-pilot-shards.sh` and `run-track-b-lab.sh` to launch node
  processes with `nohup` so local smoke processes do not disappear on shell exit.
- Updated pilot smoke to accept shard-local `fractal_proofMetrics` when
  masterchain-only global-ZK RPCs are unavailable.
- Added `tools/load-tps` to the workspace and made its head-height checks return
  normal errors instead of panicking.

## T2 Checklist Status

- [ ] Run deterministic BFT-7 torture with partitions and view changes.
  - Partial only: deterministic proposer/vote/timeout unit tests pass. No
    partition torture runner completed.
- [ ] Run sustained-load lab benchmark on target hardware.
  - Not run. Local smoke was attempted, but this was not target hardware.
- [ ] Measure and publish p50, p95, p99 finality latency.
  - Not measured. Local lab node availability blocked sampling.
- [ ] Confirm p99 finality latency is <= 900 ms under the stated load profile.
  - Not confirmed.
- [ ] Confirm partition recovery does not violate safety.
  - Not confirmed by a partition run.
- [ ] Complete M10 exit: two shards finalize independently through the RPC gateway
  with no proof-worker latency regression.
  - Not complete. Two shards produced blocks, but proof metrics did not advance.
- [ ] T2 exit sign-off: p99 target met, partition-safe, M10 complete.
  - Not signed.

## Next Required Work

- Add or restore a real deterministic partition/view-change torture runner for
  BFT-7.
- Investigate the seven-validator shard stall at height 1.
- Wire or repair the async proof worker path so `fractal_proofMetrics` advances
  under `FRACTAL_ASYNC_PROOF=1` / `FRACTAL_AUTO_VALIDITY_PROOF=1`.
- Run the sustained load profile on target lab hardware and record p50/p95/p99
  finality latency with artifacts.
