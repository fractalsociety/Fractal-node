# fractal-load-tps — live RPC load / TPS estimate

Sustained native `NoOp` submits via `eth_sendRawTransaction` (borsh `Transaction`) against a running `fractal-node`.

## Run

```bash
# Gentle (~10 submits/s) — keeps block production moving on single-process lab nodes
./scripts/load-tps-paced.sh

# Full control
LOAD_WORKERS=4 LOAD_DURATION_SECS=30 LOAD_SUBMIT_PAUSE_US=5000 ./scripts/load-tps-smoke.sh
```

Env:

| Variable | Default | Meaning |
|----------|---------|---------|
| `FRACTAL_RPC_URL` | `http://127.0.0.1:8545` | JSON-RPC endpoint |
| `LOAD_DURATION_SECS` | `30` | Measure window (+ warmup) |
| `LOAD_WORKERS` | `8` | Parallel submit threads |
| `LOAD_WARMUP_SECS` | `3` | Submit before measure window |
| `LOAD_SUBMIT_PAUSE_US` | `200` | Sleep after each successful send |

**Metrics printed**

- **submit TPS** — RPC accepts (`eth_sendRawTransaction`)
- **confirmed nonce TPS** — on-chain nonce advance (best “real” throughput)
- **confirmed chain TPS** — txs listed in new blocks (can under-count if blocks are empty while nonces move)

## Track B lab results (2026-05-17)

Host: `./scripts/run-track-b-lab.sh` — HyperBFT singleton, `FRACTAL_TARGET_BLOCK_TIME_MS=70`, RPC `8545`.

| Scenario | Submit TPS | Block rate | Confirmed TPS |
|----------|------------|------------|---------------|
| Idle (no load) | — | ~1.5 blocks/s, empty blocks | 0 |
| Aggressive (`LOAD_WORKERS=6–8`, minimal pause) | ~9,000–13,000 | **0** (producer stalled ~30s) | 0 |
| Paced (`load-tps-paced.sh`, ~10/s) | ~6–10 | ~1.1 blocks/s | **~0.07** (nonce delta) |
| Very gentle (~5/s) | ~4 | ~1.5 blocks/s | ~0 in measure window |

**Takeaways**

- Single-process lab node: heavy RPC load competes with the 70 ms producer loop; use paced submit rates for meaningful numbers.
- PRD design targets (e.g. ~5k native TPS) are not represented by this devnet configuration; see `hyperbft_bft7_torture` (~4 NoOp / 70 ms tick ≈ 57 TPS in-process) and M10 sign-off in `docs/remaining-work.md`.
- For multi-validator soak, run load from a separate client and enable `FRACTAL_DEV_INJECT_QUORUM=1` on lab validators (`run-track-b-lab.sh` sets this on restart).

## Build

```bash
cargo build -p fractal-load-tps
```
