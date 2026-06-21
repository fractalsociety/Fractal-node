#!/usr/bin/env bash
# Compare the current FractalChain baseline against the fractalchain2 experiment.
#
# This is the first benchmark harness for docs/proof-ingestion-decoupling-prd.md:
# "Add benchmark scripts that can run baseline and experiment side by side."
#
# Defaults:
#   baseline repo:   /Users/jamesstar/fractalchain
#   experiment repo: this repo
#   output dir:      ./bench-results/proof-ingestion-compare-<timestamp>
#
# Useful env:
#   BASELINE_ROOT=/path/to/fractalchain
#   EXPERIMENT_ROOT=/path/to/fractalchain2
#   BENCH_DURATION_SECS=30
#   BENCH_WARMUP_SECS=3
#   BENCH_WORKERS=2
#   BENCH_SUBMIT_PAUSE_US=100000
#   BENCH_SKIP_BUILD=1

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
EXPERIMENT_ROOT="${EXPERIMENT_ROOT:-$(cd "${SCRIPT_DIR}/.." && pwd)}"
BASELINE_ROOT="${BASELINE_ROOT:-$(cd "${EXPERIMENT_ROOT}/.." && pwd)/fractalchain}"

STAMP="$(date -u +%Y%m%dT%H%M%SZ)"
OUT_DIR="${OUT_DIR:-${EXPERIMENT_ROOT}/bench-results/proof-ingestion-compare-${STAMP}}"

BENCH_DURATION_SECS="${BENCH_DURATION_SECS:-30}"
BENCH_WARMUP_SECS="${BENCH_WARMUP_SECS:-3}"
BENCH_WORKERS="${BENCH_WORKERS:-2}"
BENCH_SUBMIT_PAUSE_US="${BENCH_SUBMIT_PAUSE_US:-100000}"
BENCH_SKIP_BUILD="${BENCH_SKIP_BUILD:-0}"

mkdir -p "$OUT_DIR"

die() {
  echo "error: $*" >&2
  exit 1
}

require_repo() {
  local root="$1"
  [[ -f "${root}/Cargo.toml" ]] || die "missing Cargo.toml in ${root}"
  [[ -x "${root}/scripts/wait-for-jsonrpc.sh" ]] || die "missing wait-for-jsonrpc.sh in ${root}"
  [[ -f "${root}/tools/load-tps/Cargo.toml" ]] || die "missing fractal-load-tps in ${root}"
}

stop_pid() {
  local pid_file="$1"
  if [[ -f "$pid_file" ]]; then
    local pid
    pid="$(cat "$pid_file")"
    if kill -0 "$pid" 2>/dev/null; then
      kill "$pid" 2>/dev/null || true
      for _ in $(seq 1 20); do
        if ! kill -0 "$pid" 2>/dev/null; then
          break
        fi
        sleep 0.2
      done
      if kill -0 "$pid" 2>/dev/null; then
        kill -9 "$pid" 2>/dev/null || true
      fi
    fi
    rm -f "$pid_file"
  fi
}

json_number_or_null() {
  local value="${1:-}"
  if [[ "$value" =~ ^-?[0-9]+([.][0-9]+)?$ ]]; then
    printf '%s' "$value"
  else
    printf 'null'
  fi
}

parse_load_output_to_json() {
  local label="$1"
  local repo_root="$2"
  local rpc_url="$3"
  local raw_log="$4"
  local json_out="$5"

  local submitted errors included confirmed submit_tps chain_tps nonce_tps block_rate avg_tx_per_block blocks_measured
  submitted="$(awk -F: '/submitted \(rpc\):/ {gsub(/[[:space:]]/, "", $2); print $2}' "$raw_log" | tail -n 1)"
  errors="$(awk -F: '/submit errors:/ {gsub(/[[:space:]]/, "", $2); print $2}' "$raw_log" | tail -n 1)"
  included="$(awk '/included in chain:/ {print $4}' "$raw_log" | tail -n 1)"
  confirmed="$(awk '/confirmed \(nonce\):/ {print $3}' "$raw_log" | tail -n 1)"
  submit_tps="$(awk -F: '/submit TPS:/ {gsub(/[[:space:]]/, "", $2); print $2}' "$raw_log" | tail -n 1)"
  chain_tps="$(awk -F: '/confirmed chain TPS:/ {gsub(/[[:space:]].*/, "", $2); print $2}' "$raw_log" | tail -n 1)"
  nonce_tps="$(awk -F: '/confirmed nonce TPS:/ {gsub(/[[:space:]].*/, "", $2); print $2}' "$raw_log" | tail -n 1)"
  block_rate="$(awk -F: '/block rate:/ {gsub(/[[:space:]].*/, "", $2); print $2}' "$raw_log" | tail -n 1)"
  avg_tx_per_block="$(awk -F: '/avg tx\/block:/ {gsub(/[[:space:]]/, "", $2); print $2}' "$raw_log" | tail -n 1)"
  blocks_measured="$(awk '/chain heights:/ {gsub(/[()]/, ""); print $6}' "$raw_log" | tail -n 1)"

  cat >"$json_out" <<JSON
{
  "label": "${label}",
  "repo_root": "${repo_root}",
  "rpc_url": "${rpc_url}",
  "duration_secs": $(json_number_or_null "$BENCH_DURATION_SECS"),
  "warmup_secs": $(json_number_or_null "$BENCH_WARMUP_SECS"),
  "workers": $(json_number_or_null "$BENCH_WORKERS"),
  "submit_pause_us": $(json_number_or_null "$BENCH_SUBMIT_PAUSE_US"),
  "submitted_rpc": $(json_number_or_null "$submitted"),
  "submit_errors": $(json_number_or_null "$errors"),
  "included_chain_txs": $(json_number_or_null "$included"),
  "confirmed_nonce_txs": $(json_number_or_null "$confirmed"),
  "submit_tps": $(json_number_or_null "$submit_tps"),
  "confirmed_chain_tps": $(json_number_or_null "$chain_tps"),
  "confirmed_nonce_tps": $(json_number_or_null "$nonce_tps"),
  "block_rate": $(json_number_or_null "$block_rate"),
  "avg_tx_per_block": $(json_number_or_null "$avg_tx_per_block"),
  "blocks_measured": $(json_number_or_null "$blocks_measured")
}
JSON
}

