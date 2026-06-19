#!/usr/bin/env bash
# Run one Track B shard with seven HyperBFT validator processes.
#
# Usage:
#   ./scripts/run-hyperbft-bft7-shard.sh start
#   ./scripts/run-hyperbft-bft7-shard.sh smoke
#   ./scripts/run-hyperbft-bft7-shard.sh smoke-start
#   ./scripts/run-hyperbft-bft7-shard.sh stop

set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PID_DIR="${ROOT}/.hyperbft-bft7-shard"
mkdir -p "$PID_DIR"

TARGET_DIR="${CARGO_TARGET_DIR:-${ROOT}/target}"
BINARY="${TARGET_DIR}/debug/fractal-node"
VALIDATORS="${HYPERBFT_VALIDATORS:-7}"
BASE_RPC="${HYPERBFT_BASE_RPC_PORT:-8650}"
BASE_P2P="${HYPERBFT_BASE_P2P_PORT:-9200}"
SHARD_ID="${FRACTAL_SHARD_ID:-0}"
SHARD_COUNT="${FRACTAL_SHARD_COUNT:-2}"
MIN_HEIGHT="${HYPERBFT_SMOKE_MIN_HEIGHT:-2}"

build_node() {
  echo "Building fractal-node -> $BINARY"
  (cd "$ROOT" && cargo build -p fractal-node)
}

stop_nodes() {
  for f in "$PID_DIR"/validator-*.pid; do
    [[ -f "$f" ]] || continue
    pid="$(cat "$f")"
    if kill -0 "$pid" 2>/dev/null; then
      echo "Stopping pid $pid ($(basename "$f" .pid))"
      kill "$pid" 2>/dev/null || true
    fi
    rm -f "$f"
  done
}

wait_bootstrap() {
  local log="$1"
  for _ in $(seq 1 80); do
    local line
    line="$(grep -m1 'FRACTAL_BOOTSTRAP=' "$log" 2>/dev/null || true)"
    if [[ -n "$line" ]]; then
      echo "${line##*FRACTAL_BOOTSTRAP=}"
      return 0
    fi
    sleep 0.25
  done
  echo "ERROR: validator-0 bootstrap not found in $log" >&2
  return 1
}

