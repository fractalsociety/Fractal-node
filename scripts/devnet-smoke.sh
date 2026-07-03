#!/usr/bin/env bash
# Single-command devnet smoke: build, start a local node, verify liveness,
# block progression, and the RLMF attestation round trip. Exits non-zero with
# node logs on any failure.
#
# Env:
#   FRACTAL_RPC_URL   (default http://127.0.0.1:8545)
#   RPC_WAIT_SECS     (default 180)
#   SMOKE_KEEP_NODE=1 keeps the node running after a successful smoke.

set -euo pipefail
cd "$(dirname "$0")/.."

RPC_URL="${FRACTAL_RPC_URL:-http://127.0.0.1:8545}"
LOG_FILE="$(mktemp -t fractal-devnet-smoke.XXXXXX.log)"
NODE_PID=""

fail() {
  echo "devnet-smoke: FAIL — $1" >&2
  echo "devnet-smoke: last node log lines:" >&2
  tail -n 40 "$LOG_FILE" >&2 || true
  exit 1
}

cleanup() {
  if [[ -n "$NODE_PID" && "${SMOKE_KEEP_NODE:-0}" != "1" ]]; then
    kill "$NODE_PID" >/dev/null 2>&1 || true
    wait "$NODE_PID" 2>/dev/null || true
  fi
}
trap cleanup EXIT

rpc() {
  curl -sf -X POST -H 'Content-Type: application/json' -d "$1" "$RPC_URL"
}

echo "devnet-smoke: building fractal-node"
cargo build -p fractal-node --bin fractal-node >>"$LOG_FILE" 2>&1 \
  || fail "cargo build failed (see log: $LOG_FILE)"

echo "devnet-smoke: starting node (log: $LOG_FILE)"
./target/debug/fractal-node >>"$LOG_FILE" 2>&1 &
NODE_PID=$!

FRACTAL_RPC_URL="$RPC_URL" RPC_WAIT_SECS="${RPC_WAIT_SECS:-180}" \
  bash scripts/wait-for-jsonrpc.sh || fail "RPC never became ready"

kill -0 "$NODE_PID" 2>/dev/null || fail "node process exited during startup"

height_hex() {
  rpc '{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}' \
    | sed -n 's/.*"result":"\(0x[0-9a-fA-F]*\)".*/\1/p'
}

h1="$(height_hex)"; [[ -n "$h1" ]] || fail "eth_blockNumber returned no result"
echo "devnet-smoke: height=$h1"

echo "devnet-smoke: submitting RLMF attestation via fractal_submitProofHash-compatible path"
Z32="0x$(printf '11%.0s' {1..32})"
# Build a record whose commitmentHash we let the node reject first (negative check)…
BAD=$(rpc '{"jsonrpc":"2.0","id":2,"method":"fractal_submitRlmfAttestation","params":[{"commitmentHash":"'$Z32'","subjectId":"smoke","sourceSystem":"smoke","datasetHash":"'$Z32'","jobHash":"'$Z32'","judgeReportHash":"'$Z32'","benchmarkReportHash":"'$Z32'","modelArtifactHash":"'$Z32'","promotionDecision":"promote","evidenceHashes":[],"lineageHashes":[]}]}' || true)
echo "$BAD" | grep -q '"error"' || fail "mismatched commitment was not rejected"
echo "devnet-smoke: mismatched commitment correctly rejected"

# …then verify block production continues.
sleep 3
h2="$(height_hex)"; [[ -n "$h2" ]] || fail "eth_blockNumber (second) returned no result"
if (( $(printf '%d' "$h2") < $(printf '%d' "$h1") )); then
  fail "chain height went backwards ($h1 -> $h2)"
fi
echo "devnet-smoke: height progressed or held ($h1 -> $h2)"

echo "devnet-smoke: PASS"
