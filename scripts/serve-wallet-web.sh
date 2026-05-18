#!/usr/bin/env bash
# Compatibility wrapper for the standalone wallet web server.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec "$ROOT/tools/wallet-web/serve.sh"
