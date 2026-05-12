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

- Static UI under `tools/explorer` — see `tools/explorer/README.md`.

## Docker devnet

- `testnets/devnet/docker-compose.yml` builds **node** + **faucet** images from this repo.
- Example multiaddrs for followers: `testnets/devnet/bootnodes.example.txt`.

Community (Discord) and a public status page are **operational** concerns outside this repository.
