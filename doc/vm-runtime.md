# VM runtime, migration, and validation

The NanoCL 0.19 VM runtime separates guest networking from provisioning. A DHCP-capable guest can boot without cloud-init. The optional browser console uses the existing daemon operator access boundary.

## Network model

Docker IPAM reserves one IPv4 address for each runtime container. That address belongs to the guest while it runs. The runtime reads the Docker endpoint's address, prefix, and gateway before moving the address out of the container's local address table. A routed TAP connects QEMU to the runtime network namespace. Proxy ARP and a guest host route carry traffic between the TAP and Docker interface without an additional guest NAT layer.

The private TAP has a helper address for dnsmasq. dnsmasq listens only on this isolated guest link and offers a single fixed lease to the persisted guest MAC. It provides the actual Docker prefix and gateway; the lease does not use a hardcoded `/16`. DHCP broadcasts are not bridged onto the Docker bridge or a physical host network. Multiple runtimes have separate TAPs and DHCP servers; Docker remains the allocator for their endpoint addresses.

The helper also forwards guest DNS queries. Configured `HostConfig.Dns` addresses are used when supplied; otherwise the runtime uses the container resolver, including Docker's embedded resolver. Translation needed for helper-originated resolver traffic does not translate the guest's ordinary traffic.

This keeps the guest reachable at its Docker endpoint address by other containers and by existing NanoCL proxy rules. Host ingress still requires an intentional proxy or routing rule; enabling a console does not publish VNC ports.

Alternative designs were considered:

| Model | Tradeoff |
| --- | --- |
| Private guest subnet and NAT | Isolates DHCP simply, but changes guest addressing and requires an ingress mapping contract. |
| TAP bridged directly to the Docker interface | Preserves the address but couples DHCP broadcast isolation and endpoint address ownership to bridge filtering. |
| Routed TAP with proxy ARP | Preserves the guest's Docker endpoint address and separates DHCP at the link boundary. Requires explicit routes, proxy ARP, and removal of duplicate local IP ownership. |

The implementation uses the third model. Only local Docker bridge networking is supported. `host`, `none`, and `container:` namespace sharing are incompatible with runtime network setup. Existing physical-interface takeover configurations must migrate to a Docker bridge.

Leave `HostConfig.LinkNetIface` unset to discover the Docker uplink. An explicit value must match the selected network's container interface prefix followed by `0` (`eth0` by default). NanoCL checks this before starting or replacing the runtime; `eth1` does not select a second endpoint.

The MAC is generated once as a locally administered unicast address and saved in the VM specification. A supplied MAC must be canonicalizable hexadecimal unicast notation. NanoCL rejects duplicate addresses across its active VMs and refuses changes to an existing identity. Omitted MAC values in Statefiles retain the persisted address. External Docker workloads and independently managed VMs remain outside this allocation check.

Stop/start and restart preserve the VM MAC. A container replacement can receive a different address from Docker IPAM; the new runtime offers that newly allocated address. A stable MAC does not reserve a permanent Docker IP across deletion or replacement.

Use NanoCL's VM stop and restart commands for guest-aware shutdown. Direct Docker stop/restart still terminates QEMU as a guest power cut and bypasses NanoCL's lifecycle checks.

## QMP lifecycle control

NanoCL mounts `${state_dir}/run/vms/{namespace.name}/{specification_uuid}` into the runtime at `/run/nanocl-vm`. QEMU creates `qmp.sock` there and, when `Console: true`, `console.sock`. The per-VM generation directory is private to the daemon and runtime. A containerized daemon must bind its state directory at the same absolute host path. The full Unix-socket path must fit the Linux path limit; overly long paths are rejected before replacing a runtime.

QMP stays on this private Unix socket. NanoCL exposes specific lifecycle operations through the existing daemon operator boundary; it does not provide a public raw-QMP endpoint. The daemon verifies the current VM key, namespace, local container ID, and specification generation before using the socket. Concurrent lifecycle operations for the same VM are serialized.

```sh
nanocl vm status global.desktop
nanocl vm stop global.desktop --timeout 120
nanocl vm restart global.desktop --timeout 120
```

Stop asks the guest to shut down through QMP's ACPI power button operation and waits for confirmed runtime exit. Restart starts the runtime only after that shutdown completes. Guest shutdown requires the guest OS to handle the ACPI request. The default grace period is 60 seconds; `--timeout` accepts 1 through 3600 seconds. A timeout reports failure and never silently kills or restarts the guest. The guest may still complete shutdown after the timeout, so query status before deciding what to do next.

