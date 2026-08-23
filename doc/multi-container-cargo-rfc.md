# RFC: Multi-container Cargo

Status: Implemented

Last updated: 2026-08-22

## 1. Purpose

This RFC defines Nanocl's multi-container Cargo model and the invariants that
must be preserved by public declarations, persistence, runtime reconciliation,
Docker-event reduction, and Cargo lifecycle commands.

A Cargo is a replicated group of named application containers. Each replica
may also contain ordered init containers and, except in Cargo-level host
network mode, an internal network sandbox. Containers in the same sandbox use
one network namespace and communicate over localhost.

This document describes the implemented contract. It supersedes the original
pre-implementation multi-container Cargo audit.

## 2. Design goals

- Allow one Cargo replica to run multiple named application containers.
- Preserve an ordered init-container phase for every replica.
- Give each non-host replica one stable network identity.
- Keep the authored declaration separate from Docker-effective and observed
  configuration.
- Give replicas and their logical processes durable identities independent of
  Docker container IDs and generated names.
- Keep the generic process model usable by Cargoes, Jobs, and VMs.
- Make readiness depend on the complete essential application topology.
- Preserve retained workloads when a safe update candidate fails.

The design does not introduce a general container dependency graph, shared
PID/IPC/UTS namespace model, Kubernetes-compatible workload API, or ambiguous
Cargo-scoped raw process identity.

## 3. Public declaration

The canonical declaration types are `CargoSpec`, `ContainerSpec`,
`CargoSpecRevision`, and `CargoSpecPatch` in
[`crates/nanocl_stubs/src/cargo_spec.rs`](../crates/nanocl_stubs/src/cargo_spec.rs).

`ContainerSpec` is deliberately flattened. Nanocl-owned fields such as `Name`,
`Essential`, and `Secrets` appear beside the typed Bollard container fields
such as `Image`, `Cmd`, `Env`, and `HostConfig`.

```yaml
ApiVersion: v0.18
Namespace: example

Cargoes:
- Name: web
  Replicas: 1
  PortBindings:
    "8080/tcp":
    - HostIp: 127.0.0.1
      HostPort: "8080"
  Secrets:
  - shared-settings
  InitContainers:
  - Name: prepare
    Image: busybox:1.36
    Cmd:
    - sh
    - -c
    - test -d /data
    HostConfig:
      Binds:
      - app-data:/data
  Containers:
  - Name: api
    Image: example/api:1
    Env:
    - LISTEN=0.0.0.0:8080
    HostConfig:
      Binds:
      - app-data:/data
  - Name: metrics
    Essential: false
    Image: example/metrics:1
    Env:
    - API=http://127.0.0.1:8080
```

### 3.1 Cargo fields

| Field | Contract |
|---|---|
| `Name` | Required Cargo name. |
| `Metadata` | Optional user metadata. |
| `Replicas` | Durable replica count. Defaults to one and must be at least one. |
| `NetworkMode` | Optional Cargo-owned Docker network mode. |
| `PortBindings` | Optional Cargo-owned Bollard `PortMap`. |
| `Secrets` | Secrets inherited by every declared app and init container. |
| `Placement` | Existing node-placement policy. |
| `ResourceRequirement` | Existing scheduling requirements. |
| `InitContainers` | Ordered init-container declarations. Defaults to empty. |
| `Containers` | Required nonempty application-container declarations. |

### 3.2 Container fields

| Field | Contract |
|---|---|
| `Name` | Required logical name, unique across app and init collections. |
| `Essential` | Whether the application participates in replica readiness. Defaults to `true`. Init containers must remain essential. |
| `Secrets` | Secrets injected only into this container. Defaults to empty. |
| `ImagePullSecret` | Optional registry credential for this container image. |
| `ImagePullPolicy` | Per-container pull policy. Defaults to `IfNotPresent`. |
| Other fields | Strictly decoded Bollard container `Config` fields. `Image` is required. |

The exact logical name `_sandbox` is reserved for Nanocl's internal sandbox.
Names such as `sandbox` and `_sandbox-helper` are not reserved by this rule.

Application and init names share one namespace and must not be duplicated. At
least one application must be essential. Unknown outer or nested Bollard fields
are rejected instead of being silently ignored.

`CargoSpec`, stored `CargoSpecRevision` declarations, and a complete spec
produced by applying `CargoSpecPatch` all use the same semantic validation.
Patch collections replace their complete corresponding collection; PATCH does
not merge individual container entries.

## 4. Network topology

The Cargo network mode determines whether a replica has a sandbox and how a
container with no explicit override is compiled.

| Cargo `NetworkMode` | Sandbox | Default app/init effective mode |
|---|---:|---|
| omitted or `nanoclbr0` | Yes | `container:<sandbox-id>` |
| custom network | Yes | `container:<sandbox-id>` |
| `bridge` or `default` | Yes | `container:<sandbox-id>` |
| `none` | Yes | `container:<sandbox-id>` |
| `host` | No | `host` |

