# Multi-container Cargo audit

Audit date: 2026-08-02

Scope: design and repository audit only. This report does not implement the refactor. Current behavior is described from this checkout; proposed names and wire shapes are intentionally breaking.

## 1. Executive summary

Nanocl cannot safely turn the current singular `Cargo.Container` into a list by changing only the public type. The runtime, database, health reducer, event flow, proxy, client, and CLI all assume that one Cargo replica is one concrete Docker process. The present `processes` table has only an ephemeral Docker ID and a polymorphic `kind_key`; it cannot identify a stable Cargo replica or a named container within it.

The recommended target is:

- A Cargo owns `Replicas`, placement, one default network mode, one real Bollard `PortMap`, Cargo-shared secrets, named init containers, and one or more named application containers.
- `ContainerSpec` owns its stable logical name, `Essential`, container secrets, image-pull settings, an optional closed `host`/`none` network escape, and an explicit nested Bollard `Container` config.
- Do not flatten Bollard `Config`. Keep `Container` as a namespace boundary and deserialize it with a strict adapter that rejects fields Bollard would otherwise ignore.
- For default/custom/`bridge` networks and Cargo-level `none`, use a dedicated internal sandbox per replica. It owns the network namespace, IP, attachment, and host port mappings. Normal application and init containers compile to `container:<sandbox-id>`. Cargo-level `host` needs no sandbox because there is no distinct replica network identity to preserve.
- Share only the network namespace. Do not synthesize IPC, PID, UTS, cgroup, or user namespace sharing and do not introduce a generic namespace matrix.
- Add Cargo-specific `cargo_replicas`, `cargo_replica_processes`, and fixed-port reservation tables, plus durable Cargo desired lifecycle/runtime-generation fields. Keep the generic `processes` table and its Job/VM uses intact.
- Keep declared specs, ephemeral compiled Docker configs, and redacted Docker inspect observations as different layers. In particular, startup recovery must never reconstruct desired state from Docker inspect.
- Persist per-process observations and per-replica reconciliation/readiness. Derive Cargo aggregate conditions and counts from desired lifecycle, runtime generation, effective-template hashes, and replicas rather than storing another overloaded aggregate enum. A spec revision UUID is history identity, not a rollout generation.
- Resource creation is successful when the desired spec is validated and durably accepted. Default `state apply` should return after durable acceptance, not runtime health or a serial health-gated rollout. Explicit bounded reconciliation/readiness waits should be available. A later unhealthy workload is not a creation failure.
- Roll out by stable container name and one replica at a time. A container-only change keeps the sandbox/IP/ports; a network or Cargo-port change replaces the whole replica.

The safest first implementation task is an unused, independently tested public-model foundation in `nanocl_stubs`: strict `ContainerSpec`, network types, strict Bollard deserialization, and reexports of the real Bollard port types. It must not yet switch the Cargo wire format, database, runtime, API, CLI, or status behavior.

## 2. Current Cargo model

### Public and stored shape

`Statefile.Cargoes` is `Option<Vec<CargoSpecPartial>>` (`crates/nanocl_stubs/src/statefile.rs:235-306`). `CargoSpecPartial` is simultaneously the Statefile/create/PUT input and the JSONB desired-spec payload. It is defined at `crates/nanocl_stubs/src/cargo_spec.rs:209-266` with these fields:

- `Name`, `Metadata`;
- singular `InitContainer: Option<Config>`;
- Cargo-wide `Secrets`, `ImagePullSecret`, and `ImagePullPolicy`;
- singular `Container: bollard_next::container::Config`;
- `Placement` and `ResourceRequirement`.

Replication is currently `Placement.Replicas`, not a public `Replication` field (`cargo_spec.rs:165-207`). Scheduling defaults it to one and clamps it to at least one in `bin/nanocld/src/utils/container/generic.rs:224-237`.

`CargoSpecUpdate` repeats the singular ownership model for PATCH (`cargo_spec.rs:268-336`). `CargoSpec` is a stored/history response containing the UUID, Cargo key, version, and creation time plus the same declaration (`cargo_spec.rs:354-419`). The names are misleading: `CargoSpecPartial` is the complete desired input, while `CargoSpec` is a revision record.

The response model also assumes one process per replica:

- `Cargo` contains one aggregate status and one `Spec` (`crates/nanocl_stubs/src/cargo.rs:17-40`).
- `CargoSummary` exposes process totals/running counts (`cargo.rs:64-84`).
- `CargoInspect.Instances` is a flat `Vec<Process>` (`cargo.rs:86-109`).
- `Process` exposes Docker ID/name, kind/key, node, and raw inspect JSON, but no replica, logical container, role, attempt, or generation (`crates/nanocl_stubs/src/process.rs:73-123`).

### Strictness is only skin-deep

`Statefile`, `CargoSpecPartial`, and `CargoSpecUpdate` use PascalCase and `deny_unknown_fields`; however, both Cargo input structs also use blanket `serde(default)` (`cargo_spec.rs:210-217,272-279`). Missing `Name` therefore becomes `""` and missing `Container` becomes `Config::default()` rather than a deserialization error. The checked-in OpenAPI has no required list and shows those empty defaults (`bin/nanocld/specs/swagger.yaml:2575-2634`).

`cargo_spec` publicly reexports Bollard `Config`, `HealthConfig`, and `HostConfig` (`cargo_spec.rs:4-7`). Pinned Bollard `Config` and its nested generated models derive `Deserialize` without `deny_unknown_fields`; a typo below `Container` is silently ignored. The current OpenAPI similarly leaves the `Config` and `HostConfig` objects open (`swagger.yaml:2806-2977,3994-4214`). Existing examples expose this exact hole: obsolete `Replication` beneath raw `Container` is accepted and discarded in `examples/fail_init_container.yml:13-14` and `examples/success_init_container.yml:13-14`.

Statefiles are parsed directly through Serde for YAML/JSON/TOML, without a separate JSON-Schema validation pass (`bin/nanocl/src/utils/state.rs:62-140`). API bodies are also direct Serde inputs (`bin/nanocld/src/services/cargo/create.rs:26-39`, `put.rs:26-42`, `patch.rs:26-46`). Runtime deserialization must therefore be authoritative; schema documents alone cannot provide strictness.

### PATCH and replication behavior

Current PATCH is not a general structural merge. `bin/nanocld/src/objects/cargo.rs:149-285` gives special treatment to only image, command, environment, and binds and can silently fail to express other Bollard changes. Current start creates requested replicas only if no runtime process exists (`bin/nanocld/src/utils/container/cargo.rs:341-418`); changing the replica count does not reconcile an existing set. Current update renames all old processes and creates exactly one local replacement (`cargo.rs:440-595`, especially line 527), independent of desired placement/replicas.

## 3. Current end-to-end creation and reconciliation flow

1. **Parse and compare.** The CLI parses the Statefile directly with Serde. It resolves relative binds before sending (`bin/nanocl/src/utils/docker.rs:180-219`; caller `bin/nanocl/src/commands/state.rs:588-609`) and later compares that transformed shape with the stored spec (`state.rs:885-900`).
2. **Accept desired state.** `POST /cargoes` receives `CargoSpecPartial`. Cargo creation inserts a spec row, an `object_process_statuses` row, and the Cargo row as separate operations (`bin/nanocld/src/objects/cargo.rs:29-82`). They are not one transaction, so a partial database failure can orphan state.
3. **Schedule counts.** Placement produces node/count assignments, not stable replica identities (`bin/nanocld/src/utils/container/generic.rs:204-237`). Node RPC payloads carry only Cargo key and replica count (`crates/nanocl_stubs/src/node.rs:26-45`; `bin/nanocld/src/tasks/cargo.rs:18-50`).
4. **Preflight networks.** The Cargo and singular init configs are scanned for raw Docker network modes (`bin/nanocld/src/utils/container/cargo.rs:65-76`). Omitted mode resolves to `nanoclbr0`; custom names are inspected or created node-locally as attachable bridge networks (`bin/nanocld/src/utils/container/network.rs:32-54,284-370`).
5. **Compile by mutation.** Creation serializes the whole `Cargo`, replaces `$$INTERNAL_GATEWAY`, and deserializes it back (`cargo.rs:226-240`; `bin/nanocld/src/utils/container/generic.rs:155-197`). It then pulls the one image, loads Cargo secrets, and constructs a new Docker `Config`.
6. **Inject runtime fields.** Compilation injects reserved labels, `NANOCL_*` environment variables, secret environment, a TLS-secret bind, hostname suffix, default `RestartPolicy=Always`, TTY/attach flags, and the effective network mode (`cargo.rs:251-320`). `AutoRemove` is rejected only here (`cargo.rs:282-288`).
7. **Run init.** If configured, one init container is created, started, and synchronously waited to a non-running state; nonzero exit fails reconciliation (`cargo.rs:108-216,384-391`). It uses the Cargo-wide secrets and image-pull settings.
8. **Create concrete processes.** Each requested local instance receives a random Docker name such as `{namespace}.{cargo}-{short}.c` and a process row keyed by Docker ID (`cargo.rs:251-336`; `bin/nanocld/src/utils/container/process.rs:48-114`). `NANOCL_CARGO_INSTANCE` is the local loop index, so it is neither stable after recreation nor globally unique across nodes.
9. **Start.** The daemon starts all processes found by Cargo `kind_key`. Without a healthcheck it immediately sets aggregate `actual=Start`; with a healthcheck it returns while status remains starting and delegates completion to Docker events (`cargo.rs:339-435`).
10. **Observe.** Docker events refresh the raw inspect JSON in `processes` and update the single Cargo status (`bin/nanocld/src/system/docker_event.rs:227-602`). There is no periodic desired-state reconciler that recreates missing processes; startup launches inventory synchronization and the Docker event listener (`bin/nanocld/src/system/init.rs:225-249`).
11. **Update/delete.** Update performs local rename/stop/create operations over flat process rows. Delete sends remote deletion by Cargo key when needed, ignores local Docker stop/remove failures, and then clears desired DB state (`bin/nanocld/src/utils/container/cargo.rs:437-655`).

### Declared state is currently contaminated by observation

The most serious current boundary violation is startup recovery. `sync_processes` inspects labelled Docker containers, converts the first Cargo container's inspected `Config` plus `HostConfig` into a new `CargoSpecPartial`, and updates the desired spec when it differs (`bin/nanocld/src/utils/system.rs:183-239`). That inspected config includes Nanocl-injected environment, labels, binds, restart defaults, effective network mode, and other daemon-normalized fields. The selected container may also be an arbitrary replica or init process.

The target controller must maintain four separate objects:

1. authored/declared `ContainerSpec` persisted in `specs.data`;
2. declared nested Bollard `Config` inside it;
3. ephemeral Nanocl-compiled Docker config;
4. observed Docker inspect response associated with the concrete `Process`, redacted before persistence/public return.

Only layer 1 is used for desired-state history and declarative diffing. Compile into a clone; never write layers 3 or 4 back to it. Expand `$$INTERNAL_GATEWAY` only in supported Docker string fields on that clone, not by replacing text in a serialized whole Cargo. The target semantics need to be explicit: inspect the sandbox attachment for default/custom networks; reject the token in a `none` container; and retain the current `nanoclbr0` fallback for host/built-in modes only if a pinned-network test proves it reachable and intentional. Startup may rebuild observed rows from reserved labels, but it must never adopt inspect data as a desired spec.

Docker inspect `Config.Env` contains secret-derived values today, and `HostConfig.Binds` exposes internal secret paths. Persist/publicly return a Cargo-specific redacted inspect snapshot: remove secret-derived values and internal bind sources while retaining the runtime/health/network fields needed by the controller. A privileged live diagnostic must apply the same redaction. Registry credentials never belong in config, labels, inspect, or status. The backup workaround that strips an injected secret bind (`bin/nanocl/src/commands/backup.rs:11-49,75-88`) should become unnecessary once this boundary is respected. A broader Job/VM inspect-redaction audit is worthwhile but outside this Cargo refactor.