json_get_number() {
  local key="$1"
  local file="$2"
  awk -v k="\"${key}\"" '
    index($0, k) {
      sub(/^.*: /, "", $0)
      sub(/,?[[:space:]]*$/, "", $0)
      print $0
      exit
    }
  ' "$file"
}

start_node() {
  local label="$1"
  local repo_root="$2"
  local rpc_port="$3"
  local p2p_port="$4"
  local run_dir="$5"

  mkdir -p "$run_dir"
  rm -rf "${run_dir}/rocksdb"
  mkdir -p "${run_dir}/rocksdb"

  local target_dir="${run_dir}/target"
  local node_bin="${target_dir}/debug/fractal-node"
  local node_log="${run_dir}/node.log"
  local pid_file="${run_dir}/node.pid"

  if [[ "$BENCH_SKIP_BUILD" != "1" ]]; then
    echo "[$label] building fractal-node"
    (cd "$repo_root" && CARGO_TARGET_DIR="$target_dir" cargo build -q -p fractal-node)
  fi

  [[ -x "$node_bin" ]] || die "missing node binary ${node_bin}; unset BENCH_SKIP_BUILD or build first"

  echo "[$label] starting node RPC=127.0.0.1:${rpc_port} P2P=${p2p_port}"
  env \
    FRACTAL_CONSENSUS_MODE=hyperbft \
    FRACTAL_DEV_INJECT_QUORUM=1 \
    FRACTAL_SHARD_COUNT=1 \
    FRACTAL_SHARD_ID=0 \
    FRACTAL_TARGET_BLOCK_TIME_MS=70 \
    FRACTAL_ANCHOR_INTERVAL=4 \
    FRACTAL_ASYNC_PROOF=1 \
    FRACTAL_AUTO_VALIDITY_PROOF=1 \
    FRACTAL_RPC_ADDR="127.0.0.1:${rpc_port}" \
    FRACTAL_P2P_LISTEN="/ip4/127.0.0.1/udp/${p2p_port}/quic-v1" \
    FRACTAL_CHAIN_ROCKSDB_PATH="${run_dir}/rocksdb" \
    FRACTAL_PROOF_ROCKSDB_PATH="${run_dir}/rocksdb" \
    nohup "$node_bin" >"$node_log" 2>&1 &
  echo $! >"$pid_file"

  FRACTAL_RPC_URL="http://127.0.0.1:${rpc_port}" \
    RPC_WAIT_SECS=45 \
    "${repo_root}/scripts/wait-for-jsonrpc.sh" >/dev/null
}

run_load() {
  local label="$1"
  local repo_root="$2"
  local rpc_port="$3"
  local run_dir="$4"

  local target_dir="${run_dir}/target"
  local raw_log="${run_dir}/load-tps.log"
  local json_out="${run_dir}/summary.json"
  local rpc_url="http://127.0.0.1:${rpc_port}"

  if [[ "$BENCH_SKIP_BUILD" != "1" ]]; then
    echo "[$label] building fractal-load-tps"
    (cd "$repo_root" && CARGO_TARGET_DIR="$target_dir" cargo build -q -p fractal-load-tps)
  fi

  echo "[$label] running load duration=${BENCH_DURATION_SECS}s warmup=${BENCH_WARMUP_SECS}s workers=${BENCH_WORKERS}"
  (
    cd "$repo_root"
    CARGO_TARGET_DIR="$target_dir" \
    FRACTAL_RPC_URL="$rpc_url" \
    LOAD_DURATION_SECS="$BENCH_DURATION_SECS" \
    LOAD_WARMUP_SECS="$BENCH_WARMUP_SECS" \
    LOAD_WORKERS="$BENCH_WORKERS" \
    LOAD_SUBMIT_PAUSE_US="$BENCH_SUBMIT_PAUSE_US" \
    cargo run -q -p fractal-load-tps
  ) | tee "$raw_log"

  parse_load_output_to_json "$label" "$repo_root" "$rpc_url" "$raw_log" "$json_out"
}

