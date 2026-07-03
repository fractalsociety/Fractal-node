# FractalChain devnet (Docker)

PRD **M6** slice: run a JSON-RPC node plus the rate-limited faucet.

```bash
# from repo root
docker compose -f testnets/devnet/docker-compose.yml up --build
```

- Node: `http://127.0.0.1:8545` (chain id **41**)
- Faucet: `http://127.0.0.1:8088` (`POST /fund` with `{"address":"0x…"}`)

## Remote server exposure

The base compose file keeps RPC and faucet private to the host by binding them
to `127.0.0.1` host ports. For a remote server that should be reachable by
FractalWork, either expose the native ports directly:

```bash
docker compose \
  -f testnets/devnet/docker-compose.yml \
  -f testnets/devnet/docker-compose.public.yml \
  up -d --build node faucet
```

Public endpoints with that override:

- Node RPC: `http://SERVER_IP:8545`
- Faucet: `http://SERVER_IP:8088/fund`

Or keep Docker private and expose only port 80 through nginx:

```bash
sudo mkdir -p /opt/fractal-explorer
sudo cp -R tools/explorer/index.html tools/explorer/app.js tools/explorer/assets /opt/fractal-explorer/
sudo cp tools/explorer/nginx-fractalchain-devnet.conf /etc/nginx/sites-available/fractalchain-devnet
sudo ln -sf /etc/nginx/sites-available/fractalchain-devnet /etc/nginx/sites-enabled/fractalchain-devnet
sudo nginx -t && sudo systemctl reload nginx
```

Public endpoints with nginx:

- Node RPC: `http://SERVER_IP/rpc`
- Faucet: `http://SERVER_IP/fund`

For a **static explorer**, serve `tools/explorer` over HTTP (see `tools/explorer/README.md`) and open `index.html?rpc=http://127.0.0.1:8545`.

## Explorer semantics

- **`docs/explorer.md`** — Ethereum vs internal tx hashes, leader vs follower JSON-RPC.

## Public bootnodes (≥3)

Production testnets should list at least three QUIC multiaddrs. See `bootnodes.example.txt` and replace hostnames / peer IDs before publishing.

**Note:** `fractal-node` follower accepts **`FRACTAL_BOOTSTRAP` as a comma-separated list** of multiaddrs that all end in the **same** `/p2p/<PeerId>` (redundant listen addresses for one producer). Example: `addr1,addr2`. You can still run multiple followers with different env values if you prefer.

Discord and a public status page are **out of repo** (operational); track links in your runbook or website. For a **minimal** RPC liveness page in-repo, see **`tools/status/`** and `./scripts/serve-status.sh`.
