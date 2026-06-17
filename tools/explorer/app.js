function rpcUrl() {
  const p = new URLSearchParams(window.location.search);
  return p.get("rpc") || window.FRACTAL_RPC_URL || "/rpc";
}

let nextId = 1;
const DEV_SIGNER_0 = "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266";
const CACHE_KEY = "fractal-explorer.snapshot.v1";
const RPC_TIMEOUT_MS = 8000;

async function rpc(method, params = []) {
  const controller = new AbortController();
  const timer = window.setTimeout(() => controller.abort(), RPC_TIMEOUT_MS);
  let r;
  try {
    r = await fetch(rpcUrl(), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ jsonrpc: "2.0", id: nextId++, method, params }),
      signal: controller.signal,
    });
  } finally {
    window.clearTimeout(timer);
  }
  const text = await r.text();
  if (!r.ok) throw new Error(`RPC HTTP ${r.status}: ${text.slice(0, 180)}`);
  let j;
  try {
    j = JSON.parse(text);
  } catch {
    throw new Error(`RPC returned non-JSON for ${method}: ${text.slice(0, 180)}`);
  }
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

/** 0x + 40 hex, case-insensitive; also accepts 40 hex without 0x. */
function normalizeAddress(raw) {
  let s = String(raw).trim();
  if (!s) return null;
  if (!s.startsWith("0x") && /^[0-9a-fA-F]{40}$/.test(s)) s = "0x" + s;
  if (!s.startsWith("0x") || s.length !== 42) return null;
  if (!/^0x[0-9a-fA-F]{40}$/i.test(s)) return null;
  return s.toLowerCase();
}

/** 0x + 64 hex; accepts 64 hex without 0x. */
function normalizeTxHash(raw) {
  let s = String(raw).trim();
  if (!s) return null;
  if (!s.startsWith("0x") && /^[0-9a-fA-F]{64}$/.test(s)) s = "0x" + s;
  if (!s.startsWith("0x") || s.length !== 66) return null;
  if (!/^0x[0-9a-fA-F]{64}$/i.test(s)) return null;
  return s.toLowerCase();
}

function shortAddr(addr) {
  if (!addr || typeof addr !== "string" || addr.length < 12) return addr || "—";
  return addr.slice(0, 8) + "…" + addr.slice(-6);
}

function finalityStatus(block) {
  const status = block && typeof block === "object" ? block.finalityStatus : null;
  return status === "proof" || status === "soft" ? status : "unknown";
}

function finalityLabel(status) {
  if (status === "proof") return "Proof-final";
  if (status === "soft") return "Soft-final";
  return "Unknown finality";
}

function updateHeroStat(id, value) {
  const el = document.getElementById(id);
  if (el) el.textContent = value;
}

function updateHeroStatIfEmpty(id, value) {
  const el = document.getElementById(id);
  if (!el) return;
  const current = el.textContent.trim();
  if (!current || current === "—") el.textContent = value;
}

function readCache() {
  try {
    const raw = window.localStorage.getItem(CACHE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw);
    return parsed && typeof parsed === "object" ? parsed : null;
  } catch {
    return null;
  }
}

function writeCache(partial) {
  try {
    const next = { ...(readCache() || {}), ...partial, cachedAt: Date.now() };
    window.localStorage.setItem(CACHE_KEY, JSON.stringify(next));
  } catch {
    // Cache is an optimization only.
  }
}

function renderMetricRows(el, rows) {
  el.innerHTML = "";
  const dl = document.createElement("dl");
  dl.className = "metric-grid";
  for (const [k, v] of rows) {
    const row = document.createElement("div");
    row.className = "metric-row";
    const dt = document.createElement("dt");
    dt.textContent = k;
    const dd = document.createElement("dd");
    dd.textContent = typeof v === "string" ? v : String(v);
    row.appendChild(dt);
    row.appendChild(dd);
    dl.appendChild(row);
  }
  el.appendChild(dl);
}

function renderCachedSnapshot() {
  const cache = readCache();
  if (!cache) return false;
  if (cache.hero) {
    updateHeroStat("heroHead", cache.hero.head ?? "—");
    updateHeroStat("heroFinality", cache.hero.finality ?? "—");
    updateHeroStat("heroTxs", cache.hero.txs ?? "—");
  }
  const summary = document.getElementById("summary");
  if (summary && Array.isArray(cache.summaryRows)) renderMetricRows(summary, cache.summaryRows);
  const blocks = document.getElementById("blocks");
  if (blocks && Array.isArray(cache.blockRows)) renderBlockRows(blocks, cache.blockRows, cache.blockCaption || "Cached recent activity.");
  return true;
}

