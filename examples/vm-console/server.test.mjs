import assert from 'node:assert/strict';
import { test } from 'node:test';
import { consoleUpstream, isConsoleRequest, startConsole } from './server.mjs';

test('console capability requires the selected path, exact origin and token', () => {
  const token = 'a'.repeat(64);
  const origin = 'http://127.0.0.1:30303';
  const request = { url: '/console', headers: { host: '127.0.0.1:30303', origin, 'sec-websocket-protocol': `binary, nanocl-${token}` } };
  assert.equal(isConsoleRequest(request, origin, token), true);
  for (const change of [
    { origin: 'http://evil.example' },
    { origin: 'null' },
    { host: 'evil.example' },
    { 'sec-websocket-protocol': 'binary' },
    { 'sec-websocket-protocol': `nanocl-${token}` },
    { 'sec-websocket-protocol': `binary, nanocl-${'b'.repeat(64)}` },
  ]) assert.equal(isConsoleRequest({ ...request, headers: { ...request.headers, ...change } }, origin, token), false);
  assert.equal(isConsoleRequest({ ...request, url: '/console?token=anything' }, origin, token), false);
});

test('console upstream follows configured transport and optional TLS settings', () => {
  for (const [host, expected] of [['http://localhost:8585', 'ws://localhost:8585'], ['https://localhost:8443', 'wss://localhost:8443']]) {
    const upstream = consoleUpstream('global.desktop', host);
    assert.equal(upstream.upstreamUrl, `${expected}/v0.19.0/vms/global.desktop/console`);
    assert.deepEqual(upstream.upstreamOptions, {});
  }
  const tls = { ca: 'configured CA', cert: 'configured certificate', key: 'configured key', rejectUnauthorized: false };
  assert.deepEqual(consoleUpstream('global.desktop', 'https://localhost:8443', tls).upstreamOptions, tls);
  assert.deepEqual(consoleUpstream('global.desktop', 'http://localhost:8585', tls).upstreamOptions, {});
  const unix = consoleUpstream('global.desktop', 'unix:///run/nanocl/nanocl.sock');
  assert.equal(unix.upstreamUrl, 'ws://localhost/v0.19.0/vms/global.desktop/console');
  assert.equal(typeof unix.upstreamOptions.createConnection, 'function');
  for (const host of ['ftp://localhost:8585', 'https://user:password@localhost:8443']) {
    assert.throws(() => consoleUpstream('global.desktop', host), /HTTP or HTTPS without URL credentials/);
  }
  assert.throws(() => consoleUpstream('desktop', 'http://localhost'), /canonical namespace.name/);
  assert.throws(() => consoleUpstream('global.desktop', 'unix://relative.sock'), /must be absolute/);
});

test('browser entry point and published noVNC ES module are served locally', async () => {
  const bridge = await startConsole({ key: 'global.desktop' });
  try {
    const url = new URL(bridge.url);
    const page = await fetch(url.origin);
    assert.equal(page.status, 200);
    const script = await fetch(new URL('/client.js', url));
    assert.equal(script.status, 200);
    assert.match(script.headers.get('content-type'), /javascript/);
    const source = await script.text();
    assert.match(source, /RFB/);
    const library = await fetch(new URL('/novnc/core/rfb.js', url));
    assert.equal(library.status, 200);
    assert.match(await library.text(), /export default class RFB/);
    assert.equal((await fetch(new URL('/novnc/core/util/browser.js', url))).status, 200);
  } finally { await bridge.close(); }
});
