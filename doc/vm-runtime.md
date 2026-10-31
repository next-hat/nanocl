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

The implementation uses the third model. Only local Docker bridge networking is supported. `host`, `none`, and `container:` namespace sharing are incompatible with runtime network setup. Statefiles must select a Docker bridge.

Leave `HostConfig.LinkNetIface` unset to discover the Docker uplink. An explicit value must match the selected network's container interface prefix followed by `0` (`eth0` by default). NanoCL checks this before starting or replacing the runtime; `eth1` does not select a second endpoint.

The MAC is generated once as a locally administered unicast address and saved in the VM specification. A supplied MAC must be canonicalizable hexadecimal unicast notation. NanoCL rejects duplicate addresses across its active VMs and refuses changes to an existing identity. Omitted MAC values in Statefiles retain the persisted address. External Docker workloads and independently managed VMs remain outside this allocation check.

Stop/start and restart preserve the VM MAC. A container replacement can receive a different address from Docker IPAM; the new runtime offers that newly allocated address. A stable MAC does not reserve a permanent Docker IP across deletion or replacement.

Use NanoCL's VM stop and restart commands for guest-aware shutdown. Direct Docker stop/restart still terminates QEMU as a guest power cut and bypasses NanoCL's lifecycle checks.

## QMP lifecycle control

NanoCL mounts `${state_dir}/run/vms/{namespace.name}/{specification_uuid}` into the runtime at `/run/nanocl-vm`. QEMU creates `qmp.sock` there and, when `Console: true`, `console.sock`. The per-VM generation directory is private to the daemon and runtime. A containerized daemon must bind its state directory at the same absolute host path. The daemon canonicalizes the configured state root and connects through a pinned socket file descriptor, so long host paths do not exceed the Unix-socket address limit. Runtime-created symlinks cannot redirect the connection.

QMP stays on this private Unix socket. NanoCL exposes specific lifecycle operations through the existing daemon operator boundary; it does not provide a public raw-QMP endpoint. The daemon verifies the current VM key, namespace, local container ID, and specification generation before using the socket. Concurrent lifecycle mutations for the same VM are rejected while an operation owns admission. A mutation can wait up to 30 seconds for an already-running Docker exit observation to finish its protected status write. An observation timeout does not start the mutation. Docker exit events must match both the immutable container ID and its current run start time. An explicit `nanocl kill global.desktop.v --signal SIGKILL` remains available during a pending guest shutdown; it signals the inspected container ID without waiting for the lifecycle lock.

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

The API provides `POST /v0.19.0/vms/{key}/stop`, `POST /v0.19.0/vms/{key}/restart`, and `GET /v0.19.0/vms/{key}/status`. POST requests require a JSON body; `{}` uses defaults, and `{"Timeout":120,"Force":false}` sets an explicit grace period. Shutdown timeout returns HTTP 504. These endpoints use the daemon's configured access controls and the same daemon-wide operator privileges as the other APIs.

Status reports actual QEMU execution state as `{"Running":true,"Status":"running"}`. A paused guest reports `Running: false` and `Status: paused` even though its container is running. A stopped runtime reports `Running: false` and `Status: exited`. Status remains available while a shutdown request waits through its grace period. Pause/resume is not exposed.

Generic process stop/restart returns HTTP 202 after accepting the bounded operation; completion or failure is reported through events and VM status. The dedicated VM stop/restart endpoints wait for completion. VM replacement and deletion also request graceful shutdown before removing a running VM runtime. A shutdown failure prevents replacement or deletion; explicitly stop with `--force` before retrying if forced power-off is necessary. Failed operations emit an error while VM status reflects the observed owned runtime: a guest left running remains `Start`. The requested specification is retained for inspection and retry. Known self-referential DNS on the currently assigned endpoint is rejected before accepting a replacement specification. A runtime image must include `io.nanocl.vm-qmp=1`. Runtime containers must carry the current revision and runtime capability labels. Pulling an updated image alone does not change existing containers.