function renderInitialSnapshot() {
  updateHeroStatIfEmpty("heroHead", "—");
  updateHeroStatIfEmpty("heroFinality", "—");
  updateHeroStatIfEmpty("heroTxs", "—");

  const summary = document.getElementById("summary");
  if (summary && !summary.children.length) {
    renderMetricRows(summary, [
      ["Chain ID", "—"],
      ["Head block", "—"],
      ["Head finality", "—"],
      ["Confirmed dev signer txs", "—"],
    ]);
  }

  const blocks = document.getElementById("blocks");
  if (blocks && !blocks.children.length) {
    renderBlockRows(blocks, [], "Recent activity will appear here as the explorer syncs in the background.");
  }
}

function finalityBadge(status) {
  const badge = document.createElement("span");
  badge.className = `finality-badge finality-${status}`;
  badge.textContent = finalityLabel(status);
  badge.title =
    status === "proof"
      ? "Validity proof accepted; suitable for settlement and bridge reliance."
      : status === "soft"
        ? "Committee/sequencer accepted; wait for proof finality for high-value settlement."
        : "RPC did not return finalityStatus for this block.";
  return badge;
}

function jumpToAddress(addr) {
  const n = normalizeAddress(addr);
  if (!n) return;
  const acct = document.getElementById("acct");
  const out = document.getElementById("accountOut");
  if (acct) acct.value = n;
  if (out) out.scrollIntoView({ behavior: "smooth", block: "nearest" });
  lookupAccount();
}

function makeAddrJump(label, fullAddr) {
  const n = normalizeAddress(fullAddr);
  if (!n) return null;
  const wrap = document.createElement("span");
  wrap.className = "addr-pair";
  const lbl = document.createElement("span");
  lbl.className = "lbl";
  lbl.textContent = label;
  const btn = document.createElement("button");
  btn.type = "button";
  btn.className = "addr-jump";
  btn.textContent = shortAddr(n);
  btn.title = `${n} — open in Address lookup`;
  btn.addEventListener("click", (e) => {
    e.preventDefault();
    jumpToAddress(n);
  });
  wrap.appendChild(lbl);
  wrap.appendChild(btn);
  return wrap;
}

function renderTxAddressStrip(container, tx, receipt) {
  container.innerHTML = "";
  const box = document.createElement("div");
  box.className = "tx-address-strip";
  const title = document.createElement("div");
  title.className = "strip-title";
  title.textContent = "Addresses — click a button to look up that account";
  box.appendChild(title);
  const row = document.createElement("div");
  row.className = "addr-chip-row";

  const from = tx && typeof tx === "object" ? tx.from : null;
  const to = tx && typeof tx === "object" ? tx.to : null;
  const rcFrom = receipt && typeof receipt === "object" ? receipt.from : null;
  const rcTo = receipt && typeof receipt === "object" ? receipt.to : null;
  const contract = receipt && typeof receipt === "object" ? receipt.contractAddress : null;

  const f = from || rcFrom;
  if (f) {
    const el = makeAddrJump("From", f);
    if (el) row.appendChild(el);
  }
  const t = to || rcTo;
  if (t) {
    const el = makeAddrJump("To", t);
    if (el) row.appendChild(el);
  }
  if (contract) {
    const el = makeAddrJump("Contract", contract);
    if (el) row.appendChild(el);
  }

  if (!row.children.length) {
    const p = document.createElement("p");
    p.className = "muted";
    p.style.margin = "0";
    p.textContent =
      "No from/to on this response (e.g. some native txs). Raw JSON is below.";
    box.appendChild(p);
  } else {
    box.appendChild(row);
  }
  container.appendChild(box);
}

function codeByteLen(hex) {
  if (typeof hex !== "string" || !hex.startsWith("0x")) return 0;
  const n = (hex.length - 2) / 2;
  return Number.isInteger(n) && n >= 0 ? n : 0;
}

