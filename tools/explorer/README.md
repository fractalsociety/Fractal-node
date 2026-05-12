# FractalChain minimal explorer (PRD M6)

Static read-only UI over JSON-RPC (CORS must be allowed on the node — enabled in `fractal-rpc::serve_http`).

```bash
cd tools/explorer
python3 -m http.server 3333
```

Open `http://127.0.0.1:3333/?rpc=http://127.0.0.1:8545` (adjust RPC if using Docker: published port **8545**).

Shows **chain id**, **block number**, and the **latest** block hash.
