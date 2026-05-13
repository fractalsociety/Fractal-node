#!/usr/bin/env bash
# PRD M5 exit-scale smoke: SETTLE_BATCH + CLAIM_PAYOUT via fractal-mvp-bridge (default 100 receipts).
# Requires a running fractal-node JSON-RPC (local or Docker). See docs/devnet.md §M5 bridge smoke.
#
# Env:
#   FRACTAL_RPC_URL   (default http://127.0.0.1:8545)
#   MVP_RECEIPT_COUNT (default 100)
#   RPC_WAIT_SECS     passed to wait-for-jsonrpc.sh (default 180)

set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

export FRACTAL_RPC_URL="${FRACTAL_RPC_URL:-http://127.0.0.1:8545}"
export MVP_RECEIPT_COUNT="${MVP_RECEIPT_COUNT:-100}"
export RPC_WAIT_SECS="${RPC_WAIT_SECS:-180}"

./scripts/wait-for-jsonrpc.sh
echo "run-mvp-bridge-smoke: MVP_RECEIPT_COUNT=$MVP_RECEIPT_COUNT FRACTAL_RPC_URL=$FRACTAL_RPC_URL"
cargo run -p fractal-mvp-backend --bin fractal-mvp-bridge --release
