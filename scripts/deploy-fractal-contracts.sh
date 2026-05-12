#!/usr/bin/env bash
# One-command Hardhat deploy against a local fractal-node (PRD M4).
# Prerequisite: `cargo run -p fractal-node` (or equivalent) with JSON-RPC on FRACTAL_RPC_URL (default http://127.0.0.1:8545).
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT/contracts"
if [[ -f package-lock.json ]]; then
  npm ci
else
  npm install
fi
npm run compile
npm run deploy
