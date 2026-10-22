import { randomBytes, timingSafeEqual } from 'node:crypto';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { createConnection } from 'node:net';
import { dirname, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { WebSocket, WebSocketServer, createWebSocketStream } from 'ws';

const directory = dirname(fileURLToPath(import.meta.url));

export function isConsoleRequest(request, origin, token) {
  const offered = String(request.headers['sec-websocket-protocol'] ?? '').split(',').map(item => item.trim());
  const supplied = offered.find(item => item.startsWith('nanocl-'))?.slice(7) ?? '';
  return request.url === '/console'
    && offered.includes('binary')
    && request.headers.origin === origin
    && request.headers.host === new URL(origin).host
    && /^[a-f0-9]{64}$/.test(supplied)
    && timingSafeEqual(Buffer.from(supplied), Buffer.from(token));
}

// The bridge holds the operator's daemon credentials. The browser gets only a
// random, process-scoped capability for this one VM, carried in a subprotocol.
export async function startConsole({ key, host = 'unix:///run/nanocl/nanocl.sock', tls = {}, port = 0 }) {
  if (!/^[^\s/?#%\\]+\.[^\s./?#%\\]+$/.test(key)) throw new Error('Supply the canonical namespace.name VM key');
  const token = randomBytes(32).toString('hex');
  const upstreamPath = `/v0.19.0/vms/${encodeURIComponent(key)}/console`;
  let upstreamUrl;
  let upstreamOptions;
  if (host.startsWith('unix://')) {
    const socketPath = host.slice(7);
    if (!socketPath.startsWith('/')) throw new Error('Daemon Unix socket path must be absolute');
    upstreamUrl = `ws://localhost${upstreamPath}`;
    upstreamOptions = { createConnection: () => createConnection({ path: socketPath }) };
  } else {
    const url = new URL(host);
    if (url.protocol !== 'https:' || url.username || url.password || !tls.ca || !tls.cert || !tls.key) {
      throw new Error('Remote consoles require HTTPS, a CA, a client certificate and its key');
    }
    url.protocol = 'wss:';
    url.pathname = upstreamPath;
    url.search = '';
    url.hash = '';
    upstreamUrl = url.href;
    upstreamOptions = { ...tls, rejectUnauthorized: true };
  }
  let origin;
  const peers = new Set();
  const server = createServer(async (request, response) => {
    if (request.method !== 'GET' || request.headers.host !== new URL(origin).host) {
      response.writeHead(403).end();
      return;
    }
    const url = new URL(request.url, origin);
    let path;
    if (url.pathname === '/') path = resolve(directory, 'index.html');
    else if (url.pathname === '/client.js') path = resolve(directory, 'client.js');
    else if (url.pathname.startsWith('/novnc/')) {
      const root = resolve(directory, 'node_modules/@novnc/novnc');
      path = resolve(root, url.pathname.slice('/novnc/'.length));
      if (!path.startsWith(root + sep)) path = undefined;
    }
    if (!path) { response.writeHead(404).end(); return; }
    try {
      const data = await readFile(path);
      response.writeHead(200, {
        'Content-Type': path.endsWith('.js') ? 'text/javascript; charset=utf-8' : 'text/html; charset=utf-8',
        'Cache-Control': 'no-store',
        'Referrer-Policy': 'no-referrer',
        'X-Content-Type-Options': 'nosniff',
        'Content-Security-Policy': "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; frame-ancestors 'none'",
      }).end(data);
    } catch { response.writeHead(404).end(); }
  });
  const websocket = new WebSocketServer({ noServer: true, maxPayload: 65_536, perMessageDeflate: false, handleProtocols: () => 'binary' });
  server.on('upgrade', (request, socket, head) => {
    if (!isConsoleRequest(request, origin, token)) {
      socket.end('HTTP/1.1 403 Forbidden\r\nConnection: close\r\nContent-Length: 0\r\n\r\n');
      return;
    }
    const upstream = new WebSocket(upstreamUrl, ['binary'], { ...upstreamOptions, handshakeTimeout: 10_000, maxPayload: 65_536, perMessageDeflate: false });
    peers.add(upstream);
    const cancelPending = () => upstream.terminate();
    socket.once('close', cancelPending);
    const rejectPending = () => {
      if (!socket.destroyed) socket.end('HTTP/1.1 502 Bad Gateway\r\nConnection: close\r\nContent-Length: 0\r\n\r\n');
    };
    upstream.once('error', rejectPending);
    upstream.once('close', () => peers.delete(upstream));
    upstream.once('open', () => {
      upstream.removeListener('error', rejectPending);
      socket.removeListener('close', cancelPending);
      if (socket.destroyed) { upstream.terminate(); return; }
      websocket.handleUpgrade(request, socket, head, browser => {
        peers.add(browser);
        const browserStream = createWebSocketStream(browser, { highWaterMark: 65_536 });
        const upstreamStream = createWebSocketStream(upstream, { highWaterMark: 65_536 });
        const close = () => {
          peers.delete(browser);
          browser.terminate();
          upstream.terminate();
          browserStream.destroy();
          upstreamStream.destroy();
        };
        browserStream.on('error', close);
        upstreamStream.on('error', close);
        browser.once('close', close);
        upstream.once('close', close);
        browserStream.pipe(upstreamStream).pipe(browserStream);
      });
    });
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(port, '127.0.0.1', resolve);
  });
  origin = `http://127.0.0.1:${server.address().port}`;
  return {
    url: `${origin}/#${token}`,
    async close() {
      for (const peer of peers) peer.terminate();
      websocket.close();
      server.closeAllConnections();
      await new Promise(resolve => server.close(resolve));
    },
  };
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  const tls = {};
  for (const [name, variable] of [['ca', 'NANOCL_TLS_CA'], ['cert', 'NANOCL_TLS_CERT'], ['key', 'NANOCL_TLS_KEY']]) {
    if (process.env[variable]) tls[name] = await readFile(process.env[variable]);
  }
  const consoleServer = await startConsole({ key: process.argv[2], host: process.env.NANOCL_HOST, tls });
  console.log(`Open ${consoleServer.url}`);
  console.log('This link grants console access to the selected VM until this process exits.');
  for (const signal of ['SIGINT', 'SIGTERM']) process.once(signal, async () => { await consoleServer.close(); });
}
