// Explicitly destructive release test for one already-created disposable VM.
// It stops, starts, restarts, replaces, and finally deletes that VM.
import assert from 'node:assert/strict';
import { once } from 'node:events';
import { readFile } from 'node:fs/promises';
import { request as httpRequest } from 'node:http';
import { request as httpsRequest } from 'node:https';
import { createConnection } from 'node:net';
import { setImmediate as yieldTurn } from 'node:timers/promises';
import { WebSocket } from 'ws';

const [key, consent] = process.argv.slice(2);
assert(consent === '--disposable' && /^[^\s/?#%\\]+\.[^\s./?#%\\]+$/.test(key), 'Usage: npm run test:api -- namespace.disposable-vm --disposable (deletes this VM)');
const host = process.env.NANOCL_HOST ?? 'unix:///run/nanocl/nanocl.sock';
const tls = {};
for (const [name, variable] of [['ca', 'NANOCL_TLS_CA'], ['cert', 'NANOCL_TLS_CERT'], ['key', 'NANOCL_TLS_KEY']]) {
  if (process.env[variable]) tls[name] = await readFile(process.env[variable]);
}
const api = '/v0.19.0';
const vmPath = `/vms/${encodeURIComponent(key)}`;
const requestTimeout = 30_000;
// Default ACPI grace (60s), restart startup (30s), then response delivery.
const lifecycleTimeout = 60_000 + 30_000 + requestTimeout;
const peers = new Set();
const passed = [];
const skipped = [];
function pass(name) { passed.push(name); console.error(`PASS ${name}`); }
function endpoint(host, path, credentials = tls) {
  if (host.startsWith('unix://')) {
    const socketPath = host.slice(7);
    return { url: `http://localhost${api}${path}`, options: { socketPath, createConnection: () => createConnection({ path: socketPath }) } };
  }
  const url = new URL(host);
  url.pathname = api + path;
  url.search = '';
  url.hash = '';
  return { url: url.href, options: { ...credentials, rejectUnauthorized: true } };
}
async function request(method, path, body, timeout = requestTimeout) {
  const { url, options } = endpoint(host, path);
  const data = body === undefined ? undefined : JSON.stringify(body);
  return await new Promise((resolve, reject) => {
    const request = (url.startsWith('https:') ? httpsRequest : httpRequest)(url, {
      ...options, method, signal: AbortSignal.timeout(timeout), headers: data ? { 'Content-Type': 'application/json', 'Content-Length': Buffer.byteLength(data) } : {},
    }, response => {
      const chunks = [];
      response.on('data', chunk => chunks.push(chunk));
      response.once('end', () => {
        const text = Buffer.concat(chunks).toString();
        let body;
        try { body = JSON.parse(text); } catch { body = text; }
        resolve({ status: response.statusCode, body });
      });
    });
    request.setTimeout(timeout, () => request.destroy(new Error('API request timed out')));
    request.once('error', reject);
    request.end(data);
  });
}
function connect(target = host, path = vmPath + '/console', headers = {}, credentials = tls) {
  const { url, options } = endpoint(target, path, credentials);
  const socket = new WebSocket(url.replace(/^http/, 'ws'), ['binary'], { ...options, headers, handshakeTimeout: 10_000, maxPayload: 65_536, perMessageDeflate: false });
  peers.add(socket);
  socket.once('close', () => peers.delete(socket));
  return socket;
}
async function expectRejected(target, path, headers, expected, credentials = tls) {
  const socket = connect(target, path, headers, credentials);
  try {
    await new Promise((resolve, reject) => {
      socket.once('open', () => reject(new Error('Unauthorized console upgraded')));
      socket.once('unexpected-response', (_request, response) => {
        response.resume();
        try { assert.equal(response.statusCode, expected); resolve(); } catch (error) { reject(error); }
      });
      socket.once('error', error => {
        const rejectedTls = /^ERR_SSL_/.test(error.code ?? '')
          || (error.code === 'ECONNRESET' && /before secure TLS connection was established/.test(error.message));
        if (expected === 'tls' && rejectedTls) resolve();
        else reject(error);
      });
    });
  } finally { socket.terminate(); }
}
async function readyConsole(target = host, headers = {}) {
  const deadline = Date.now() + 60_000;
  while (true) {
    const socket = connect(target, vmPath + '/console', headers);
    try {
      await new Promise((resolve, reject) => {
        const timeout = setTimeout(() => reject(new Error('Console did not send an RFB banner')), 10_000);
        const succeed = () => { clearTimeout(timeout); resolve(); };
        const fail = error => { clearTimeout(timeout); reject(error); };
        let banner = Buffer.alloc(0);
        socket.once('unexpected-response', (_request, response) => {
          response.resume();
          const error = new Error(`Console HTTP ${response.statusCode}`);
          error.status = response.statusCode;
          fail(error);
        });
        socket.once('error', fail);
        socket.once('close', () => fail(new Error('Console closed before the RFB banner')));
        socket.on('message', (data, binary) => {
          try {
            assert.equal(binary, true);
            banner = Buffer.concat([banner, data]);
            if (banner.length >= 12) {
              assert.match(banner.subarray(0, 12).toString(), /^RFB 003\.00[378]\n$/);
              succeed();
            }
          } catch (error) { fail(error); }
        });
      });
      return socket;
    } catch (error) {
      socket.terminate();
      if (Date.now() >= deadline || error.status !== 409) throw error;
      // Retry on the actual not-ready response; no fixed boot delay is proof.
      await yieldTurn();
    }
  }
}
const closed = (socket, timeout = requestTimeout) => once(socket, 'close', { signal: AbortSignal.timeout(timeout) });
async function close(socket) { const done = closed(socket); socket.close(); await done; }
async function mutate(method, path, body) {
  const result = await request(method, path, body, lifecycleTimeout);
  assert(result.status >= 200 && result.status < 300, `${method} ${path}: ${result.status} ${JSON.stringify(result.body)}`);
  return result.body;
}
async function settled(actual, consoleEnabled) {
  const deadline = Date.now() + lifecycleTimeout;
  while (true) {
    const result = await request('GET', vmPath + '/inspect');
    assert.equal(result.status, 200);
    const vm = result.body;
    const runtime = vm.Instances?.find(instance => instance.Name === `${key}.v`);
    const labels = runtime?.Data?.Config?.Labels;
    if (vm.Status.Actual === actual && vm.Status.Wanted === actual
      && (consoleEnabled === undefined || (vm.Spec.Console === consoleEnabled
        && labels?.['io.nanocl.vm-spec'] === vm.Spec.Key
        && labels?.['io.nanocl.vm-console'] === String(consoleEnabled)))) return;
    assert(Date.now() < deadline, `VM did not settle: ${JSON.stringify(vm.Status)}`);
    // Mutation responses schedule work; observe its completion before the next.
    await yieldTurn();
  }
}
try {
  const original = await request('GET', vmPath + '/inspect');
  assert.equal(original.status, 200);
  assert.equal(original.body.Spec.Console, true, 'Disposable VM must initially enable Console');
  let socket = await readyConsole();
  await close(socket);
  socket = await readyConsole();
  await close(socket);
  pass('RFB binary banner and disconnect/reconnect');
  for (const payload of ['text frames are not RFB', Buffer.alloc(65_537)]) {
    socket = await readyConsole();
    const rejected = closed(socket);
    socket.send(payload);
    await rejected;
  }
  pass('text and oversized input close the console');
  for (const origin of ['https://foreign.example', 'null']) {
    socket = await readyConsole(host, { Origin: origin });
    await close(socket);
  }
  pass('console follows daemon access policy with foreign/null Origin');
  await expectRejected(host, '/vms/not-a-canonical-key/console', {}, 400);
  pass('malformed key rejected');
  if (process.env.NANOCL_PLAINTEXT_HOST) {
    socket = await readyConsole(process.env.NANOCL_PLAINTEXT_HOST);
    await close(socket);
    pass('console works through the configured plaintext listener');
  } else skipped.push('plaintext listener: NANOCL_PLAINTEXT_HOST unset');
  if (host.startsWith('https:')) {
    await expectRejected(host, vmPath + '/console', {}, 'tls', { ca: tls.ca });
    pass('missing client certificate rejected after successful mTLS console');
  } else skipped.push('missing client certificate: NANOCL_HOST is not HTTPS');
  socket = await readyConsole();
  let oldClosed = closed(socket, lifecycleTimeout);
  await mutate('POST', `/processes/vm/${encodeURIComponent(key)}/stop`);
  await oldClosed;
  await settled('stop');
  await expectRejected(host, vmPath + '/console', {}, 409);
  pass('stop closes old console and rejects stopped runtime');
  await mutate('POST', `/processes/vm/${encodeURIComponent(key)}/start`);
  await settled('start', true);
  socket = await readyConsole();
  pass('start reconnects');
  oldClosed = closed(socket, lifecycleTimeout);
  await mutate('POST', `/processes/vm/${encodeURIComponent(key)}/restart`);
  await oldClosed;
  await settled('start', true);
  socket = await readyConsole();
  pass('restart closes old console and reconnects');
  oldClosed = closed(socket, lifecycleTimeout);
  await mutate('PATCH', vmPath, { Console: false });
  await expectRejected(host, vmPath + '/console', {}, 403);
  await oldClosed;
  await settled('start', false);
  pass('console disable rejects access and closes old generation');
  await mutate('PATCH', vmPath, { Console: true });
  await settled('start', true);
  socket = await readyConsole();
  oldClosed = closed(socket, lifecycleTimeout);
  await mutate('PATCH', vmPath, { Hostname: 'console-replacement-test' });
  await oldClosed;
  await settled('start', true);
  socket = await readyConsole();
  pass('replacement closes old console and reconnects');
  oldClosed = closed(socket, lifecycleTimeout);
  await mutate('DELETE', vmPath);
  await oldClosed;
  // Delete API schedules work; use its observed missing state as the barrier.
  const deadline = Date.now() + requestTimeout;
  while ((await request('GET', vmPath + '/inspect')).status !== 404) {
    assert(Date.now() < deadline, 'Deleted VM remained in the API');
    await yieldTurn();
  }
  await expectRejected(host, vmPath + '/console', {}, 404);
  pass('delete closes console and rejects deleted VM');
  console.log(JSON.stringify({ key, passed, skipped }, null, 2));
} catch (error) {
  console.log(JSON.stringify({ key, passed, skipped, failure: String(error) }, null, 2));
  throw error;
} finally {
  for (const peer of peers) peer.terminate();
}
