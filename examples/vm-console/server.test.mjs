import assert from 'node:assert/strict';
import { test } from 'node:test';
import { isConsoleRequest, startConsole } from './server.mjs';

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

test('remote console configuration requires verified mutual TLS credentials', async () => {
  for (const host of ['http://localhost:8585', 'https://localhost:8443', 'https://user:password@localhost:8443']) {
    await assert.rejects(startConsole({ key: 'global.desktop', host }), /Remote consoles require HTTPS/);
  }
  await assert.rejects(startConsole({ key: 'desktop' }), /canonical namespace.name/);
  await assert.rejects(startConsole({ key: 'global.desktop', host: 'unix://relative.sock' }), /must be absolute/);
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
