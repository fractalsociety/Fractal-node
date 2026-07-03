const $ = (id) => document.getElementById(id);

const state = {
  settings: null,
  traces: [],
  report: null,
  manifest: null,
  checkpoint: null,
  registry: null,
};

function log(message, kind = "dim") {
  const el = $("console");
  const line = document.createElement("div");
  line.className = kind;
  line.textContent = `${new Date().toLocaleTimeString()}  ${message}`;
  el.appendChild(line);
  el.scrollTop = el.scrollHeight;
}

async function api(path, options = {}) {
  const init = {
    method: options.method || "GET",
    headers: { "Content-Type": "application/json" },
  };
  if (options.body) init.body = JSON.stringify(options.body);
  const res = await fetch(path, init);
  const data = await res.json().catch(() => ({}));
  if (!res.ok || data.ok === false) {
    throw new Error(data.error || data.stderr || data.stdout || `HTTP ${res.status}`);
  }
  return data;
}

function adapterIdForTarget(target) {
  return `${target || "router"}-adapter`;
}

function setBusy(button, busy) {
  if (!button) return;
  button.disabled = busy || !canRunLocal();
}

function canRunLocal() {
  return Boolean(state.settings && state.settings.local_only);
}

function renderSettings(settings) {
  $("set-target").value = settings.target || "router";
  $("set-local-only").checked = Boolean(settings.local_only);
  $("set-base-model").value = settings.base_model || "tiny-router-base";
  $("set-actor-model").value = settings.actor_model || "local-tiny-model";
  $("locality-badge").textContent = settings.local_only
    ? "local-only · hashes only"
    : "local-only disabled";
  $("locality-badge").style.color = settings.local_only ? "var(--green)" : "var(--red)";
  const currentAdapter = $("adapter-id").value.trim();
  if (!currentAdapter || currentAdapter.endsWith("-adapter")) {
    $("adapter-id").value = adapterIdForTarget(settings.target);
  }
}

function renderTraces(traces) {
  const tbody = $("trace-rows");
  tbody.innerHTML = "";
  $("trace-count").textContent = `${traces.length} trace${traces.length === 1 ? "" : "s"}`;
  if (!traces.length) {
    tbody.innerHTML = '<tr><td colspan="4" class="empty">no traces - generate a demo set</td></tr>';
    return;
  }
  for (const trace of traces) {
    const tr = document.createElement("tr");
    tr.innerHTML = `
      <td class="mono">${escapeHtml(trace.trace_id || trace.name || "unknown")}</td>
      <td>${escapeHtml(trace.task_id || "unknown")}</td>
      <td>${formatNumber(trace.final_reward)}</td>
      <td>${trace.turn_count ?? 0}</td>
    `;
    tbody.appendChild(tr);
  }
}

function metricRows(report) {
  if (!report) return [];
  return [
    ["Traces", report.trace_count],
    ["Final answer accuracy", percent(report.final_answer_accuracy)],
    ["Checkpoint coverage", percent(report.checkpoint_coverage)],
    ["Correct route rate", percent(report.correct_route_rate)],
    ["Redundant question rate", percent(report.redundant_question_rate)],
    ["Private-data leakage", percent(report.private_data_leakage_rate)],
    ["Average cost", formatNumber(report.average_cost)],
    ["Average latency", `${formatNumber(report.average_latency_ms)} ms`],
  ];
}

function renderEval(report) {
  const el = $("eval-summary");
  if (!report) {
    el.innerHTML = '<div class="empty">no eval report yet</div>';
    $("eval-status").textContent = "";
    return;
  }
  $("eval-status").innerHTML = '<span class="pill green">complete</span>';
  el.innerHTML = kv(metricRows(report));
}

function renderTrain(checkpoint) {
  const el = $("train-summary");
  if (!checkpoint) {
    el.innerHTML = '<div class="empty">no adapter checkpoint yet</div>';
    return;
  }
  const evalSummary = checkpoint.eval || {};
  el.innerHTML = kv([
    ["Adapter", checkpoint.adapter_id],
    ["Base model", checkpoint.base_model_id],
    ["Adapter-only update", checkpoint.adapter_only_update ? "yes" : "no"],
    ["Rollouts", checkpoint.rollout_count],
    ["Groups", checkpoint.group_count],
    ["Before reward", formatNumber(evalSummary.before_avg_reward)],
    ["After reward", formatNumber(evalSummary.after_avg_reward_estimate)],
    ["Improved", evalSummary.improved ? "yes" : "no"],
  ]);
}

function renderManifest(manifest) {
  const el = $("manifest-view");
  if (!manifest) {
    el.innerHTML = '<div class="empty">no exported adapter bundle yet</div>';
    $("export-status").textContent = "";
    return;
  }
  $("export-status").innerHTML = '<span class="pill green">exported</span>';
  const files = Array.isArray(manifest.files) ? manifest.files.length : 0;
  el.innerHTML = kv([
    ["Adapter hash", mono(manifest.adapter_hash || "unknown")],
    ["Format", manifest.format_version || manifest.format || "unknown"],
    ["Files", files],
    ["Created", manifest.created_at_ms || manifest.timestamp_ms || "unknown"],
  ]);
}

