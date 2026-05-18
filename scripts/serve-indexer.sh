#!/usr/bin/env bash
# Serve fractal-indexer GraphQL (PRD §14.4). Requires node RPC at INDEXER_RPC_URL.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
export INDEXER_RPC_URL="${INDEXER_RPC_URL:-http://127.0.0.1:8545}"
export INDEXER_DB_PATH="${INDEXER_DB_PATH:-$ROOT/target/fractal_indexer.db}"
export INDEXER_GRAPHQL_BIND="${INDEXER_GRAPHQL_BIND:-0.0.0.0:8088}"

# Avoid "Address already in use" when an old indexer or another app still holds the port.
_port="${INDEXER_GRAPHQL_BIND##*:}"
if pids="$(lsof -ti TCP:"${_port}" -sTCP:LISTEN 2>/dev/null)"; then
  echo "serve-indexer: freeing TCP ${_port} (listener PIDs: $(echo "$pids" | tr '\n' ' '))"
  kill $pids 2>/dev/null || true
  sleep 0.4
fi

echo "serve-indexer: bind ${INDEXER_GRAPHQL_BIND}  (open http://127.0.0.1:${_port}/graphiql)"
echo "serve-indexer: reputation: Settle* merge ON by default (INDEXER_REPUTATION_MERGE_SETTLEMENTS=0 to disable)"
echo "serve-indexer: test  curl -sS http://127.0.0.1:${_port}/health"
echo "serve-indexer: test  curl -sS http://127.0.0.1:${_port}/graphql -H 'Content-Type: application/json' -d '{\"query\":\"{ indexerStatus { lastIndexedBlock } }\"}'"

exec cargo run -p fractal-indexer
