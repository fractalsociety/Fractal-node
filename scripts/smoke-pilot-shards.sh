#!/usr/bin/env bash
# Automated two-shard smoke (run after ./scripts/run-pilot-shards.sh or via smoke subcommand).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PID_DIR="${ROOT}/.pilot-shards"

SHARD0_RPC="${SHARD0_RPC:-http://127.0.0.1:8545}"
SHARD1_RPC="${SHARD1_RPC:-http://127.0.0.1:8547}"
MIN_HEIGHT="${PILOT_SMOKE_MIN_HEIGHT:-8}"
ANCHOR_INTERVAL="${FRACTAL_ANCHOR_INTERVAL:-4}"
EXPECT_ZK=1
if [[ "$ANCHOR_INTERVAL" -gt 16 ]]; then
  EXPECT_ZK=0
fi

fail() {
  echo "smoke-pilot-shards: FAIL: $*" >&2
  exit 1
}

rpc_raw() {
  local url="$1"
  local method="$2"
  local params="${3:-[]}"
  curl -sf -X POST "$url" -H 'Content-Type: application/json' \
    --data "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":$params,\"id\":1}"
}

rpc_result() {
  local body="$1"
  if echo "$body" | grep -q '"error"'; then
    echo ""
    return 1
  fi
  # Quoted hex/string (portable sed — matches smoke-track-b-e2e.sh).
  local r
  r="$(echo "$body" | sed -n 's/.*"result":"\([^"]*\)".*/\1/p' | head -1)"
  if [[ -n "$r" ]]; then
    echo "$r"
    return 0
  fi
  # Bare JSON number/bool.
  r="$(echo "$body" | sed -n 's/.*"result":\([^,}]*\).*/\1/p' | head -1)"
  if [[ -n "$r" ]]; then
    echo "$r"
    return 0
  fi
  echo ""
  return 1
}

hex_u64() {
  local s="$1"
  s="${s#\"}"
  s="${s%\"}"
  s="${s#0x}"
  echo $((16#${s:-0}))
}

wait_height() {
  local url="$1"
  local label="$2"
  local min="$3"
  local h=0
  for _ in $(seq 1 90); do
    local body
    body="$(rpc_raw "$url" eth_blockNumber)" || fail "$label RPC unreachable at $url"
    local r
    r="$(rpc_result "$body")" || fail "$label eth_blockNumber error: $body"
    h="$(hex_u64 "$r")"
    if [[ "$h" -ge "$min" ]]; then
      echo "  $label height=$h (ok)"
      return 0
    fi
    sleep 0.5
  done
  fail "$label stuck below height $min (last=$h)"
}

check_shard() {
  local url="$1"
  local expect_id="$2"
  local label="shard-$expect_id"

  echo "== $label @ $url =="

  local body id count mode
  body="$(rpc_raw "$url" fractal_getShardId)" || fail "$label RPC down"
  id="$(rpc_result "$body")" || fail "$label fractal_getShardId: $body"
  id_hex="$(hex_u64 "$id")"
  [[ "$id_hex" -eq "$expect_id" ]] || fail "$label shardId=$id_hex want $expect_id"

  body="$(rpc_raw "$url" fractal_getShardCount)" || fail "$label RPC down"
  count="$(rpc_result "$body")" || fail "$label fractal_getShardCount: $body"
  [[ "$(hex_u64 "$count")" -eq 2 ]] || fail "$label shardCount != 2"

  body="$(rpc_raw "$url" fractal_getConsensusMode)" || fail "$label RPC down"
  mode="$(rpc_result "$body")" || fail "$label fractal_getConsensusMode: $body"
  mode="${mode#\"}"
  mode="${mode%\"}"
  [[ "$mode" == "hyperbft" ]] || fail "$label consensus=$mode want hyperbft"

  wait_height "$url" "$label" "$MIN_HEIGHT"

  body="$(rpc_raw "$url" fractal_getCheckpointProofDigest '["0x1"]')" || true
  if echo "$body" | grep -q '"result"'; then
    echo "  $label checkpoint digest height=1 ok"
  else
    echo "  $label checkpoint digest height=1 (pending — STWO may still be running)"
  fi

  if [[ "$EXPECT_ZK" -eq 1 ]]; then
    body="$(rpc_raw "$url" fractal_getGlobalZkRoot)" || fail "$label fractal_getGlobalZkRoot failed"
    if echo "$body" | grep -q '"result"'; then
      gz="$(rpc_result "$body")" || fail "$label no globalZkRoot yet (anchor_interval=$ANCHOR_INTERVAL): $body"
      gz_hex="${gz#\"}"
      gz_hex="${gz_hex%\"}"
      gz_hex="${gz_hex#0x}"
      [[ "$gz_hex" != "$(printf '0%.0s' {1..64})" ]] || fail "$label globalZkRoot is zero"
      echo "  $label globalZkRoot=${gz:0:18}... ok"

      body="$(rpc_raw "$url" fractal_getGlobalZkProof)" || fail "$label fractal_getGlobalZkProof failed"
      echo "$body" | grep -q '"snarkBytes"' || fail "$label missing Plonky2 bundle"
      echo "  $label Plonky2 bundle ok"
    else
      body="$(rpc_raw "$url" fractal_proofMetrics)" || fail "$label fractal_proofMetrics failed"
      accepted="$(echo "$body" | sed -n 's/.*"proofsAccepted":"0x\([0-9a-fA-F]*\)".*/\1/p' | head -1)"
      [[ -n "$accepted" ]] || fail "$label missing proof metrics: $body"
      [[ $((16#${accepted:-0})) -gt 0 ]] || fail "$label proof worker has not accepted proofs yet: $body"
      echo "  $label proofMetrics proofsAccepted=0x$accepted ok"
    fi
  else
    echo "  $label ZK checks skipped (FRACTAL_ANCHOR_INTERVAL=$ANCHOR_INTERVAL > 16)"
  fi
}

shard_pids_running() {
  for id in 0 1; do
    local f="${PID_DIR}/shard-${id}.pid"
    [[ -f "$f" ]] || return 1
    local pid
    pid="$(cat "$f")"
    kill -0 "$pid" 2>/dev/null || return 1
  done
  return 0
}

if ! shard_pids_running; then
  fail "pilot shards not running — start with: ./scripts/run-pilot-shards.sh (or: ./scripts/run-pilot-shards.sh smoke-start)"
fi

echo "Pilot two-shard smoke (anchor_interval=$ANCHOR_INTERVAL expect_zk=$EXPECT_ZK)"
check_shard "$SHARD0_RPC" 0
check_shard "$SHARD1_RPC" 1

echo ""
echo "smoke-pilot-shards: PASS (both shards healthy)"
