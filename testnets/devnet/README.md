# FractalChain devnet (Docker)

PRD **M6** slice: run a JSON-RPC node plus the rate-limited faucet.

```bash
# from repo root
docker compose -f testnets/devnet/docker-compose.yml up --build
```

- Node: `http://127.0.0.1:8545` (chain id **41**)
- Faucet: `http://127.0.0.1:8088` (`POST /fund` with `{"address":"0x…"}`)

For a **static explorer**, serve `tools/explorer` over HTTP (see `tools/explorer/README.md`) and open `index.html?rpc=http://127.0.0.1:8545`.

## Public bootnodes (≥3)

Production testnets should list at least three QUIC multiaddrs. See `bootnodes.example.txt` and replace hostnames / peer IDs before publishing.

Discord and a public status page are **out of repo** (operational); track links in your runbook or website.
