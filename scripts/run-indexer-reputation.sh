#!/usr/bin/env bash
# Poll devnet and mirror governance WalletReputationSnapshotV1 into a JSON store (snapshot-only by default).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export INDEXER_RPC_URL="${INDEXER_RPC_URL:-http://127.0.0.1:8545}"
export INDEXER_POLL_MS="${INDEXER_POLL_MS:-3000}"
export INDEXER_REPUTATION_STORE_PATH="${INDEXER_REPUTATION_STORE_PATH:-$ROOT/target/indexer_reputation.json}"
export INDEXER_REPUTATION_MERGE_SETTLEMENTS="${INDEXER_REPUTATION_MERGE_SETTLEMENTS:-0}"
export INDEXER_JSON_LOG="${INDEXER_JSON_LOG:-1}"
cd "$ROOT"
exec cargo run -p fractal-indexer-stub
