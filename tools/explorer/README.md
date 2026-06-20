# Fractal Society Block Explorer

Static read-only UI over JSON-RPC (**CORS** is enabled on `fractal-rpc::serve_http` for dev). The redesigned explorer is intended for the public subdomain:

**`https://blockexplorer.fractalsociety.org`**

If DNS is intentionally configured as `blockexpleor.fractalsociety.org`, point that host at the same static files or add it as an alias.

## Deploy

Deploy the contents of this folder as a static site:

- `index.html`
- `app.js`
- `assets/fractal-explorer-hero.png`

The page defaults to the built-in same-origin `/rpc` endpoint. The production nginx config proxies `/rpc` to the deployed FractalChain RPC server (`http://192.3.47.245:8545`), so public/mobile visitors do not need a query parameter and browsers avoid mixed-content blocking. Public deployments can still pass a different RPC endpoint with:

```text
https://blockexplorer.fractalsociety.org/?rpc=https://YOUR_RPC_HOST
```

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

Open **`http://127.0.0.1:3333/?rpc=http://127.0.0.1:8545`** when testing against a local node. Without the `rpc` query parameter, the explorer uses `/rpc`.

Override port: **`EXPLORER_PORT=4000 ./scripts/serve-explorer.sh`**

For testing from a phone on the same network, bind the dev server to all interfaces and open the
computer's LAN IP on the phone:

```bash
EXPLORER_HOST=0.0.0.0 ./scripts/serve-explorer.sh
```

Use `http://YOUR_COMPUTER_LAN_IP:3333/?rpc=http://YOUR_COMPUTER_LAN_IP:8545` if your node is also bound on the LAN, or omit `?rpc=` to test the dev server's `/rpc` proxy.

## What it shows

- **Chain** — `eth_chainId`, `net_version`, head block, `eth_gasPrice`, `web3_clientVersion`, head gas/timestamp/tx count, and head finality.
- **Finality** — block rows show `finalityStatus`: **Soft-final** for committee/sequencer acceptance, **Proof-final** after accepted validity proof. Block detail also shows proof circuit version, coverage manifest digest, and covered feature mask when the RPC returns them.
- **Recent activity** — scans the latest **512** blocks and shows up to **10** newest blocks with transactions, falling back to the latest head blocks when the chain is idle. **Click a row** to show block metadata, finality, and a list of transaction hashes; each hash fills the tx field and runs lookup. The summary also shows the confirmed transaction count for the dev signer used by the load tool.
- **Account** — `eth_getBalance` + `eth_getTransactionCount` + `eth_getCode` for a pasted `0x` address (includes bytecode length and whether it looks like a contract).
- **Transaction** — `eth_getTransactionByHash` + `eth_getTransactionReceipt` for a pasted `0x` hash (JSON as returned by the node).

See **`docs/devnet.md`** for faucet and Docker devnet.
See **`docs/finality-ops.md`** for soft-final versus proof-final operating rules.

## Transaction hashes (RPC)

Blocks list **tx hashes as returned by your RPC node**. Those can be **Ethereum-style** `keccak256(raw EIP-1559)` or **internal** `keccak256(borsh(tx))` depending on submission path; followers may disagree with the producer until maps are replicated. Read **`docs/explorer.md`**.

## RPC liveness (status stub)

Minimal JSON-RPC probe UI: **`tools/status/`** — `./scripts/serve-status.sh` (port **`STATUS_PORT`**, default **3355**).
