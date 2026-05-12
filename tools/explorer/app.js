function rpcUrl() {
  const p = new URLSearchParams(window.location.search);
  return p.get("rpc") || "http://127.0.0.1:8545";
}

let nextId = 1;
async function rpc(method, params = []) {
  const r = await fetch(rpcUrl(), {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: nextId++, method, params }),
  });
  const j = await r.json();
  if (j.error) throw new Error(JSON.stringify(j.error, null, 2));
  return j.result;
}

function hexToBigInt(h) {
  if (typeof h !== "string" || !h.startsWith("0x")) throw new Error("expected hex quantity");
  return BigInt(h);
}

function numToHex(n) {
  return "0x" + n.toString(16);
}

function shortHash(h, n = 10) {
  if (!h || typeof h !== "string" || h.length < 4) return h;
  return h.slice(0, n + 2) + "…" + h.slice(-6);
}

async function loadSummary(el) {
  const [chainId, netVer, bnHex, gasPrice, client] = await Promise.all([
    rpc("eth_chainId"),
    rpc("net_version"),
    rpc("eth_blockNumber"),
    rpc("eth_gasPrice"),
    rpc("web3_clientVersion"),
  ]);
  const block = await rpc("eth_getBlockByNumber", [bnHex, false]);
  el.innerHTML = "";
  const dl = document.createElement("dl");
  const rows = [
    ["Chain ID", chainId],
    ["net_version", netVer],
    ["Head block", `${bnHex} (${hexToBigInt(bnHex).toString()})`],
    ["Gas price (stub)", gasPrice],
    ["Client", client],
    ["Head hash", block?.hash || "—"],
    ["Head gas used", block?.gasUsed ?? "—"],
    ["Head timestamp", block?.timestamp ?? "—"],
    ["Txs in head", String(block?.transactions?.length ?? 0)],
  ];
  for (const [k, v] of rows) {
    const dt = document.createElement("dt");
    dt.textContent = k;
    const dd = document.createElement("dd");
    dd.textContent = typeof v === "string" ? v : String(v);
    dl.appendChild(dt);
    dl.appendChild(dd);
  }
  dl.style.display = "grid";
  dl.style.gridTemplateColumns = "auto 1fr";
  dl.style.columnGap = "1rem";
  dl.style.rowGap = "0.35rem";
  el.appendChild(dl);
}

async function loadBlocks(el) {
  const bnHex = await rpc("eth_blockNumber");
  const head = hexToBigInt(bnHex);
  const count = 10n;
  const low = head + 1n > count ? head - (count - 1n) : 0n;
  const tags = [];
  for (let h = head; h >= low; h--) tags.push(numToHex(h));

  const blocks = await Promise.all(
    tags.map((tag) => rpc("eth_getBlockByNumber", [tag, false]).catch(() => null)),
  );

  el.innerHTML = "";
  const table = document.createElement("table");
  const thead = document.createElement("thead");
  thead.innerHTML = "<tr><th>#</th><th>Hash</th><th>Gas used</th><th>Time</th><th>Txs</th></tr>";
  table.appendChild(thead);
  const tbody = document.createElement("tbody");
  for (let i = 0; i < blocks.length; i++) {
    const b = blocks[i];
    const tr = document.createElement("tr");
    if (!b) {
      tr.innerHTML = `<td colspan="5" class="muted">(missing)</td>`;
      tbody.appendChild(tr);
      continue;
    }
    const n = b.number ?? tags[i];
    const txc = Array.isArray(b.transactions) ? b.transactions.length : 0;
    tr.innerHTML = "";
    for (const [x, cls] of [
      [n, "mono"],
      [shortHash(b.hash, 8), "mono"],
      [b.gasUsed ?? "—", ""],
      [b.timestamp ?? "—", "mono"],
      [String(txc), ""],
    ]) {
      const td = document.createElement("td");
      td.textContent = x;
      if (cls) td.className = cls;
      tr.appendChild(td);
    }
    tbody.appendChild(tr);
  }
  table.appendChild(tbody);
  el.appendChild(table);
}

async function refresh() {
  const sum = document.getElementById("summary");
  const blk = document.getElementById("blocks");
  sum.textContent = "Loading…";
  blk.textContent = "…";
  try {
    await Promise.all([loadSummary(sum), loadBlocks(blk)]);
  } catch (e) {
    sum.innerHTML = "";
    const p = document.createElement("p");
    p.className = "err";
    p.textContent = String(e);
    sum.appendChild(p);
    blk.textContent = "";
  }
}

async function lookupAccount() {
  const raw = document.getElementById("acct").value.trim();
  const out = document.getElementById("accountOut");
  out.textContent = "…";
  try {
    if (!raw.startsWith("0x") || raw.length !== 42) {
      out.innerHTML = '<p class="err">Enter a 20-byte hex address (0x + 40 hex chars).</p>';
      return;
    }
    const [bal, nonce] = await Promise.all([
      rpc("eth_getBalance", [raw, "latest"]),
      rpc("eth_getTransactionCount", [raw, "latest"]),
    ]);
    const pre = document.createElement("pre");
    pre.textContent = JSON.stringify({ address: raw, balance: bal, transactionCount: nonce }, null, 2);
    out.innerHTML = "";
    out.appendChild(pre);
  } catch (e) {
    out.innerHTML = "";
    const p = document.createElement("p");
    p.className = "err";
    p.textContent = String(e);
    out.appendChild(p);
  }
}

async function lookupTx() {
  const raw = document.getElementById("txhash").value.trim();
  const out = document.getElementById("txOut");
  out.textContent = "…";
  try {
    if (!raw.startsWith("0x") || raw.length !== 66) {
      out.innerHTML = '<p class="err">Enter a 32-byte tx hash (0x + 64 hex chars).</p>';
      return;
    }
    const [tx, rc] = await Promise.all([
      rpc("eth_getTransactionByHash", [raw]),
      rpc("eth_getTransactionReceipt", [raw]),
    ]);
    const pre = document.createElement("pre");
    pre.textContent = JSON.stringify({ transaction: tx, receipt: rc }, null, 2);
    out.innerHTML = "";
    out.appendChild(pre);
  } catch (e) {
    out.innerHTML = "";
    const p = document.createElement("p");
    p.className = "err";
    p.textContent = String(e);
    out.appendChild(p);
  }
}

document.getElementById("refresh").onclick = refresh;
document.getElementById("lookupAcct").onclick = lookupAccount;
document.getElementById("lookupTx").onclick = lookupTx;
window.addEventListener("load", refresh);
