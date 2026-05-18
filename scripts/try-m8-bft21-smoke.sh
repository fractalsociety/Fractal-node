#!/usr/bin/env bash
# PRD M8 dev slice: BFT-21 fixture keys + consensus quorum smoke (no 21-process compose).
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "try-m8-bft21-smoke: fractal-consensus BFT-21 tests"
cargo test -p fractal-consensus bft21 -- --nocapture
cargo test -p fractal-consensus thirteen_of_twenty_one -- --nocapture

echo ""
echo "try-m8-bft21-smoke: fractal-node BFT-21 onboarding report (first 40 lines)"
FRACTAL_VALIDATOR_SET=bft21 cargo run -p fractal-node -- print-devnet-validator-keys 2>/dev/null | head -40

echo ""
echo "try-m8-bft21-smoke: node integration (BFT-21 validator count)"
cargo test -p fractal-node devnet_with_bft21_fixture -- --nocapture

echo ""
echo "try-m8-bft21-smoke: ok (on-chain snapshot fast sync is not implemented; see docs/devnet.md §M8)"