write_report() {
  local baseline_json="$1"
  local experiment_json="$2"
  local report="$3"

  local b_submit b_chain b_nonce b_blocks b_avg b_err
  local e_submit e_chain e_nonce e_blocks e_avg e_err
  b_submit="$(json_get_number submit_tps "$baseline_json")"
  b_chain="$(json_get_number confirmed_chain_tps "$baseline_json")"
  b_nonce="$(json_get_number confirmed_nonce_tps "$baseline_json")"
  b_blocks="$(json_get_number block_rate "$baseline_json")"
  b_avg="$(json_get_number avg_tx_per_block "$baseline_json")"
  b_err="$(json_get_number submit_errors "$baseline_json")"
  e_submit="$(json_get_number submit_tps "$experiment_json")"
  e_chain="$(json_get_number confirmed_chain_tps "$experiment_json")"
  e_nonce="$(json_get_number confirmed_nonce_tps "$experiment_json")"
  e_blocks="$(json_get_number block_rate "$experiment_json")"
  e_avg="$(json_get_number avg_tx_per_block "$experiment_json")"
  e_err="$(json_get_number submit_errors "$experiment_json")"

  cat >"$report" <<MD
# Proof-Ingestion Benchmark Comparison

Generated: ${STAMP}

| Scenario | Repo | Submit TPS | Confirmed chain TPS | Confirmed nonce TPS | Block rate | Avg tx/block | Submit errors |
|---|---|---:|---:|---:|---:|---:|---:|
| baseline | ${BASELINE_ROOT} | ${b_submit} | ${b_chain} | ${b_nonce} | ${b_blocks} | ${b_avg} | ${b_err} |
| experiment | ${EXPERIMENT_ROOT} | ${e_submit} | ${e_chain} | ${e_nonce} | ${e_blocks} | ${e_avg} | ${e_err} |

## Parameters

- duration: ${BENCH_DURATION_SECS}s
- warmup: ${BENCH_WARMUP_SECS}s
- workers: ${BENCH_WORKERS}
- submit pause: ${BENCH_SUBMIT_PAUSE_US} us

## Artifacts

- baseline raw log: baseline/load-tps.log
- baseline JSON: baseline/summary.json
- baseline node log: baseline/node.log
- experiment raw log: experiment/load-tps.log
- experiment JSON: experiment/summary.json
- experiment node log: experiment/node.log
MD
}

main() {
  require_repo "$BASELINE_ROOT"
  require_repo "$EXPERIMENT_ROOT"

  local baseline_dir="${OUT_DIR}/baseline"
  local experiment_dir="${OUT_DIR}/experiment"
  mkdir -p "$baseline_dir" "$experiment_dir"

  local baseline_pid="${baseline_dir}/node.pid"
  local experiment_pid="${experiment_dir}/node.pid"
  trap 'stop_pid "$baseline_pid"; stop_pid "$experiment_pid"' EXIT

  echo "Output: ${OUT_DIR}"
  echo "Baseline: ${BASELINE_ROOT}"
  echo "Experiment: ${EXPERIMENT_ROOT}"

  start_node "baseline" "$BASELINE_ROOT" 18545 19010 "$baseline_dir"
  run_load "baseline" "$BASELINE_ROOT" 18545 "$baseline_dir"
  stop_pid "$baseline_pid"

  start_node "experiment" "$EXPERIMENT_ROOT" 28545 29010 "$experiment_dir"
  run_load "experiment" "$EXPERIMENT_ROOT" 28545 "$experiment_dir"
  stop_pid "$experiment_pid"

  write_report "${baseline_dir}/summary.json" "${experiment_dir}/summary.json" "${OUT_DIR}/comparison.md"

  cat >"${OUT_DIR}/manifest.json" <<JSON
{
  "generated_at": "${STAMP}",
  "baseline_root": "${BASELINE_ROOT}",
  "experiment_root": "${EXPERIMENT_ROOT}",
  "baseline_summary": "baseline/summary.json",
  "experiment_summary": "experiment/summary.json",
  "comparison_report": "comparison.md"
}
JSON

  echo ""
  echo "Benchmark artifacts written to ${OUT_DIR}"
  echo "Report: ${OUT_DIR}/comparison.md"
}

main "$@"