## 4. Current database relationships

| Table | Current role | Important relationships/gaps |
|---|---|---|
| `specs` | All versioned desired objects in JSONB | UUID key, `kind_name`, `kind_key`, version, data, metadata (`bin/nanocld/migrations/2022-05-24-185444_specs/up.sql:2-18`). Cargo serialization/reconstruction is in `bin/nanocld/src/repositories/spec.rs:111-159`. |
| `object_process_statuses` | One generic wanted/actual/health record | Shared by Cargoes, Jobs, and VMs; only string lifecycle/health fields (`2022-01-10-150631_object_process_statuses/up.sql:2-20`). It cannot represent replicas or containers. |
| `cargoes` | Namespace-scoped Cargo identity | References one current spec, one aggregate status, and one namespace (`2022-06-17-122356_cargos/up.sql:1-15`). |
| `processes` | Concrete Docker runtime object | Primary key is ephemeral Docker ID; stores name, kind, inspect JSONB, node FK, and untyped `kind_key`. There is no Cargo FK or logical uniqueness (`2023-11-19-101132_processes/up.sql:2-20`). Diesel joins it only to `nodes` (`bin/nanocld/src/schema.rs:189-200`). |
| `networks` | Node-local observed/managed Docker network | Unique by node/name (`2026-07-18-000000_networks/up.sql:1-16`). It is not a Cargo ownership relation. |

`processes` is deliberately polymorphic. Jobs create a process for every sequential container (`bin/nanocld/src/utils/container/job.rs:33-105`); VMs store runtime and init container processes there (`bin/nanocld/src/utils/container/vm.rs:65-115,159-276`). Renaming the table, making Cargo columns mandatory, or redefining `kind_key` would impose an unrelated Job/VM migration. Keep it generic.

Current Cargo-process association is only `processes.kind = cargo` plus string `kind_key = cargo key`. There is no stable relationship to a namespace except through the Cargo key/labels, no replica entity, and no way to distinguish sandbox/init/application roles or old/candidate generations. Random Docker names and node-local ordinals are not suitable identities.

Deletion is also non-transactional: the repository manually clears Cargo/history/status (`bin/nanocld/src/repositories/cargo.rs:287-293`). All desired DB mutations should become transactional; Docker work remains outside the transaction and is made retryable/idempotent from durable desired state.

One migration hazard needs release-history verification: Git history shows commit `fe90b6bf` later edited the original 2022 status migration for Cargo health, and there is no later forward migration that adds the column. Databases that had already recorded the earlier migration before that release may be missing it. Determine the affected released versions; do not edit historical migrations again, and add a forward, idempotently tested repair where needed.

## 5. Current health-check and deployment-blocking behavior

The daemon API and CLI currently expose different effective semantics:

The public lifecycle enum is `Create`, `Starting`, `Start`, `Updating`, `Update`, `Destroying`, `Destroy`, `Stopping`, `Stop`, `Fail`, `Finish`, or `Unknown`; aggregate health is only `Healthy`, `Unhealthy`, or `Unknown` (`crates/nanocl_stubs/src/system.rs:53-105`). One `ObjPsStatus` stores wanted/actual/previous values plus that health (`system.rs:150-162`). This overloaded model is the behavior being replaced, not extended.

1. Raw Bollard `Config.Healthcheck` comes through the Statefile/API. Image-inherited healthchecks are discovered only from Docker inspect (`bin/nanocld/src/utils/container/cargo.rs:45-62`; `process.rs:47-114`).
2. The start service marks `Starting`, schedules Docker work, and returns `202` (`bin/nanocld/src/services/process/start.rs:39-82`). Thus the HTTP boundary is asynchronous.
3. Cargo start/update does not publish aggregate `Start` when any declared or inspected healthcheck is present (`bin/nanocld/src/utils/container/cargo.rs:411-433,539-595`).
4. `health_status` events inspect all active Cargo processes. Only when all are Docker-healthy does the event handler set aggregate health/actual state and emit `Start` (`bin/nanocld/src/system/docker_event.rs:227-378`).
5. Unhealthy during start/update sets aggregate `actual=Fail`, emits an error, and attempts rollback. A later runtime unhealthy transition can also set `Fail`, conflating reconciliation failure with workload health.
6. For a new Cargo, `state apply` creates the DB object, then subscribes, then requests start; for an update it subscribes before PUT (`bin/nanocl/src/commands/state.rs:859-908`). It waits for a Cargo `Start` or error event. `wait_process_state` has no timeout (`bin/nanocl/src/utils/process.rs:47-80`). Applies are sequential, so one forever-starting healthcheck prevents later objects from applying.
7. A clean event-stream EOF is treated as success even without a matching event. Events are in-memory and daemon-local, so reconnects, daemon restarts, missed events, and remote-node observations can strand the wait.
8. `ncproxy` also gates healthchecked routes on the aggregate health signal (`bin/ncproxy/src/subsystem/event.rs:58-91`).

Additional correctness problems become blockers for multiple containers:

- `Healthcheck.is_some()` treats a disabled Docker check such as `Test: ["NONE"]` as readiness-gated, but no healthy event may ever arrive.
- A healthless container remains health `Unknown`; an all-processes-healthy reducer would block a mixed healthchecked/healthless replica.
- Aggregation can fetch cluster-wide process rows and inspect their Docker IDs through the local Docker daemon (`docker_event.rs:195-224`).
- It checks existing rows, not desired replica count or generation, so an under-scheduled Cargo can be called healthy.
- Startup maps `running` to aggregate `Start` without restoring Docker health (`bin/nanocld/src/utils/system.rs:113-141`).
- Docker events are the only health trigger; there is no periodic re-observation to repair missed/out-of-order events.
- Healthy rollout cleanup of `tmp-` processes occurs before publishing Healthy/Start; cleanup failure can suppress readiness even when every candidate is healthy (`bin/nanocld/src/system/docker_event.rs:323-375`). A healthchecked `die` is excluded from part of the normal Fail path (`docker_event.rs:504-569`), permitting stale aggregate health.
- Group restart calls Docker and emits Restart without first making lifecycle/health Starting/Unknown (`bin/nanocld/src/utils/container/process.rs:196-239`).
- Reapplying an unchanged Cargo can print `(running)` even when aggregate actual state is Fail, Stop, or Starting (`bin/nanocl/src/commands/state.rs:885-900`). Generic `cargo start` prints a start/readiness error but returns success (`bin/nanocl/src/commands/generic.rs:180-203`).
- `ObjPsStatusUpdate` has no `updated_at` field, so status transition updates do not advance the timestamp (`bin/nanocld/src/models/object_process_status.rs:37-50`).
- Cargo list renders `actual/wanted (health)` and Docker-running counts, while process list has no Docker-health column (`bin/nanocl/src/models/cargo.rs:319-345`; `models/process.rs:83-120`). Inspect includes all flat Process rows, while summary/list filters differ; neither count is replica readiness (`bin/nanocld/src/objects/cargo.rs:288-313`; `repositories/cargo.rs:222-284`).

No single new status field fixes this. Reconciliation, runtime state, Docker health, replica readiness, desired generation, and aggregate availability must be separate observations/conditions.

No focused automated test currently protects the blocking contract or health reducer. Daemon Cargo service tests exercise ordinary lifecycle/init and event subscription but make no health-state assertions (`bin/nanocld/src/services/cargo/mod.rs:213-535`); client tests start Cargo without waiting on health (`crates/nanocld_client/src/cargo.rs:251-332`); `tests/e2e.bats:49-91` covers generic Cargo run/apply success; and `examples/health_check_image.yml` is a manual fixture, not an enforcing test. Inline Docker-event tests cover other helpers rather than end-to-end health/wait semantics. This is a material coverage gap, not evidence that current blocking behavior is intended.

## 6. Proposed public Statefile shape

The examples below are complete examples of the proposed breaking shape. `v0.19` is an illustrative next API token, not a claim that this checkout accepts it; the release task must choose the actual version. Outer Nanocl fields use PascalCase. Nested Bollard fields retain Bollard's existing names such as `Image`, `Cmd`, `Env`, `HostConfig`, `Binds`, `HostIp`, and `HostPort`.

### Normative field shape

```yaml
ApiVersion: v0.19
Namespace: example
Cargoes:
- Name: service
  Replicas: 2
  NetworkMode: private-net
  PortBindings: {}
  Secrets: []
  Placement: {}
  ResourceRequirement: {}
  InitContainers: []
  Containers:
  - Name: app
    Essential: true
    Secrets: []
    ImagePullPolicy: IfNotPresent
    ImagePullSecret: registry-credentials
    NetworkMode: host
    Container:
      Image: example/app:1
```

`Name` and a nonempty `Containers` list are required. `Replicas` defaults to one at that field only and must be at least one. `Essential` defaults to true. `InitContainers`, secrets, placement, resource requirements, network mode, and port bindings may be omitted; do not apply blanket defaults to the whole input struct.

Container names must be unique across application and init collections and use a lower-case DNS-label form (1-63 characters, alphanumeric endpoints, interior `-` allowed). Every app/init must declare a nonempty `Container.Image`, and at least one application must be essential so readiness cannot become vacuously true. Names are durable selectors, not Docker IDs or DNS records. Application ordering is presentation/determinism only, not a dependency graph. Init ordering is sequential and meaningful.

Cargo `NetworkMode` is a transparent string/newtype because arbitrary custom network names are valid. It serializes as the scalar text shown here. Omitted or `nanoclbr0` uses the managed default. `host` and `none` are reserved lowercase modes. Preserve currently accepted Docker built-ins `bridge` and `default` as sandbox attachment modes. Reject Cargo-level `container:<id>`. Any other valid name is inspected and automatically created as today.

Container `NetworkMode` is a closed Nanocl enum serialized lowercase: omitted, `host`, or `none`. Raw `Container.HostConfig.NetworkMode` is forbidden because the compiler owns it.

### 1. Single-container Cargo

```yaml
ApiVersion: v0.19
Namespace: demo
Cargoes:
- Name: web
  Replicas: 1
  Containers:
  - Name: app
    Container:
      Image: nginx:1.27
```

### 2. Multi-container Cargo sharing one IP

```yaml
ApiVersion: v0.19
Namespace: demo
Cargoes:
- Name: web-with-metrics
  Replicas: 2
  Containers:
  - Name: app
    Container:
      Image: ghcr.io/example/web:1.4.0
      Env:
      - METRICS_ADDR=127.0.0.1:9100
  - Name: metrics
    Essential: false
    Container:
      Image: ghcr.io/example/metrics-sidecar:1.2.0
      Cmd:
      - --listen=0.0.0.0:9100
```

Both normal containers join the same per-replica sandbox network namespace and communicate over `127.0.0.1`; two replicas still have two distinct IPs.

### 3. Custom automatically created network

```yaml
ApiVersion: v0.19
Namespace: demo
Cargoes:
- Name: api
  Replicas: 2
  NetworkMode: private-api
  Containers:
  - Name: app
    Container:
      Image: ghcr.io/example/api:3.1.0
```

`private-api` is ensured on each selected node before its replica sandbox is created.

### 4. Cargo-level `host`

```yaml
ApiVersion: v0.19
Namespace: system
Cargoes:
- Name: host-agent
  Replicas: 1
  NetworkMode: host
  Containers:
  - Name: agent
    Container:
      Image: ghcr.io/example/host-agent:2.0.0
      Env:
      - LISTEN_ADDR=0.0.0.0:9200
```

Omitted container modes compile directly to `host`; there is no Cargo sandbox and no Cargo port publishing. Because Nanocl cannot infer application listeners, users should select distinct-node placement when replicas could collide; start failures remain authoritative.

### 5. Cargo-level `none`