function renderDecision(registry) {
  const el = $("decision-view");
  if (!registry) {
    el.innerHTML = '<div class="empty">no approval decision yet</div>';
    return;
  }
  const count = Array.isArray(registry.adapters) ? registry.adapters.length : 0;
  el.innerHTML = `<span class="pill green">approved</span> ${count} local adapter${count === 1 ? "" : "s"} registered`;
}

function renderSteps() {
  $("step-traces").classList.toggle("done", state.traces.length > 0);
  $("step-eval").classList.toggle("done", Boolean(state.report));
  $("step-train").classList.toggle("done", Boolean(state.checkpoint));
  $("step-review").classList.toggle("done", Boolean(state.manifest));
  $("step-approve").classList.toggle("done", Boolean(state.registry));

  const local = canRunLocal();
  $("gen-traces").disabled = !local;
  $("run-eval").disabled = !local || state.traces.length === 0;
  $("run-train").disabled = !local || state.traces.length === 0;
  $("run-export").disabled = !local || !state.checkpoint;
  $("approve").disabled = !local || !state.manifest;
  $("reject").disabled = !state.manifest;
  $("settings-status").textContent = local
    ? "Local-only mode is active."
    : "Enable local-only mode before running RLVR actions.";
}

function renderAll() {
  renderSettings(state.settings || {});
  renderTraces(state.traces || []);
  renderEval(state.report);
  renderTrain(state.checkpoint);
  renderManifest(state.manifest);
  renderDecision(state.registry);
  renderSteps();
}

async function refreshState() {
  const data = await api("/rlvr/state");
  state.settings = data.settings || {};
  state.traces = data.traces || [];
  state.report = data.report || null;
  state.manifest = data.manifest || null;
  state.checkpoint = data.train_checkpoint || null;
  state.registry = data.registry || null;
  renderAll();
}

async function saveSettings() {
  const target = $("set-target").value;
  const settings = await api("/rlvr/settings", {
    method: "POST",
    body: {
      target,
      local_only: $("set-local-only").checked,
      base_model: $("set-base-model").value.trim(),
      actor_model: $("set-actor-model").value.trim(),
    },
  });
  state.settings = settings;
  $("adapter-id").value = adapterIdForTarget(target);
  renderAll();
  log("settings saved", "ok");
}

async function runAction(buttonId, label, path, body = {}) {
  const btn = $(buttonId);
  try {
    setBusy(btn, true);
    log(`${label} started`);
    const data = await api(path, { method: "POST", body });
    if (data.stdout) log(data.stdout, "ok");
    if (data.stderr) log(data.stderr, "err");
    await refreshState();
    log(`${label} complete`, "ok");
    return data;
  } catch (err) {
    log(`${label} failed: ${err.message}`, "err");
    const box = document.createElement("div");
    box.className = "errbox";
    box.textContent = err.message;
    $("console").appendChild(box);
  } finally {
    setBusy(btn, false);
    renderSteps();
  }
}

function kv(rows) {
  return `<div class="kv">${rows
    .map(([k, v]) => `<div class="k">${escapeHtml(k)}</div><div>${escapeHtml(v == null ? "unknown" : v)}</div>`)
    .join("")}</div>`;
}

function mono(value) {
  return String(value);
}

function percent(value) {
  const n = Number(value);
  return Number.isFinite(n) ? `${(n * 100).toFixed(1)}%` : "unknown";
}

function formatNumber(value) {
  const n = Number(value);
  if (!Number.isFinite(n)) return value == null ? "unknown" : String(value);
  return Math.abs(n) >= 100 ? n.toFixed(0) : n.toFixed(4).replace(/0+$/, "").replace(/\.$/, "");
}

function escapeHtml(value) {
  return String(value)
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

$("settings-btn").addEventListener("click", () => {
  $("settings-panel").toggleAttribute("hidden");
});
$("save-settings").addEventListener("click", () => void saveSettings());
$("set-target").addEventListener("change", () => {
  $("adapter-id").value = adapterIdForTarget($("set-target").value);
});
$("gen-traces").addEventListener("click", () => void runAction("gen-traces", "rollout", "/rlvr/rollout", { n: 6, perTask: 2 }));
$("refresh-traces").addEventListener("click", () => void refreshState());
$("run-eval").addEventListener("click", () => void runAction("run-eval", "eval", "/rlvr/eval"));
$("run-train").addEventListener("click", () =>
  void runAction("run-train", "train", "/rlvr/train", { adapter: $("adapter-id").value.trim() })
);
$("run-export").addEventListener("click", () =>
  void runAction("run-export", "export", "/rlvr/export", { adapter: $("adapter-id").value.trim(), rank: 8 })
);
$("approve").addEventListener("click", () =>
  void runAction("approve", "approve", "/rlvr/approve", {
    decision: "approve",
    adapter: $("adapter-id").value.trim(),
    rank: 8,
  })
);
$("reject").addEventListener("click", () =>
  void runAction("reject", "reject", "/rlvr/approve", { decision: "reject" })
);

refreshState().catch((err) => {
  log(`initial load failed: ${err.message}`, "err");
});
