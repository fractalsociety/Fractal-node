#!/usr/bin/env bash
# Sustained native NoOp load + chain TPS estimate against a running fractal-node.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

export FRACTAL_RPC_URL="${FRACTAL_RPC_URL:-http://127.0.0.1:8545}"
export LOAD_DURATION_SECS="${LOAD_DURATION_SECS:-30}"
export LOAD_WORKERS="${LOAD_WORKERS:-2}"
export LOAD_WARMUP_SECS="${LOAD_WARMUP_SECS:-3}"

echo "load-tps-smoke: building fractal-load-tps"
cargo build -q -p fractal-load-tps

echo "load-tps-smoke: RPC=$FRACTAL_RPC_URL duration=${LOAD_DURATION_SECS}s workers=$LOAD_WORKERS"
exec cargo run -q -p fractal-load-tps