```yaml
ApiVersion: v0.19
Namespace: batch
Cargoes:
- Name: isolated-workers
  Replicas: 1
  NetworkMode: none
  Containers:
  - Name: producer
    Container:
      Image: ghcr.io/example/producer:1.0.0
      Env:
      - TARGET=127.0.0.1:7000
  - Name: consumer
    Container:
      Image: ghcr.io/example/consumer:1.0.0
      Env:
      - LISTEN=0.0.0.0:7000
```

The replica has one sandbox in Docker `none` mode. Its normal containers share only that isolated loopback/network namespace; no external attachment or port publishing exists.

### 6. One container overriding to `host`

```yaml
ApiVersion: v0.19
Namespace: demo
Cargoes:
- Name: app-with-host-agent
  Replicas: 1
  Containers:
  - Name: app
    Container:
      Image: ghcr.io/example/app:1.0.0
  - Name: host-agent
    Essential: false
    NetworkMode: host
    Container:
      Image: ghcr.io/example/host-agent:2.0.0
```

`host-agent` does not share the Cargo IP or localhost and is not reached through Cargo port bindings. It shares the node host network and must be scheduled with host-port conflicts in mind.

### 7. One container overriding to `none`

```yaml
ApiVersion: v0.19
Namespace: demo
Cargoes:
- Name: app-with-offline-helper
  Replicas: 1
  Containers:
  - Name: app
    Container:
      Image: ghcr.io/example/app:1.0.0
  - Name: offline-helper
    Essential: false
    NetworkMode: none
    Container:
      Image: ghcr.io/example/offline-helper:1.0.0
```

`offline-helper` gets its own isolated loopback namespace. It cannot use Cargo localhost, Cargo IP, Cargo DNS attachment, or Cargo port bindings. `none` is worth supporting at container level because it is a simple, auditable isolation escape rather than an arbitrary network attachment.

### 8. Cargo-shared and container-specific secrets

```yaml
ApiVersion: v0.19
Namespace: demo
Secrets:
- Name: shared-env
  Kind: nanocl.io/env
  Data:
  - LOG_LEVEL=info
- Name: database-env
  Kind: nanocl.io/env
  Data:
  - DATABASE_URL=postgres://db/app
- Name: metrics-env
  Kind: nanocl.io/env
  Data:
  - METRICS_TOKEN=example-token
Cargoes:
- Name: api
  Secrets:
  - shared-env
  Containers:
  - Name: app
    Secrets:
    - database-env
    Container:
      Image: ghcr.io/example/api:3.1.0
  - Name: metrics
    Essential: false
    Secrets:
    - metrics-env
    Container:
      Image: ghcr.io/example/metrics-sidecar:1.2.0
```

Every app and init container receives `shared-env`; only the named container receives its additional secret. The sandbox receives none.

### 9. Cargo-level port bindings

```yaml
ApiVersion: v0.19
Namespace: demo
Cargoes:
- Name: web
  Replicas: 1
  PortBindings:
    "8080/tcp":
    - HostIp: "0.0.0.0"
      HostPort: "8080"
  Containers:
  - Name: app
    Container:
      Image: ghcr.io/example/web:1.4.0
      Env:
      - LISTEN_ADDR=0.0.0.0:8080
```

The mapping is installed on the sandbox, not the app. Any process in the shared namespace can bind port 8080, so conflicting listeners remain a runtime error.

### 10. Per-container custom binds

```yaml
ApiVersion: v0.19
Namespace: demo
Cargoes:
- Name: files
  Replicas: 1
  Containers:
  - Name: app
    Container:
      Image: ghcr.io/example/files:1.0.0
      HostConfig:
        Binds:
        - /srv/files/config:/app/config:ro
        - files-data:/app/data
  - Name: backup
    Essential: false
    Container:
      Image: ghcr.io/example/backup:1.0.0
      HostConfig:
        Binds:
        - files-data:/data:ro
```

No Cargo volume abstraction is required to share the named Docker volume; each container declares its own bind/mount.

### 11. Sequential init containers

Current Cargo already supports one synchronous init container (`bin/nanocld/src/utils/container/cargo.rs:108-216`), so preserving and generalizing it is justified.

```yaml
ApiVersion: v0.19
Namespace: demo
Cargoes:
- Name: prepared-api
  Replicas: 1
  InitContainers:
  - Name: prepare
    Container:
      Image: busybox:1.36
      Cmd:
      - sh
      - -c
      - mkdir -p /data && test -w /data
      HostConfig:
        Binds:
        - app-data:/data
  - Name: seed-local-data
    ImagePullPolicy: IfNotPresent
    Container:
      Image: ghcr.io/example/api-data-init:3.1.0
      HostConfig:
        Binds:
        - app-data:/data
  Containers:
  - Name: app
    Container:
      Image: ghcr.io/example/api:3.1.0
      HostConfig:
        Binds:
        - app-data:/data
```

Init containers run in list order once per replica and desired runtime generation, after sandbox creation and before applications. This intentionally makes them per replica rather than preserving the current ambiguous once-per-Cargo behavior: they must be safe to run once for every replica, including later scale-up. Singleton database migrations belong in a Job. Init containers must exit zero, do not participate in ongoing readiness, and are not rerun for a routine restart of one unchanged app. `Essential: false` is invalid on an init declaration. After the rollout reaches Succeeded/Failed, retain its stopped init Docker containers for a configurable diagnostic TTL (recommended default: one hour), then a reconciler removes them and nulls the mapping's process FK while preserving exit code/reason/timestamps. Cargo deletion removes them immediately. This bounds Docker/process accumulation and makes post-TTL log unavailability explicit.

## 7. Proposed Rust type model

Keep the declaration types together initially in `crates/nanocl_stubs/src/cargo_spec.rs`; a new module would create unnecessary cross-module churn before the wire cutover. Recommended conceptual types are:

```rust
pub struct CargoSpec {
  pub name: String,
  pub replicas: u32, // field-level default of 1, validate >= 1
  pub metadata: Option<serde_json::Value>,
  pub network_mode: Option<CargoNetworkMode>,
  pub port_bindings: Option<bollard_next::models::PortMap>,
  pub secrets: Vec<String>,
  pub init_containers: Vec<ContainerSpec>,
  pub containers: Vec<ContainerSpec>,
  pub placement: Option<CargoPlacementSpec>, // scheduling only; remove Replicas
  pub resource_requirement: Option<CargoResourceRequirementSpec>,
}

pub struct ContainerSpec {
  pub name: String,
  pub essential: bool, // field-level default true
  pub secrets: Vec<String>,
  pub image_pull_secret: Option<String>,
  pub image_pull_policy: ImagePullPolicy, // field-level default IfNotPresent
  pub network_mode: Option<ContainerNetworkMode>, // only Host or None
  pub container: bollard_next::container::Config,
}
```

Both structs use `serde(deny_unknown_fields, rename_all = "PascalCase")`, but not struct-wide `serde(default)`. Required fields remain required. Empty vectors may have field-level defaults only where omission is valid. Rename the present response/history types to avoid the current inversion:

- desired create/PUT input: `CargoSpec`;
- persisted/history record: `CargoSpecRevision`;
- PATCH input, if retained: `CargoSpecPatch`.

### Bollard representation decision

| Option | Decision |
|---|---|
| Flatten `Config` into `ContainerSpec` | Reject. It exports all Docker keys into Nanocl's namespace, creates present/future name collisions, makes every Bollard upgrade an implicit top-level API change, and obscures compiler-owned fields. |
| Explicit `Container: Config` | Recommend. It preserves direct typed Bollard access and a visible, stable namespace boundary. |
| Transparent wrapper alone | Reject as insufficient. `serde(transparent)` does not make permissive Bollard deserialization strict. |
| Explicit field plus strict adapter | Recommend. Keep the Rust field as the real `Config`, but deserialize it through a reusable ignored-field detector and fail with the full nested field path. Apply the same treatment to Cargo `PortMap`. |
| `serde_json::Value` or duplicated Nanocl Docker model | Reject. The former loses typed access/schema value; the latter inevitably drifts and is less capable than Bollard. |

Pinned Serde can combine flattening with an outer deny at runtime, so the rejection is not based on a false claim that it never works. The problems are collision semantics and schema divergence: Utoipa 5.5 represents flattened structs via `allOf` and does not close the composed object, while Schemars 0.8.22 merges it differently. `nanocl_stubs` enables both generators (`crates/nanocl_stubs/Cargo.toml:15-33`); Utoipa is active in `nanocld`'s `dev` feature and writes `bin/nanocld/specs/swagger.yaml` during service configuration (`bin/nanocld/src/services/mod.rs:27-45`).

The strict adapter should detect fields ignored anywhere in `Config`, nested `HostConfig`, and `PortBinding` and return a path such as `Container.HostConfig.HostPrt`. It automatically recognizes newly supported keys after a deliberate Bollard upgrade because the source type changes; it does not require a duplicate allowlist. Utoipa/Schemars schema generation should use the Bollard schemas but close generated struct objects (not map-valued objects) in a tested schema postprocessor. Runtime acceptance remains authoritative. Add golden tests proving that runtime, Schemars, and OpenAPI agree on required outer fields and representative unknown nested fields.

Semantic validation, after deserialization and before persistence, must reject:

- empty/duplicate logical names, an empty application list, no essential application, and a missing/empty image in any app/init;
- zero replicas;
- raw `HostConfig.NetworkMode`, `HostConfig.PortBindings`, `HostConfig.PublishAllPorts`, `NetworkingConfig`, and `NetworkDisabled`;
- user `container:<id>` references anywhere;
- Docker fields incompatible with compiled `container:<sandbox-id>` mode, including user hostname/DNS/add-host/MAC settings for a normal shared-network container;
- reserved `io.nanocl.*` labels, `com.docker.compose.project` if that compatibility label is retained, and reserved `NANOCL_*` environment keys;
- `AutoRemove`, as today, but at ingress rather than late runtime creation;
- Cargo ports with Cargo `host`/`none`, invalid protocol keys, and statically detectable host-port conflicts.

Forward compatibility is explicit: new Bollard fields become accepted only after the dependency and generated schemas are deliberately upgraded and tested. Silently accepting a misspelling is not forward compatibility.

## 8. Cargo versus Container field ownership

| Concern | Declared owner | Compiled/observed treatment |
|---|---|---|
| Name, metadata | Cargo | Cargo identity and user metadata; never copied from inspect. |
| Desired lifecycle/runtime generation | Cargo deployment state | Persist Created/Running/Stopped/Deleting intent and a generation separate from spec history; conditions remain derived. |
| Replica count | Cargo `Replicas` | Scheduler creates stable replica rows before Docker work. Remove `Placement.Replicas`; placement remains scheduling policy. |
| Placement/resources | Cargo | Used to choose a node for the whole replica, not individual containers. |
| Default/custom network | Cargo `NetworkMode` | Compiler attaches the sandbox or applies host mode. |
| Published ports | Cargo `PortBindings: bollard_next::models::PortMap` | Compiler installs mappings only on the sandbox; observed allocations appear per replica. |
| Shared secrets | Cargo `Secrets` | Inherited by every app/init, never sandbox; compiled env/files only. |
| Container collection/order | Cargo | Application names are identity; init list order is lifecycle order. No general dependency graph. |
| Stable logical name | `ContainerSpec.Name` | Written to labels/mapping rows and used by inspect/log/exec/restart selectors. |
| Essentiality | `ContainerSpec.Essential` | Default true; participates in readiness reducer, not Docker config. |
| Container secrets | `ContainerSpec.Secrets` | Added after shared secrets under deterministic semantics. |
| Pull policy/credential | `ContainerSpec` | Applied separately for each distinct image. Never supplied to sandbox. |
| Network escape | `ContainerSpec.NetworkMode` | Omitted compiles to shared namespace; only `host` or `none` escape. |
| Image, command, entrypoint, raw env/labels | Bollard `Config` | Preserve typed access. Compiler validates reserved collisions and injects its fields into a clone. |
| Healthcheck | Bollard `Config.Healthcheck` or image | Normalize observed `none/starting/healthy/unhealthy`; do not move to Cargo. |
| Restart policy/security/resources | Bollard `Config`/`HostConfig` | Preserve per-container settings; compiler may apply a documented default restart policy. |
| Volumes/mounts/binds | Bollard `Config.Volumes`, `HostConfig.Mounts/Binds` | Per-container. Shared named volumes are expressed by repeating the same named mount/bind. |
| Effective network mode/port owner | Compiled runtime config | `container:<sandbox-id>` on normal apps/init; `PortMap` on sandbox. Never persisted as declared config. |
| Reserved labels/env/secret bind | Compiled runtime config | Generated from identity and secrets in deterministic order. Reject user ownership of reserved keys. |
| Effective-template fingerprint | Compiler/deployment state | Canonical declaration + compiler version + non-secret dependency revisions; never use secret values or ephemeral sandbox IDs. |
| Docker ID/name/inspect/runtime/health/reason | Concrete process observation | Store normalized Cargo mapping/observation plus redacted inspect in generic `processes`; never use as declaration or expose secret-derived values. |