async function loadSummary(el) {
  const [chainId, netVer, bnHex, gasPrice, client, signerNonce] = await Promise.all([
    rpc("eth_chainId"),
    rpc("net_version"),
    rpc("eth_blockNumber"),
    rpc("eth_gasPrice"),
    rpc("web3_clientVersion"),
    rpc("eth_getTransactionCount", [DEV_SIGNER_0, "latest"]).catch(() => null),
  ]);
  const block = await rpc("eth_getBlockByNumber", [bnHex, false]);
  const finality = finalityStatus(block);
  updateHeroStat("heroHead", hexToBigInt(bnHex).toString());
  updateHeroStat("heroFinality", finalityLabel(finality));
  const hero = {
    head: hexToBigInt(bnHex).toString(),
    finality: finalityLabel(finality),
    txs: typeof signerNonce === "string" ? hexToBigInt(signerNonce).toString() : "—",
  };
  if (typeof signerNonce === "string") updateHeroStat("heroTxs", hero.txs);
  const rows = [
    ["Chain ID", chainId],
    ["net_version", netVer],
    ["Head block", `${bnHex} (${hexToBigInt(bnHex).toString()})`],
    ["Gas price (stub)", gasPrice],
    ["Client", client],
    ["Head hash", block?.hash || "—"],
    ["Head finality", finalityLabel(finalityStatus(block))],
    ["Head gas used", block?.gasUsed ?? "—"],
    ["Head timestamp", block?.timestamp ?? "—"],
    ["Txs in head", String(block?.transactions?.length ?? 0)],
    ["Confirmed dev signer txs", typeof signerNonce === "string" ? hexToBigInt(signerNonce).toString() : "—"],
  ];
  renderMetricRows(el, rows);
  writeCache({ hero, summaryRows: rows });
}

function wireBlockRowClicks(tbody) {
  tbody.addEventListener("click", (ev) => {
    const tr = ev.target.closest("tr[data-block-tag]");
    if (!tr) return;
    const tag = tr.getAttribute("data-block-tag");
    if (tag) showBlockDetail(tag);
  });
}

async function loadBlocks(el) {
  const detail = document.getElementById("blockDetail");
  if (detail) detail.innerHTML = "";

  const bnHex = await rpc("eth_blockNumber");
  const head = hexToBigInt(bnHex);
  const displayCount = 10;
  const scanCount = 160n;
  const low = head + 1n > scanCount ? head - (scanCount - 1n) : 0n;
  const scanTags = [];
  for (let h = head; h >= low; h--) scanTags.push(numToHex(h));

  const scanned = [];
  const batchSize = 6;
  for (let i = 0; i < scanTags.length; i += batchSize) {
    const batchTags = scanTags.slice(i, i + batchSize);
    const batch = await Promise.all(
      batchTags.map((tag) => rpc("eth_getBlockByNumber", [tag, false]).catch(() => null)),
    );
    scanned.push(...batch);
    const found = scanned.filter((block) => Array.isArray(block?.transactions) && block.transactions.length > 0).length;
    if (found >= displayCount) break;
  }
  const scannedTags = scanTags.slice(0, scanned.length);
  const nonempty = scanned
    .map((block, index) => ({ block, tag: scanTags[index] }))
    .filter(({ block }) => Array.isArray(block?.transactions) && block.transactions.length > 0);
  const rows = (nonempty.length ? nonempty : scanned.slice(0, displayCount).map((block, index) => ({ block, tag: scanTags[index] })))
    .slice(0, displayCount);
  const caption = document.createElement("p");
  caption.className = "muted";
  caption.textContent = nonempty.length
    ? `Showing ${rows.length} newest transaction-bearing block${rows.length === 1 ? "" : "s"} from the last ${scannedTags.length} scanned blocks.`
    : `No transactions found in the last ${scannedTags.length} scanned blocks; showing the latest ${rows.length} head blocks.`;
  const blockRows = rows.map(({ block, tag }) => ({
    tag,
    number: block?.number ?? tag,
    hash: block?.hash || "—",
    finality: finalityStatus(block),
    gasUsed: block?.gasUsed ?? "—",
    timestamp: block?.timestamp ?? "—",
    txCount: Array.isArray(block?.transactions) ? block.transactions.length : 0,
    missing: !block,
  }));
  renderBlockRows(el, blockRows, caption.textContent);
  writeCache({ blockRows, blockCaption: caption.textContent });
}

