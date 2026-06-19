#!/usr/bin/env bash
# Pilot: two independent shard validators (PRD §7.9 / M10 lab net).
#
# Usage:
#   ./scripts/run-pilot-shards.sh              # start (default anchor every 100 blocks)
#   ./scripts/run-pilot-shards.sh stop         # stop processes
#   ./scripts/run-pilot-shards.sh smoke-start  # start with anchor=4, run smoke, leave running
#   ./scripts/run-pilot-shards.sh smoke        # smoke test only (shards must be running)
#
# Shard 0 JSON-RPC: http://127.0.0.1:8545
# Shard 1 JSON-RPC: http://127.0.0.1:8547

set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PID_DIR="${ROOT}/.pilot-shards"
mkdir -p "$PID_DIR"

TARGET_DIR="${CARGO_TARGET_DIR:-${ROOT}/target}"
BINARY="${TARGET_DIR}/debug/fractal-node"

# Pilot RPC / QUIC ports — kill stray listeners so restarts don't hit EADDRINUSE.
PILOT_PORTS=(8545 8547 9000 9002)

free_pilot_ports() {
  local pids=""
  for port in "${PILOT_PORTS[@]}"; do
    while read -r pid; do
      [[ -n "$pid" && "$pid" != "$$" ]] && pids+="$pid "
    done < <(lsof -tiTCP:"$port" -sTCP:LISTEN 2>/dev/null || true)
    while read -r pid; do
      [[ -n "$pid" && "$pid" != "$$" ]] && pids+="$pid "
    done < <(lsof -tiUDP:"$port" 2>/dev/null || true)
  done
  local killed=0
  while read -r pid; do
    [[ -z "$pid" ]] && continue
    if kill -0 "$pid" 2>/dev/null; then
      echo "Stopping stray listener pid $pid on pilot port(s)"
      kill "$pid" 2>/dev/null || true
      killed=1
    fi
  done < <(echo "$pids" | tr ' ' '\n' | sort -u)
  if [[ "$killed" -eq 1 ]]; then
    sleep 0.5
  fi
}

stop_shards() {
  for f in "$PID_DIR"/shard-*.pid; do
    [[ -f "$f" ]] || continue
    pid="$(cat "$f")"
    if kill -0 "$pid" 2>/dev/null; then
      echo "Stopping pid $pid ($(basename "$f" .pid))"
      kill "$pid" 2>/dev/null || true
    fi
    rm -f "$f"
  done
  free_pilot_ports
}

wait_shard_rpc() {
  local url="$1"
  local label="$2"
  for _ in $(seq 1 60); do
    if curl -sf -X POST "$url" -H 'Content-Type: application/json' \
      --data '{"jsonrpc":"2.0","method":"fractal_getShardId","params":[],"id":1}' \
      | grep -q '"result"'; then
      echo "  $label RPC ready at $url"
      return 0
    fi
    sleep 0.25
  done
  echo "ERROR: $label RPC not ready at $url (see ${PID_DIR}/shard-*.log)" >&2
  return 1
}

verify_shards_up() {
  local ok=1
  for id in 0 1; do
    local f="${PID_DIR}/shard-${id}.pid"
    if [[ ! -f "$f" ]]; then
      echo "ERROR: missing ${f}" >&2
      ok=0
      continue
    fi
    local pid
    pid="$(cat "$f")"
    if ! kill -0 "$pid" 2>/dev/null; then
      echo "ERROR: shard $id pid $pid not running — tail ${PID_DIR}/shard-${id}.log" >&2
      ok=0
    fi
  done
  [[ "$ok" -eq 1 ]] || return 1
  wait_shard_rpc "http://127.0.0.1:8545" "shard-0"
  wait_shard_rpc "http://127.0.0.1:8547" "shard-1"
}

build_node() {
  echo "Building fractal-node -> $BINARY"
  (cd "$ROOT" && cargo build -p fractal-node)
}

run_shard() {
  local id="$1"
  local rpc_port="$2"
  local p2p_port="$3"
  local log="${PID_DIR}/shard-${id}.log"
  local db="${PID_DIR}/rocksdb-shard-${id}"
  rm -rf "$db"
  mkdir -p "$db"

  echo "Starting shard $id (RPC :${rpc_port}, P2P udp/${p2p_port}) -> $log"
  env \
    FRACTAL_CONSENSUS_MODE=hyperbft \
    FRACTAL_SHARD_COUNT=2 \
    FRACTAL_SHARD_ID="$id" \
    FRACTAL_TARGET_BLOCK_TIME_MS=70 \
    FRACTAL_ANCHOR_INTERVAL="${FRACTAL_ANCHOR_INTERVAL:-100}" \
    FRACTAL_ASYNC_PROOF=1 \
    FRACTAL_AUTO_VALIDITY_PROOF=1 \
    FRACTAL_RPC_ADDR="127.0.0.1:${rpc_port}" \
    FRACTAL_P2P_LISTEN="/ip4/127.0.0.1/udp/${p2p_port}/quic-v1" \
    FRACTAL_CHAIN_ROCKSDB_PATH="$db" \
    FRACTAL_PROOF_ROCKSDB_PATH="$db" \
    FRACTAL_MASTERCHAIN_RPC="${FRACTAL_MASTERCHAIN_RPC:-}" \
    nohup "$BINARY" >"$log" 2>&1 &
  echo $! >"${PID_DIR}/shard-${id}.pid"
}

start_shards() {
  build_node
  stop_shards
  run_shard 0 8545 9000
  run_shard 1 8547 9002
  verify_shards_up
  echo ""
  echo "Pilot shards running (FRACTAL_ANCHOR_INTERVAL=${FRACTAL_ANCHOR_INTERVAL:-100})."
  echo "  Shard 0 RPC: http://127.0.0.1:8545"
  echo "  Shard 1 RPC: http://127.0.0.1:8547"
  echo "  Smoke:     ./scripts/run-pilot-shards.sh smoke"
  echo "  Stop:      ./scripts/run-pilot-shards.sh stop"
  echo "  Logs:      tail -f ${PID_DIR}/shard-0.log ${PID_DIR}/shard-1.log"
}

cmd="${1:-start}"
case "$cmd" in
  stop)
    stop_shards
    "${ROOT}/scripts/run-masterchain-bft.sh" stop 2>/dev/null || true
    ;;
  start-with-masterchain)
    "${ROOT}/scripts/run-masterchain-bft.sh" start
    export FRACTAL_MASTERCHAIN_RPC="${FRACTAL_MASTERCHAIN_RPC:-http://127.0.0.1:8550}"
    start_shards
    echo "  Masterchain: http://127.0.0.1:8550 (FRACTAL_MASTERCHAIN_RPC set on shards)"
    ;;
  smoke)
    exec "${ROOT}/scripts/smoke-pilot-shards.sh"
    ;;
  smoke-start)
    export FRACTAL_ANCHOR_INTERVAL="${FRACTAL_ANCHOR_INTERVAL:-4}"
    start_shards
    echo "Waiting for HyperBFT + anchor cadence..."
    sleep "${PILOT_PROOF_WAIT_SECS:-8}"
    FRACTAL_ANCHOR_INTERVAL="$FRACTAL_ANCHOR_INTERVAL" \
      "${ROOT}/scripts/smoke-pilot-shards.sh"
    ;;
  start|"")
    start_shards
    ;;
  *)
    echo "Usage: $0 [start|start-with-masterchain|stop|smoke|smoke-start]" >&2
    exit 1
    ;;
esac