An omitted mode uses Nanocl's default network. A custom network is validated
and ensured through Nanocl's existing network handling. Cargo-level
`container:<id>` is forbidden because declarations cannot depend on an
ephemeral Docker process.

A sandbox-backed replica shares only its network namespace. Nanocl does not
implicitly share PID, IPC, UTS, cgroup, or user namespaces. The `none` Cargo
mode still creates a sandbox: its containers share localhost with each other
but have no external network attachment.

A declared app or init container may explicitly set
`HostConfig.NetworkMode` to `host` or `none`. Such a container does not join
the Cargo sandbox. Other per-container network modes are rejected.

Docker fields that conflict with `container:<sandbox-id>` are rejected before
Docker mutation. This includes container-owned hostname, MAC address, exposed
ports, DNS settings, extra hosts, and links. Raw per-container port bindings,
publish-all-ports, auto-remove, and user-authored `container:<id>` namespace
references are also forbidden.

## 5. Port ownership

Host port bindings belong to the Cargo, not to an individual application.
Nanocl installs the Cargo `PortBindings` and matching exposed-port keys on the
sandbox. Every application in the shared network namespace can bind the
container port, so application authors must avoid internal listener conflicts.

Cargo port bindings are invalid with Cargo-level `host` or `none`. Keys must
use `<port>/<tcp|udp|sctp>`, and explicit host ports must be valid numeric
ports. Dynamic host ports remain supported through an empty or zero host-port
value.

The current local runtime rejects multiple replicas assigned to one node when
the Cargo declares a fixed host port. Docker remains authoritative for
conflicts with processes not managed by Nanocl.

## 6. Declared and effective configuration

The authored Cargo revision is desired state. Runtime compilation always works
on a clone and must not write injected Docker values back into the revision.

The pure compiler in
[`bin/nanocld/src/utils/container/cargo_compiler.rs`](../bin/nanocld/src/utils/container/cargo_compiler.rs)
owns:

- the effective Docker network mode;
- Cargo identity, replica, role, logical-container, and essential labels;
- Nanocl runtime environment variables;
- application restart-policy defaults;
- Cargo port installation on the sandbox;
- `$$INTERNAL_GATEWAY` expansion in the effective configuration; and
- rejection of user labels and environment variables reserved by Nanocl.

Cargo-level secrets are inherited by every app and init declaration.
Container-level secrets are added only to that container. The sandbox receives
neither application secrets nor application configuration.

The three representations remain distinct:

1. authored `CargoSpec` persisted in spec history;
2. ephemeral Docker configuration produced by the Cargo compiler; and
3. Docker inspect observations stored with concrete generic processes.

## 7. Durable runtime identity

A Cargo replica is not a Docker container. It is a stable UUID plus a
zero-based ordinal and node assignment stored in `cargo_replicas`.

Each logical replica process is stored in `cargo_replica_processes` with:

- its replica UUID;
- role: `sandbox`, `init`, or `app`;
- logical container name;
- essential flag; and
- optional attachment to the current generic `processes` row.

The `_sandbox` name is valid only with the `sandbox` role. Init and sandbox
roles are always essential. A concrete process may be attached to only one
logical mapping.

Reserved Docker labels repeat the stable replica UUID, ordinal, role, logical
name, and essential flag. Generated Docker names are operational details and
must not be used as durable identity.

The generic `processes` table remains the concrete Docker inventory for
Cargoes, Jobs, and VMs. Cargo-specific identity augments it rather than
changing its meaning for other workload kinds.

## 8. Replica lifecycle

### 8.1 Start and reconciliation

Replicas are reconciled in ordinal order on their assigned node:

1. validate networks, ports, secrets, and effective container configuration;
2. create or reuse the replica sandbox when required and start it;
3. run init containers sequentially in declaration order and require exit code
   zero;
4. create and start application containers in declaration order; and
5. wait for the complete required topology to become ready.

A completed init container is retained as an observation. Starting or
restarting an unchanged Cargo does not rerun a successfully completed init
container.

### 8.2 Restart

Explicit Cargo restart validates the complete persisted topology, then issues
Docker restart only for mapped application containers. It preserves the
sandbox and completed init containers. Success is emitted only after every
essential application and required sandbox is ready again.

### 8.3 Update and revert

PUT, PATCH, and revert persist a validated declaration and use the same Cargo
update path. The current safe rollout does not support changing `Replicas` or
`Placement`, and it rejects a rollout that requires remote-node coordination.

For supported local updates, replacement candidates are preflighted and made
ready before promotion. The prior generation is retained for rollback and
route handoff when its network/port ownership permits overlap. Fixed host-port
and host-network replacements stop the old listener first because the old and
new processes cannot safely own the same host resources concurrently.