Start, replacement, and deletion acquire lifecycle admission and their distributed mutex before changing desired state or recording a pending transition. The accepted task keeps that ownership through execution, failure reconciliation, and cleanup, even if the HTTP caller disconnects. A mutex acquisition failure leaves the pre-request specification and status untouched; an existing owner's mutex is never removed by that failed request. Error notifications after completion do not write VM status. Task completion and waiters retire only their own registered task handle.

### Shared operations and atomic VM acceptance

Operation history uses the existing specification store and two small shared tables:

| Table or relationship | Purpose |
| --- | --- |
| `resource_incarnations` | Retained UUID, resource kind/name, creation/deletion times, and authoritative `current_operation_key`. A deleted name can be reused with a new UUID. |
| `operations` | One immutable request UUID, incarnation, kind/name snapshot, action, accepted specification UUID, owner node/boot/file identity, acceptance/completion times, outcome, and diagnostic. |
| `vms.incarnation_key` | Associates the live definition with its retained identity. |
| `specs.incarnation_key` | Associates VM revisions, including boot revisions, with that identity. VM deletion retains these revisions and operation history. |

An operation is pending while its outcome is null; its terminal outcome is `completed`, `failed`, or `interrupted`. These are request outcomes, independent of VM runtime statuses. Accepted identity and inputs do not change. The current-operation pointer remains after completion and deletion. A historical UUID, matching revision, or absence of a pending operation cannot authorize a stale completion: every conditional write also checks the exact current association and incarnation. A new request inserts a new operation even when it uses the same specification. A partial unique index allows only one unresolved operation per incarnation; kind/node pending scans and kind/incarnation history queries have dedicated indexes.

Replacement acceptance inserts its specification, changes the live specification pointer, records `Updating`, inserts the operation, and changes the authoritative current-operation pointer in one transaction on one connection. Start and deletion commit their pending status and operation association the same way. Any pre-commit failure rolls everything back. Only serialization failures are retried; Docker, QMP, events, and dispatch remain outside transactions. An uncertain commit is preserved for inspection and recovery rather than assumed to be rollback.

The containerized daemon runs as the single controller for its node and persistent mounted state directory. It holds an exclusive kernel lock at `${state_dir}/run/operations.gate` for its entire process lifetime, including runtime teardown. A second controller using the same directory fails before startup. The state directory and node identity must remain stable across daemon replacement; copied state roots do not provide shared ownership protection.

Before acceptance, the owner holds VM lifecycle admission and an exclusive kernel lock at `${state_dir}/run/operations/{operation_uuid}.lock`. The UUID is also the exact distributed mutex token. The file contains bounded immutable identity metadata. Detached acceptance and execution retain both guards if the HTTP caller or TaskManager waiter disconnects. Database workers receive their own guard references, so cancellation of an awaiting future cannot release ownership before the actual transaction and COMMIT finish. Expiry or deletion of the database mutex does not release the kernel lock.

Shared code in `repositories/operation.rs` and `utils/operation.rs` handles retained identity, currentness, queries, owned database work, and file locks/cleanup. VM transactions remain in `repositories/vm_operation.rs`; tasks, QEMU, provisioning, runtime observation, and recovery policy remain VM-specific. Cargo and Job execution are unchanged.

### Interrupted operations and automatic recovery

After startup inventory, the daemon recovery loop scans up to 32 pending VM operations per page every ten seconds. Its cursor advances past unclassifiable owners. Recovery takes the ordinary VM mutation/observation guards, verifies the same node, opens the exact regular owner file without following symlinks, verifies its serialized accepted token, and acquires its exclusive lock nonblockingly. Same-boot recovery also requires the recorded device/inode. A live operation's lock prevents takeover even if its database mutex has expired. Missing tasks or elapsed time alone never establish abandonment.

A replacement daemon holding the exclusive controller lock can recover an operation whose recorded owner boot differs from the current boot. The serialized accepted token must still match, and the operation's file lock must be free; a changed boot ID alone is insufficient. Device/inode values can change after a remount, so previous-boot recovery pins the opened file's current identity for safe cleanup while retaining the historical owner metadata. This recovery runs automatically through the same startup/periodic path as same-boot owner loss. Missing files, mismatched tokens, malformed metadata, foreign node identity, and other ambiguous evidence remain pending without altering accepted state. Recovery requires a recorded operation and never invents missing request history.

