#!/usr/bin/env python3
"""Minimal tool-provider stub (`docs/wallet.md` §7.2): POST /v1/quote.

- Default: unsigned JSON (W6-d) for quick wiring tests.
- Optional: Ed25519 detached signature over **borsh(QuoteBody)** (same as `fractal_wallet::Quote::sign`)
  when `PROVIDER_ED25519_SEED_HEX` is set (64 hex chars = 32-byte seed) and PyNaCl is installed:

      pip install -r requirements-signing.txt

  Regenerate / inspect borsh bytes: `cargo run -p fractal-wallet --example dump_quote_body_borsh`
"""
from __future__ import annotations

import json
import os
from http.server import BaseHTTPRequestHandler, HTTPServer


def _hex_to_bytes_32(s: str, field: str) -> bytes:
    t = s.strip().lower().removeprefix("0x")
    if len(t) != 64:
        raise ValueError(f"{field} must be 32 bytes hex (64 nibbles), got {len(t)//2} bytes")
    return bytes.fromhex(t)


def borsh_quote_body(
    quote_id: bytes,
    intent_id: bytes,
    provider_id: bytes,
    price: int,
    expiry_ms: int,
) -> bytes:
    if len(quote_id) != 32 or len(intent_id) != 32 or len(provider_id) != 32:
        raise ValueError("ids must be 32 bytes")
    return (
        quote_id
        + intent_id
        + provider_id
        + price.to_bytes(16, "little", signed=False)
        + expiry_ms.to_bytes(8, "little", signed=False)
    )


def _maybe_sign(
    quote_id: bytes,
    intent_id: bytes,
    price: int,
    expiry_ms: int,
) -> tuple[bytes, str, str]:
    """Returns (provider_id_bytes, signature_hex_with_0x, provider_pk_hex_with_0x) or raises."""
    seed_hex = os.environ.get("PROVIDER_ED25519_SEED_HEX", "").strip()
    if not seed_hex:
        raise RuntimeError("internal: _maybe_sign without seed")
    try:
        from nacl.signing import SigningKey
    except ImportError as e:
        raise RuntimeError(
            "PROVIDER_ED25519_SEED_HEX set but PyNaCl missing; pip install -r requirements-signing.txt"
        ) from e
    try:
        import blake3  # type: ignore[import-untyped]
    except ImportError as e:
        raise RuntimeError(
            "PROVIDER_ED25519_SEED_HEX set but blake3 missing; pip install -r requirements-signing.txt"
        ) from e
    seed = bytes.fromhex(seed_hex)
    if len(seed) != 32:
        raise ValueError("PROVIDER_ED25519_SEED_HEX must be 64 hex chars (32 bytes)")
    sk = SigningKey(seed)
    vk = sk.verify_key.encode()
    provider_id = blake3.blake3(vk).digest()
    msg = borsh_quote_body(quote_id, intent_id, provider_id, price, expiry_ms)
    sig = sk.sign(msg).signature
    return provider_id, "0x" + sig.hex(), "0x" + vk.hex()


def _maybe_sign_optional(
    quote_id: bytes,
    intent_id: bytes,
    provider_id: bytes,
    price: int,
    expiry_ms: int,
) -> tuple[bytes, str | None, str | None]:
    """If seed set: real provider_id (BLAKE3(pk)) + sig; else (request provider_id, None, None)."""
    if not os.environ.get("PROVIDER_ED25519_SEED_HEX", "").strip():
        return provider_id, None, None
    pid, sig_hex, pk_hex = _maybe_sign(quote_id, intent_id, price, expiry_ms)
    return pid, sig_hex, pk_hex


class H(BaseHTTPRequestHandler):
    def log_message(self, fmt: str, *args) -> None:
        print(f"[provider-sample] {fmt % args}")

    def do_GET(self) -> None:
        if self.path in ("/", "/health"):
            self._send(200, {"ok": True, "service": "fractal-provider-http-sample"})
            return
        self._send(404, {"error": "not_found"})

    def do_POST(self) -> None:
        if self.path != "/v1/quote":
            self._send(404, {"error": "not_found"})
            return
        ln = int(self.headers.get("Content-Length", "0") or 0)
        raw = self.rfile.read(ln) if ln else b"{}"
        try:
            body = json.loads(raw.decode("utf-8") or "{}")
        except json.JSONDecodeError:
            self._send(400, {"error": "invalid_json"})
            return
        try:
            intent_raw = body.get("intentId") or body.get("intent_id") or ("0x" + "00" * 32)
            intent_id = _hex_to_bytes_32(str(intent_raw), "intentId")
            quote_id = _hex_to_bytes_32(
                str(body.get("quoteId") or body.get("quote_id") or ("0x" + "11" * 32)),
                "quoteId",
            )
            provider_id = _hex_to_bytes_32(
                str(body.get("providerId") or body.get("provider_id") or ("0x" + "22" * 32)),
                "providerId",
            )
            price = int(str(body.get("maxPrice") or body.get("price") or "1"))
            expiry_ms = int(str(body.get("expiryMs") or body.get("expiry_ms") or "9999999999999"))
        except ValueError as e:
            self._send(400, {"error": "bad_request", "detail": str(e)})
            return

        try:
            provider_out, sig_hex, pk_hex = _maybe_sign_optional(
                quote_id, intent_id, provider_id, price, expiry_ms
            )
        except (RuntimeError, ValueError) as e:
            self._send(500, {"error": "signing_failed", "detail": str(e)})
            return

        out: dict = {
            "quoteId": "0x" + quote_id.hex(),
            "intentId": "0x" + intent_id.hex(),
            "providerId": "0x" + provider_out.hex(),
            "price": str(price),
            "expiryMs": str(expiry_ms),
            "note": "stub — verify with fractal_wallet::Quote::verify",
        }
        if sig_hex:
            out["signature"] = sig_hex
            out["providerPublicKey"] = pk_hex
            out["signed"] = True
        else:
            out["signed"] = False

        self._send(200, out)

    def _send(self, code: int, obj: object) -> None:
        b = json.dumps(obj).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(b)))
        self.end_headers()
        self.wfile.write(b)


def main() -> None:
    port = int(os.environ.get("PORT", "8765"))
    httpd = HTTPServer(("127.0.0.1", port), H)
    print(f"provider-http-sample listening on http://127.0.0.1:{port}/ (POST /v1/quote)")
    httpd.serve_forever()


if __name__ == "__main__":
    main()
