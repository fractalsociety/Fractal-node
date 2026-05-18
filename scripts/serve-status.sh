#!/usr/bin/env bash
# Serve PRD M6-d minimal RPC status page (tools/status). Default: http://127.0.0.1:3355/
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PORT="${STATUS_PORT:-3355}"
cd "$ROOT/tools/status"
echo "FractalChain status stub: http://127.0.0.1:${PORT}/"
exec python3 -m http.server "$PORT"