Current compilation always forces attach/TTY fields (`bin/nanocld/src/utils/container/cargo.rs:304-319`). The refactor should inventory whether CLI log behavior still needs those defaults; it should not silently overwrite an explicit Bollard value. That decision is independent of multi-container identity.

## 9. Networking and namespace model

### Cargo modes

| Cargo mode | Sandbox | Default container compilation | Consequences |
|---|---|---|---|
| omitted / `nanoclbr0` | Yes, attached to `nanoclbr0` | `container:<sandbox-id>` | One stable IP per replica; custom/default proxy discovery uses sandbox IP. |
| custom name | Yes, attached after ensure/create | `container:<sandbox-id>` | Same name is ensured node-locally as an attachable bridge, preserving current behavior. |
| `bridge` / `default` | Yes, attached using Docker built-in semantics | `container:<sandbox-id>` | Preserves currently accepted built-in modes without treating them as managed custom networks. |
| `none` | Yes, created in `none` | `container:<sandbox-id>` | Replica containers share an isolated loopback/network stack; no external IP or publishing. |
| `host` | No | `host` | Every default app uses the node host stack. Replicas on one node have the same host network identity; there is no Cargo IP to preserve and port publishing is invalid. |

Current classification explicitly excludes empty/default network, `bridge`, `host`, `none`, and `container:*` from custom-network creation (`bin/nanocld/src/utils/container/network.rs:32-54`). The target must explicitly reject user Cargo `container:*`, which would bypass ownership.

An automatically created named network is node-managed and may be shared by multiple Cargos; attachment does not imply exclusive Cargo ownership. Removing a Cargo detaches/removes its sandboxes but does not delete that network. Any future unused-network garbage collection must use the node-scoped network registry and live Docker attachments, not a single Cargo delete path.

### Container overrides

| Container mode | Valid | Consequence |
|---|---:|---|
| omitted | Yes | Inherits Cargo behavior: joins the sandbox for all non-host Cargo modes; uses host directly for Cargo `host`. |
| `host` | Yes | Uses node host network independently. No shared Cargo localhost/IP/ports. Nanocl cannot infer arbitrary listening ports, so the user must choose safe placement and bind failures remain observable runtime errors. |
| `none` | Yes | Gets its own loopback-only namespace. No sibling localhost, Cargo IP, DNS attachment, or Cargo ports. |
| custom, `bridge`, `default` | No | Would create a second arbitrary network identity and make Cargo IP/port semantics ambiguous. |
| `container:<id>` | No | Leaks ephemeral Docker identity into the declaration and bypasses sandbox ownership/recovery. |

