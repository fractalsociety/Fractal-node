#!/usr/bin/env bash
# Block until JSON-RPC responds to eth_blockNumber (or exit 1 after timeout).
# Env: FRACTAL_RPC_URL (default http://127.0.0.1:8545), RPC_WAIT_SECS (default 180).

set -euo pipefail
RPC_URL="${FRACTAL_RPC_URL:-http://127.0.0.1:8545}"
MAX="${RPC_WAIT_SECS:-180}"
BODY='{"jsonrpc":"2.0","id":1,"method":"eth_blockNumber","params":[]}'

if ! command -v curl >/dev/null 2>&1; then
  echo "wait-for-jsonrpc: curl is required" >&2
  exit 1
fi

for ((i = 1; i <= MAX; i++)); do
  if curl -sf -X POST -H 'Content-Type: application/json' -d "$BODY" "$RPC_URL" | grep -q '"result"'; then
    echo "wait-for-jsonrpc: ok after ${i}s ($RPC_URL)"
    exit 0
  fi
  sleep 1
done

echo "wait-for-jsonrpc: timed out after ${MAX}s ($RPC_URL)" >&2
exit 1
