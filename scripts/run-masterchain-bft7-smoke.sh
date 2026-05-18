#!/usr/bin/env bash
# Seven-process dedicated masterchain BFT gossip smoke.
#
# Starts seven `fractal-masterchain` processes on localhost, connects validators
# 1..6 to validator 0 over masterchain gossipsub, submits one shard anchor to
# validator 0, and waits for a 5-of-7 QC formed from gossiped votes.

set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PID_DIR="${ROOT}/.masterchain-bft7-smoke"
TARGET_DIR="${CARGO_TARGET_DIR:-${ROOT}/target}"
BINARY="${TARGET_DIR}/debug/fractal-masterchain"
mkdir -p "$PID_DIR"

stop_all() {
  if compgen -G "${PID_DIR}/*.pid" >/dev/null; then
    for f in "${PID_DIR}"/*.pid; do
      pid="$(cat "$f" 2>/dev/null || true)"
      [[ -n "$pid" ]] && kill "$pid" 2>/dev/null || true
      rm -f "$f"
    done
  fi
  for port in $(seq 8550 8556) $(seq 9300 9306); do
    while read -r p; do
      [[ -n "$p" ]] && kill "$p" 2>/dev/null || true
    done < <(lsof -tiTCP:"$port" -sTCP:LISTEN 2>/dev/null || true)
    while read -r p; do
      [[ -n "$p" ]] && kill "$p" 2>/dev/null || true
    done < <(lsof -tiUDP:"$port" 2>/dev/null || true)
  done
}

if [[ "${1:-smoke}" == "stop" ]]; then
  stop_all
  exit 0
fi

stop_all
rm -f "${PID_DIR}"/*.log
echo "Building fractal-masterchain -> $BINARY"
(cd "$ROOT" && cargo build -p fractal-masterchain)

start_validator() {
  local idx="$1"
  local rpc_port=$((8550 + idx))
  local p2p_port=$((9300 + idx))
  local log="${PID_DIR}/v${idx}.log"
  local bootstrap="${2:-}"
  local db="${PID_DIR}/rocksdb-v${idx}"
  rm -rf "$db"
  mkdir -p "$db"
  echo "Starting masterchain validator ${idx} rpc=:${rpc_port} p2p=:${p2p_port} -> ${log}"
  FRACTAL_VALIDATOR_SET=7 \
  FRACTAL_VALIDATOR_INDEX="$idx" \
  FRACTAL_SHARD_COUNT=2 \
  FRACTAL_MASTERCHAIN_BLOCK_MS=300 \
  FRACTAL_MASTERCHAIN_RPC_ADDR="127.0.0.1:${rpc_port}" \
  FRACTAL_MASTERCHAIN_P2P_LISTEN="/ip4/127.0.0.1/udp/${p2p_port}/quic-v1" \
  FRACTAL_MASTERCHAIN_BOOTSTRAP="$bootstrap" \
  FRACTAL_MASTERCHAIN_ROCKSDB_PATH="$db" \
    "$BINARY" >"$log" 2>&1 &
  echo $! >"${PID_DIR}/v${idx}.pid"
}

wait_rpc() {
  local port="$1"
  for _ in $(seq 1 80); do
    if curl -sf -X POST "http://127.0.0.1:${port}" -H 'Content-Type: application/json' \
      --data '{"jsonrpc":"2.0","method":"fractal_getMasterchainHeight","params":[],"id":1}' \
      | grep -q '"result"'; then
      return 0
    fi
    sleep 0.25
  done
  return 1
}

start_validator 0
wait_rpc 8550 || {
  echo "validator 0 RPC not ready" >&2
  tail -100 "${PID_DIR}/v0.log" >&2 || true
  exit 1
}

BOOTSTRAP=""
for _ in $(seq 1 80); do
  BOOTSTRAP="$(grep -Eo '/ip4/127\.0\.0\.1/udp/[0-9]+/quic-v1/p2p/[A-Za-z0-9]+' "${PID_DIR}/v0.log" | tail -1 || true)"
  [[ -n "$BOOTSTRAP" ]] && break
  sleep 0.25
done
if [[ -z "$BOOTSTRAP" ]]; then
  echo "validator 0 p2p address not found" >&2
  tail -100 "${PID_DIR}/v0.log" >&2 || true
  exit 1
fi

for idx in $(seq 1 6); do
  start_validator "$idx" "$BOOTSTRAP"
  wait_rpc $((8550 + idx)) || {
    echo "validator ${idx} RPC not ready" >&2
    tail -100 "${PID_DIR}/v${idx}.log" >&2 || true
    exit 1
  }
done

sleep 2
curl -sf -X POST "http://127.0.0.1:8550" -H 'Content-Type: application/json' \
  --data '{"jsonrpc":"2.0","method":"fractal_submitShardAnchor","params":[{"shardId":"0x0","blockHeight":"0x4","stateRoot":"0x0101010101010101010101010101010101010101010101010101010101010101","witnessCommitment":"0x0202020202020202020202020202020202020202020202020202020202020202"}],"id":1}' \
  >/dev/null

for _ in $(seq 1 80); do
  if grep -q 'formed QC height=1' "${PID_DIR}/v0.log"; then
    echo "masterchain-bft7-smoke: PASS"
    echo "  bootstrap=${BOOTSTRAP}"
    echo "  logs=${PID_DIR}"
    exit 0
  fi
  sleep 0.25
done

echo "masterchain-bft7-smoke: FAIL waiting for QC" >&2
tail -150 "${PID_DIR}/v0.log" >&2 || true
exit 1