### 8.4 Stop and destroy

Stop handles application containers before the sandbox. Init containers are
normally already stopped. Runtime deletion removes applications, then init
containers, then the sandbox so no live application retains a deleted network
namespace.

Destroy removes runtime processes before deleting desired Cargo state and its
dependent replica mappings.

## 9. Readiness and Docker events

A desired replica is ready only when:

- its exact essential application set is present and running;
- every essential application with an enabled Docker healthcheck is healthy;
- it has exactly one running sandbox when the Cargo is not in host mode; and
- it has no sandbox when the Cargo is in host mode.

A Docker healthcheck with `Test: ["NONE"]` is disabled. Applications without
an enabled healthcheck are ready when running. Nonessential applications do
not gate aggregate readiness.

Aggregate Cargo readiness requires every desired durable replica to satisfy
the replica rule. A first or arbitrary process must never represent the whole
Cargo.

Only the active stable Cargo runtime participates in Docker-event reduction.
Stopped init containers and retained or candidate rollout processes do not
become runtime failures. Intentional stop and destroy transitions also suppress
unexpected-death handling. During synchronous reconciliation, the reconciler
owns promotion and rollback so out-of-order Docker events cannot promote an
incomplete candidate.

## 10. Startup recovery

Startup inventory rebuilds concrete process observations from Docker and uses
the reserved identity labels to reattach stable Cargo mappings. Existing
authored Cargo revisions remain the desired declaration during ordinary
reattachment.

Installer/bootstrap workloads are the explicit adoption exception. When
desired rows are absent, or an installer generation must be finalized, Nanocl
may reconstruct a validated declaration from a complete stable labelled
generation. Reconstruction removes compiler-owned labels, runtime environment,
internal secret mounts, effective network references, and rollout artifacts
before persisting the declaration. Contradictory or incomplete identity is an
error rather than a reason to guess.

## 11. API, CLI, and observability boundaries

Cargo-wide create, start, stop, restart, update, revert, remove, logs, and
stats operations remain workload lifecycle operations. A Cargo key identifies
the group; it does not identify one Docker process.

Operations that require one concrete runtime process must resolve an exact
managed process name or ID. Cargo-scoped raw exec and grouped kill semantics
are intentionally not part of this contract.

`nanocl cargo ls` reports Cargo resources and aggregate replica counts.
`nanocl ps` reports concrete managed processes, including internal sandbox and
init processes where applicable. Seeing a sandbox in process inventory but not
as a separate Cargo in `cargo ls` is expected.

Cargo inspect currently exposes a flat process list for compatibility. Durable
replica/process mappings and labels remain authoritative for internal topology;
consumers must not infer replica identity from list order or generated names.

## 12. Current implementation limits

- Safe update does not change replica count or placement.
- Safe update does not coordinate a candidate rollout across remote nodes.
- Multiple local replicas cannot share one fixed host port.
- Container dependencies are limited to ordered init containers followed by
  applications; there is no general dependency graph.
- Only network namespaces are shared implicitly.

These are explicit boundaries of the implemented RFC, not placeholders for
automatic compatibility behavior.

## 13. Primary implementation locations

- Public model and semantic validation:
  [`crates/nanocl_stubs/src/cargo_spec.rs`](../crates/nanocl_stubs/src/cargo_spec.rs)
- Pure declared-to-effective compiler:
  [`bin/nanocld/src/utils/container/cargo_compiler.rs`](../bin/nanocld/src/utils/container/cargo_compiler.rs)
- Runtime reconciliation, restart, rollout, readiness, and deletion:
  [`bin/nanocld/src/utils/container/cargo.rs`](../bin/nanocld/src/utils/container/cargo.rs)
- Docker-event reduction:
  [`bin/nanocld/src/system/docker_event.rs`](../bin/nanocld/src/system/docker_event.rs)
- Startup inventory and bootstrap adoption:
  [`bin/nanocld/src/utils/system.rs`](../bin/nanocld/src/utils/system.rs)
- Durable replica models and repository:
  [`bin/nanocld/src/models/cargo_replica.rs`](../bin/nanocld/src/models/cargo_replica.rs)
  and
  [`bin/nanocld/src/repositories/cargo_replica.rs`](../bin/nanocld/src/repositories/cargo_replica.rs)
- Durable replica schema:
  [`cargo_replicas`](../bin/nanocld/migrations/2026-08-22-000000_cargo_replicas/up.sql)
  and
  [`cargo_replica_processes`](../bin/nanocld/migrations/2026-08-22-000001_cargo_replica_processes/up.sql)

Changes to these areas must preserve the public declaration, identity,
topology, readiness, and lifecycle invariants defined above. Focused unit and
integration tests remain the executable conformance suite.