The separate offline maintenance command and its quiescence-confirmation workflow have been removed. Routine recovery uses the daemon's configured database, Docker connection, node identity, and mounted state without operator-supplied operation UUIDs or credentials.

Recovery verifies the live incarnation and current operation again before writing. It observes the owned Docker runtime and its saved boot revision, immutable container ID, local node, and current run timestamp. It also inventories VM-labeled init, temporary, and renamed containers, including containers with no process row. A running or unknown auxiliary container defers recovery because it may still be provisioning a disk. Wait for it to exit or inspect and explicitly stop it. Recovery does not replay, start, stop, replace, or delete a runtime.

| Accepted action | Reconciliation result |
| --- | --- |
| `start` | Record `interrupted`; expose observed `Start` or `Stop`. A running container does not prove the request completed. |
| `update` | Retain the accepted desired revision, record `interrupted`, and expose observed `Start` or `Stop`, including an older running boot revision from the same incarnation. |
| `delete` | Retain live VM metadata, record `interrupted`, and expose observed `Start` or `Stop`. Deletion requires an explicit new request. |

Database/Docker errors, ambiguous ownership, and invalid boot-generation evidence defer reconciliation without changing accepted state. Runtime inspection has a five-second bound; final database writes retain ownership through completion. Repeated reconciliation is harmless; a stale task, observation, or recovery candidate cannot settle a newer operation even after it completes. Recovery emits an interruption event. Runtime execution failures store a safe stage/operation reference rather than copying raw error strings or resolved secrets into history; existing VM events carry detailed runtime errors.

For example, an update accepts desired revision B while revision A is running. If graceful shutdown fails, the operation is `failed` and observed status remains `Start`. If its owner crashes before recording the outcome, verified recovery records `interrupted` and `Start`, with A as the observed boot revision and B still desired. Neither case claims B is running. Inspect the revisions/runtime/events, resolve the cause, and submit a fresh update; the new UUID becomes current while the earlier record remains historical. Console access to A still requires both A and B to enable it.

### Ownership-file cleanup and migration

Completion and definitively rejected acceptance attempt exact file cleanup while holding its lock. Cleanup locks the retained incarnation row before deciding whether an operation is absent, fencing any in-flight acceptance transaction. A pending record, database error, unknown commit, live flock, malformed metadata, or identity mismatch preserves the evidence. Terminal records and definitively absent attempts are eligible; cleanup rechecks the exact path/device/inode before unlinking. Unknown or corrupt evidence is not silently removed.

The recovery loop also visits at most 32 directory entries per pass with a persistent directory cursor, eventually revisiting eligible files missed by a crash. It never bulk-deletes the directory or removes operation/specification history. Retaining every historical lock file is unnecessary; no history-retention policy is introduced.

The six `2026-10-24` migrations introduce the shared schema directly, with a required incarnation on every newly created VM. They replace the earlier, unmerged PR-only migrations; those migrations were absent from released tags and `nightly` when checked. Deployment requires a fresh cluster recreated from NanoCL Statefiles. There is no old-schema backfill or runtime adoption. A development database that applied an earlier revision of this PR must be recreated with its cluster, not upgraded in place.

Stop all daemons and operation owners before cluster recreation or schema reversal. Overlapping old/new binaries are unsupported. The down migrations are checked in reverse dependency order: they discard retained operations/incarnations and their associations while preserving specification rows. Reapplying the new schema requires empty VM state. Production deployment uses cluster recreation and Statefile apply rather than an in-place upgrade or downgrade.

### Automatic recovery validation

The automatic-recovery correction passed formatting, Clippy, the development-binary build, and focused `cargo make tests` runs in a task-owned Docker engine with fresh state, both DNS/proxy profiles, a two-CPU cap, one Cargo job, and one test thread. The VM filter passed 123 tests, the operation filter passed 21, and the CLI-removal check passed one; these filters overlap. Three subprocess helpers are marked ignored for direct harness execution and are invoked by their parent tests.

