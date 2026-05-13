# Devnet & M6 operator notes

This page ties together **PRD §18 M6** pieces in-repo (anchored on **`docs/prd.md`**, not `docs/wallet.md`).

## JSON-RPC

- Default local bind: `http://127.0.0.1:8545` (`FRACTAL_RPC_ADDR`).
- **CORS** is enabled on the HTTP JSON-RPC server so browser tools (e.g. `tools/explorer`) can call `eth_*` methods cross-origin during development.

## Faucet (tFRAC-style native balance)

- Binary: `cargo run -p fractal-faucet` (or Docker service in `testnets/devnet/docker-compose.yml`).
- Treasury address: **`fractal_core::DEVNET_FAUCET_TREASURY`** (prefunded in `NodeInner::devnet()`).
- Env: `FRACTAL_RPC_URL`, `FAUCET_BIND`, `FAUCET_DRIP_AMOUNT`, `FAUCET_COOLDOWN_SECS`.

## Explorer

- Static UI under `tools/explorer` — chain summary, recent blocks (click a row for tx hashes), account (balance, nonce, code) + tx lookup (`tools/explorer/README.md`).
- **Semantics:** `docs/explorer.md` (tx hash identity, leader vs follower).
- From repo root: **`./scripts/serve-explorer.sh`** (optional **`EXPLORER_PORT`**).

## RPC liveness (status stub)

- **`tools/status/`** — `./scripts/serve-status.sh` (optional **`STATUS_PORT`**, default **3355**). Polls `eth_chainId`, `eth_blockNumber`, `web3_clientVersion` from a pasted RPC URL (CORS must allow the status page origin).

## Docker devnet

- `testnets/devnet/docker-compose.yml` builds **node** + **faucet** images from this repo.
- Example multiaddrs for followers: `testnets/devnet/bootnodes.example.txt`.

Community (Discord) and a public status page are **operational** concerns outside this repository.

## M5 bridge smoke (PRD exit scale)

PRD **M5** exit line includes ≥100 receipts settled then claimed with no manual intervention (`docs/prd.md` §M5).

- **Local / CI:** from repo root, with JSON-RPC listening on **`FRACTAL_RPC_URL`** (default `http://127.0.0.1:8545`):

  ```bash
  ./scripts/run-mvp-bridge-smoke.sh
  ```

  Uses **`MVP_RECEIPT_COUNT`** (default **100**) synthetic receipts via `fractal-mvp-bridge`. The script waits for RPC with **`./scripts/wait-for-jsonrpc.sh`** (`RPC_WAIT_SECS`, default 180).

- **Docker devnet:** `docker compose -f testnets/devnet/docker-compose.yml up -d node` then the same smoke script against `http://127.0.0.1:8545`.

- **GitHub Actions:** workflow definition lives at **`docs/ci/mvp-bridge-smoke.workflow.yml`** (not under `.github/workflows/` in git) so HTTPS pushes work with default PAT scopes. **Install:** copy that file to `.github/workflows/mvp-bridge-smoke.yml`, or push with a PAT that includes the **`workflow`** scope, or create the workflow in the GitHub Actions UI. The script **`./scripts/run-mvp-bridge-smoke.sh`** is unchanged.

- **Off-chain-shaped batch:** set **`MVP_RECEIPTS_JSON`** to a file (see `crates/mvp-backend/testdata/mvp_receipts_sample.json`) instead of synthetic counts; see `crates/mvp-backend` module docs on `fractal-mvp-bridge`.
