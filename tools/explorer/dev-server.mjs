import { createReadStream, existsSync } from 'node:fs';
import { createServer } from 'node:http';
import { extname, join, normalize } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = fileURLToPath(new URL('.', import.meta.url));
const port = Number(process.env.EXPLORER_PORT || 3333);
const rpcUrl = process.env.EXPLORER_RPC_URL || 'http://127.0.0.1:8545';

const types = {
  '.css': 'text/css; charset=utf-8',
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.mjs': 'text/javascript; charset=utf-8',
  '.png': 'image/png',
  '.svg': 'image/svg+xml',
};

function send(res, status, body, type = 'text/plain; charset=utf-8') {
  res.writeHead(status, {
    'content-type': type,
    'access-control-allow-origin': '*',
  });
  res.end(body);
}

async function proxyRpc(req, res) {
  if (req.method === 'OPTIONS') {
    res.writeHead(204, {
      'access-control-allow-origin': '*',
      'access-control-allow-methods': 'POST, OPTIONS',
      'access-control-allow-headers': 'content-type',
    });
    res.end();
    return;
  }
  if (req.method !== 'POST') {
    send(res, 405, 'RPC proxy only accepts POST');
    return;
  }

  const chunks = [];
  for await (const chunk of req) chunks.push(chunk);
  try {
    const upstream = await fetch(rpcUrl, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: Buffer.concat(chunks),
    });
    const text = await upstream.text();
    res.writeHead(upstream.status, {
      'content-type': upstream.headers.get('content-type') || 'application/json; charset=utf-8',
      'access-control-allow-origin': '*',
    });
    res.end(text);
  } catch (error) {
    send(res, 502, JSON.stringify({
      jsonrpc: '2.0',
      id: null,
      error: {
        code: -32099,
        message: `RPC proxy failed: ${error instanceof Error ? error.message : String(error)}`,
      },
    }), 'application/json; charset=utf-8');
  }
}

createServer((req, res) => {
  const url = new URL(req.url || '/', `http://${req.headers.host || '127.0.0.1'}`);
  if (url.pathname === '/rpc') {
    void proxyRpc(req, res);
    return;
  }

  const requested = url.pathname === '/' ? '/index.html' : decodeURIComponent(url.pathname);
  const safe = normalize(requested).replace(/^(\.\.[/\\])+/, '');
  const filePath = join(root, safe);
  if (!filePath.startsWith(root) || !existsSync(filePath)) {
    send(res, 404, 'Not found');
    return;
  }
  res.writeHead(200, {
    'content-type': types[extname(filePath)] || 'application/octet-stream',
  });
  createReadStream(filePath).pipe(res);
}).listen(port, '127.0.0.1', () => {
  console.log(`FractalChain explorer: http://127.0.0.1:${port}/`);
  console.log('RPC proxy: /rpc -> configured upstream');
});
