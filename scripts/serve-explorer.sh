#!/usr/bin/env bash
# Serve PRD M6 static explorer (tools/explorer). Default: http://127.0.0.1:3333/?rpc=http://127.0.0.1:8545
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${EXPLORER_PORT:-3333}"
cd "$ROOT/tools/explorer"
echo "FractalChain explorer: http://127.0.0.1:${PORT}/?rpc=http://127.0.0.1:8545"
exec python3 -m http.server "$PORT"
