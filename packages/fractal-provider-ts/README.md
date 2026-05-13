# `@fractalwork/provider` (reference stub)

TypeScript types for **tool provider** HTTP/JSON bridges aligned with `docs/wallet.md` §7 and §21.2.

- **Rust surface:** `fractal_sdk::provider` in `crates/sdk-rust` (re-exports `fractal_wallet` market types + `IndexerCursor` / `IntentPollFilter`).
- **Build:** `npm install && npm run build` (emits `dist/`).
- **Check only:** `npm run check`

This package does **not** ship crypto or borsh codecs yet — use `fractal-wallet-cli` / in-process Rust for signing, or add WASM later.
