# Provider HTTP sample (wallet PRD W6-d / W6-e)

Minimal **`POST /v1/quote`** stub aligned with `docs/wallet.md` §7.2 and `packages/fractal-provider-ts` wire types.

```bash
cd tools/provider-http-sample
python3 server.py
# or: PORT=9000 python3 server.py
```

- **GET** `/` or `/health` → `{ "ok": true, "service": "..." }`
- **POST** `/v1/quote` with JSON body → returns `quoteId`, `intentId`, `providerId`, `price`, `expiryMs` (32-byte hex ids; default stub ids `0x11…`, `0x00…`, `0x22…` if omitted).
- **Unsigned** (default): `"signed": false` for quick wiring tests.
- **Signed** (optional): set **`PROVIDER_ED25519_SEED_HEX`** to 64 hex chars (32-byte Ed25519 seed). Install **`pip install -r requirements-signing.txt`**. The server returns **`signature`** (detached Ed25519 over **borsh(`QuoteBody`)**), **`providerPublicKey`**, and sets **`providerId`** to `BLAKE3(providerPublicKey)` to match `fractal_wallet::provider_id_from_public_key`. Verify in Rust with `Quote::verify`.

Borsh layout reference: **`cargo run -p fractal-wallet --example dump_quote_body_borsh`** (golden bytes are asserted in `fractal_wallet` unit tests).

From repo root: **`./scripts/run-provider-http-sample.sh`**
