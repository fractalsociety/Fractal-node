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
- **Recent blocks** — up to **10** blocks from head downward (`eth_getBlockByNumber`). **Click a row** to show block metadata and a list of transaction hashes; each hash fills the tx field and runs lookup.
- **Account** — `eth_getBalance` + `eth_getTransactionCount` + `eth_getCode` for a pasted `0x` address (includes bytecode length and whether it looks like a contract).
- **Transaction** — `eth_getTransactionByHash` + `eth_getTransactionReceipt` for a pasted `0x` hash (JSON as returned by the node).

See **`docs/devnet.md`** for faucet and Docker devnet.

## Transaction hashes (RPC)

Blocks list **tx hashes as returned by your RPC node**. Those can be **Ethereum-style** `keccak256(raw EIP-1559)` or **internal** `keccak256(borsh(tx))` depending on submission path; followers may disagree with the producer until maps are replicated. Read **`docs/explorer.md`**.

## RPC liveness (status stub)

Minimal JSON-RPC probe UI: **`tools/status/`** — `./scripts/serve-status.sh` (port **`STATUS_PORT`**, default **3355**).
