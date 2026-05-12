function rpcUrl() {
  const p = new URLSearchParams(window.location.search);
  return p.get("rpc") || "http://127.0.0.1:8545";
}

async function rpc(method, params = []) {
  const r = await fetch(rpcUrl(), {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ jsonrpc: "2.0", id: 1, method, params }),
  });
  const j = await r.json();
  if (j.error) throw new Error(JSON.stringify(j.error));
  return j.result;
}

async function refresh() {
  const el = document.getElementById("status");
  el.textContent = "Loading…";
  try {
    const chainId = await rpc("eth_chainId");
    const bn = await rpc("eth_blockNumber");
    const block = await rpc("eth_getBlockByNumber", [bn, false]);
    const hash = block && block.hash ? block.hash : "(no hash)";
    el.innerHTML = "";
    const pre = document.createElement("pre");
    pre.textContent = JSON.stringify(
      {
        rpc: rpcUrl(),
        chainId,
        blockNumber: bn,
        blockHash: hash,
      },
      null,
      2,
    );
    el.appendChild(pre);
  } catch (e) {
    el.textContent = String(e);
  }
}

document.getElementById("refresh").onclick = refresh;
window.addEventListener("load", refresh);