`--force` explicitly requests immediate power-off and bypasses the guest grace period. It can lose guest writes. Use it only when that outcome is intended:

```sh
nanocl vm stop global.desktop --force
nanocl vm restart global.desktop --force
```

The API provides `POST /v0.19.0/vms/{key}/stop`, `POST /v0.19.0/vms/{key}/restart`, and `GET /v0.19.0/vms/{key}/status`. POST requests require a JSON body; `{}` uses defaults, and `{"Timeout":120,"Force":false}` sets an explicit grace period. Shutdown timeout returns HTTP 504. These endpoints accept the daemon Unix socket or verified mutual TLS; plaintext TCP is denied, and a browser Origin must exactly match the HTTPS endpoint. They use the same daemon-wide operator privileges as the console.

Status reports actual QEMU execution state as `{"Running":true,"Status":"running"}`. A paused guest reports `Running: false` and `Status: paused` even though its container is running. A stopped runtime reports `Running: false` and `Status: exited`. Status remains available while a shutdown request waits through its grace period. Pause/resume is not exposed.

Generic process stop/restart, VM replacement, and VM deletion also request graceful shutdown before removing a running VM runtime. A shutdown failure prevents replacement or deletion; explicitly stop with `--force` before retrying if forced power-off is necessary. A runtime image must include `io.nanocl.vm-qmp=1`. For older running containers, stop them from inside the guest or explicitly force-stop before replacing them with a matching runtime. Pulling an updated image alone does not change existing containers.

## Live resource growth

Reserve CPU and memory ceilings when creating the VM. `HostConfig.MaxCpu` and `HostConfig.MaxMemory` are optional; omitted values use the initial `Cpu` and `Memory`, leaving no capacity for live growth of that resource. Memory values are MiB. The ceilings cannot be changed by a live resize: changing them through PATCH replaces the runtime and requires guest shutdown.

```yaml
HostConfig:
  Cpu: 2
  Memory: 1024
  MaxCpu: 8
  MaxMemory: 8192
  Kvm: true
```

The equivalent CLI flags are `--max-cpu` and `--max-memory` on `vm create`, `vm run`, and `vm patch`. Existing containers need replacement with the updated runtime and desired ceilings before they can use CPU or memory hot-add. The image must advertise `io.nanocl.vm-resize=1`; selecting a new image tag does not modify an already-created container.

```sh
nanocl vm resources global.desktop
nanocl vm resize global.desktop --cpu 4 --memory 2048
nanocl vm resize global.desktop --disk-size 34359738368
```

Each value is the requested total, not an increment. `--disk-size` is bytes (the example requests 32 GiB). The VM must be running. Live resize only grows resources; requests below current QEMU capacity are rejected. A request equal to current capacity is accepted so that a previous persistence failure can be repaired.

| Resource | Requirements and effect |
| --- | --- |
| CPU | At most the boot-time `MaxCpu`, up to 255 vCPUs. QEMU adds CPU devices; the guest must support and online them. |
| Memory | At most the boot-time `MaxMemory`. Each increase must be a multiple of 128 MiB. The runtime reserves 32 DIMM slots per boot; restarting folds the new allocation into base memory and makes those slots available again. Memory and ceiling values must be positive and at most 8,796,093,022,207 MiB. The guest must support and online added memory. |
| Boot disk | New size must be aligned to 512 bytes. Supported disks are unencrypted raw or qcow2 images without backing files or internal snapshots. QEMU enlarges the block device; the guest's partition table and filesystems must be grown separately. |

NanoCL does not execute guest commands or install a guest agent for resizing. A successful API response confirms QEMU capacity, not guest CPU/memory online state or usable filesystem space. Host memory and storage capacity must still support the requested allocation.

The API exposes `POST /v0.19.0/vms/{key}/resize` with a JSON body such as `{"Cpu":4,"Memory":2048,"DiskSize":34359738368}`, and `GET /v0.19.0/vms/{key}/resources`. Both use the same trusted operator transport and Origin checks as lifecycle operations. At least one resource must be supplied. The response contains `Cpu`, `Memory`, `DiskSize`, `MaxCpu`, `MaxMemory`, and `Persistent`.

