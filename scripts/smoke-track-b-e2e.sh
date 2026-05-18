#!/usr/bin/env bash
# Smoke: shard RPC + masterchain ZK pipeline (requires ./scripts/run-track-b-lab.sh).
set -euo pipefail

RPC="${FRACTAL_RPC_URL:-http://127.0.0.1:8545}"

rpc() {
  local method="$1"
  local params="${2:-[]}"
  curl -sf -X POST "$RPC" -H 'Content-Type: application/json' \
    --data "{\"jsonrpc\":\"2.0\",\"method\":\"$method\",\"params\":$params,\"id\":1}"
}

echo "== fractal_getShardId =="
rpc fractal_getShardId

echo ""
echo "== fractal_getConsensusMode =="
rpc fractal_getConsensusMode

echo ""
echo "== eth_blockNumber (wait for height >= 4) =="
for _ in $(seq 1 60); do
  bn="$(rpc eth_blockNumber | sed -n 's/.*"result":"\(0x[^"]*\)".*/\1/p')"
  h=$((bn))
  echo "  height=$h"
  if [[ "$h" -ge 4 ]]; then
    break
  fi
  sleep 0.5
done

echo ""
echo "== fractal_getCheckpointProofDigest height 1 =="
rpc fractal_getCheckpointProofDigest '["0x1"]' || true

echo ""
echo "== fractal_getMasterchainHead =="
rpc fractal_getMasterchainHead || echo "(no head yet — wait for anchor at block 4)"

echo ""
echo "== fractal_getGlobalZkRoot =="
rpc fractal_getGlobalZkRoot || echo "(no globalZkRoot yet — need STWO + anchor)"

echo ""
echo "== fractal_getGlobalZkProof =="
rpc fractal_getGlobalZkProof || echo "(no Plonky2 bundle yet)"

echo ""
echo "Done. If globalZkRoot is missing, tail .track-b-lab/node.log for STWO / tier1 lines."
