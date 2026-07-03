# Fractal RLVR UI — "Improve My Local Model"

A small, dependency-free local web app that lets a non-technical user run the
RLVR post-training loop from the browser: choose traces → run eval → train an
adapter → review the report → approve or reject. It drives the **real** local
harness via the `fractal-rlvr` CLI, and all training data stays under a local
workspace (local-only is enforced — raw prompts/answers/traces never leave the
machine and are never shown in the UI).

This implements **RLVR-054** (the UI entry + 8-step loop) on top of a thin local
HTTP API (the **RLVR-053** substrate) that bridges to the CLI.

## Run

```bash
# 1. build the CLI once
cargo build -p fractal-rlvr

# 2. start the UI server (serves the page + the API on http://127.0.0.1:9180)
node tools/rlvr-ui/server.mjs
```

Then open <http://127.0.0.1:9180>.

### Configuration (env)

| var | default | meaning |
|-----|---------|---------|
| `FRACTAL_RLVR_BIN` | `<repo>/target/debug/fractal-rlvr`, else `fractal-rlvr` on PATH | the CLI binary the API bridges to |
| `FRACTAL_RLVR_WORKSPACE` | `<repo>/fractal_rlvr_ui_work` | local dir holding traces, checkpoints, adapter bundle, registry |
| `RLVR_UI_PORT` | `9180` | HTTP port |

## The loop (8 steps → RLVR-054)

1. **Settings** (gear button) — choose training target (router / assistant /
   critic / compressor), confirm local-only mode, set base/actor model ids.
2. **Choose traces** — generate a deterministic demo trace set (`fractal-rlvr
   rollout`) or refresh. The list shows only metadata (trace/task id, reward,
   turn count) — never raw content.
3. **Run eval** — score the base behavior (`fractal-rlvr eval-report`); shows
   accuracy, checkpoint coverage, route rate, leakage rate, cost, latency.
4. **Train adapter** — GRPO adapter-only update from verifier rewards
   (`fractal-rlvr train --method grpo`); shows rollouts/groups and before→after
   reward.
5. **Review report & export** — build the loadable, hash-verified adapter bundle
   (`fractal-rlvr export`); shows the manifest (adapter_hash, format, files).
6. **Approve / reject** — approve registers the adapter in the local registry;
   reject leaves it unregistered.

An activity console at the bottom shows each CLI command and its result.

## API surface (RLVR-053 substrate)

| method | path | bridges to |
|--------|------|------------|
| `GET`  | `/rlvr/state` | snapshot: settings, traces, report, manifest, checkpoint, registry |
| `GET`/`POST` | `/rlvr/settings` | read/write `settings.json` |
| `GET`  | `/rlvr/traces` | list trace metadata (no raw content) |
| `POST` | `/rlvr/rollout` | `fractal-rlvr rollout --per-task` |
| `POST` | `/rlvr/eval` | `fractal-rlvr eval-report` |
| `POST` | `/rlvr/train` | `fractal-rlvr train --method grpo` |
| `POST` | `/rlvr/export` | `fractal-rlvr export` |
| `POST` | `/rlvr/approve` | approve → `export --registry`; reject → no-op |
| `GET`  | `/rlvr/report`, `/rlvr/manifest`, `/rlvr/registry` | read artifacts |

> **Note:** this HTTP layer bridges to the CLI so the browser can drive the loop
> with no extra dependencies. The canonical RLVR API contract also exists as pure
> functions in `crates/rlvr/src/api/mod.rs` (`fractal_run_rlvr_rollout`,
> `fractal_run_rlvr_eval`, `fractal_export_rlvr_adapter`, …) for in-process / node
> callers. Submitting a proof to the running node (the last RLVR-053 endpoint)
> remains pending the node RPC wiring (RLVR-050+).

## Safety

- **Local-only is enforced server-side**: every action endpoint returns `409`
  when `local_only` is off, and the UI disables its buttons.
- The `/rlvr/traces` response carries only metadata; raw turn content is never
  serialized to the browser.
- Chain commitments are hash-only by construction (see the global invariants in
  `docs/rlvr-proof-of-route-task-checklist.md`); this UI never touches raw data
  on-chain.
