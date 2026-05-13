async function loadBuiltins() {
  const r = await fetch("builtins.json", { cache: "no-store" });
  if (!r.ok) throw new Error(`builtins.json: HTTP ${r.status}`);
  return r.json();
}

function el(tag, attrs, children) {
  const n = document.createElement(tag);
  if (attrs) {
    for (const [k, v] of Object.entries(attrs)) {
      if (k === "className") n.className = v;
      else if (k === "textContent") n.textContent = v;
      else if (k === "innerHTML") n.innerHTML = v;
      else n.setAttribute(k, v);
    }
  }
  if (children) for (const c of children) n.appendChild(c);
  return n;
}

function renderTemplateCard(t) {
  const name = t.name || `template ${t.templateId}`;
  const desc = t.description || "";
  const pre = el("pre", { className: "json-snippet" });
  pre.textContent = JSON.stringify(
    {
      templateId: t.templateId,
      name,
      description: desc,
      totalCap: t.totalCap,
      perToolCap: t.perToolCap,
      rateLimits: t.rateLimits,
      suggestedToolMask: t.suggestedToolMask,
      caveats: t.caveats,
    },
    null,
    2,
  );
  const mintCmd = `cargo run -p fractal-cli -- cap mint --template ${t.templateId} --chain-id <id> --not-after-ms <ms> [--workspace <n>]`;
  const cmd = el("p", { className: "cli-hint" });
  cmd.appendChild(document.createTextNode("Mint (offline): "));
  const code = el("code", { className: "mono", textContent: mintCmd });
  cmd.appendChild(code);
  return el("article", { className: "policy-card" }, [
    el("h3", { textContent: name }),
    el("p", { className: "muted", textContent: desc }),
    pre,
    cmd,
  ]);
}

async function main() {
  const host = document.getElementById("policies");
  const err = document.getElementById("loadError");
  err.textContent = "";
  host.innerHTML = "";
  try {
    const data = await loadBuiltins();
    const list = data.templates;
    if (!Array.isArray(list) || list.length === 0) {
      err.textContent = "builtins.json: missing templates array";
      return;
    }
    for (const t of list) host.appendChild(renderTemplateCard(t));
  } catch (e) {
    err.textContent = String(e);
  }
}

main();
