// Run only against a disposable guest at a visible login prompt. This types a
// marker without Enter, captures it for review, then erases it with Backspace.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { EventEmitter, once } from 'node:events';
import { mkdir, mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { WebSocket } from 'ws';
import { startConsole } from './server.mjs';

const [key, output] = process.argv.slice(2);
assert(key && output, 'Usage: npm run smoke -- namespace.disposable-vm /absolute/evidence-directory');
assert(output.startsWith('/'), 'Evidence directory must be absolute and outside this example');
await mkdir(output, { recursive: true });
const tls = {};
for (const [name, variable] of [['ca', 'NANOCL_TLS_CA'], ['cert', 'NANOCL_TLS_CERT'], ['key', 'NANOCL_TLS_KEY']]) {
  if (process.env[variable]) tls[name] = await readFile(process.env[variable]);
}
const bridge = await startConsole({ key, host: process.env.NANOCL_HOST, tls });
const profile = await mkdtemp(join(tmpdir(), 'nanocl-console-browser-'));
const chrome = spawn(process.env.CHROME_BIN ?? '/usr/bin/google-chrome', [
  '--headless=new', '--disable-gpu', '--disable-dev-shm-usage', '--no-first-run',
  '--no-default-browser-check', '--remote-debugging-address=127.0.0.1',
  '--remote-debugging-port=0', `--user-data-dir=${profile}`, 'about:blank',
], { stdio: ['ignore', 'ignore', 'pipe'] });
let websocket;
let captureFailure;
try {
  const endpoint = await new Promise((resolve, reject) => {
    let log = '';
    const timeout = setTimeout(() => reject(new Error('Chrome did not expose its local debugging socket')), 20_000);
    chrome.once('error', reject);
    chrome.once('exit', code => { clearTimeout(timeout); reject(new Error(`Chrome exited (${code})`)); });
    chrome.stderr.on('data', data => {
      log = (log + data).slice(-8192);
      const url = log.match(/DevTools listening on (ws:\/\/[^\s]+)/)?.[1];
      if (url) { clearTimeout(timeout); resolve(url); }
    });
  });
  websocket = new WebSocket(endpoint);
  await once(websocket, 'open');
  let sequence = 0;
  const pending = new Map();
  const events = new EventEmitter();
  const exceptions = [];
  websocket.on('message', data => {
    const message = JSON.parse(data);
    if (message.id) {
      const request = pending.get(message.id);
      if (!request) return;
      pending.delete(message.id);
      clearTimeout(request.timeout);
      if (message.error) request.reject(new Error(message.error.message));
      else request.resolve(message.result);
    } else {
      if (message.method === 'Runtime.exceptionThrown') exceptions.push(message.params.exceptionDetails.exception?.description ?? message.params.exceptionDetails.text);
      events.emit(message.method, message.params);
    }
  });
  const send = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
    const id = ++sequence;
    const timeout = setTimeout(() => { pending.delete(id); reject(new Error(`CDP timeout: ${method}`)); }, 40_000);
    pending.set(id, { resolve, reject, timeout });
    websocket.send(JSON.stringify({ id, method, params, ...(sessionId ? { sessionId } : {}) }));
  });
  const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
  const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
  const command = (method, params) => send(method, params, sessionId);
  const evaluate = async expression => {
    const result = await command('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
    if (result.exceptionDetails) throw new Error(`${result.exceptionDetails.exception?.description ?? result.exceptionDetails.text}; browser exceptions: ${JSON.stringify(exceptions)}`);
    return result.result.value;
  };
  await command('Runtime.enable');
  await command('Page.enable');
  await command('Emulation.setDeviceMetricsOverride', { width: 1280, height: 800, deviceScaleFactor: 1, mobile: false });
  const navigate = async () => {
    // The capability is a fragment; navigate away so reconnect is a new page,
    // even after client.js removes that fragment from browser history.
    const blankLoaded = once(events, 'Page.loadEventFired', { signal: AbortSignal.timeout(20_000) });
    await command('Page.navigate', { url: 'about:blank' });
    await blankLoaded;
    const loaded = once(events, 'Page.loadEventFired', { signal: AbortSignal.timeout(20_000) });
    const result = await command('Page.navigate', { url: bridge.url });
    assert(!result.errorText, result.errorText);
    await loaded;
    await evaluate(`new Promise((resolve, reject) => {
      const deadline = performance.now() + 30000;
      function check() {
        const status = document.getElementById('status')?.textContent;
        const canvas = document.querySelector('canvas');
        if (status === 'Connected' && canvas?.width && canvas?.height) {
          const pixels = canvas.getContext('2d').getImageData(0, 0, canvas.width, canvas.height).data;
          for (let i = 4; i < pixels.length; i += 4) {
            if (pixels[i] !== pixels[0] || pixels[i + 1] !== pixels[1] || pixels[i + 2] !== pixels[2]) return resolve(true);
          }
        }
        if (performance.now() > deadline) return reject(new Error('No connected, rendered guest framebuffer: ' + status));
        requestAnimationFrame(check);
      }
      check();
    })`);
  };
  const screenshot = async name => {
    const { data } = await command('Page.captureScreenshot', { format: 'png' });
    await writeFile(resolve(output, name), Buffer.from(data, 'base64'));
  };
  captureFailure = async error => {
    await screenshot('console-failure.png');
    await writeFile(resolve(output, 'console-failure.json'), JSON.stringify({ error: String(error), browserExceptions: exceptions }, null, 2));
  };
  const framebufferHash = `(() => {
    const c = document.querySelector('canvas');
    const p = c.getContext('2d').getImageData(0, 0, c.width, c.height).data;
    let hash = 2166136261;
    for (let i = 0; i < p.length; i++) hash = Math.imul(hash ^ p[i], 16777619);
    return hash >>> 0;
  })()`;
  await navigate();
  await screenshot('console-connected.png');
  await evaluate(`document.querySelector('canvas').focus()`);
  const marker = 'nanocl-console-probe';
  const press = async key => {
    const before = await evaluate(framebufferHash);
    const params = { key, windowsVirtualKeyCode: key === 'Backspace' ? 8 : key === '-' ? 189 : key.toUpperCase().charCodeAt(0) };
    await command('Input.dispatchKeyEvent', { type: 'keyDown', ...params, ...(key === 'Backspace' ? {} : { text: key }) });
    await command('Input.dispatchKeyEvent', { type: 'keyUp', ...params });
    // Observe each guest display update before sending the next key. This
    // avoids capturing only the first partial update of a queued marker.
    await evaluate(`new Promise((resolve, reject) => {
    const deadline = performance.now() + 10000;
    function check() {
      if (${framebufferHash} !== ${before}) return resolve(true);
      if (performance.now() > deadline) return reject(new Error('Guest framebuffer did not change after keyboard input'));
      requestAnimationFrame(check);
    }
    check();
    })`);
  };
  for (const key of marker) await press(key);
  await evaluate(`new Promise((resolve, reject) => {
    const deadline = performance.now() + 10000;
    let previous, unchanged = 0;
    function check() {
      const current = ${framebufferHash};
      unchanged = current === previous ? unchanged + 1 : 0;
      previous = current;
      if (unchanged >= 10) return resolve(true);
      if (performance.now() > deadline) return reject(new Error('Guest display did not settle for the marker screenshot'));
      requestAnimationFrame(check);
    }
    check();
  })`);
  await screenshot('console-keyboard-marker.png');
  for (const _ of marker) await press('Backspace');
  await evaluate(`document.getElementById('disconnect').click()`);
  await evaluate(`new Promise((resolve, reject) => {
    const element = document.getElementById('status');
    const timeout = setTimeout(() => { observer.disconnect(); reject(new Error('noVNC did not disconnect')); }, 10000);
    const observer = new MutationObserver(check);
    function check() { if (element.textContent.startsWith('Disconnected')) { clearTimeout(timeout); observer.disconnect(); resolve(true); } }
    observer.observe(element, { childList: true }); check();
  })`);
  await navigate();
  await screenshot('console-reconnected.png');
  assert.deepEqual(exceptions, [], 'Browser exceptions');
  await writeFile(resolve(output, 'console-result.json'), JSON.stringify({ key, connected: true, framebufferChangedAfterInput: true, reconnected: true, browserExceptions: exceptions, keyboardEvidence: 'Inspect console-keyboard-marker.png for the marker on the guest display.' }, null, 2));
  console.log(`Connected, rendered, changed after input and reconnected. Review guest marker screenshots in ${output}.`);
  await command('Page.close');
  await send('Browser.close');
} catch (error) {
  await captureFailure?.(error).catch(() => {});
  throw error;
} finally {
  websocket?.terminate();
  chrome.kill('SIGTERM');
  await Promise.race([once(chrome, 'exit').catch(() => {}), new Promise(resolve => setTimeout(resolve, 2000))]);
  await bridge.close();
  await rm(profile, { recursive: true, force: true, maxRetries: 3 }).catch(error => console.error(`Could not remove browser test profile ${profile}: ${error.message}`));
}