`Persistent: true` means observed CPU and memory match both the saved VM specification and the runtime's restart configuration. Successful growth survives a same-container restart; CPU and memory changes also create specification history without replacing the running VM. Disk growth is stored in the disk image itself. Keep desired Statefiles consistent with the new allocation before applying them again.

Resize is not atomic. NanoCL preflights the requested resources, but a later QMP command or persistence write can fail after earlier changes succeeded. A timeout or error does not undo those changes and must not be treated as proof that nothing happened. Query `vm resources` before retrying. If `Persistent` is false, retry the desired CPU/memory totals to reconcile the observed allocation with the saved specification and restart configuration; avoid restarting until persistence is confirmed. The client does not retry resize requests automatically.

## Provisioning and console options

```yaml
ApiVersion: v0.19
Namespace: global
VirtualMachines:
- Name: desktop
  Image: /var/lib/nanocl/vm/desktop.qcow2
  CloudInit: false
  Console: true
  HostConfig:
    Cpu: 2
    Memory: 2048
    Runtime: ghcr.io/next-hat/nanocl-qemu:8.0.5.0
    NetworkMode: vm-private
```

`CloudInit` defaults to `true` for existing configurations. When enabled without `CloudInitUserData`, it provisions users, SSH keys, passwords, and hostname from NanoCL's fields. Its DHCP configuration matches the persisted MAC and gives the interface the stable name `nanocl0`, so early boot readiness does not retain a name that udev later changes. `CloudInit: false` generates and attaches no seed; the guest must already have a DHCP client and usable login credentials. Its interface naming remains guest-managed. Provisioning fields do not create guest accounts when cloud-init is disabled.

When generated provisioning is enabled, `User` must start with a lowercase ASCII letter or underscore and contain only lowercase ASCII letters, digits, underscores, or hyphens. An omitted or empty value uses `cloud`. NanoCL rejects unsupported usernames before accepting a create or update. With `CloudInit: false` or custom `CloudInitUserData`, these generated provisioning fields are ignored.

### Custom cloud-init user-data

Supply `CloudInitUserData` as a cloud-config object in the API or Statefile. Keep cloud-init's own lowercase property names:

```yaml
ApiVersion: v0.19
Namespace: global
VirtualMachines:
- Name: provisioned
  Image: /var/lib/nanocl/vm/provisioned.qcow2
  CloudInit: true
  CloudInitUserData:
    hostname: provisioned
    users:
    - default
    package_update: true
    packages:
    - curl
    write_files:
    - path: /etc/nanocl-provisioned
      permissions: '0644'
      content: "Provisioned by user configuration\n"
    runcmd:
    - [sh, -c, 'date -u > /var/log/nanocl-provisioned-at']
  HostConfig:
    Cpu: 2
    Memory: 2048
    Runtime: ghcr.io/next-hat/nanocl-qemu:8.0.5.0-nightly
```

