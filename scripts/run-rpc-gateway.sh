#!/usr/bin/env bash
# Fronting JSON-RPC gateway for Track B pilot shards.
#
# Env:
#   FRACTAL_GATEWAY_ADDR      bind address (default 127.0.0.1:8549)
#   FRACTAL_GATEWAY_SHARDS    comma list: 0=http://127.0.0.1:8545,1=http://127.0.0.1:8547
#   FRACTAL_SHARD_RPC_URLS    alternate comma list without ids; index = shard id

set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

cd "$ROOT"
exec cargo run -p fractal-rpc --bin fractal-rpc-gateway