wait_rpc_height() {
  local port="$1"
  local label="$2"
  local last=0
  for _ in $(seq 1 120); do
    local body
    body="$(curl -sf -X POST "http://127.0.0.1:${port}" -H 'Content-Type: application/json' \
      --data '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' || true)"
    local h
    h="$(echo "$body" | sed -n 's/.*"result":"0x\([0-9a-fA-F]*\)".*/\1/p' | head -1)"
    if [[ -n "$h" ]]; then
      last=$((16#${h:-0}))
      if [[ "$last" -ge "$MIN_HEIGHT" ]]; then
        echo "  $label height=$last"
        return 0
      fi
    fi
    sleep 0.5
  done
  echo "ERROR: $label below height $MIN_HEIGHT (last=$last)" >&2
  return 1
}

run_validator() {
  local idx="$1"
  local bootstrap="${2:-}"
  local rpc_port=$((BASE_RPC + idx))
  local p2p_port=$((BASE_P2P + idx))
  local log="${PID_DIR}/validator-${idx}.log"
  local db="${PID_DIR}/rocksdb-validator-${idx}"
  local identity="${PID_DIR}/validator-${idx}.identity"
  rm -rf "$db"
  mkdir -p "$db"

  echo "Starting validator $idx (RPC :${rpc_port}, P2P udp/${p2p_port}) -> $log"
  local -a cmd=(
    env
    FRACTAL_VALIDATOR_SET=7
    FRACTAL_VALIDATOR_INDEX="$idx"
    FRACTAL_CONSENSUS_MODE=hyperbft
    FRACTAL_SHARD_COUNT="$SHARD_COUNT"
    FRACTAL_SHARD_ID="$SHARD_ID"
    FRACTAL_TARGET_BLOCK_TIME_MS="${FRACTAL_TARGET_BLOCK_TIME_MS:-70}"
    FRACTAL_PACEMAKER_BASE_MS="${FRACTAL_PACEMAKER_BASE_MS:-1000}"
    FRACTAL_FAST_SYNC="${FRACTAL_FAST_SYNC:-0}"
    FRACTAL_ASYNC_PROOF="${FRACTAL_ASYNC_PROOF:-0}"
    FRACTAL_DEV_INJECT_QUORUM="${FRACTAL_DEV_INJECT_QUORUM:-1}"
    FRACTAL_RPC_ADDR="127.0.0.1:${rpc_port}"
    FRACTAL_P2P_LISTEN="/ip4/127.0.0.1/udp/${p2p_port}/quic-v1"
    FRACTAL_P2P_IDENTITY_PATH="$identity"
    FRACTAL_CHAIN_ROCKSDB_PATH="$db"
    FRACTAL_PROOF_ROCKSDB_PATH="$db"
  )
  if [[ -n "$bootstrap" ]]; then
    cmd+=(FRACTAL_BOOTSTRAP="$bootstrap")
  fi
  "${cmd[@]}" "$BINARY" >"$log" 2>&1 &
  echo $! >"${PID_DIR}/validator-${idx}.pid"
}

start_nodes() {
  build_node
  stop_nodes
  run_validator 0 ""
  local bootstrap
  bootstrap="$(wait_bootstrap "${PID_DIR}/validator-0.log")"
  for idx in $(seq 1 $((VALIDATORS - 1))); do
    run_validator "$idx" "$bootstrap"
  done
  echo ""
  echo "HyperBFT BFT-7 shard running."
  echo "  Shard: ${SHARD_ID}/${SHARD_COUNT}"
  echo "  RPC ports: ${BASE_RPC}..$((BASE_RPC + VALIDATORS - 1))"
  echo "  Bootstrap: $bootstrap"
  echo "  Smoke: ./scripts/run-hyperbft-bft7-shard.sh smoke"
  echo "  Stop:  ./scripts/run-hyperbft-bft7-shard.sh stop"
}

rpc_height() {
  local port="$1"
  local body
  body="$(curl -sf -X POST "http://127.0.0.1:${port}" -H 'Content-Type: application/json' \
    --data '{"jsonrpc":"2.0","method":"eth_blockNumber","params":[],"id":1}' || true)"
  local h
  h="$(echo "$body" | sed -n 's/.*"result":"0x\([0-9a-fA-F]*\)".*/\1/p' | head -1)"
  echo $((16#${h:-0}))
}

rpc_call() {
  local port="$1"
  local method="$2"
  curl -sf -X POST "http://127.0.0.1:${port}" -H 'Content-Type: application/json' \
    --data "{\"jsonrpc\":\"2.0\",\"method\":\"${method}\",\"params\":[],\"id\":1}" || true
}

capture_consensus_diagnostics() {
  local reason="$1"
  local stamp
  stamp="$(date +%Y%m%d-%H%M%S)"
  local out="${PID_DIR}/consensus-diagnostics-${stamp}.log"
  {
    echo "hyperbft-bft7-shard diagnostics"
    echo "reason=${reason}"
    echo "time=${stamp}"
    echo "validators=${VALIDATORS}"
    echo "shard=${SHARD_ID}/${SHARD_COUNT}"
    echo "min_height=${MIN_HEIGHT}"
    echo ""
    for idx in $(seq 0 $((VALIDATORS - 1))); do
      local port=$((BASE_RPC + idx))
      local log="${PID_DIR}/validator-${idx}.log"
      echo "===== validator-${idx} rpc eth_blockNumber ====="
      rpc_call "$port" "eth_blockNumber"
      echo ""
      echo "===== validator-${idx} rpc fractal_consensusDiagnostics ====="
      rpc_call "$port" "fractal_consensusDiagnostics"
      echo ""
      echo "===== validator-${idx} full log ====="
      if [[ -f "$log" ]]; then
        cat "$log"
      else
        echo "missing log: $log"
      fi
      echo ""
    done
  } >"$out"
  echo "Diagnostics captured: $out" >&2
}

smoke_nodes() {
  for idx in $(seq 0 $((VALIDATORS - 1))); do
    local f="${PID_DIR}/validator-${idx}.pid"
    [[ -f "$f" ]] || { echo "ERROR: missing $f" >&2; capture_consensus_diagnostics "missing-pid-${idx}"; return 1; }
    local pid
    pid="$(cat "$f")"
    kill -0 "$pid" 2>/dev/null || { echo "ERROR: validator $idx not running" >&2; capture_consensus_diagnostics "validator-${idx}-not-running"; return 1; }
  done
  local max_h=0
  for _ in $(seq 1 120); do
    max_h=0
    for idx in $(seq 0 $((VALIDATORS - 1))); do
      local h
      h="$(rpc_height "$((BASE_RPC + idx))")"
      if [[ "$h" -gt "$max_h" ]]; then
        max_h="$h"
      fi
    done
    if [[ "$max_h" -ge "$MIN_HEIGHT" ]]; then
      echo "  cluster max_height=$max_h (need >= $MIN_HEIGHT)"
      echo "hyperbft-bft7-shard: PASS"
      return 0
    fi
    sleep 0.5
  done
  echo "ERROR: cluster max_height=$max_h below $MIN_HEIGHT after 60s" >&2
  capture_consensus_diagnostics "height-timeout-max-${max_h}"
  return 1
}

cmd="${1:-start}"
case "$cmd" in
  start|"")
    start_nodes
    ;;
  stop)
    stop_nodes
    ;;
  smoke)
    smoke_nodes
    ;;
  smoke-start)
    start_nodes
    smoke_nodes
    ;;
  *)
    echo "Usage: $0 [start|stop|smoke|smoke-start]" >&2
    exit 1
    ;;
esac