function renderBlockRows(el, rows, captionText) {
  el.innerHTML = "";
  const caption = document.createElement("p");
  caption.className = "muted";
  caption.textContent = captionText;
  el.appendChild(caption);
  const wrap = document.createElement("div");
  wrap.className = "table-wrap";
  const table = document.createElement("table");
  const thead = document.createElement("thead");
  thead.innerHTML = "<tr><th>#</th><th>Hash</th><th>Finality</th><th>Gas used</th><th>Time</th><th>Txs</th></tr>";
  table.appendChild(thead);
  const tbody = document.createElement("tbody");
  wireBlockRowClicks(tbody);
  if (!rows.length) {
    const tr = document.createElement("tr");
    const td = document.createElement("td");
    td.colSpan = 6;
    td.className = "muted";
    td.textContent = "—";
    tr.appendChild(td);
    tbody.appendChild(tr);
  }
  for (let i = 0; i < rows.length; i++) {
    const row = rows[i];
    const tr = document.createElement("tr");
    tr.className = "block-row";
    tr.setAttribute("data-block-tag", row.tag);
    tr.title = "Show transactions in this block";
    if (row.missing) {
      const td = document.createElement("td");
      td.colSpan = 6;
      td.className = "muted";
      td.textContent = "(missing)";
      tr.appendChild(td);
      tbody.appendChild(tr);
      continue;
    }
    const cells = [
      { text: row.number, cls: "mono" },
      { text: shortHash(row.hash, 8), cls: "mono" },
      { badge: finalityBadge(row.finality) },
      { text: row.gasUsed },
      { text: row.timestamp, cls: "mono" },
      { text: String(row.txCount) },
    ];
    for (const cell of cells) {
      const td = document.createElement("td");
      if (cell.badge) {
        td.appendChild(cell.badge);
      } else {
        td.textContent = cell.text;
      }
      if (cell.cls) td.className = cell.cls;
      tr.appendChild(td);
    }
    tbody.appendChild(tr);
  }
  table.appendChild(tbody);
  wrap.appendChild(table);
  el.appendChild(wrap);
}

async function showBlockDetail(blockTag) {
  const detail = document.getElementById("blockDetail");
  if (!detail) return;
  detail.textContent = "Fetching block…";
  try {
    const b = await rpc("eth_getBlockByNumber", [blockTag, false]);
    const txs = Array.isArray(b.transactions) ? b.transactions : [];
    detail.innerHTML = "";

    const h3 = document.createElement("h3");
    h3.textContent = `Block ${b.number ?? blockTag} `;
    h3.appendChild(finalityBadge(finalityStatus(b)));
    detail.appendChild(h3);

    if (b.hash) {
      const hashLine = document.createElement("p");
      hashLine.className = "block-hash-line";
      hashLine.textContent = `hash ${b.hash}`;
      detail.appendChild(hashLine);
    }

    const explain = document.createElement("p");
    explain.className = "muted";
    explain.innerHTML =
      "This is <strong>block header</strong> data (identifiers + state roots). It is not a wallet “address.” " +
      "<strong>Account addresses</strong> (20-byte <code>0x…</code>) appear on <strong>transactions</strong> " +
      "(<code>from</code> / <code>to</code>) after you open a block that has txs, or use <strong>Address lookup</strong> below.";
    detail.appendChild(explain);

    const meta = document.createElement("pre");
    meta.className = "block-meta-json";
    const metaObj = {
      number: b.number,
      hash: b.hash,
      parentHash: b.parentHash,
      miner: b.miner,
      extraData: b.extraData,
      stateRoot: b.stateRoot,
      transactionsRoot: b.transactionsRoot,
      gasUsed: b.gasUsed,
      gasLimit: b.gasLimit,
      timestamp: b.timestamp,
      baseFeePerGas: b.baseFeePerGas,
      finalityStatus: finalityStatus(b),
      proofCircuitVersion: b.proofCircuitVersion || null,
      proofCoverageManifestDigest: b.proofCoverageManifestDigest || null,
      proofCoveredFeatures: b.proofCoveredFeatures || null,
      transactionCount: txs.length,
    };
    meta.textContent = JSON.stringify(metaObj, null, 2);
    detail.appendChild(meta);

    if (txs.length === 0) {
      const p = document.createElement("p");
      p.className = "muted";
      p.innerHTML =
        "No transactions in this block — so there are no <code>from</code>/<code>to</code> account lines to show. " +
        "The long <code>hash</code> above is the block’s own id (32 bytes), not an Ethereum account.";
      detail.appendChild(p);
      return;
    }

    const txObjs = await Promise.all(
      txs.map((th) => rpc("eth_getTransactionByHash", [th]).catch(() => null)),
    );

    const ul = document.createElement("ul");
    for (let i = 0; i < txs.length; i++) {
      const th = txs[i];
      const txo = txObjs[i];
      const li = document.createElement("li");
      li.className = "tx-list-item";
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "tx-link mono";
      btn.textContent = th;
      btn.addEventListener("click", (e) => {
        e.stopPropagation();
        document.getElementById("txhash").value = th;
        document.getElementById("txOut").scrollIntoView({ behavior: "smooth", block: "nearest" });
        lookupTxWithHash(th);
      });
      li.appendChild(btn);
      const fromLine = document.createElement("div");
      fromLine.className = "tx-from-line";
      if (txo && typeof txo === "object" && txo.from) {
        const j = makeAddrJump("From", txo.from);
        if (j) fromLine.appendChild(j);
      } else {
        fromLine.textContent = "Sender not returned for this hash (try the tx link).";
      }
      li.appendChild(fromLine);
      ul.appendChild(li);
    }
    detail.appendChild(ul);
  } catch (e) {
    detail.innerHTML = "";
    const p = document.createElement("p");
    p.className = "err";
    p.textContent = String(e);
    detail.appendChild(p);
  }
}

