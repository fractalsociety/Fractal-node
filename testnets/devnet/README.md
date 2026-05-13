# FractalChain devnet (Docker)

PRD **M6** slice: run a JSON-RPC node plus the rate-limited faucet.

```bash
# from repo root
docker compose -f testnets/devnet/docker-compose.yml up --build
```

- Node: `http://127.0.0.1:8545` (chain id **41**)
- Faucet: `http://127.0.0.1:8088` (`POST /fund` with `{"address":"0x…"}`)

For a **static explorer**, serve `tools/explorer` over HTTP (see `tools/explorer/README.md`) and open `index.html?rpc=http://127.0.0.1:8545`.

## Explorer semantics

- **`docs/explorer.md`** — Ethereum vs internal tx hashes, leader vs follower JSON-RPC.

## Public bootnodes (≥3)

Production testnets should list at least three QUIC multiaddrs. See `bootnodes.example.txt` and replace hostnames / peer IDs before publishing.

**Note:** `fractal-node` follower accepts **`FRACTAL_BOOTSTRAP` as a comma-separated list** of multiaddrs that all end in the **same** `/p2p/<PeerId>` (redundant listen addresses for one producer). Example: `addr1,addr2`. You can still run multiple followers with different env values if you prefer.

Discord and a public status page are **out of repo** (operational); track links in your runbook or website. For a **minimal** RPC liveness page in-repo, see **`tools/status/`** and `./scripts/serve-status.sh`.