NanoCL writes the object as `#cloud-config` followed by JSON, which cloud-init accepts as YAML. OpenAPI and Statefile schemas describe the object through the [official cloud-init 26.2 cloud-config schema](https://raw.githubusercontent.com/canonical/cloud-init/26.2/cloudinit/config/schemas/schema-cloud-config-v1.json). Editors and schema tools that resolve external references can use it for completion and validation. NanoCL does not run that upstream schema validator; the guest's installed cloud-init version determines which settings it supports.

For an existing complete user-data document, use a raw string instead:

```yaml
  CloudInitUserData: |
    #cloud-config
    write_files:
    - path: /etc/custom-provisioning
      content: "Hello from cloud-init\n"
```

Raw text may contain cloud-config YAML, a shell script with a shebang, or MIME multipart user-data. Statefiles pass through NanoCL's existing Liquid template compiler first; the resulting user-data is then passed unchanged. For literal cloud-init Jinja syntax, use a CLI file or direct API string, or wrap the Jinja content in Liquid's `{% raw %}` and `{% endraw %}` tags in the Statefile. The CLI loads raw UTF-8 text from a local file without Statefile templating:

```sh
nanocl vm create guest /var/lib/nanocl/vm/guest.qcow2 --cloud-init-user-data ./user-data.yaml
nanocl vm run guest /var/lib/nanocl/vm/guest.qcow2 --cloud-init-user-data ./user-data.yaml
nanocl vm patch global.guest --cloud-init-user-data ./replacement.yaml
```

Custom user-data replaces the entire generated user-data document; it is not merged with `User`, `Password`, `SshKey`, or the generated hostname. Include the accounts, SSH keys, hostname, and other provisioning you need in the custom document. An empty raw string or empty object still selects custom provisioning and creates no NanoCL-generated accounts. NanoCL continues to supply the stable VM instance ID and the managed DHCP network-config separately.

User-data must contain no NUL bytes and must fit in 64 KiB after rendering. CLI files must be UTF-8. These limits also apply while `CloudInit: false`; the configuration is retained but unused until cloud-init is enabled. User-data is saved in the VM specification and its history and passed through the Docker runtime environment, so authorized operators who can inspect them can read it. The `state apply` confirmation preview (without `-y`) also prints the full rendered user-data. `state diff` redacts custom user-data in its change display.

PATCH and Statefile apply preserve existing user-data when the field is omitted or `null`. Use `CloudInit: false` to disable seed attachment; an explicit empty string selects blank custom user-data, rather than restoring generated accounts. Updating the specification or restarting a VM does not force first-boot cloud-init modules to rerun: the instance ID stays stable and the guest keeps its cloud-init state on disk. Test new provisioning with a fresh guest disk, or explicitly manage reprovisioning from inside the guest.

Custom user-data requires a runtime image with `io.nanocl.vm-cloud-init-user-data=1` in addition to the runtime API label. Pull the updated nightly image with `docker pull ghcr.io/next-hat/nanocl-qemu:8.0.5.0-nightly`, then select that tag through `HostConfig.Runtime` as above. NanoCL uses an `IfNotPresent` pull policy, so a cached older nightly without the capability is rejected when custom user-data is enabled; pull the updated image explicitly before creating or replacing the runtime. A matching locally built runtime can also be selected through `HostConfig.Runtime`.

### Browser console

`Console` defaults to `false`. When enabled, QEMU serves VNC through a Unix socket owned by the runtime. NanoCL relays binary RFB frames through `GET /v0.19.0/vms/{namespace.name}/console`. The runtime does not expose an unauthenticated VNC or WebSocket TCP port. QEMU's native WebSocket listener and a separate websockify process are unnecessary for this Unix-socket relay.

The endpoint accepts the daemon Unix socket or verified mutual TLS. Plaintext daemon TCP, including development TCP, is rejected. A browser's Origin must exactly match the HTTPS endpoint. Access grants the same daemon-wide operator privileges as the underlying authenticated transport; there is no per-user VM ownership model. The daemon verifies the VM key, namespace, local runtime identity, running state, and specification generation before opening its socket.

A containerized daemon must bind its state directory at the same absolute path on the Docker host, as the standard installer already does. Console and QMP sockets share the private per-VM generation directory described above.

Use the [noVNC example](../examples/vm-console/README.md) for a local browser connection. Its loopback bridge holds operator credentials server-side and fixes the target VM. The WebSocket sends raw binary RFB frames, with an optional `binary` subprotocol and a maximum 64 KiB client frame.

| Response | Meaning |
| --- | --- |
| 101 | WebSocket established. |
| 400 | Invalid canonical key or WebSocket handshake. |
| 403 | Disabled console, untrusted transport, or rejected Origin. |
| 404 | VM does not exist. |
| 409 | Runtime is absent, stopped, ambiguous, stale, or its console is not ready. |

CLI create/run/patch accepts `--cloud-init true|false`, `--cloud-init-user-data FILE`, and `--console true|false`. PATCH preserves omitted options. Use a Statefile to select a local candidate runtime explicitly.

## Migration

1. Build the matching `nanocl-qemu` checkout locally and select it explicitly:

   ```sh
   docker build -t ghcr.io/next-hat/nanocl-qemu:8.0.5.0 /path/to/nanocl-qemu
   ```

   This is the new default runtime tag. The image must be built locally or published through the release process before another installation can use it; this change does not publish an image. `HostConfig.Runtime` can select an explicit candidate. NanoCL requires the image labels `io.nanocl.vm-runtime-api=0.19` and `io.nanocl.vm-qmp=1`; an older runtime without those contracts is rejected before replacement.

2. Move `NetworkMode: host`, legacy `RuntimeNetwork: host`, and physical `LinkNetIface` configurations onto a named Docker bridge. `RuntimeNetwork` remains a spelling alias for `NetworkMode`.
3. Existing VM images that retain cloud-init static network configuration need guest-side migration to DHCP. Disabling future seed generation does not remove guest state already written by an earlier seed. Back up the image and use its distribution's network tools to enable DHCP before replacing the runtime.
4. Legacy VM specs without a MAC receive a persisted address before their next runtime creation. Their former runtime-generated MAC was not a durable identity. Check guest network rules that match the old MAC. A legacy VM may supply an explicit `MacAddress` before identity is assigned.
5. Shut down a legacy guest from inside the guest OS, or explicitly use `nanocl vm stop <key> --force` if a power cut is acceptable. Replace its runtime to use the new DHCP, QMP, and console behavior. Stop/start alone can reuse an existing old container configuration. Retain `CloudInit: true` when existing guest provisioning is needed; set it to `false` for a prepared non-cloud-init image.
6. Enable `Console` only for the VMs that need it, and use the authenticated endpoint or the loopback example. Do not restore the old public websockify port mapping.

## Reproducible release validation

Use disposable guest disk copies and a development NanoCL daemon. Record the NanoCL commit, runtime commit/image ID, Docker version, QEMU version, network CIDR, and whether KVM was available. These instructions define required evidence; they do not claim a successful release-validation run.

1. Build the local runtime and prepare two separate writable guest images: a cloud-init image and an ordinary installed image with DHCP enabled and no cloud-init dependency. Use `CloudInit: true` and `false`, respectively, with the explicit candidate runtime. The ordinary guest must have independently configured login credentials.
2. Start both VMs on the same named Docker bridge. Inspect each runtime with `docker inspect <namespace.name>.v` and each VM with `nanocl vm inspect <namespace.name>`. Save the allocated endpoint IPv4 and the persisted MAC. Inside each guest, inspect `ip -4 address`, `ip -4 route`, and resolver configuration. Verify matching endpoint address/prefix, expected gateway, and distinct addresses.
3. Test a DNS lookup, outbound HTTP, gateway reachability, and intended ingress from another container or the configured NanoCL proxy. Use controlled reachable endpoints in the test environment. Capture UDP ports 67/68 on the Docker bridge while renewing guest leases: guest DHCP must stay on the private TAP.
4. Stop and start each VM through NanoCL, use `nanocl vm restart`, and restart the development daemon. Recheck MAC, lease, gateway, DNS, egress, and ingress. Verify graceful shutdown while the guest writes data and separately test a guest that ignores ACPI: a short timeout must report failure without power-off or restart. Verify explicit `--force` separately. Query QMP status after daemon reconnection, and reject a stale VM generation or wrong container identity. Replace a spec separately and record any Docker IP change. Do not confuse replacement behavior with same-container restart.
5. Confirm the no-cloud-init VM receives DHCP with no seed attached. Check the runtime's QEMU arguments and guest devices. Confirm cloud-init provisioning and SSH keys work on the enabled image.
6. Enable `Console`, start the [noVNC example](../examples/vm-console/README.md), and use a real browser to observe display output and send keyboard input. Test disconnect/reconnect, runtime restart, and replacement. Verify the old connection closes when its backend exits.
7. Attempt the console from plaintext TCP, with a foreign/null Origin, with no trusted client certificate, with `Console: false`, against a missing VM, and against a stopped runtime. Record the actual rejection status. An operator trusted by the daemon can access enabled consoles across namespaces by design.
8. Delete the VMs. Verify their runtime containers, TAP devices, dnsmasq/QEMU processes, and console sockets disappear. Delete only test-owned images/networks after checking that no unrelated workloads use them.

For live resizing, start a guest with explicit CPU/memory ceilings and the resize-capable runtime. Record QEMU resources, guest online CPU/memory, and disk/filesystem sizes separately. Grow each resource and a combined request, verify shrink/ceiling/alignment rejection before mutation, then verify persistence across a same-container restart. Exercise a partial failure, inspect `Persistent`, and retry the same totals. Confirm stale generation and foreign container identities are rejected and console access still works after a resource-only specification revision.

The unit tests cover parsing, identity rules, configuration propagation, and console checks. Real guest boot, DHCP renewal, DNS, ingress, simultaneous VMs, and browser interaction require the above runtime evidence. A namespace probe or a successful image build alone does not establish them.