The recovery fixture terminates only its newly created child controller, then invokes the production recovery scan. Previous-boot coverage uses simulated boot-identity fixtures, including changed filesystem identity and exact cleanup. No host reboot was performed, no host boot identity was changed, and the installed cluster was untouched. Broader suite validation runs in PR CI rather than repeating the full suite locally.

## Reverting a configuration

```sh
nanocl vm history global.desktop
nanocl vm revert global.desktop SPEC_UUID
```

Revert restores the complete saved declaration as a new desired revision using
the existing VM update lifecycle. The selected specification must belong to the
live VM and its current incarnation. Revisions remain immutable. Running guests
use the usual graceful shutdown and runtime replacement; the CLI waits for the
matching update operation and reports its failure if replacement cannot complete.
Guest disk contents are not rolled back.

The API is `PATCH /v0.19.0/vms/{vm_key}/histories/{history_key}/revert` with no
request body. HTTP 200 returns the accepted VM and `X-Nanocl-Operation`; clients
use that operation UUID to match terminal events. The SDK exposes `revert_vm`
and `revert_vm_operation` for callers that need the receipt.

Revision completion uses bounded history pages. `GET /vms/{vm_key}/histories`
accepts the existing JSON `filter` query with `limit` and `offset`; omitting them
keeps the complete-history response. The SDK exposes `list_history_vm_page` for
paged reads. Results are ordered by creation time and revision UUID, newest first.

## Live resource growth

Reserve CPU and memory ceilings when creating the VM. `HostConfig.MaxCpu` and `HostConfig.MaxMemory` are optional; omitted values use the initial `Cpu` and `Memory`, leaving no capacity for live growth of that resource. Memory values are MiB. The ceilings cannot be changed by a live resize: changing them through PATCH replaces the runtime and requires guest shutdown. `HostConfig` PATCH merges only supplied fields under the VM lifecycle lock; omitted fields retain their current values and explicit null clears nullable settings.

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

The API exposes `POST /v0.19.0/vms/{key}/resize` with a JSON body such as `{"Cpu":4,"Memory":2048,"DiskSize":34359738368}`, and `GET /v0.19.0/vms/{key}/resources`. Both use the daemon's configured operator access controls. At least one resource must be supplied. The response contains `Cpu`, `Memory`, `DiskSize`, `MaxCpu`, `MaxMemory`, and `Persistent`.

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
    Runtime: ghcr.io/next-hat/nanocl-qemu:8.0.5.0-nightly
    NetworkMode: vm-private
```

`CloudInit` defaults to `true` for existing configurations. When enabled without `CloudInitUserData`, it provisions users, SSH keys, passwords, and hostname from NanoCL's fields. Its DHCP configuration matches the persisted MAC and gives the interface the stable name configured by `HostConfig.NetIface` (default `nanocl0`), so early boot readiness does not retain a name that udev later changes. `CloudInit: false` generates and attaches no seed; the guest must already have a DHCP client and usable login credentials. Its interface naming remains guest-managed. Provisioning fields do not create guest accounts when cloud-init is disabled.

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

NanoCL writes the object as `#cloud-config` followed by JSON, which cloud-init accepts as YAML. OpenAPI and Statefile schemas bundle the [official cloud-init 26.2 cloud-config schema](https://raw.githubusercontent.com/canonical/cloud-init/26.2/cloudinit/config/schemas/schema-cloud-config-v1.json) with local references, so editors can provide completion and validation without fetching an external schema. NanoCL does not run that upstream schema validator; the guest's installed cloud-init version determines which settings it supports.

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

PATCH and Statefile apply preserve existing user-data when the field is omitted. Explicit `CloudInitUserData: null` clears custom user-data and restores generated account provisioning; the CLI equivalent is `nanocl vm patch global.guest --clear-cloud-init-user-data`. An empty string remains blank custom user-data. Use `CloudInit: false` to disable seed attachment. Updating the specification or restarting a VM does not force first-boot cloud-init modules to rerun: the instance ID stays stable and the guest keeps its cloud-init state on disk. Test new provisioning with a fresh guest disk, or explicitly manage reprovisioning from inside the guest.

