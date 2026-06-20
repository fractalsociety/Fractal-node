import { createServer } from "node:http";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname } from "node:path";

const HOST = process.env.EXPLORER_RPC_CACHE_HOST || "127.0.0.1";
const PORT = Number(process.env.EXPLORER_RPC_CACHE_PORT || "18546");
const UPSTREAM = process.env.EXPLORER_RPC_UPSTREAM || "http://192.3.47.245:8545";
const CACHE_FILE = process.env.EXPLORER_RPC_CACHE_FILE || "/var/lib/fractal-explorer/rpc-cache.json";
const UPSTREAM_TIMEOUT_MS = Number(process.env.EXPLORER_RPC_TIMEOUT_MS || "4500");
const MAX_BODY_BYTES = Number(process.env.EXPLORER_RPC_MAX_BODY_BYTES || "1048576");

const CACHEABLE_METHODS = new Set([
  "eth_blockNumber",
  "eth_chainId",
  "eth_gasPrice",
  "eth_getBalance",
  "eth_getBlockByNumber",
  "eth_getCode",
  "eth_getTransactionByHash",
  "eth_getTransactionCount",
  "eth_getTransactionReceipt",
  "net_version",
  "web3_clientVersion",
]);

let cache = new Map();
let saveTimer = null;

function cacheKey(req) {
  return JSON.stringify({ method: req.method, params: Array.isArray(req.params) ? req.params : [] });
}

function jsonRpcError(id, code, message) {
  return { jsonrpc: "2.0", id: id ?? null, error: { code, message } };
}

function isCacheable(req) {
  return req && typeof req.method === "string" && CACHEABLE_METHODS.has(req.method);
}

async function loadCache() {
  try {
    const raw = await readFile(CACHE_FILE, "utf8");
    const parsed = JSON.parse(raw);
    if (parsed && Array.isArray(parsed.entries)) {
      cache = new Map(parsed.entries);
    }
  } catch {
    cache = new Map();
  }
}

function scheduleSave() {
  if (saveTimer) return;
  saveTimer = setTimeout(async () => {
    saveTimer = null;
    try {
      await mkdir(dirname(CACHE_FILE), { recursive: true });
      await writeFile(
        CACHE_FILE,
        JSON.stringify({ savedAt: new Date().toISOString(), entries: [...cache.entries()] }),
      );
    } catch (err) {
      console.error(`cache save failed: ${err.message}`);
    }
  }, 250);
}

async function readBody(req) {
  const chunks = [];
  let size = 0;
  for await (const chunk of req) {
    size += chunk.length;
    if (size > MAX_BODY_BYTES) throw new Error("request body too large");
    chunks.push(chunk);
  }
  return Buffer.concat(chunks).toString("utf8");
}

async function forward(rawBody) {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), UPSTREAM_TIMEOUT_MS);
  try {
    const response = await fetch(UPSTREAM, {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: rawBody,
      signal: controller.signal,
    });
    const text = await response.text();
    if (!response.ok) throw new Error(`upstream HTTP ${response.status}: ${text.slice(0, 160)}`);
    return JSON.parse(text);
  } finally {
    clearTimeout(timer);
  }
}

function withRequestId(cachedResponse, id) {
  return { ...cachedResponse, id: id ?? null };
}

function cachedFallbackFor(requestPayload) {
  const requests = Array.isArray(requestPayload) ? requestPayload : [requestPayload];
  const responses = requests.map((req) => {
    if (!isCacheable(req)) return jsonRpcError(req?.id, -32098, "upstream unavailable and method is not cached");
    const cached = cache.get(cacheKey(req));
    return cached ? withRequestId(cached.response, req.id) : jsonRpcError(req?.id, -32099, "upstream unavailable and no cached response");
  });
  return Array.isArray(requestPayload) ? responses : responses[0];
}

function remember(requestPayload, responsePayload) {
  const requests = Array.isArray(requestPayload) ? requestPayload : [requestPayload];
  const responses = Array.isArray(responsePayload) ? responsePayload : [responsePayload];
  let changed = false;
  for (let i = 0; i < requests.length; i++) {
    const req = requests[i];
    const res = responses[i];
    if (!isCacheable(req) || !res || res.error || !Object.hasOwn(res, "result")) continue;
    cache.set(cacheKey(req), {
      savedAt: new Date().toISOString(),
      response: { jsonrpc: "2.0", result: res.result },
    });
    changed = true;
  }
  if (changed) scheduleSave();
}

function sendJson(res, status, body, extraHeaders = {}) {
  const text = JSON.stringify(body);
  res.writeHead(status, {
    "content-type": "application/json; charset=utf-8",
    "access-control-allow-origin": "*",
    "access-control-allow-methods": "POST, OPTIONS, GET",
    "access-control-allow-headers": "Content-Type",
    "cache-control": "no-store",
    ...extraHeaders,
  });
  res.end(text);
}

async function handleRpc(req, res) {
  let rawBody;
  let requestPayload;
  try {
    rawBody = await readBody(req);
    requestPayload = JSON.parse(rawBody);
  } catch (err) {
    sendJson(res, 400, jsonRpcError(null, -32700, err.message));
    return;
  }

  try {
    const upstreamResponse = await forward(rawBody);
    remember(requestPayload, upstreamResponse);
    sendJson(res, 200, upstreamResponse, { "x-fractal-rpc-cache": "fresh" });
  } catch (err) {
    const fallback = cachedFallbackFor(requestPayload);
    console.warn(`serving cached RPC fallback: ${err.message}`);
    sendJson(res, 200, fallback, { "x-fractal-rpc-cache": "stale" });
  }
}

await loadCache();

const server = createServer((req, res) => {
  if (req.method === "OPTIONS") {
    sendJson(res, 204, {});
    return;
  }
  if (req.method === "GET" && req.url === "/healthz") {
    sendJson(res, 200, {
      ok: true,
      upstream: UPSTREAM,
      cache_entries: cache.size,
      cache_file: CACHE_FILE,
    });
    return;
  }
  if (req.method === "POST" && (req.url === "/" || req.url === "/rpc")) {
    void handleRpc(req, res);
    return;
  }
  sendJson(res, 404, { error: "not found" });
});

server.listen(PORT, HOST, () => {
  console.log(`fractal explorer RPC cache listening on http://${HOST}:${PORT}; upstream=${UPSTREAM}`);
});
