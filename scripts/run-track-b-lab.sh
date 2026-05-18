#!/usr/bin/env bash
# Track B lab: one HyperBFT shard + STWO condenser + tier1→Plonky2 masterchain pipeline.
#
# Usage:
#   ./scripts/run-track-b-lab.sh          # start
#   ./scripts/run-track-b-lab.sh stop
#   ./scripts/smoke-track-b-e2e.sh        # curl checks (node must be running)
#
# JSON-RPC: http://127.0.0.1:8545

set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PID_DIR="${ROOT}/.track-b-lab"
mkdir -p "$PID_DIR"

TARGET_DIR="${CARGO_TARGET_DIR:-${ROOT}/target}"
BINARY="${TARGET_DIR}/debug/fractal-node"
echo "Building fractal-node -> $BINARY"
(cd "$ROOT" && cargo build -p fractal-node)

stop_lab() {
  if [[ -f "${PID_DIR}/node.pid" ]]; then
    pid="$(cat "${PID_DIR}/node.pid")"
    if kill -0 "$pid" 2>/dev/null; then
      echo "Stopping pid $pid"
      kill "$pid" 2>/dev/null || true
    fi
    rm -f "${PID_DIR}/node.pid"
  fi
}

if [[ "${1:-}" == "stop" ]]; then
  stop_lab
  exit 0
fi

stop_lab

DB="${PID_DIR}/rocksdb"
rm -rf "$DB"
mkdir -p "$DB"
LOG="${PID_DIR}/node.log"

echo "Starting Track B lab node -> $LOG"
FRACTAL_CONSENSUS_MODE=hyperbft \
FRACTAL_DEV_INJECT_QUORUM=1 \
FRACTAL_SHARD_COUNT=1 \
FRACTAL_SHARD_ID=0 \
FRACTAL_TARGET_BLOCK_TIME_MS=70 \
FRACTAL_ANCHOR_INTERVAL=4 \
FRACTAL_ASYNC_PROOF=1 \
FRACTAL_AUTO_VALIDITY_PROOF=1 \
FRACTAL_RPC_ADDR=127.0.0.1:8545 \
FRACTAL_P2P_LISTEN=/ip4/127.0.0.1/udp/9010/quic-v1 \
FRACTAL_CHAIN_ROCKSDB_PATH="$DB" \
FRACTAL_PROOF_ROCKSDB_PATH="$DB" \
  "$BINARY" >"$LOG" 2>&1 &
echo $! >"${PID_DIR}/node.pid"

echo ""
echo "Track B lab running (shard 0, anchor every 4 blocks, STWO→tier1→Plonky2 on seal)."
echo "  Logs:  tail -f $LOG"
echo "  Smoke: ./scripts/smoke-track-b-e2e.sh"
echo "  Stop:  ./scripts/run-track-b-lab.sh stop"
