#!/usr/bin/env bash
# Quick check that fractal-indexer is responding (avoids broken line wraps in manual curl).
set -euo pipefail
HOST="${1:-127.0.0.1}"
PORT="${2:-8088}"
BASE="http://${HOST}:${PORT}"
echo "check-indexer: GET ${BASE}/health"
curl -sS -f "${BASE}/health" | tee /dev/stderr | grep -q ok || { echo "check-indexer: health failed"; exit 1; }
echo
echo "check-indexer: POST ${BASE}/graphql"
curl -sS -f "${BASE}/graphql" \
  -H 'Content-Type: application/json' \
  -d '{"query":"{ indexerStatus { lastIndexedBlock txCount chainRpcUrl } }"}'
echo
echo "check-indexer: GET ${BASE}/api/v1/explorer/status"
curl -sS -f "${BASE}/api/v1/explorer/status"
echo
echo "check-indexer: ok"
