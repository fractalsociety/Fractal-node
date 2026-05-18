# FractalChain

FractalChain is a sharded L1 for agent workloads: **HyperBFT** execution shards, a **masterchain** for anchors and ZK proofs, native **FractalCore** syscalls plus **EVM** (revm), and an async **STWO → Plonky2** proof pipeline. Product and protocol details live in [`docs/prd.md`](docs/prd.md).

This README covers **how to build and run a local dev chain** on your machine.

---

## Prerequisites

| Requirement | Notes |
|-------------|--------|
| **Rust (nightly)** | Repo pins **`rust-toolchain.toml`** (`nightly-2025-09-15`) for STWO / `portable_simd`. Install [rustup](https://rustup.rs); `cargo` in the repo root will use the pinned toolchain automatically. |
| **macOS / Linux** | Primary dev targets. |
| **curl** | Used by smoke scripts. |
| **Docker** (optional) | For `testnets/devnet/docker-compose.yml` only. |

```bash
rustc --version   # should match rust-toolchain.toml after cd into the repo
cargo --version
```

---

## Quick start — single local node (Track B lab)

The fastest path is **one HyperBFT shard** with JSON-RPC, masterchain anchors, and STWO→Plonky2 on a short interval:

```bash
git clone https://github.com/fractalsociety/Fractal-node.git
cd Fractal-node

# Build and start (RPC http://127.0.0.1:8545, fresh DB under .track-b-lab/)
./scripts/run-track-b-lab.sh
```

In another terminal, confirm the chain is live:

```bash
./scripts/smoke-track-b-e2e.sh

# or manually:
curl -s http://127.0.0.1:8545 -H 'Content-Type: application/json' \
  -d '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}'
```

Stop the node:

```bash
./scripts/run-track-b-lab.sh stop
```

**Logs:** `tail -f .track-b-lab/node.log`

**Defaults for this lab node**

- Consensus: **HyperBFT**, 70 ms target block time, singleton validator
- RPC: **`127.0.0.1:8545`** (`FRACTAL_RPC_ADDR`)
- Masterchain anchor every **4** shard blocks; auto tier-1 proofs + Plonky2 seal
- RocksDB data: **`.track-b-lab/rocksdb`** (wiped on each `run-track-b-lab.sh` start)

---

## Submit a native transaction (dev)

Dev accounts match **Hardhat account #0** (prefunded). Submit a native **`NoOp`** via borsh `eth_sendRawTransaction` using the CLI pattern in `crates/cli`, or use the load tool:

```bash
# Optional: sustained NoOp load / TPS estimate (see tools/load-tps/README.md)
./scripts/load-tps-paced.sh
```

For EVM-style transfers, run the dev **faucet** against the same RPC (see [Faucet](#faucet) below).

---

## Block explorer (FractalScan)

With the node running on **8545**:

```bash
./scripts/serve-explorer.sh
```

Open **http://127.0.0.1:3333/?rpc=http://127.0.0.1:8545**

Optional deep index (blocks, search, address history):

```bash
./scripts/serve-indexer.sh
# then open explorer with ?indexer=http://127.0.0.1:8088
```

See [`tools/explorer/README.md`](tools/explorer/README.md) and [`docs/explorer.md`](docs/explorer.md).

---

## Other ways to run the chain

| Goal | Command | RPC (typical) |
|------|---------|----------------|
| **Two pilot shards** (HyperBFT) | `./scripts/run-pilot-shards.sh` | shard 0 **8545**, shard 1 **8547** |
| Pilot + smoke (start and test) | `./scripts/run-pilot-shards.sh smoke-start` | same |
| **Dedicated masterchain** + shards | `./scripts/run-pilot-shards.sh start-with-masterchain` | masterchain **8550** |
| **BFT-7 validator lab** (7 processes) | `./scripts/run-hyperbft-bft7-shard.sh smoke-start` | **8650**–**8656** |
| **Docker devnet** (node + faucet) | `docker compose -f testnets/devnet/docker-compose.yml up --build` | **8545** |
| **RPC gateway** (multi-shard) | `./scripts/run-rpc-gateway.sh` | **8549** |

Operator env vars, validator sets, metrics, and P2P: **[`docs/devnet.md`](docs/devnet.md)**.

---

## Faucet

With JSON-RPC up:

```bash
export FRACTAL_RPC_URL=http://127.0.0.1:8545
cargo run -p fractal-faucet
```

Default bind and drip settings are documented in [`docs/devnet.md`](docs/devnet.md#faucet-tfrac-style-native-balance).

---

## Build and test

```bash
# Full workspace (uses pinned nightly)
cargo test --workspace

# Focused slices
cargo test -p fractal-core
cargo test -p fractal-node --test stwo_plonky2_pipeline
cargo test -p fractal-proof-condenser   # STWO; see docs/stwo-run-notes.md
```

**STWO / proof condenser:** vendored `third_party/stwo` + nightly pin — see [`docs/stwo-run-notes.md`](docs/stwo-run-notes.md).

---

## Repository map

| Path | Purpose |
|------|---------|
| [`crates/node`](crates/node) | `fractal-node` binary — consensus + RPC + P2P |
| [`crates/core`](crates/core) | State, native syscalls, transactions |
| [`crates/proof-condenser`](crates/proof-condenser) | Tier-1 STWO checkpoint proofs |
| [`crates/proof-aggregator`](crates/proof-aggregator) | Tier-2 Plonky2 → `globalZkRoot` |
| [`crates/masterchain`](crates/masterchain) | Masterchain BFT coordination crate |
| [`scripts/`](scripts) | `run-track-b-lab.sh`, pilot shards, smoke tests, explorer |
| [`tools/explorer`](tools/explorer) | FractalScan static UI |
| [`tools/load-tps`](tools/load-tps) | Live RPC load / TPS estimate |
| [`docs/prd.md`](docs/prd.md) | Product requirements |
| [`docs/devnet.md`](docs/devnet.md) | Devnet operator reference |
| [`docs/remaining-work.md`](docs/remaining-work.md) | Backlog |

---

## JSON-RPC highlights

Default URL: **http://127.0.0.1:8545**

Standard Ethereum methods (`eth_blockNumber`, `eth_getBlockByNumber`, `eth_sendRawTransaction`, …) plus Fractal extensions, including:

- `fractal_getShardId`, `fractal_getConsensusMode`
- `fractal_getMasterchainHead`, `fractal_getGlobalZkRoot`, `fractal_getGlobalZkProof`
- `fractal_getCheckpointProof`, `fractal_getCheckpointProofDigest`

CORS is enabled for local browser tools. Full list: [`docs/devnet.md`](docs/devnet.md).

---

## Getting help

- **Operator / env reference:** [`docs/devnet.md`](docs/devnet.md)
- **Wallet / agent product:** [`docs/wallet.md`](docs/wallet.md)
- **Validator fixture keys:** `cargo run -p fractal-node -- print-devnet-validator-keys`

License: MIT OR Apache-2.0 (see crate manifests).
