#!/usr/bin/env bash
# Compatibility wrapper; FractalScan is self-contained under tools/explorer.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec "$ROOT/tools/explorer/serve.sh"