Custom user-data requires a runtime image with `io.nanocl.vm-cloud-init-user-data=1` in addition to the runtime API label. Pull the updated nightly image with `docker pull ghcr.io/next-hat/nanocl-qemu:8.0.5.0-nightly`, then select that tag through `HostConfig.Runtime` as above. NanoCL uses an `IfNotPresent` pull policy, so a cached older nightly without the capability is rejected when custom user-data is enabled; pull the updated image explicitly before creating or replacing the runtime. A matching locally built runtime can also be selected through `HostConfig.Runtime`.

### Browser console

`Console` defaults to `false`. When enabled, QEMU serves VNC through a Unix socket owned by the runtime. NanoCL relays binary RFB frames through `GET /v0.19.0/vms/{namespace.name}/console`. The runtime does not expose an unauthenticated VNC or WebSocket TCP port. QEMU's native WebSocket listener and a separate websockify process are unnecessary for this Unix-socket relay.

The endpoint uses the daemon's configured access controls, including its Unix, TCP, or mutual TLS listeners. It adds no VM-specific transport or Origin restriction. Access grants the same daemon-wide operator privileges as the other APIs; there is no per-user VM ownership model. The daemon verifies the VM key, namespace, local runtime identity, running state, and specification generation before opening its socket.

For a custom Unix daemon socket, the connecting account must already have filesystem access. Socket ownership and group permissions are configured by the operator; the daemon does not automatically grant group access to custom paths. This access grants daemon-wide operator privileges.

A containerized daemon must bind its state directory at the same absolute path on the Docker host, as the standard installer already does. Console and QMP sockets share the private per-VM generation directory described above.

After replacement preflight fails or graceful shutdown times out, a new console connection resolves the original running boot specification from the container generation. Pending hostname, DNS, runtime, or provisioning changes do not block recovery access. Both the current desired specification and the running boot specification must enable the console; `Console: false` in either remains denied. Resize retains its stricter specification compatibility checks.

Use the [noVNC example](../examples/vm-console/README.md) for a local browser connection. Its loopback bridge holds operator credentials server-side and fixes the target VM. The WebSocket sends raw binary RFB frames, with an optional `binary` subprotocol and a maximum 64 KiB client frame.

| Response | Meaning |
| --- | --- |
| 101 | WebSocket established. |
| 400 | Invalid canonical key or WebSocket handshake. |
| 403 | Disabled console. |
| 404 | VM does not exist. |
| 409 | Runtime is absent, stopped, ambiguous, stale, or its console is not ready. |

CLI create/run/patch accepts `--cloud-init true|false`, `--cloud-init-user-data FILE`, and `--console true|false`. PATCH also accepts `--clear-cloud-init-user-data`, mutually exclusive with the user-data file. PATCH preserves omitted options. Use a Statefile to select a local candidate runtime explicitly.

## Fresh-cluster deployment

1. Pull the published nightly runtime for current development and CI testing:

   ```sh
   docker pull ghcr.io/next-hat/nanocl-qemu:8.0.5.0-nightly
   ```

   Nightly NanoCL builds default to this tag. Stable builds retain `ghcr.io/next-hat/nanocl-qemu:8.0.5.0`, which requires stable release publication. `HostConfig.Runtime` can select the nightly or an explicitly built local candidate. Pull updated nightly images explicitly because NanoCL uses `IfNotPresent`; an existing cached image is not refreshed automatically. NanoCL requires the image labels `io.nanocl.vm-runtime-api=0.19` and `io.nanocl.vm-qmp=1`; an older runtime without those contracts is rejected before replacement.

2. Recreate the cluster and its database/state, then apply current NanoCL Statefiles. Use `HostConfig.NetworkMode` with a named Docker bridge and DHCP-capable guest images.
3. VM creation assigns and persists a MAC when the Statefile omits it; an explicit `MacAddress` must be valid and unique. Existing definitions are not backfilled or adopted.
4. Use `CloudInit: true` for cloud-init provisioning or `false` for a prepared image with its own DHCP configuration and login credentials.
5. Enable `Console` only for the VMs that need it, and connect through the configured daemon endpoint or the loopback example.

