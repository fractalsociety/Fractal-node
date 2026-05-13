#!/usr/bin/env bash
# Run tools/provider-http-sample/server.py (PORT default 8765)
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PORT="${PORT:-8765}"
cd "$ROOT/tools/provider-http-sample"
exec python3 server.py
