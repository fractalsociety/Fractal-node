#!/usr/bin/env bash
# Dedicated masterchain BFT coordinator (PRD §7.10).
#
# Usage:
#   ./scripts/run-masterchain-bft.sh          # start (default RPC :8550)
#   ./scripts/run-masterchain-bft.sh stop
#
# Shard validators should set:
#   export FRACTAL_MASTERCHAIN_RPC=http://127.0.0.1:8550
# so anchors are submitted here instead of sealed locally.

set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PID_DIR="${ROOT}/.masterchain-bft"
mkdir -p "$PID_DIR"

TARGET_DIR="${CARGO_TARGET_DIR:-${ROOT}/target}"
BINARY="${TARGET_DIR}/debug/fractal-masterchain"

stop_mc() {
  local f="${PID_DIR}/masterchain.pid"
  [[ -f "$f" ]] || return 0
  local pid
  pid="$(cat "$f")"
  if kill -0 "$pid" 2>/dev/null; then
    echo "Stopping masterchain pid $pid"
    kill "$pid" 2>/dev/null || true
  fi
  rm -f "$f"
  while read -r p; do
    [[ -n "$p" ]] && kill "$p" 2>/dev/null || true
  done < <(lsof -tiTCP:8550 -sTCP:LISTEN 2>/dev/null || true)
}

cmd="${1:-start}"
case "$cmd" in
  stop)
    stop_mc
    ;;
  start|"")
    stop_mc
    echo "Building fractal-masterchain -> $BINARY"
    (cd "$ROOT" && cargo build -p fractal-masterchain)
    LOG="${PID_DIR}/masterchain.log"
    DB="${PID_DIR}/rocksdb"
    rm -rf "$DB"
    mkdir -p "$DB"
    echo "Starting masterchain BFT (RPC :8550) -> $LOG"
    FRACTAL_SHARD_COUNT="${FRACTAL_SHARD_COUNT:-2}" \
    FRACTAL_MASTERCHAIN_BLOCK_MS="${FRACTAL_MASTERCHAIN_BLOCK_MS:-1000}" \
    FRACTAL_MASTERCHAIN_RPC_ADDR="127.0.0.1:8550" \
    FRACTAL_MASTERCHAIN_ROCKSDB_PATH="$DB" \
      "$BINARY" >"$LOG" 2>&1 &
    echo $! >"${PID_DIR}/masterchain.pid"
    for _ in $(seq 1 40); do
      if curl -sf -X POST "http://127.0.0.1:8550" -H 'Content-Type: application/json' \
        --data '{"jsonrpc":"2.0","method":"fractal_getMasterchainHeight","params":[],"id":1}' \
        | grep -q '"result"'; then
        echo "Masterchain RPC ready at http://127.0.0.1:8550"
        echo "  Shard nodes: export FRACTAL_MASTERCHAIN_RPC=http://127.0.0.1:8550"
        echo "  Logs: tail -f $LOG"
        exit 0
      fi
      sleep 0.25
    done
    echo "ERROR: masterchain RPC not ready — tail $LOG" >&2
    exit 1
    ;;
  *)
    echo "Usage: $0 [start|stop]" >&2
    exit 1
    ;;
esac