## Reproducible release validation

Use disposable guest disk copies and a development NanoCL daemon. Record the NanoCL commit plus any uncommitted diff, runtime commit and immutable image digest, Docker version, QEMU version, network CIDR, and whether KVM was available. Establish image/source provenance with build metadata or exact comparisons of the packaged launcher/resource files to the recorded runtime revision; a mutable tag alone is insufficient. These instructions define required evidence; they do not claim a successful release-validation run.

1. Build the local runtime and prepare two separate writable guest images: a cloud-init image and an ordinary installed image with DHCP enabled and no cloud-init dependency. Use `CloudInit: true` and `false`, respectively, with the explicit candidate runtime. The ordinary guest must have independently configured login credentials.
2. Start both VMs on the same named Docker bridge. Inspect each runtime with `docker inspect <namespace.name>.v` and each VM with `nanocl vm inspect <namespace.name>`. Save the allocated endpoint IPv4 and the persisted MAC. Inside each guest, inspect `ip -4 address`, `ip -4 route`, and resolver configuration. Verify matching endpoint address/prefix, expected gateway, and distinct addresses.
3. Test a DNS lookup, outbound HTTP, gateway reachability, and intended ingress from another container or the configured NanoCL proxy. Use controlled reachable endpoints in the test environment. Capture UDP ports 67/68 on the Docker bridge while renewing guest leases: guest DHCP must stay on the private TAP.
4. Stop and start each VM through NanoCL, use `nanocl vm restart`, and restart the development daemon. Recheck MAC, lease, gateway, DNS, egress, and ingress. Verify graceful shutdown while the guest writes data and separately test a guest that ignores ACPI: a short timeout must report failure without power-off or restart. Verify explicit `--force` separately, including immediate awaited start → forced stop while running and after an early runtime exit. Inject task setup acquisition failure and confirm it leaves the pre-request specification/status and an existing owner untouched. Query QMP status after daemon reconnection, and reject a stale VM generation or wrong container identity. Replace a spec separately and record any Docker IP change. Do not confuse replacement behavior with same-container restart.
5. Confirm the no-cloud-init VM receives DHCP with no seed attached. Check the runtime's QEMU arguments and guest devices. Confirm cloud-init provisioning and SSH keys work on the enabled image.
6. Enable `Console`, start the [noVNC example](../examples/vm-console/README.md), and use a real browser to observe display output and send keyboard input. Test disconnect/reconnect, runtime restart, and replacement. Fail replacement both in preflight and by timing out graceful shutdown, then reconnect to the original guest and verify display and keyboard input. Repeat with desired `Console: false` and a mismatched runtime identity and verify denial. Verify the old connection closes when its backend exits.
7. Verify console access through the configured daemon listeners, including development TCP when enabled. Confirm that requests with a foreign/null Origin follow the same daemon access policy. Check rejection with `Console: false`, against a missing VM, and against a stopped runtime. If the daemon requires mutual TLS, verify that missing or invalid client certificates are rejected by that listener. Separately verify the example bridge's own Origin/Host restrictions. An operator trusted by the daemon can access enabled consoles across namespaces by design.
8. Delete the VMs. Verify their runtime containers, TAP devices, dnsmasq/QEMU processes, and console sockets disappear. Delete only test-owned images/networks after checking that no unrelated workloads use them.

For live resizing, use the NanoCL resize API or CLI throughout; manually writing `resources.json` does not validate daemon persistence or recovery. Start a guest with explicit CPU/memory ceilings and the resize-capable runtime. Record QEMU resources, guest online CPU/memory, and disk/filesystem sizes separately. Grow each resource and a combined request, verify shrink/ceiling/alignment rejection before mutation, then verify persistence across a same-container restart. Exercise a partial failure, inspect `Persistent`, and retry the same totals. Confirm stale generation and foreign container identities are rejected and console access still works after a resource-only specification revision.

The unit tests cover parsing, identity rules, configuration propagation, and console checks. Real guest boot, DHCP renewal, DNS, ingress, simultaneous VMs, and browser interaction require the above runtime evidence. A namespace probe or a successful image build alone does not establish them.
