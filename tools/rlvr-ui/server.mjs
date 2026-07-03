#!/usr/bin/env node
// RLVR-053 / RLVR-054: local RLVR API server for the "Improve My Local Model" UI.
//
// A zero-dependency Node HTTP server that (a) serves the static UI
// (`index.html`, `app.js`) and (b) exposes a small REST surface that bridges to
// the `fractal-rlvr` CLI. The UI drives the full loop — choose traces, run eval,
// train an adapter, review the report, approve/reject — against the real local
// harness functions (rollout, eval-report, train --method grpo, export), with all
// raw training data kept under the local workspace.
//
// Run:  node tools/rlvr-ui/server.mjs
// Env:  FRACTAL_RLVR_BIN (default: <repo>/target/debug/fractal-rlvr, then PATH)
//       FRACTAL_RLVR_WORKSPACE (default: <repo>/fractal_rlvr_ui_work)
//       RLVR_UI_PORT (default: 9180)

import { createServer } from "node:http";
import { spawnSync } from "node:child_process";
import {
  readFileSync,
  writeFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  rmSync,
} from "node:fs";
import { join, resolve, dirname, extname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const REPO_ROOT = resolve(__dirname, "../..");

const PORT = Number(process.env.RLVR_UI_PORT || 9180);
const WORKSPACE =
  process.env.FRACTAL_RLVR_WORKSPACE || join(REPO_ROOT, "fractal_rlvr_ui_work");

function resolveBinary() {
  if (process.env.FRACTAL_RLVR_BIN) return process.env.FRACTAL_RLVR_BIN;
  const dev = join(REPO_ROOT, "target/debug/fractal-rlvr");
  if (existsSync(dev)) return dev;
  return "fractal-rlvr";
}
const BIN = resolveBinary();

const DIRS = {
  rollout: join(WORKSPACE, "rollout"),
  train: join(WORKSPACE, "train"),
  adapter: join(WORKSPACE, "adapter"),
  report: join(WORKSPACE, "report"),
};
const SETTINGS_PATH = join(WORKSPACE, "settings.json");
const REGISTRY_PATH = join(WORKSPACE, "registry.json");

function ensureWorkspace() {
  mkdirSync(WORKSPACE, { recursive: true });
  for (const d of Object.values(DIRS)) mkdirSync(d, { recursive: true });
}
ensureWorkspace();

const DEFAULT_SETTINGS = {
  local_only: true,
  target: "router", // router | assistant | critic | compressor
  base_model: "tiny-router-base",
  actor_model: "local-tiny-model",
};
const TARGETS = new Set(["router", "assistant", "critic", "compressor"]);

function readSettings() {
  if (!existsSync(SETTINGS_PATH)) return { ...DEFAULT_SETTINGS };
  try {
    return { ...DEFAULT_SETTINGS, ...JSON.parse(readFileSync(SETTINGS_PATH, "utf8")) };
  } catch {
    return { ...DEFAULT_SETTINGS };
  }
}
function writeSettings(next) {
  const clean = sanitizeSettings(next);
  writeFileSync(SETTINGS_PATH, JSON.stringify(clean, null, 2));
  return clean;
}

function sanitizeSettings(next = {}) {
  const target = TARGETS.has(next.target) ? next.target : DEFAULT_SETTINGS.target;
  return {
    local_only: Boolean(next.local_only),
    target,
    base_model: cleanId(next.base_model, DEFAULT_SETTINGS.base_model),
    actor_model: cleanId(next.actor_model, DEFAULT_SETTINGS.actor_model),
  };
}

function cleanId(value, dflt) {
  const s = String(value ?? "").trim();
  return s.length ? s : dflt;
}

function runCli(args) {
  const r = spawnSync(BIN, args, {
    cwd: WORKSPACE,
    encoding: "utf8",
    maxBuffer: 64 * 1024 * 1024,
  });
  return {
    ok: r.status === 0,
    code: r.status,
    stdout: (r.stdout || "").trim(),
    stderr: (r.stderr || "").trim(),
  };
}

function readJsonIfExists(p) {
  if (!existsSync(p)) return null;
  try {
    return JSON.parse(readFileSync(p, "utf8"));
  } catch {
    return null;
  }
}

// Return only metadata for trace files — never raw turn content.
function listTraces(dir) {
  if (!existsSync(dir)) return [];
  const out = [];
  for (const name of readdirSync(dir).sort()) {
    if (extname(name) !== ".json") continue;
    let t;
    try {
      t = JSON.parse(readFileSync(join(dir, name), "utf8"));
    } catch {
      continue;
    }
    out.push({
      name,
      trace_id: t.trace_id ?? null,
      task_id: t.task_id ?? null,
      final_reward: t.final_reward ?? null,
      turn_count: Array.isArray(t.turns) ? t.turns.length : 0,
    });
  }
  return out;
}

// ---------------------------------------------------------------------------
// HTTP plumbing
// ---------------------------------------------------------------------------

function send(res, status, body, headers = {}) {
  const base = { "cache-control": "no-store" };
  res.writeHead(status, { ...base, ...headers });
  res.end(body);
}
function sendJson(res, status, obj) {
  send(res, status, JSON.stringify(obj, null, 2), { "content-type": "application/json" });
}

function readBody(req) {
  return new Promise((resolve) => {
    let raw = "";
    req.on("data", (c) => (raw += c));
    req.on("end", () => {
      if (!raw) return resolve({});
      try {
        resolve(JSON.parse(raw));
      } catch {
        resolve({});
      }
    });
  });
}

function parseQuery(url) {
  const q = url.split("?")[1];
  const out = {};
  if (!q) return out;
  for (const pair of q.split("&")) {
    const [k, v] = pair.split("=");
    if (k) out[decodeURIComponent(k)] = decodeURIComponent(v || "");
  }
  return out;
}

function summarizeEvalReport(report) {
  if (!report) return null;
  return {
    schema_version: report.schema_version ?? null,
    trace_count: report.trace_count ?? null,
    final_answer_accuracy: report.final_answer_accuracy ?? null,
    checkpoint_coverage: report.checkpoint_coverage ?? null,
    redundant_question_rate: report.redundant_question_rate ?? null,
    premature_answer_rate: report.premature_answer_rate ?? null,
    correct_route_rate: report.correct_route_rate ?? null,
    unnecessary_escalation_rate: report.unnecessary_escalation_rate ?? null,
    private_data_leakage_rate: report.private_data_leakage_rate ?? null,
    average_cost: report.average_cost ?? null,
    average_latency_ms: report.average_latency_ms ?? null,
  };
}

function requireLocalOnly(res, settings) {
  if (settings.local_only) return true;
  sendJson(res, 409, {
    ok: false,
    error: "Enable local-only mode before running RLVR actions.",
  });
  return false;
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

const server = createServer(async (req, res) => {
  const url = req.url.split("?")[0];
  const query = parseQuery(req.url);
  const settings = readSettings();

  try {
    // Static UI
    if (req.method === "GET" && url === "/") {
      return send(res, 200, readFileSync(join(__dirname, "index.html"), "utf8"), {
        "content-type": "text/html; charset=utf-8",
      });
    }
    if (req.method === "GET" && url === "/app.js") {
      return send(res, 200, readFileSync(join(__dirname, "app.js"), "utf8"), {
        "content-type": "text/javascript; charset=utf-8",
      });
    }

    // State / settings
    if (req.method === "GET" && url === "/rlvr/state") {
      return sendJson(res, 200, {
        bin: BIN,
        workspace: WORKSPACE,
        dirs: DIRS,
        settings,
        traces: listTraces(DIRS.rollout),
        report: summarizeEvalReport(readJsonIfExists(join(DIRS.report, "eval_report.json"))),
        manifest: readJsonIfExists(join(DIRS.adapter, "manifest.json")),
        train_checkpoint: readJsonIfExists(lastCheckpoint(DIRS.train)),
        registry: readJsonIfExists(REGISTRY_PATH),
      });
    }

    if (req.method === "GET" && url === "/rlvr/settings") {
      return sendJson(res, 200, settings);
    }
    if (req.method === "POST" && url === "/rlvr/settings") {
      const body = await readBody(req);
      const next = writeSettings({ ...settings, ...body });
      return sendJson(res, 200, next);
    }

    if (req.method === "GET" && url === "/rlvr/traces") {
      const dir = query.dir ? join(WORKSPACE, query.dir) : DIRS.rollout;
      return sendJson(res, 200, { dir, traces: listTraces(dir) });
    }

    // Generate demo traces (RLVR-053 "run rollout")
    if (req.method === "POST" && url === "/rlvr/rollout") {
      if (!requireLocalOnly(res, settings)) return;
      const body = await readBody(req);
      const n = clampInt(body.n, 1, 50, 6);
      const perTask = clampInt(body.perTask, 1, 8, 2);
      rmSync(DIRS.rollout, { recursive: true, force: true });
      mkdirSync(DIRS.rollout, { recursive: true });
      const r = runCli([
        "rollout",
        "--n",
        String(n),
        "--per-task",
        String(perTask),
        "--actor",
        settings.actor_model || "local-tiny-model",
        "--out",
        DIRS.rollout,
      ]);
      return sendJson(res, r.ok ? 200 : 500, {
        ok: r.ok,
        stdout: r.stdout,
        stderr: r.stderr,
        traces: listTraces(DIRS.rollout),
      });
    }

    // Train adapter (RLVR-053 "run train")
    if (req.method === "POST" && url === "/rlvr/train") {
      if (!requireLocalOnly(res, settings)) return;
      const body = await readBody(req);
      const adapter = (body.adapter || "router-adapter").trim();
      rmSync(DIRS.train, { recursive: true, force: true });
      mkdirSync(DIRS.train, { recursive: true });
      const r = runCli([
        "train",
        "--method",
        "grpo",
        "--rollouts",
        DIRS.rollout,
        "--adapter",
        adapter,
        "--base-model",
        settings.base_model || "tiny-router-base",
        "--out",
        DIRS.train,
      ]);
      return sendJson(res, r.ok ? 200 : 500, {
        ok: r.ok,
        stdout: r.stdout,
        stderr: r.stderr,
        checkpoint: readJsonIfExists(lastCheckpoint(DIRS.train)),
      });
    }

    // Run eval (RLVR-053 "run eval")
    if (req.method === "POST" && url === "/rlvr/eval") {
      if (!requireLocalOnly(res, settings)) return;
      const inputDir = DIRS.rollout;
      rmSync(DIRS.report, { recursive: true, force: true });
      mkdirSync(DIRS.report, { recursive: true });
      const r = runCli([
        "eval-report",
        "--input",
        inputDir,
        "--out",
        DIRS.report,
      ]);
      return sendJson(res, r.ok ? 200 : 500, {
        ok: r.ok,
        stdout: r.stdout,
        stderr: r.stderr,
        report: summarizeEvalReport(readJsonIfExists(join(DIRS.report, "eval_report.json"))),
      });
    }

    // Export adapter bundle (RLVR-053 "export adapter")
    if (req.method === "POST" && url === "/rlvr/export") {
      if (!requireLocalOnly(res, settings)) return;
      const body = await readBody(req);
      const adapter = (body.adapter || "router-adapter").trim();
      const rank = clampInt(body.rank, 1, 64, 8);
      rmSync(DIRS.adapter, { recursive: true, force: true });
      mkdirSync(DIRS.adapter, { recursive: true });
      const r = runCli([
        "export",
        "--adapter",
        adapter,
        "--base-model",
        settings.base_model || "tiny-router-base",
        "--rank",
        String(rank),
        "--out",
        DIRS.adapter,
      ]);
      return sendJson(res, r.ok ? 200 : 500, {
        ok: r.ok,
        stdout: r.stdout,
        stderr: r.stderr,
        manifest: readJsonIfExists(join(DIRS.adapter, "manifest.json")),
      });
    }

    // Approve/reject adapter (RLVR-054 final step). Approve registers it locally.
    if (req.method === "POST" && url === "/rlvr/approve") {
      const body = await readBody(req);
      const decision = (body.decision || "").toLowerCase();
      if (decision !== "approve" && decision !== "reject") {
        return sendJson(res, 400, { ok: false, error: "decision must be approve|reject" });
      }
      let registered = null;
      if (decision === "approve") {
        if (!requireLocalOnly(res, settings)) return;
        const adapter = (body.adapter || "router-adapter").trim();
        const r = runCli([
          "export",
          "--adapter",
          adapter,
          "--base-model",
          settings.base_model || "tiny-router-base",
          "--rank",
          String(clampInt(body.rank, 1, 64, 8)),
          "--out",
          DIRS.adapter,
          "--registry",
          REGISTRY_PATH,
        ]);
        if (!r.ok) {
          return sendJson(res, 500, { ok: false, stderr: r.stderr, stdout: r.stdout });
        }
        registered = readJsonIfExists(REGISTRY_PATH);
      }
      return sendJson(res, 200, {
        ok: true,
        decision,
        message:
          decision === "approve"
            ? "Adapter approved and registered locally (data stays local-only)."
            : "Adapter rejected — not registered.",
        registry: registered,
      });
    }

    if (req.method === "GET" && url === "/rlvr/registry") {
      return sendJson(res, 200, readJsonIfExists(REGISTRY_PATH) ?? { adapters: [] });
    }

    if (req.method === "GET" && url === "/rlvr/report") {
      return sendJson(res, 200, readJsonIfExists(join(DIRS.report, "eval_report.json")));
    }
    if (req.method === "GET" && url === "/rlvr/manifest") {
      return sendJson(res, 200, readJsonIfExists(join(DIRS.adapter, "manifest.json")));
    }

    return sendJson(res, 404, { error: "not found", path: url });
  } catch (err) {
    return sendJson(res, 500, { error: String(err && err.message ? err.message : err) });
  }
});

function lastCheckpoint(dir) {
  if (!existsSync(dir)) return join(dir, "checkpoint.json");
  const files = readdirSync(dir).filter((f) => f.endsWith("-grpo-checkpoint.json"));
  if (!files.length) return join(dir, "checkpoint.json");
  return join(dir, files.sort().slice(-1)[0]);
}

function clampInt(v, min, max, dflt) {
  const n = Number.parseInt(v, 10);
  if (!Number.isFinite(n)) return dflt;
  return Math.max(min, Math.min(max, n));
}

server.listen(PORT, "127.0.0.1", () => {
  // eslint-disable-next-line no-console
  console.log(
    `Fractal RLVR UI on http://127.0.0.1:${PORT}\n  binary:    ${BIN}\n  workspace: ${WORKSPACE}`
  );
});
