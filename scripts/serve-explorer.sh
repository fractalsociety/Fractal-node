#!/usr/bin/env bash
# Serve PRD M6 static explorer (tools/explorer).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${EXPLORER_PORT:-3333}"
cd "$ROOT/tools/explorer"
echo "FractalChain explorer: http://127.0.0.1:${PORT}/"
echo "Set EXPLORER_RPC_URL to choose the upstream RPC for the local /rpc proxy."
exec node dev-server.mjs
