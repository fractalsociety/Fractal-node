# FractalWork reference wallet (web stub) — PRD `docs/wallet.md` §25.1 / §29

Static read-only UI for **Phase 1 built-in policy templates** (§15.2). Data is loaded from `builtins.json`, which must stay in sync with `fractal-wallet` / `fractal-wallet-cli`:

```bash
cargo run -p fractal-cli -- policy dump-builtins > tools/wallet-web/builtins.json
```

`crates/cli/tests/wallet_cli.rs` asserts the file matches the CLI output.

See also **`packages/fractal-provider-ts/`** for TypeScript wire types (`npm run check`) and **`fractal_sdk::provider`** in Rust for signing / `ToolMarket` types.

## Run

From repo root:

```bash
./scripts/serve-wallet-web.sh
```

Or:

```bash
cd tools/wallet-web
python3 -m http.server 3344
```

Open **http://127.0.0.1:3344/** (override with `WALLET_WEB_PORT`).

## Capability verify

Decoding a capability token is **offline crypto** (borsh + Ed25519). This stub does not ship WASM yet; use the CLI:

```bash
cargo run -p fractal-cli -- cap show <0x… hex from cap mint>
```

Next wallet slice (**W6-d**): native opcodes beyond anchor, long-running indexer, sample provider HTTP server.
