#!/usr/bin/env bash
# Serve PRD W6-b reference wallet web stub (tools/wallet-web). Default: http://127.0.0.1:3344/
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${WALLET_WEB_PORT:-3344}"
cd "$ROOT/tools/wallet-web"
echo "FractalWork wallet web stub: http://127.0.0.1:${PORT}/"
exec python3 -m http.server "$PORT"