async function refresh(options = {}) {
  const { silent = false } = options;
  const sum = document.getElementById("summary");
  const blk = document.getElementById("blocks");
  const button = document.getElementById("refresh");
  if (button && !silent) button.disabled = true;
  try {
    await Promise.all([loadSummary(sum), loadBlocks(blk)]);
  } catch (e) {
    if (!readCache()) {
      updateHeroStat("heroHead", "Offline");
      updateHeroStat("heroFinality", "Unavailable");
      updateHeroStat("heroTxs", "—");
      sum.innerHTML = "";
      const p = document.createElement("p");
      p.className = "err";
      p.textContent = String(e);
      sum.appendChild(p);
      blk.textContent = "";
    }
  } finally {
    if (button && !silent) button.disabled = false;
  }
}

async function lookupAccount() {
  const raw = document.getElementById("acct").value;
  const out = document.getElementById("accountOut");
  out.textContent = "…";
  try {
    const addr = normalizeAddress(raw);
    if (!addr) {
      out.innerHTML =
        '<p class="err">Enter a 20-byte address: <code>0x</code> plus 40 hex characters, or 40 hex characters without the prefix.</p>';
      return;
    }
    document.getElementById("acct").value = addr;
    const [bal, nonce, code] = await Promise.all([
      rpc("eth_getBalance", [addr, "latest"]),
      rpc("eth_getTransactionCount", [addr, "latest"]),
      rpc("eth_getCode", [addr, "latest"]),
    ]);
    const pre = document.createElement("pre");
    const codeLen = codeByteLen(code);
    pre.textContent = JSON.stringify(
      {
        address: addr,
        balance: bal,
        transactionCount: nonce,
        codeHex: code,
        codeByteLength: codeLen,
        isContract: codeLen > 0,
      },
      null,
      2,
    );
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

async function lookupTxWithHash(raw) {
  const out = document.getElementById("txOut");
  out.textContent = "…";
  try {
    const h = normalizeTxHash(raw);
    if (!h) {
      out.innerHTML =
        '<p class="err">Enter a 32-byte transaction hash: <code>0x</code> plus 64 hex characters, or 64 hex without the prefix.</p>';
      return;
    }
    const txInput = document.getElementById("txhash");
    if (txInput) txInput.value = h;
    const [tx, rc] = await Promise.all([
      rpc("eth_getTransactionByHash", [h]),
      rpc("eth_getTransactionReceipt", [h]),
    ]);
    out.innerHTML = "";
    const stripHost = document.createElement("div");
    renderTxAddressStrip(stripHost, tx, rc);
    out.appendChild(stripHost);
    const pre = document.createElement("pre");
    pre.textContent = JSON.stringify({ transaction: tx, receipt: rc }, null, 2);
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
  const raw = document.getElementById("txhash").value;
  await lookupTxWithHash(raw);
}

document.getElementById("refresh").onclick = () => refresh();
document.getElementById("lookupAcct").onclick = lookupAccount;
document.getElementById("lookupTx").onclick = lookupTx;
const fillH = document.getElementById("fillHardhat0");
if (fillH) {
  fillH.addEventListener("click", () => {
    document.getElementById("acct").value = "0xf39fd6e51aad88f6f4ce6ab8827279cfffb92266";
    document.getElementById("accountOut").scrollIntoView({ behavior: "smooth", block: "nearest" });
    lookupAccount();
  });
}
window.addEventListener("load", () => {
  renderCachedSnapshot();
  renderInitialSnapshot();
  void refresh({ silent: true });
});
