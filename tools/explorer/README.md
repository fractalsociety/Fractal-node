# FractalChain dev explorer (PRD M6)

Static read-only UI over JSON-RPC (**CORS** is enabled on `fractal-rpc::serve_http` for dev).

## Run

From repo root:

```bash
./scripts/serve-explorer.sh
```

Or manually:

```bash
cd tools/explorer
python3 -m http.server 3333
```

Open **`http://127.0.0.1:3333/?rpc=http://127.0.0.1:8545`** (change `rpc` if your node is elsewhere, e.g. Docker published **8545**).

Override port: **`EXPLORER_PORT=4000 ./scripts/serve-explorer.sh`**

## What it shows

- **Chain** — `eth_chainId`, `net_version`, head block, `eth_gasPrice`, `web3_clientVersion`, head gas/timestamp/tx count.
- **Recent blocks** — up to **10** blocks from head downward (`eth_getBlockByNumber`).
- **Account** — `eth_getBalance` + `eth_getTransactionCount` for a pasted `0x` address.
- **Transaction** — `eth_getTransactionByHash` + `eth_getTransactionReceipt` for a pasted `0x` hash (JSON as returned by the node).

See **`docs/devnet.md`** for faucet and Docker devnet.