Nanocl should request only network namespace sharing. IPC, PID, UTS, cgroup, and user namespaces remain Docker defaults/independent; no sibling-ID namespace references are compiled. Do not add a generic sharing matrix without a demonstrated use case. Docker's container-network mode also makes several per-container network settings incompatible, so validation must fail those declarations before mutation rather than rely on a late Docker error. The relevant engine semantics are documented in Docker's [networking overview](https://docs.docker.com/engine/network/) and [host network driver](https://docs.docker.com/engine/network/drivers/host/).

Sharing an IP does not give each logical container a separate DNS name. Containers in a replica address each other through `localhost` and agreed ports. Cargo/custom-network DNS and proxy discovery identify a replica endpoint, not a particular container. `ncproxy` currently derives IPs from app inspect and deliberately returns none for `container:*` (`bin/ncproxy/src/utils/rule.rs:171-221,345-366`). Replace this with an endpoint abstraction: sandbox IP for default/bridge/custom replicas, the assigned node address for a Cargo-level `host` replica when the proxy can route it, and no routable endpoint for Cargo `none`. Container-level host/none escapes do not replace the Cargo endpoint. Proxy only publishes currently assigned, serving-ready endpoints.

## 10. Port-binding model using the actual Bollard types

The workspace resolves `bollard-next 0.18.1` and `bollard-next-stubs 1.45.1-rc.26.0.1` (`Cargo.lock:250-295`). The exact reusable model is:

```rust
use bollard_next::models::{PortBinding, PortMap};

// Equivalent upstream definitions (abridged):
// type PortMap = HashMap<String, Option<Vec<PortBinding>>>;
// struct PortBinding {
//   host_ip: Option<String>,   // serialized as HostIp
//   host_port: Option<String>, // serialized as HostPort
// }
```

`bollard_next::models::HostConfig.port_bindings` is `Option<PortMap>`. Keys use `<container-port>/<tcp|udp|sctp>`. The similarly named `nanocl_stubs::generic::PortMap` is only a hand-written Utoipa marker, not a data type (`crates/nanocl_stubs/src/generic.rs:643-689`); do not use or extend it as the wire model. Reexport the real `PortMap` and `PortBinding` from `cargo_spec` for discoverability.

Normative rules:

- Store one `Option<PortMap>` at Cargo level.
- Compile it to the non-host replica sandbox's `HostConfig.port_bindings` and matching `Config.exposed_ports` keys. Do not copy it to app/init configs.
- Reject raw app/init `HostConfig.PortBindings` and `PublishAllPorts`, even for `host`/`none` overrides. `Config.ExposedPorts` is metadata rather than a host binding: preserve direct Bollard access unless a pinned Docker API conformance test proves it incompatible with `container:<sandbox-id>`; never promote it implicitly into Cargo host mappings.
- Reject a nonempty Cargo map with Cargo `host` or `none`.
- Validate key port range and protocol and validate numeric/range syntax for explicit `HostPort` before persistence. Keep `HostIp`/`HostPort` optional exactly as Bollard defines them.
- `None`/empty binding values expose metadata but publish no host port. Empty/`0` dynamic host ports are allowed only if the resulting per-replica allocation is reported by inspect/status.
- A fixed host port/protocol prevents two replicas on the same node under the recommended conservative reservation rule. Reserve it transactionally in `cargo_port_reservations` before Docker work and fail if eligible nodes are insufficient. Treat empty/`0.0.0.0`/`::` host IPs as wildcard conflicts; initially reserve the port/protocol for the whole node even when a specific IP is supplied rather than risk overlap. Docker remains the final external/non-Nanocl conflict authority.
- A fixed mapping also prevents old/new sandbox overlap during a sandbox replacement; use stop-then-create for that replica. Current update already stops old instances when it detects port mappings (`bin/nanocld/src/utils/container/cargo.rs:501-520`).

Port ownership stays stable while an individual app is replaced because the sandbox and its NAT rules remain. The app that listens on the published container port is a workload convention; two apps attempting the same shared-namespace port will receive a normal bind/start failure.

## 11. Secrets, image pulling, volumes, and binds

### Exact secret inheritance

Cargo `Secrets` applies to every declared application and init container and never to the internal sandbox. For a container, form the effective reference list as Cargo references followed by container references; remove repeated identical secret names while preserving the first occurrence. Missing references fail reconciliation before replacing an existing workload.

Environment materialization must be deterministic:

1. Cargo secret environment, in Cargo reference order and then secret `Data` order;
2. container secret environment, in container reference/Data order;
3. explicit raw `Container.Env`;
4. controller-owned `NANOCL_*` values in a fixed order.

Reject duplicate keys within any single source/layer. Across layers, later layers replace earlier values: a container secret overrides a Cargo-shared secret and explicit raw environment overrides secret-derived environment. Preserve the key's first-seen output position while replacing its value, so serialized output is stable. Any user or secret definition of reserved `NANOCL_*` is an error; controller values are not overrideable.

Current secret DB `IN` queries do not preserve requested order (`bin/nanocld/src/utils/secret.rs:18-38`), so results must be reordered against the declared reference list. TLS secrets currently share one Cargo directory and are mounted into every instance (`secret.rs:41-87`; `bin/nanocld/src/utils/container/cargo.rs:242-303`), which would leak a container-specific TLS secret to siblings. Materialize an atomic, permission-restricted directory per Cargo/replica/runtime-generation/role/container attempt containing only that container's effective Cargo-plus-container TLS set; bind it read-only, reject mount/filename collisions, and remove it only after the concrete process is gone. Do not expose secrets to the sandbox or place secret values in identity labels/status.

Secret changes must advance a durable secret revision and trigger a new Cargo runtime generation for dependents. Use secret key/revision identifiers—not secret values or unsalted value hashes—in effective-template fingerprints and status. The current secret table has `updated_at` but no explicit revision (`bin/nanocld/migrations/2023-10-04-045333_secrets/up.sql:2-16`); verify/update its mutation path or add a monotonic revision before relying on it.

### Image pulling

Move `ImagePullPolicy` and `ImagePullSecret` to each app/init `ContainerSpec`; different images may use different registries and policies. Preserve `Never`, `Always`, and default `IfNotPresent` from `crates/nanocl_stubs/src/generic.rs:327-340`. Pull distinct images before destructive rollout work where possible. Current download and credential resolution is Cargo-wide (`bin/nanocld/src/utils/container/image.rs:17-31,71-104`; caller `cargo.rs:234-240`). The sandbox uses a pinned Nanocl-owned image and no user registry credential.

### Storage and binds

Do not add a Cargo volume abstraction in the first design. Keep these direct Bollard capabilities per container:

- `Config.Volumes` for declared container volume paths;
- `HostConfig.Mounts` for typed mounts;
- `HostConfig.Binds` for raw bind/named-volume syntax.

Users can share a named Docker volume by repeating it in selected declarations, as the example shows. That name is Docker-node scoped and may also be shared by same-node replicas; Nanocl does not make it replica-private or replicate its contents. A future Cargo-owned volume abstraction is justified only if Nanocl must own create/delete/retention/backup lifecycle; symmetry alone is insufficient.

Relative host binds currently depend on CLI working-directory rewriting. Resolve them against the Statefile source directory as an explicit normalization input and retain the authored declaration if exact backup round trips matter. The resulting host path must exist with compatible contents/permissions on every eligible remote node; client-local resolution does not make it portable. Do not let node-local compilation mutate the persisted spec.

## 12. Runtime sandbox alternatives

| Alternative | Advantages | Failures/costs | Decision |
|---|---|---|---|
| Dedicated sandbox/pause process | Stable replica IP and port owner; ordinary app replacement does not invalidate siblings; symmetric apps; clear network attachment | One extra minimal container per non-host replica; image distribution/liveness must be managed | Recommend for omitted/default/custom/bridge/none Cargo modes. |
| First application is namespace owner | No extra process | Owner is asymmetric; replacing/removing it invalidates future `container:<id>` joins, risks IP/port churn, and couples declaration order to infrastructure | Reject. |
| Give every app its own network namespace | Simple Docker creation | Does not satisfy one-IP/localhost/one-port-map invariant | Reject for normal containers; keep only explicit `host`/`none` escapes. |
| External Linux netns management | No pause image | Substantially more privileged, platform-specific, and outside current Docker/Bollard abstractions | Reject. |

### Recommended lifecycle

1. Transactionally allocate a stable replica UUID/display ordinal, target node, and assignment epoch/fencing token. Node RPCs, labels, mutations, and observations carry that epoch and are accepted only while it matches the DB assignment.
2. Ensure network and images/secrets; create and start the labelled sandbox; persist its process mapping.
3. Run labelled init containers sequentially in the sandbox namespace.
4. Create all app processes with stable identity labels and compiled `container:<sandbox-id>` mode, then start them.
5. Persist local observations; centrally reduce reconciliation/readiness.

Stop apps before the sandbox. Delete app/init processes and mappings first, sandbox second, and the replica only after node acknowledgement. Scale-down drains the selected highest ordinals and does the same order. A normal app restart/replacement keeps the sandbox running. Restarting the same sandbox container keeps its Docker ID/network ownership; a missing/recreated sandbox requires rebuilding every joined app in that replica.

The sandbox owns network attachment, IP, `PortMap`, and no user workload fields. It receives no secrets, mounts, app healthcheck, or user logs. Exclude it from default Cargo log streams and application process counts, but expose its observation under an `Internal`/sandbox field in replica inspection and diagnostics.

Cargo `host` is the exception: create no sandbox, compile omitted app modes to `host`, and represent the stable replica in the DB without a sandbox process. Its network identity is the assigned node, shared with other host-mode processes—not a unique stable replica IP. A container-level `none` remains independent. This avoids relying on a `container:<id>` join to a host-network owner when no IP preservation benefit exists.

The sandbox image is an unresolved release artifact: it should be pinned by digest, minimal, multi-architecture, Nanocl-owned, and available under the project's offline/install strategy. Do not use an arbitrary public `latest` image.

## 13. Health, readiness, and status model

Use durable, runtime-generation-aware observations plus a pure aggregate reducer. An immutable `spec_revision_key` identifies declaration/history. A separate monotonically increasing Cargo `runtime_generation` identifies a group deployment operation and also advances for group restart/reload/secret-revision changes that do not create a spec revision. A targeted single-container restart advances only that logical mapping's restart generation/nonce, not the Cargo-wide generation. A metadata-only spec revision need not advance either. Per-sandbox and per-container effective-template fingerprints determine whether an existing process remains valid across a new runtime generation; fingerprints include non-secret secret revision identifiers, not secret values.

Docker events are a latency optimization, not the sole source: each node periodically re-inspects only its locally assigned processes, persists normalized observations carrying the assignment epoch, and repairs missing/out-of-order events. Stale-epoch observations are ignored.

For a concrete app process:

```text
ready = runtime == running
        && (health == none || health == healthy)
```

Normalize an absent or explicitly disabled (`Test: ["NONE"]`) check to `health=none`; reserve `unknown` for lack of observation. Persist runtime state, normalized health, ready flag, reason/message, immutable creation generation, mutable acknowledged runtime/restart generations, effective-template fingerprint, assignment epoch, monotonic observation sequence, and observation time alongside the Cargo-process mapping. Store only the redacted inspect snapshot described in section 3 in `processes.data`.

For ordering, serialize event and polling triggers through one node-local observer per process; every trigger performs a fresh Docker inspect rather than trusting stale event payload, then atomically writes only if assignment epoch is current and its DB-backed `observation_seq` is newer. Resume from the stored sequence after daemon restart. This prevents a delayed same-attempt event from overwriting a newer poll.

`Essential` defaults true:

- Every essential app whose effective template is desired must exist and be ready for the replica to be Ready at the desired runtime generation.
- An essential unhealthy/exited app makes that replica unavailable.
- A nonessential pull/create/start/health failure is recorded as completed reconciliation with degradation; it does not make the generation Failed or clear Ready if all essential apps are ready.
- A sandbox, when present, is infrastructure-essential.
- Init containers gate `Reconciled`; after successful completion they do not participate in ongoing readiness.

Require at least one essential app at validation. Persist per-replica reconciliation/readiness because a replica exists before and between concrete process attempts. Derive Cargo aggregate counts/conditions from desired replicas plus those rows; do not add a second persisted aggregate truth. A cached summary may be added later only as a rebuildable projection keyed by runtime generation.

### Orthogonal state axes

Persist user intent; derive observation:

- **Desired lifecycle:** `Created` (accepted, never started), `Running`, `Stopped`, or `Deleting`. POST preserves current create-then-start semantics by initially creating `Created`; state apply/start durably changes it to `Running`. Intentional stop sets `Stopped`; it never implies Unhealthy or Failed. Deletion remains a tombstone until cleanup acknowledgement.
- **Reconcile phase for the desired runtime generation:** `Accepted`, `Planning`, `Applying`, `Succeeded`, or `Failed`. Essential sandbox/init/app failure becomes Failed only after durable acceptance and the retry/rollout deadline. Request validation failure is synchronous 4xx and creates no Cargo condition.
- **Independent observations/conditions:** `Ready`, `Available`, `Degraded`, normalized `Health`, and `Failed`, each with reason/message, runtime generation, and transition time.
- **Counts:** desired replicas, updated replicas (desired template/generation acknowledged), ready replicas at the desired generation, available serving replicas across retained generations, unhealthy replicas, and failed desired-generation replicas.

`Available` is necessary during rollout and rollback. An old-generation replica that is still ready counts as available and may remain a proxy endpoint, but it does not count as ready/updated for the desired generation. Thus Progressing can coexist with Available and Degraded; after a failed rollout the Cargo may be Failed and Degraded while still serving the old generation.

### Condition definitions

The requested terms map to those axes as follows; they are not one mutually exclusive `ObjPsStatusKind`:

| Term | Exact meaning |
|---|---|
| Created | Strict and semantic validation passed and the declaration plus `desired_lifecycle=Created` was durably stored. It makes no runtime claim. A rejected request is not Created. |
| Progressing | Desired lifecycle is Running and the desired runtime generation is Planning/Applying, replica count/templates are incomplete, init is running, or an essential candidate is health-gating before its rollout deadline. It may coexist with Available/Degraded. |
| Ready | Desired lifecycle is Running, reconcile phase is Succeeded, desired replica count exists, and every desired replica's essential sandbox/apps match their desired effective templates and are ready. A nonessential failure may coexist as Degraded. |
| Degraded | Desired lifecycle is Running and service has some availability but available/updated/ready counts are below desired, an older generation is serving, or a nonessential process failed. |
| Unhealthy | Desired lifecycle is Running, zero replicas are available, and essential workloads are observed unhealthy/exited/restarting. If some replicas still serve, the aggregate term is Degraded while per-replica/process health and unhealthy counts remain explicit. A recoverable health transition does not itself set reconcile Failed. |
| Failed | After durable acceptance, an essential sandbox/init/pull/network/create/start/controller operation or rollout deadline made the desired runtime generation terminally fail. Synchronous validation rejection and a later health transition of a previously Ready generation are not this condition. |

`Reconciled` means the desired runtime generation reached `Succeeded`: essential objects/init completed and candidates passed the rollout gate; attempted nonessential failure is allowed and reported Degraded. Because safe serial promotion can wait on essential health, Reconciled is explicitly not the default apply boundary. `ncproxy` routes the per-replica serving-ready endpoint abstraction, including retained old-generation endpoints during rollout/rollback, rather than checking the old single Cargo health field.

For one primary list value, use this precedence: `Deleting`; lifecycle `Created`/`Stopped`; reconcile `Failed` (include serving count); `Progressing` (include available/desired and health); zero-available `Unhealthy`; `Degraded`; `Ready`. This is presentation only—the API returns all axes/conditions.

### Apply/wait behavior

Keep create/PUT/PATCH/start HTTP operations asynchronous and return `202` with the accepted spec revision key and desired runtime generation. For CLI operations:

- default: `--wait=accepted`, returning after validation and durable desired-state/runtime-generation storage, then print current reconcile/readiness counts;
- explicit controller completion: `--wait=reconciled --timeout ...`;
- explicit desired-generation readiness: `--wait=ready --timeout ...`.

Use a recommended initial five-minute default for explicit waits and make it configurable. For a multi-resource `state apply`, `--timeout` is one command-wide deadline, not a fresh timeout per resource. Wait by persisted runtime/restart generation using polling or durable/replayable events, never only an unreplayed daemon-local stream. Reconcile Failed terminates either explicit wait immediately. A recoverable Unhealthy state keeps `--wait=ready` waiting until recovery or timeout. Timeout is nonzero CLI exit with a diagnostic naming the blocking replica/container, available old-generation count, and last reason; the accepted Cargo remains deployed. A clean event EOF without the requested condition is not success.

Apply the same accepted default to start, stop, restart, and remove. Start/group-restart may explicitly wait for Reconciled or Ready; targeted restart waits on its mapping restart generation and then that process's readiness. Stop may use finite `--wait=stopped`; remove may use finite `--wait=deleted` for tombstone cleanup. Without those explicit waits they return after desired lifecycle/operation acceptance. Run, PUT/PATCH, and revert use the same Accepted/Reconciled/Ready meanings.

## 14. Database and migration options

### Options

| Option | Assessment |
|---|---|
| Reuse one Cargo `object_process_statuses` row | Reject. It cannot express multiple replicas/processes/generations and is shared by Jobs/VMs. |
| Put nullable Cargo identity/status columns directly on `processes` | Technically possible, but spreads Cargo-only concepts through Job/VM storage and cannot represent a desired/missing process before a Docker ID exists. Not preferred. |
| Add only per-process status | Insufficient. A replica exists during scheduling, init, sandbox loss, and zero-process failures. |
| Add Cargo desired-deployment fields, replica plus Cargo-process mapping/observation | Recommend. It persists user intent/topology while preserving the generic process table. |
| Persist another Cargo aggregate status | Reject as source of truth. It will drift from replicas; derive aggregate conditions/counts. |

### Recommended Cargo deployment fields and additive tables

Add to `cargoes` (or an exactly one-to-one `cargo_deployments` row if altering it is undesirable):

- desired lifecycle (`Created`, `Running`, `Stopped`, `Deleting`);
- current immutable `spec_revision_key` (the existing spec FK remains history identity);
- desired `runtime_generation BIGINT`, advanced transactionally for runtime-relevant spec changes, scale, start/group-restart/reload, and secret revision changes but not metadata-only edits or targeted app restart;
- a durable reconcile trigger/operation nonce for explicit restart/reload so matching template fingerprints do not incorrectly turn a requested action into a no-op; targeted container restart records the requested/observed restart generation on its logical mapping;
- `deletion_requested_at` tombstone and reconcile reason/timestamps.

This is durable user intent, not a derived aggregate. Keep the current status FK during transition; later remove only Cargo's dependency after Job/VM consumers are proven unaffected.

`cargo_replicas`:

- `key UUID PRIMARY KEY` (authoritative replica identity);
- `cargo_key VARCHAR NOT NULL REFERENCES cargoes(key) ON DELETE CASCADE`;
- `ordinal INTEGER NOT NULL` for stable display while the replica exists;
- nullable/current `node_name` FK to `nodes` with `ON DELETE SET NULL`;
- nonnegative `assignment_epoch`/fencing token, incremented on every reassignment;
- desired and observed runtime generations plus desired/serving spec revision FKs (`RESTRICT` while active; nullable historical serving reference may use `SET NULL` only after observation retention is defined);
- desired sandbox/effective-template fingerprint;
- reconcile phase, ready/health condition, reason/message, timestamps;
- `UNIQUE(cargo_key, ordinal)` and checks for ordinal/generation/epoch ranges.

An ordinal may be reused only after the old replica and its cleanup tombstone are fully gone. Node is an assignment/observation, not part of identity.

`cargo_replica_processes`:

- own UUID primary key so an expected/missing attempt can exist before a Docker ID;
- `replica_key` FK with cascade;
- nullable unique `process_key` FK to generic `processes`, with deliberate `ON DELETE SET NULL`/history behavior;
- non-null `container_name` (use internal reserved `_sandbox` for the sandbox), role (`sandbox`, `init`, `app`), immutable creation spec revision/runtime generation, mutable acknowledged runtime/restart generations, effective-template fingerprint, assignment epoch, nonnegative attempt number, lifecycle slot (`candidate`, `active`, `retiring`), and `essential`;
- normalized runtime/health/ready/reason/message/observed-at plus monotonic `observation_seq`; never secret values;
- unique `(replica_key, role, container_name, created_runtime_generation, attempt)`;
- partial uniqueness for one active logical `(replica_key, role, container_name)` and one active sandbox.

Add checks restricting role/slot values, requiring `_sandbox` iff role is sandbox, requiring sandbox essentiality, and making `process_key` unique when non-null so one Docker process cannot map to multiple attempts. Verify partial-index behavior on the repository's actual PostgreSQL-compatible deployment target.

`cargo_port_reservations`:

- `key UUID PRIMARY KEY`; non-null replica FK with cascade; non-null assigned node FK; non-null logical binding key/ordinal, normalized host port and protocol, original host IP, lifecycle (`held` or `releasing`), creation/release runtime generations, timestamps;
- checks for numeric port range, `tcp|udp|sctp`, valid lifecycle, and nonnegative generations; one logical reservation is shared by old/candidate sandboxes of the same replica rather than duplicated per process;
- conservative `UNIQUE(node_name, host_port, protocol)` so wildcard/specific-address overlap cannot race;
- created/reassigned/deleted in the same DB transaction as replica placement, before Docker creation. Keep old and new distinct-port reservations through handoff; carry an unchanged reservation across generations and release the old one only after promotion. Dynamic host allocations are observed after Docker creation rather than pre-reserved.

The stable public tuple is Cargo key + replica UUID/ordinal + role + declared container name. Creation runtime generation/attempt distinguishes rollout processes; effective fingerprints let unchanged processes acknowledge a later generation without recreating them. Node and Docker ID are observations. Docker labels are immutable and therefore contain Cargo key, namespace, replica UUID/ordinal, logical name, role, **creation** spec revision/runtime generation, effective-template fingerprint, assignment epoch, and attempt. Mutable acknowledged generation/restart and observation sequence live only in DB mappings. Recovery may adopt an older-created process when its logical identity, current assignment epoch, attempt, and effective fingerprint match desired state; it must not require the immutable creation-generation label to equal the latest desired generation. An old partitioned node must verify the epoch before every mutation; proxy/status ignore its stale observations. Rescheduling should prefer fencing acknowledgement and report when it deliberately chooses availability over duplicate-execution risk.

### Forward migration

1. Add Cargo desired-deployment columns and the replica/process/reservation tables, constraints, indexes, models, and repositories without changing Job/VM rows or removing the current status FK.
2. Build an application-level transformer for every historical Cargo `specs.data` row—not just the current `cargoes.spec_key`—so history/revert remains usable: move `Placement.Replicas` to top-level `Replicas`, name the legacy app `main`, convert init to `init`, materialize the current fallback-to-main image when legacy init omitted `Image`, move the app's raw network/ports to Cargo ownership, and copy image-pull settings to each applicable app/init.
3. Classify legacy replication/networking explicitly. Missing replicas becomes one; current `Placement.Replicas: 0` is normalized to its effective clamped value one; values that do not fit target `u32` are upgrade errors. Current init mode resolves independently: omitted init means `nanoclbr0`, not inheritance. It may become omitted/shared only when the app's effective Cargo mode is also `nanoclbr0`; init `host`/`none` becomes the closed override. An omitted init beside custom/host/none/bridge/default app mode, a different custom/built-in init network, any init port binding, user `container:<id>`, `PublishAllPorts`, `NetworkingConfig`, `NetworkDisabled`, and other newly forbidden ownership cannot be translated silently and requires user correction.
4. Detect boot-recovery-contaminated specs before transforming. Remove only provably controller-owned labels, reserved env, and internal binds; never persist secret-derived values. Default restart/TTY/attach, hostname suffixes, overwritten network mode, and arbitrary selected replica/init content may be impossible to distinguish from authored input. Block/report ambiguous Cargo histories and require an authoritative Statefile/manual choice rather than guessing.
5. The old binary cannot deserialize the new JSON shape. Take a database backup, stop/version-fence all old readers, run the data transform atomically with schema/version marking, deploy new readers, and rehearse rollback. If online upgrade is required, use shadow versioned storage and cut over only after all readers understand it; do not mutate JSON in place under mixed versions.
6. Legacy running Cargos have no sandbox. Assign provisional replica identities only where unambiguous, mark them legacy, and replace them through a controlled rollout/restart. Do not pretend random names/node-local indices are durable identities.
7. Switch Cargo reads/controller/status to the new fields/tables. Keep `processes` and `object_process_statuses` for Jobs/VMs; remove only Cargo's dependency in a later forward migration after all consumers switch.

Migration tests must cover both a fresh database and upgrades from released schemas. Never edit an already-published migration in place.

## 15. API and CLI impact

Current Cargo routes are registered at `bin/nanocld/src/services/cargo/mod.rs:23-33`: create/list, inspect, PUT, PATCH, delete, count, history, and revert. Keep those route identities where their meaning remains coherent. Current process-wide routes act by `{kind}/{key}` and naturally remain Cargo-wide (`bin/nanocld/src/services/process/mod.rs:25-42`).

### API responses and mutations

- Create/PUT accept the new desired `CargoSpec` atomically and return the immutable spec revision key plus desired runtime generation (which are deliberately different identities).
- Make PUT the canonical initial mutation. If PATCH remains, `Containers` and `InitContainers` are whole-field replacements keyed/diffed by stable `Name`; never index-merge vectors or repeat the current hand-written partial Bollard merge. Explicit null/clear semantics must be documented and tested.
- `CargoSummary` reports desired/updated/current-generation-ready/available-serving/unhealthy/failed replica counts plus desired lifecycle and conditions, not process counts named as instances.
- `CargoInspect` becomes `Spec` plus `Replicas[]`; each replica contains stable identity, ordinal, node, generation/conditions, an internal sandbox observation, named init observations, and named app observations.
- Keep low-level concrete Process inspect routes for diagnostics, but do not expose Docker names/IDs as durable Cargo selectors.

Minimal logical routes needed to remove ambiguity are equivalent to the following. Freeze `{replica}` as the stable replica UUID in APIs; expose ordinal only as a display value and let the CLI resolve `--replica` UUID or unambiguous current ordinal.

```text
GET  /cargoes/{key}/replicas/{replica}
GET  /cargoes/{key}/replicas/{replica}/containers/{name}
GET  /cargoes/{key}/replicas/{replica}/containers/{name}/logs
POST /cargoes/{key}/replicas/{replica}/containers/{name}/restart
POST /cargoes/{key}/exec   # body includes replica and container selectors
```

The existing client method for `/cargoes/{key}/instances` has no matching daemon/OpenAPI route (`crates/nanocld_client/src/cargo.rs:227-248`) and should be replaced during the breaking client change. Current exec hardcodes `{cargo_key}.c`, which does not even match current random creation names (`bin/nanocld/src/utils/exec.rs:16-28` versus `utils/container/cargo.rs:263-267`); require replica/container when ambiguous and permit omission only for exactly one replica with one app.

### CLI

Retain Cargo-wide start, stop, restart, and remove. Extend existing commands with selectors only where needed:

- `cargo inspect KEY [--replica N] [--container NAME]`;
- `cargo logs KEY [--replica N] [--container NAME]`, defaulting to all app logs with `replica/container` prefixes and excluding sandbox/init;
- `cargo restart KEY` remains group-wide; `--replica N --container NAME` targets one app;
- `cargo exec KEY --replica N --container NAME ...`, with the single/single implicit shortcut;
- create/run/patch flags that assume one image/config are replaced by the new spec input or deliberately limited single-container shorthand.

`bin/nanocl/src/models/cargo.rs:17-183,297-345` currently assumes one image for create/run/patch/exec/list; `bin/nanocl/src/commands/cargo.rs:195-215` aggregates all process logs. Generated man pages under `doc/man/` and shell completions must follow the final CLI contract.

All node operations must become replica-aware and execute Docker calls only on the assigned node. Current group start/stop/restart/kill/log/stats paths can load cluster-wide rows and use a local Docker daemon (`bin/nanocld/src/utils/container/process.rs:176-297`; `services/process/log.rs:87-133`; `services/process/stats.rs:29-63`).

## 16. Update, rollout, and failure-recovery behavior

The spec revision key identifies desired history; a separate runtime generation identifies reconciliation. Compute stable effective-template fingerprints for the sandbox and each named app/init from normalized declaration, compiler version, and referenced secret revisions. Reconcile named containers by fingerprint, not list indices, Docker names, or spec UUID equality. Metadata-only revisions can acknowledge without runtime replacement; scaling can retain matching existing replicas; secret reload can advance runtime generation without inventing a spec revision.

### Diff and rollout rules

- **No effective runtime change:** update the spec revision/metadata and acknowledge it without advancing/replacing matching process templates.
- **Explicit restart:** a Cargo-wide restart persists a trigger and advances Cargo runtime generation; a targeted app restart advances only that logical mapping's restart nonce/generation. Execute Docker restart even when the effective-template fingerprint is unchanged, then record the observed operation. Fingerprint reuse applies to spec/scale reconciliation, not an explicit action.
- **One app declaration/image/secret changes:** for each replica keep its sandbox, pull prerequisites and create the stopped candidate while the old app serves, stop the old app immediately before the conflicting start, start/health-gate the candidate, promote it, then delete the old attempt. Advance one replica at a time.
- **App added/removed:** add/remove that stable name in every replica, one replica at a time. Rename is delete+add unless a future explicit rename operation exists.
- **Shared secret changes:** a durable secret revision event/reload advances runtime generation and changes every inheriting template fingerprint; no secret value is put in the hash/status.
- **Init changes/new runtime generation:** run the needed per-replica init sequence before starting affected app templates. Failed init is reconciliation Failed and leaves the previous serving generation in service where possible.
- **Cargo network or `PortBindings` changes, or missing sandbox:** replace the entire replica. Fixed host ports require stopping/removing the old sandbox before creating the new one; perform replicas serially. That replica cannot retain the old namespace for instant rollback: rollback must recreate the old sandbox/apps and incurs outage.
- **Scale up:** persist stable replica rows first, then schedule/reconcile them. **Scale down:** drain selected highest ordinals, remove apps/init, remove sandbox, then tombstone/delete the replica.

Old and new apps sharing one namespace often cannot listen on the same internal port simultaneously. Docker image pull and stopped-container creation can happen first, but conflicting processes cannot both be started. Availability comes from rolling across multiple replicas, not pretending a one-replica shared-netns replacement is zero downtime. For a single replica, document the brief stop or require an application-specific handoff.

### Recovery invariants

- Docker creation is idempotently adopted when immutable labels match the desired logical replica/name/role, current assignment epoch, attempt, and effective-template fingerprint. Creation revision/generation labels may be older when a matching process was deliberately retained; DB acknowledged generation is advanced after reconciliation.
- Crash after Docker create but before DB association is repaired from labels.
- Duplicate candidates for one logical slot are selected deterministically; extras are stopped/removed only after verifying the active/desired attempt.
- A missing app is recreated in its existing sandbox. A missing sandbox causes whole-replica rebuild because future `container:<old-id>` joins are impossible.
- Node-local agents verify the current assignment epoch before mutation, inspect only local Docker IDs, and persist epoch-tagged observations; a central reducer never sends a remote ID to the local daemon and ignores stale epochs. A reassignment increments/fences the old node before the new node becomes routable.
- Remove is asynchronous/tombstoned until every assigned node acknowledges app-before-sandbox cleanup. Do not clear desired DB state after ignored Docker errors as current delete does.
- Rollback promotes/restarts the retained old attempt when a candidate fails, except that replaced fixed-port sandboxes require full recreation. It never rewrites desired history from inspect. Report desired runtime generation, desired-revision key, and serving generation while rollback is active.
- Nonessential failure completes with degradation and does not roll back essential apps. Essential startup/reconcile failure halts that replica's promotion and may roll back the runtime generation according to availability/deadline policy.

## 17. Affected files and modules

| Area | Known affected locations/reason |
|---|---|
| Public model | `crates/nanocl_stubs/src/cargo_spec.rs`, `cargo.rs`, `process.rs`, `statefile.rs`, `node.rs`, `system.rs`, `generic.rs`: desired types, inspect tree, selectors/status, node payloads, real port reexports. |
| Schema generation | `crates/nanocl_stubs/Cargo.toml`, `bin/nanocld/src/services/openapi.rs`, `services/mod.rs`, `bin/nanocld/specs/swagger.yaml`: Utoipa/Schemars features, strict schema tests/snapshot. |
| Database | `bin/nanocld/migrations/*_{specs,cargoes,object_process_statuses,processes,jobs,vms,networks}/*`, new desired-deployment/replica/process/port-reservation forward migrations, `bin/nanocld/src/schema.rs`. |
| Models/repositories | `bin/nanocld/src/models/{cargo,spec,process,object_process_status,job,vm,network}.rs`, corresponding `repositories/*`, plus new replica/mapping modules. |
| Desired-object mutations | `bin/nanocld/src/objects/cargo.rs`: transactions, validation, removal of bespoke singular Config merge. |
| Scheduling/node RPC | `bin/nanocld/src/utils/container/generic.rs`, `tasks/cargo.rs`, `services/node/cargo.rs`, `crates/nanocl_stubs/src/node.rs`, client node methods: stable replica assignments instead of counts. |
| Runtime compiler/controller | `bin/nanocld/src/utils/container/{cargo,process,network,generic,image,job,vm}.rs`, `utils/{secret,system,exec}.rs`: sandbox, per-container compilation, local-only actions, declared/compiled boundary. Job/VM paths must be regression-protected, not redesigned. |
| Observation/events | `bin/nanocld/src/system/{docker_event,event,init}.rs`, `models/raw_emitter.rs`: labels, local inspection, periodic resync, durable generation conditions. |
| Daemon API | `bin/nanocld/src/services/cargo/*`, `process/*`, `exec/*`, `openapi.rs`: new payloads/inspect/selectors/waits. |
| Rust client | `crates/nanocld_client/src/{cargo,process,node}.rs`: response tree, selectors, accepted generation, remove stale instances call. |
| CLI | `bin/nanocl/src/models/cargo.rs`, `commands/{cargo,state,backup,install,uninstall,process}.rs`, `utils/{state,docker,process}.rs`: multi-container flags/output, bounded generation waits, authored/effective bind handling. |
| Proxy/DNS | `bin/ncproxy/src/subsystem/event.rs`, `utils/{rule,resource}.rs`: serving-ready replica endpoint abstraction instead of app `container:*` inspect. `bin/ncdns/src/event.rs` and `bin/ncdns/README.md` should be checked because the README's Cargo-DNS claim is stale. |
| Examples/docs | `README.md`, root `Statefile.yml`, `installer.yml`, all Cargo-bearing `examples/*`, backup fixtures, `doc/man/nanocl-cargo*.md`, and state command pages. There are at least 28 YAML Cargo examples plus JSON/TOML/extensionless variants. |
| Tests | `bin/nanocld/src/services/cargo/mod.rs`, inline runtime/network tests, `crates/nanocld_client/src/cargo.rs`, CLI model tests, proxy address tests, `tests/e2e.bats`, `tests/network_partitioning.yml`, `tests/relative_bind.yml`, image/init/health/port fixtures, and `tests/backup/*.yml`. |

Important current consumers that must not be missed include `ncproxy`'s singular `spec.container` health logic (`bin/ncproxy/src/subsystem/event.rs:58-91`), backup's injected-bind stripping, and Cargo list's one-image `CargoRow` (`bin/nanocl/src/models/cargo.rs:319-345`).

## 18. Testing strategy

### Contract and schema

- YAML/JSON/TOML round trips for PascalCase proposed examples.
- Required `Name`, `Containers`, container `Name`, and nested `Container`; field-only defaults for replicas/essential/policy.
- Reject unknown outer, `Config`, nested `HostConfig`, and `PortBinding` fields with full paths.
- Reject duplicate/empty names, zero replicas, no essential app, missing image, forbidden raw network/port fields, `AutoRemove`, reserved labels/env, invalid modes, and incompatible ports.
- Exact `bollard_next::models::PortMap` serialization including `None`, multiple bindings, `HostIp`, and `HostPort`.
- Schemars and Utoipa required/additional-properties golden assertions; checked-in OpenAPI snapshot diff.
- Parse every repository Statefile fixture. This should deliberately expose obsolete ignored `Replication` examples rather than silently preserve them.

### Pure compiler/reducer

- Declared input is unchanged after compile; token expansion/injected labels/env/binds/network exist only in effective output.
- Deterministic secret reference/data order, de-duplication, precedence, reserved/collision failures, per-container TLS isolation/cleanup, secret revision triggers, and inspect/status redaction.
- Effective configs for all Cargo/container network combinations, sandbox `PortMap`, binds, image policy, and init ordering.
- Identity labels/name generation, effective-template fingerprints, assignment-epoch fencing, and recovery parsing.
- Healthless, disabled, starting, healthy, essential unhealthy, nonessential pull/start/health failure, partial replicas, zero ready, runtime-generation mismatch, old-generation availability, recovery, duplicate/out-of-order/stale-epoch observations.

### Database and controller

- Fresh and forward migration tests, including a database created before any historical health-column change.
- Migration of current and historical singular specs including `Placement.Replicas`, history/revert, contaminated-spec detection/redaction/blocking, differing init/app networks/ports, untranslatable publish-all failure, mixed-reader prevention, and Job/VM rows unchanged.
- Transaction rollback for spec/status/Cargo/replica writes.
- Fake-Docker tests for create order, app-before-sandbox deletion, missing app, missing sandbox, crash between Docker/DB writes, duplicate adoption, fenced reassignment, local-only multi-node observation, init failure/retention cleanup, port conflict/reservation race, and retry/deadline behavior.
- Rollout tests for metadata-only revision, one-container change, add/remove, secret-revision reload, sandbox replacement, fixed-port rollback outage, scale up/down without replacing matching templates, and one-at-a-time availability.

### API, CLI, and downstream

- Atomic PUT and explicitly defined whole-field PATCH behavior.
- Replica-grouped inspect and selectors for logs/exec/restart; single/single implicit exec and ambiguity error.
- Default accepted wait completing while a serial rollout is still health-progressing; explicit reconciled/ready/stopped/deleted success; targeted-restart generation; intentionally Created/Stopped Cargoes; reconciliation failure; partial availability as Degraded; recoverable late Unhealthy; one command-wide timeout across multiple resources; clean EOF; missed event/restart recovery; and consistent exit codes across apply/run/start/stop/restart/remove/put/revert.
- Proxy includes only serving-ready endpoint abstractions (sandbox, routable host, or none) and handles old-generation/degraded/partial replicas; app/sandbox/init logs are separated.
- Non-destructive E2E fixtures for shared localhost/IP, custom network, both Cargo escapes, both container escapes, ports, secrets, binds, init, delayed health, permanently unhealthy, disabled healthcheck, and multi-node replicas. These are future implementation tests, not part of this audit run.

### Audit validation performed

The following Rust checks were run against the clean pre-report checkout and passed again after the report-only change. No live daemon/cluster, database migration, Docker mutation, or destructive E2E was run.

| Command | Result | Coverage/limitation |
|---|---|---|
| `cargo fmt --all -- --check` | Passed | Workspace Rust formatting. |
| `cargo check -p nanocl_stubs --all-features` | Passed | Serde, Schemars, and Utoipa feature compilation for public stubs. |
| `cargo check -p nanocld --features dev` | Passed | Daemon plus OpenAPI/Utoipa integration compiles. It does not execute service registration or rewrite the checked-in schema. |
| `cargo check -p nanocld_client -p nanocl` | Passed | Directly affected client and CLI compile. |
| Ruby/Psych fenced-block parser (exact command below) | Passed, 12/12 | One normative shape plus all 11 complete proposed examples are syntactically valid YAML; this does not claim the current v0.18 parser accepts the proposed v0.19 shape. |

~~~sh
ruby -ryaml -e 'text = File.read(ARGV.fetch(0)); blocks = text.scan(/```yaml\n(.*?)```/m).flatten; abort "expected 12 YAML blocks, got #{blocks.length}" unless blocks.length == 12; blocks.each_with_index { |body, index| YAML.safe_load(body, aliases: false); puts "yaml block #{index + 1}: parsed" }' docs/MULTI_CONTAINER_CARGO_AUDIT.md
~~~

No standalone offline schema-generation command was found. The checked-in `swagger.yaml` was inspected, and the `dev` feature path was compiled; starting the service merely to regenerate it was intentionally avoided because this audit prohibits live-cluster/stateful behavior. There were no baseline failures and no failed check limited the audit.

## 19. Ordered implementation plan

Each task should compile and be reviewable independently. Dependencies are explicit; tasks that introduce unused foundations are intentional staging, not compatibility promises.

### Task 1 — strict public-model foundation (safest first)

- **Objective:** Add `ContainerSpec`, Cargo/container network types, strict Bollard/PortMap deserializers, real `PortMap`/`PortBinding` reexports, and the complete target declaration under the explicitly temporary internal name `CargoSpecVNext` without switching current Cargo fields.
- **Likely files:** `crates/nanocl_stubs/src/cargo_spec.rs`, feature dependencies/tests in `crates/nanocl_stubs`.
- **Boundaries:** New types and pure validation only; no current `CargoSpecPartial`/history `CargoSpec` change and no production consumer. At Task 9 cutover, rename old history `CargoSpec` to `CargoSpecRevision` and `CargoSpecVNext` to final public `CargoSpec`; do not expose the temporary name in a released API.
- **Tests:** Required/default/PascalCase behavior, nested typo rejection, collision cases, PortMap round trips, Schemars/Utoipa assertions.
- **Migration:** None.
- **Must not change yet:** DB, runtime, routes, client/CLI, health, examples, or existing wire behavior.

### Task 2 — pure declared-to-effective compiler

- **Depends on:** Task 1.
- **Objective:** Define pure compiler inputs/outputs, effective-template fingerprints, and semantic validation for network ownership, ports, reserved fields, secret merge order/isolation, labels, and identity metadata.
- **Likely files:** a new focused module under `bin/nanocld/src/utils/container/`, plus `network.rs`, `secret.rs`, `image.rs` test helpers.
- **Boundaries:** No Docker calls and no persisted/current Cargo mutation; retain the current controller path.
- **Tests:** Complete network/token matrix, config immutability, deterministic secrets/env, TLS isolation, non-secret revision fingerprints, compiler-owned fields, invalid combinations, inspect redaction.
- **Migration:** None.
- **Must not change yet:** Runtime creation, DB schema, API payloads, CLI.

### Task 3 — additive Cargo deployment/replica database schema

- **Depends on:** Identity/status decisions locked by Tasks 1-2; it can otherwise proceed independently.
- **Objective:** Add Cargo desired lifecycle/runtime generation/reconcile trigger, targeted restart observation, `cargo_replicas`, `cargo_replica_processes`, `cargo_port_reservations`, assignment fencing, constraints, Diesel schema, models, and repositories.
- **Likely files:** new forward migration, `bin/nanocld/src/schema.rs`, new model/repository modules.
- **Boundaries:** Tables may be unused; do not alter generic `processes`, Job/VM rows, or remove aggregate status.
- **Tests:** Fresh/up/down migration, FK/cascade/set-null/check/partial-index behavior, generation increments, fence rejection, concurrent/wildcard port reservation, active-slot uniqueness, Job/VM regression fixtures.
- **Migration:** Additive only; audit the historical health column with an explicit repair migration if needed.
- **Must not change yet:** Backfill/cutover, Docker behavior, public wire.

### Task 4 — historical desired-spec transformer

- **Depends on:** Task 1; coordinate final columns with Task 3.
- **Objective:** Implement/test an explicit one-way transform for every Cargo history row, contamination detector/sanitizer, and classification of untranslatable configurations.
- **Likely files:** forward data migration/helper, `repositories/spec.rs`, migration fixtures.
- **Boundaries:** No implicit compatibility deserializer in the new API; transformation is upgrade-only. Build/test it independently, but do not execute an in-place JSON transform under old readers.
- **Tests:** missing/zero/overflowing `Placement.Replicas`, singular app/init including inherited init image, omitted init beside default versus nondefault app network, same/differing explicit modes and init ports, secrets/pull settings, PortMap extraction, reserved-artifact sanitization, ambiguous contamination block, all history/revert, publish-all error, idempotence.
- **Migration:** Define backup, all-node maintenance/version fence or shadow storage, atomic reader cutover, and controlled legacy-runtime replacement; do not fabricate authored state or sandbox identity from inspect.
- **Must not change yet:** Do not deploy/execute the data transform until the Task 9 reader cutover is coordinated; do not remove old tables.

### Task 5 — replica scheduling and identity labels

- **Depends on:** Task 3.
- **Objective:** Allocate stable replica rows/ordinals/nodes, fixed-port reservations, and assignment epochs; carry identity/fencing through node RPCs and reserved Docker labels.
- **Likely files:** `utils/container/generic.rs`, `tasks/cargo.rs`, node service/stubs/client, process recovery helpers.
- **Boundaries:** Keep one current app per replica temporarily; do not add sandbox/multi-app rollout.
- **Tests:** scale assignment, fenced reschedule/stale observation, concurrent fixed-port reservation, label round trip, local/remote routing.
- **Migration:** Mark legacy current processes for controlled replacement rather than treating random names as identity.
- **Must not change yet:** Health aggregate/public inspect and Job/VM scheduling.

### Task 6 — sandbox lifecycle and node-local reconciliation

- **Depends on:** Tasks 2, 3, and 5.
- **Objective:** Create/manage one sandbox for supported non-host replicas, compile normal containers to it, and make recovery idempotent/local-only.
- **Likely files:** `utils/container/{cargo,process,network}.rs`, `utils/system.rs`, `system/{init,docker_event}.rs`, replica repositories.
- **Boundaries:** Initially reconcile one named app translated from legacy shape; no public multi-container cutover.
- **Tests:** creation/deletion order, sandbox restart/loss, crash adoption, custom/default/none/host modes and endpoint abstraction, port owner, no secrets/logs on sandbox, redacted persisted inspect.
- **Migration:** Persist sandbox/app mappings; no generic process-table redesign.
- **Must not change yet:** Multi-app rollout, final status API, CLI.

### Task 7 — named multi-container/init reconciler and rollout

- **Depends on:** Task 6 and new desired types from Tasks 1-2.
- **Objective:** Reconcile full named app/init collections by runtime generation/effective-template fingerprint and implement serial replica rollout/rollback.
- **Likely files:** Cargo controller/tasks/compiler/runtime modules and object mutation diffing.
- **Boundaries:** PUT is canonical; no fine-grained public per-container PATCH endpoint.
- **Tests:** metadata-only revision, explicit group/target restart despite matching fingerprints, add/remove/change one app, secret-revision reload, sequential per-replica init/cleanup, precreate then internal-port handoff, fixed-port sandbox replacement/rollback outage, scale without replacing matching templates.
- **Migration:** Consume transformed spec revision keys while maintaining a separate runtime generation.
- **Must not change yet:** CLI presentation and removal of old status storage.

### Task 8 — durable observation and condition reducer

- **Depends on:** Tasks 3, 6, and 7.
- **Objective:** Normalize fenced local Docker observations, add periodic repair, calculate lifecycle/reconcile/ready/available/degraded/health conditions, and derive Cargo aggregate counts.
- **Likely files:** `system/docker_event.rs`, periodic controller, replica models/repositories, `crates/nanocl_stubs/src/{cargo,system}.rs`, proxy input.
- **Boundaries:** Do not remove Job/VM `ObjPsStatus`; do not make runtime health a create failure.
- **Tests:** reducer/display-precedence matrix, Created/Stopped/Deleting, disabled/image health, partial essential/nonessential failure, old-generation availability/rollback, missed events, same-attempt out-of-order sequence rejection, daemon restart sequence resume, stale-node fencing, desired count/runtime generation.
- **Migration:** Populate normalized observation fields; aggregate remains derived.
- **Must not change yet:** Final CLI wait UX until durable query is available.

### Task 9 — breaking API/client cutover

- **Depends on:** Tasks 4, 7, and 8.
- **Objective:** Switch create/PUT/PATCH/history/inspect to the new contract and add minimal logical replica/container selectors.
- **Likely files:** daemon Cargo/process/exec services, OpenAPI, `nanocld_client`, public inspect/stub types.
- **Boundaries:** Keep existing Cargo-wide lifecycle routes; use whole-field PATCH semantics; no new general dependency/namespace/volume APIs.
- **Tests:** payload/schema snapshots, history/revert, grouped inspect, UUID/ordinal selector resolution, authorization/errors, async accepted spec revision/runtime generation.
- **Migration:** Require the forward data/runtime transition; no dual old/new public compatibility path.
- **Must not change yet:** Remove old DB structures only after all consumers are switched.

### Task 10 — CLI, proxy, backup, and documentation cutover

- **Depends on:** Task 9.
- **Objective:** Update Statefile application, bounded waits, selectors/output, proxy serving-endpoint abstraction, backups, examples, man pages, and completions.
- **Likely files:** affected CLI/ncproxy/docs/examples listed in section 17.
- **Boundaries:** Default wait is Accepted; Reconciled/Ready are explicit finite waits; sandbox/init excluded from normal logs/counts; no unrelated CLI redesign.
- **Tests:** parser/output goldens, accepted/reconciled/ready/stopped/deleted waits, command-wide timeouts/EOF, proxy host/none/bridge and old-generation availability, backup declared-state round trip/redaction, fixture parse sweep.
- **Migration:** Remove stale singular fixtures and document the one-way breaking Statefile upgrade.
- **Must not change yet:** Job/VM semantics and unrelated proxy/resource behavior.

### Task 11 — cleanup and full integration qualification

- **Depends on:** Tasks 1-10 and successful upgrade rehearsal.
- **Objective:** Remove Cargo-only reliance on old aggregate status/legacy paths, run workspace and controlled E2E qualification, and verify upgrade/rollback operations.
- **Likely files:** old Cargo branches across status/process/controller code, migrations, CI/E2E fixtures.
- **Boundaries:** Delete only proven-dead Cargo compatibility code; retain generic tables/routes used by Jobs/VMs.
- **Tests:** full non-E2E workspace suite, then isolated Docker/multi-node E2E including every section-6 example and failure case.
- **Migration:** A later forward cleanup migration only after deployed readers no longer need old Cargo status fields.
- **Must not change yet:** Unrelated architecture or a new volume/dependency/namespace framework.

## 20. Risks and unresolved decisions

| Risk/decision | Current recommendation or required resolution |
|---|---|
| Sandbox image availability/security | Build and pin a Nanocl-owned multi-arch digest; define offline installation and upgrade policy before runtime cutover. |
| Docker fields incompatible with `container:<id>` | Produce and test an explicit semantic rejection list against pinned Docker API/Bollard; hostname/DNS/add-host/MAC/publish fields are known concerns. |
| `Config.ExposedPorts` under shared netns | It is metadata, not a host binding. Preserve it provisionally and add a pinned Docker API test; reject it only if the engine actually rejects `ExposedPorts` with `container:<sandbox-id>`. Cargo `PortMap` remains the sole host-publish source. |
| Existing `bridge`/`default` behavior | Preserve them as Cargo-only built-ins, although the simplified prompt listed only omitted/custom/host/none. Container overrides remain only host/none. Confirm this intentional surface before Task 1 freezes the type. |
| Fixed host ports and replicas | Use conservative transactional node/port/protocol reservations; rehearse wildcard-IP and concurrent-Cargo races. Dynamic allocation must be observable. |
| Host-mode listener conflicts | Nanocl cannot infer arbitrary app listeners when Cargo/container mode is `host`. Document user-managed/distinct placement and report bind failures; do not claim automatic reservation. |
| Sandbox health | Treat running sandbox as infrastructure-ready; it has no user healthcheck. Define controller retry/deadline and whole-replica replacement for loss. |
| Node partition/reassignment | Assignment epochs fence DB mutations/routing but cannot instantly stop a partitioned Docker process. Freeze the acknowledgement/lease timeout and availability-versus-duplicate-execution policy before rescheduling. |
| Stable ordinal reuse | UUID is identity; reuse a display ordinal only after cleanup/tombstone completion. Confirm any external consumers of `NANOCL_CARGO_INSTANCE`. |
| Internal port conflicts | Cannot be inferred reliably from Docker config; surface start errors clearly and roll replicas serially. Document localhost port ownership between app authors. |
| Secret precedence | The proposed shared < container < raw precedence is deterministic but differs from current append behavior in edge cases. Lock it with security review/tests before migration. |
| Secret value-triggered rollout | A spec containing only secret names does not change when secret data changes. Add a durable secret revision/event and runtime-generation bump; do not hash secret values into public specs/status. |
| Secret/inspect exposure | Use per-container TLS directories and redact secret-derived env/internal bind sources from persisted/public inspect. Audit generic Job/VM Process exposure separately. |
| Runtime fingerprint algorithm | Keep spec revision, runtime generation, and effective-template fingerprints distinct. Freeze canonicalization/compiler-version/secret-revision inputs without persisting secret values. |
| Init rerun policy | Proposed once per replica/runtime generation when its effective template requires it, including scale-up, not ordinary app restart. Init must be per-replica safe; singleton work remains a Job. |
| PATCH semantics | Prefer PUT first. If PATCH is retained, whole-list replacement/clear behavior must be frozen before client implementation; do not infer array merge. |
| Aggregate projection performance | Start with derived queries/conditions. Add only a rebuildable runtime-generation-keyed cache if measured query/proxy load requires it. |
| Historical health migration | Git history (`fe90b6bf`) confirms the 2022 migration file was later edited for Cargo health. Determine which released databases predate that edit and add a forward repair; never rely on migration-file edits. |
| Legacy desired-spec contamination | Known injected fields can be detected, but original TTY/restart/hostname/network/secret-env intent may be unrecoverable. Block ambiguous rows and require authoritative user input; never guess or preserve leaked secrets. |
| Legacy live workload transition | Existing processes have no stable replica identity or sandbox. Plan all-node reader/version fencing plus controlled one-replica-at-a-time replacement and state the availability impact. |
| `ncproxy`/DNS behavior | Proxy must use serving-ready replica endpoints: sandbox IP, routable assigned-node address for Cargo host, or none for Cargo none. Verify whether Cargo DNS is a supported contract or stale documentation. |
| `$$INTERNAL_GATEWAY` | Stop whole-Cargo text replacement. Freeze/test sandbox-network, host/built-in fallback, and `none` rejection semantics before the pure compiler becomes contract. |
| Explicit wait timeout | Five minutes is a concrete starting recommendation for Reconciled/Ready; expose configuration and validate it against large image pulls. Default Accepted must never inherit an implicit health wait. |
| UTS and raw namespace fields | Nanocl should share only networking. Audit/reject user raw sibling/container namespace references; preserve independent raw modes only where they do not defeat identity/recovery. |

The intentional compatibility break should be clean: one documented Statefile/API migration and a forward database/runtime transition, not a permanent dual-model path. The unresolved items above should be decided before the task that first switches public Cargo inputs; none requires broadening the first foundation diff.
