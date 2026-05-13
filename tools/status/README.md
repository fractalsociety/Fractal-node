# FractalChain — minimal status checker (M6)

Static page that polls JSON-RPC (`eth_chainId`, `eth_blockNumber`, `web3_clientVersion`). Same **CORS** rules as the explorer: the node must allow your origin (dev `fractal-rpc` does).

## Run

From repo root:

```bash
./scripts/serve-status.sh
```

Open **http://127.0.0.1:3355/** (override with **`STATUS_PORT`**), paste your RPC URL (e.g. `http://127.0.0.1:8545`), click **Check**.

This is **not** a hosted status page product — it is a reference for operators wiring UptimeRobot / Grafana / `curl` probes against `eth_blockNumber` or similar.

See **`docs/devnet.md`** and **`docs/explorer.md`**.
