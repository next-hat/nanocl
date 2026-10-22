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

The MAC is generated once as a locally administered unicast address and saved in the VM specification. A supplied MAC must be canonicalizable hexadecimal unicast notation. NanoCL rejects duplicate addresses across its active VMs and refuses changes to an existing identity. Omitted MAC values in Statefiles retain the persisted address. External Docker workloads and independently managed VMs remain outside this allocation check.

Stop/start and restart preserve the VM MAC. A container replacement can receive a different address from Docker IPAM; the new runtime offers that newly allocated address. A stable MAC does not reserve a permanent Docker IP across deletion or replacement.

Docker stop/restart currently terminates QEMU as a guest power cut. Shut down from inside the guest before a planned stop when filesystem durability matters. The restart acceptance test explicitly flushes completed provisioning writes with guest `sync`; it proves networking and identity persistence, not safe termination of arbitrary in-flight writes.

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

`CloudInit` defaults to `true` for existing configurations. When enabled, it provisions users, SSH keys, passwords, and hostname. Its DHCP configuration matches the persisted MAC and gives the interface the stable name `nanocl0`, so early boot readiness does not retain a name that udev later changes. `CloudInit: false` generates and attaches no seed; the guest must already have a DHCP client and usable login credentials. Its interface naming remains guest-managed. Provisioning fields do not create guest accounts when cloud-init is disabled.

`Console` defaults to `false`. When enabled, QEMU serves VNC through a Unix socket owned by the runtime. NanoCL relays binary RFB frames through `GET /v0.19.0/vms/{namespace.name}/console`. The runtime does not expose an unauthenticated VNC or WebSocket TCP port. QEMU's native WebSocket listener and a separate websockify process are unnecessary for this Unix-socket relay.

The endpoint accepts the daemon Unix socket or verified mutual TLS. Plaintext daemon TCP, including development TCP, is rejected. A browser's Origin must exactly match the HTTPS endpoint. Access grants the same daemon-wide operator privileges as the underlying authenticated transport; there is no per-user VM ownership model. The daemon verifies the VM key, namespace, local runtime identity, running state, and specification generation before opening its socket.

A containerized daemon must bind its state directory at the same absolute path on the Docker host, as the standard installer already does. Console socket directories are private to the daemon/runtime. The full socket path must fit the Linux Unix-socket limit; an overly long state path or VM key is rejected before runtime replacement.

Use the [noVNC example](../examples/vm-console/README.md) for a local browser connection. Its loopback bridge holds operator credentials server-side and fixes the target VM. The WebSocket sends raw binary RFB frames, with an optional `binary` subprotocol and a maximum 64 KiB client frame.

| Response | Meaning |
| --- | --- |
| 101 | WebSocket established. |
| 400 | Invalid canonical key or WebSocket handshake. |
| 403 | Disabled console, untrusted transport, or rejected Origin. |
| 404 | VM does not exist. |
| 409 | Runtime is absent, stopped, ambiguous, stale, or its console is not ready. |

CLI create/run/patch accepts `--cloud-init true|false` and `--console true|false`. PATCH preserves omitted options. Use a Statefile to select a local candidate runtime explicitly.

## Migration

1. Build the matching `nanocl-qemu` checkout locally and select it explicitly:

   ```sh
   docker build -t ghcr.io/next-hat/nanocl-qemu:8.0.5.0 /path/to/nanocl-qemu
   ```

   This is the new default runtime tag, validated locally with Linux x86_64 KVM and Ubuntu 24.04 guests. The image must be built locally or published through the release process before another installation can use it; this change does not publish an image. `HostConfig.Runtime` can select an explicit candidate. NanoCL requires the image label `io.nanocl.vm-runtime-api=0.19`; an older runtime without that contract is rejected before replacement.

2. Move `NetworkMode: host`, legacy `RuntimeNetwork: host`, and physical `LinkNetIface` configurations onto a named Docker bridge. `RuntimeNetwork` remains a spelling alias for `NetworkMode`.
3. Existing VM images that retain cloud-init static network configuration need guest-side migration to DHCP. Disabling future seed generation does not remove guest state already written by an earlier seed. Back up the image and use its distribution's network tools to enable DHCP before replacing the runtime.
4. Legacy VM specs without a MAC receive a persisted address before their next runtime creation. Their former runtime-generated MAC was not a durable identity. Check guest network rules that match the old MAC. A legacy VM may supply an explicit `MacAddress` before identity is assigned.
5. Replace a legacy runtime to use the new DHCP and console behavior. Stop/start alone can reuse an existing old container configuration. Retain `CloudInit: true` when existing guest provisioning is needed; set it to `false` for a prepared non-cloud-init image.
6. Enable `Console` only for the VMs that need it, and use the authenticated endpoint or the loopback example. Do not restore the old public websockify port mapping.

## Reproducible release validation

Use disposable guest disk copies and a development NanoCL daemon. Record the NanoCL commit, runtime commit/image ID, Docker version, QEMU version, network CIDR, and whether KVM was available. These instructions define required evidence; they do not claim a successful release-validation run.

1. Build the local runtime and prepare two separate writable guest images: a cloud-init image and an ordinary installed image with DHCP enabled and no cloud-init dependency. Use `CloudInit: true` and `false`, respectively, with the explicit candidate runtime. The ordinary guest must have independently configured login credentials.
2. Start both VMs on the same named Docker bridge. Inspect each runtime with `docker inspect <namespace.name>.v` and each VM with `nanocl vm inspect <namespace.name>`. Save the allocated endpoint IPv4 and the persisted MAC. Inside each guest, inspect `ip -4 address`, `ip -4 route`, and resolver configuration. Verify matching endpoint address/prefix, expected gateway, and distinct addresses.
3. Test a DNS lookup, outbound HTTP, gateway reachability, and intended ingress from another container or the configured NanoCL proxy. Use controlled reachable endpoints in the test environment. Capture UDP ports 67/68 on the Docker bridge while renewing guest leases: guest DHCP must stay on the private TAP.
4. Stop and start each VM, restart the runtime container, and restart the development daemon. Recheck MAC, lease, gateway, DNS, egress, and ingress. Force a spec replacement separately and record any Docker IP change. Do not confuse replacement behavior with same-container restart.
5. Confirm the no-cloud-init VM receives DHCP with no seed attached. Check the runtime's QEMU arguments and guest devices. Confirm cloud-init provisioning and SSH keys work on the enabled image.
6. Enable `Console`, start the [noVNC example](../examples/vm-console/README.md), and use a real browser to observe display output and send keyboard input. Test disconnect/reconnect, runtime restart, and replacement. Verify the old connection closes when its backend exits.
7. Attempt the console from plaintext TCP, with a foreign/null Origin, with no trusted client certificate, with `Console: false`, against a missing VM, and against a stopped runtime. Record the actual rejection status. An operator trusted by the daemon can access enabled consoles across namespaces by design.
8. Delete the VMs. Verify their runtime containers, TAP devices, dnsmasq/QEMU processes, and console sockets disappear. Delete only test-owned images/networks after checking that no unrelated workloads use them.

The unit tests cover parsing, identity rules, configuration propagation, and console checks. Real guest boot, DHCP renewal, DNS, ingress, simultaneous VMs, and browser interaction require the above runtime evidence. A namespace probe or a successful image build alone does not establish them.
