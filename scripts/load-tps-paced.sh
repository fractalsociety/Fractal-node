#!/usr/bin/env bash
# Pace NoOp submits (~10/s) and measure confirmed chain TPS without stalling the node.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export FRACTAL_RPC_URL="${FRACTAL_RPC_URL:-http://127.0.0.1:8545}"
export LOAD_DURATION_SECS="${LOAD_DURATION_SECS:-45}"
export LOAD_SUBMIT_PAUSE_US="${LOAD_SUBMIT_PAUSE_US:-100000}"
export LOAD_WORKERS="${LOAD_WORKERS:-1}"
exec ./scripts/load-tps-smoke.sh
