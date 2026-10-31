# NanoCL noVNC console example

Set `Console: true` on the VM, apply the configuration and wait for the runtime
to restart. The default is `false`. VNC runs only on a private Unix socket;
there is no VNC or WebSocket container port to publish.

From this directory, with Node.js 22 or later:

```sh
npm ci
npm start -- global.my-vm
```

Open the printed URL. The example serves noVNC on an ephemeral loopback port
and connects to `unix:///run/nanocl/nanocl.sock`. The pinned noVNC 1.7 package
provides browser ES modules served locally; no scripts load from a CDN.
The account running it must
already have permission to access NanoCL. Its random link grants access only
to the selected VM while the bridge runs. Keep the link private; stop the
bridge with Ctrl-C when finished. The token travels in a WebSocket subprotocol,
never a request query, and is removed from browser history after loading.
The bridge checks the exact browser Origin and Host to prevent cross-site
WebSocket connections and DNS rebinding. It is intentionally loopback-only.

For a custom Unix socket, the bridge account must already have filesystem access
to that socket. Configure its ownership and group permissions in your deployment;
the daemon does not automatically grant group access to custom socket paths.

For an existing daemon configured with mutual TLS:

```sh
NANOCL_HOST=https://nanocl.example:8443 \
NANOCL_TLS_CA=/path/to/ca.pem \
NANOCL_TLS_CERT=/path/to/client.pem \
NANOCL_TLS_KEY=/path/to/client-key.pem \
npm start -- global.my-vm
```

Set `NANOCL_HOST=http://127.0.0.1:8585` to use a configured HTTP daemon listener.
HTTPS uses system certificate trust by default; `NANOCL_TLS_CA` supplies a custom
CA, and `NANOCL_TLS_CERT`/`NANOCL_TLS_KEY` supply client credentials when required
by the daemon. The browser never receives daemon credentials. The bridge's
loopback token, Host, and Origin checks apply with every upstream transport.

The API is `GET /v0.19.0/vms/{namespace.name}/console`, with RFC6455 binary
frames containing raw RFB bytes (64 KiB per frame). Native clients can call
`NanocldClient::console_vm`; HTTPS requires the client crate's `openssl`
feature. Direct browser clients can use noVNC through the daemon's configured
listeners. If the daemon requires mutual TLS, install its client certificate in
the browser. The API adds no VM-specific transport or Origin restriction.
Access follows NanoCL's existing daemon-operator trust boundary: a trusted
operator can control every VM. Namespaces are not per-user authorization.
The API verifies the canonical VM, current specification generation, local
process record, Docker container identity and labels before connecting.

Disconnecting the browser releases the Unix connection without stopping the
VM. A VM stop, restart, replacement or deletion disconnects its console; reopen
the bridge URL to reconnect to the current runtime. No sessions are persisted.

Run the bridge authorization and browser module tests with `npm test`. Release validation
must additionally use a real VM: observe the noVNC `connect` event and rendered
guest display, send keyboard input, disconnect/reconnect, restart/delete the VM,
and confirm every old socket closes. Verify disabled consoles and wrong runtime
labels are rejected, and check the daemon's configured listener access policy.
The example bridge must reject foreign Origins; a mutual TLS daemon listener
must reject missing/invalid client certificates. A successful WebSocket upgrade
alone does not prove RFB or guest input works.

Two opt-in release runners are included. Use a disposable VM with `Console:
true`, a visible guest login prompt, and an isolated daemon/Docker engine.
The browser runner needs Chrome (`CHROME_BIN` overrides its path). It types
`nanocl-console-probe` without Enter, saves screenshots, erases the marker,
disconnects and reconnects. Inspect the keyboard screenshot to confirm that
the marker reached the guest; framebuffer changes alone are insufficient.

```sh
NANOCL_HOST=unix:///path/to/task.sock npm run smoke -- global.test-vm /tmp/console-evidence
NANOCL_HOST=unix:///path/to/task.sock npm run test:api -- global.test-vm --disposable
```

The API runner **deletes the named VM** after exercising stop/start, restart,
console disable/enable, and runtime replacement. It checks raw RFB, binary
frames, disconnect/reconnect, daemon access with foreign/null Origins, and
malformed keys. Set `NANOCL_PLAINTEXT_HOST=http://127.0.0.1:port` to verify access
through a configured plaintext listener.
Run against `NANOCL_HOST=https://...` with the TLS variables above to additionally
check missing-client-certificate rejection. Unconfigured transport checks are
reported as skipped. These runners do not install a daemon or create guests.
