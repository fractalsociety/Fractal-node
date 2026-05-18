#!/usr/bin/env bash
# Compatibility wrapper for the standalone contracts deploy flow.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
exec "$ROOT/contracts/deploy.sh"
