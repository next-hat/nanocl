# Job configurations, executions, and retained history

Jobs execute containers sequentially. Each `nanocl job start` accepts a new run
with a UUID and fresh containers. One nonterminal run is allowed per incarnation,
including runs whose runtime state is unknown. Cron uses the same admission path
and does not queue overlapping ticks or catch up missed schedules.

## Relationships and lifetimes

The live `jobs` row references an immutable `specs` revision, a retained
`resource_incarnations` row, and its current run. Specifications contain authored
declarations and secret references before runtime secret injection. Resolved
environment secrets and injected credentials are not stored in run history.
Like Cargo and VM inspection, the returned `Spec` includes immutable revision
metadata: `Key`, `JobKey`, `Version`, and `CreatedAt`. Jobs use their global name
as `JobKey`. Authored `JobPartial` input contains only the declaration; revision
metadata is assigned by the daemon and is also returned with retained run detail.

Shared `operations` records describe start, stop, and delete requests. Start
atomically commits its operation, run, ordered pending steps, pinned definition,
and current status. Rejection changes none of them. The start operation completes
when the detached executor owns the accepted run, before Docker execution. This
acknowledges dispatch, not workload success. The executor retains ownership
evidence for its run while freeing the pending control slot for stop/delete.

`job_runs` retains name, incarnation, spec, initiating operation, timestamps,
state, and reason. `job_run_steps` retains ordered results and immutable Docker
IDs. Unstarted or unobserved steps have no synthetic exit code. Failure prevents
later steps from launching; success requires every step's observed zero exit.
Docker restart policies that automatically retry commands are rejected.

Deletion retains incarnations, definitions, operations, and run/step outcomes.
Delete and TTL cleanup remove the current run's containers; containers from
previous completed runs may remain.
Statefile apply still deletes/recreates and starts Jobs. A same-name replacement
has a new incarnation and cannot change earlier definitions or history.
CLI removal and Statefile replacement submit the inspected incarnation/run with
their deletion request; a concurrent new run causes a conflict rather than
deleting an execution the caller did not select.

## Control, interruption, and TTL

Start and stop acknowledge accepted control requests. Another start conflicts
while a run is active or unknown. Stop/delete durably request cancellation before
acting on the affected run's exact containers, preventing another step from
launching. Cancelling a Rust future does not prove Docker stopped: unavailable
runtime evidence preserves an active `Unknown` run. Normal stop/delete remain
the operator path to resolve it.

Whole-Job process start/stop use this lifecycle. Whole-Job restart is rejected:
stop, wait for settlement, then start a new run. Individual managed Job container
stop/restart/kill are rejected. Other process kinds keep their behavior.

Startup and periodic reconciliation use durable IDs, retained ownership evidence,
and exact Docker labels/IDs. They never replay commands or start an unexecuted
step. Observed completed work can settle; still-running or unobserved work remains
active. A crash after reserving a step but before recording its container can
require normal stop/delete to resolve uncertainty. There is no manual daemon
reconciliation command.

Callbacks and cleanup are fenced by incarnation/run identity. Docker events
refresh their own container observation and cannot complete a Job by name.
Cron requests carry the incarnation that installed the rule. `Ttl` removes the
live Job and schedule after execution, not history. An old timer cannot remove
a successor run or a same-name replacement. Reconciliation handles lost timers.

## Configuration history and execution commands

```sh
nanocl job history backup
nanocl job history backup --limit 20 --offset 20
nanocl job history backup --id SPEC_UUID
nanocl job history backup --id SPEC_UUID --display json
nanocl job start backup
nanocl job wait backup
nanocl job runs backup
nanocl job runs backup --limit 20 --offset 20
nanocl job runs backup --id RUN_UUID
nanocl job runs backup --id RUN_UUID --display json
nanocl job stop backup
```

Job, Cargo, and VM history commands list immutable configuration revisions,
including the current revision. Job history retains configurations across
deletion, TTL cleanup, and recreation under the same global name. Statefile
apply replaces Jobs, so earlier configurations remain available. A Job does not
need to have executed for its specification to appear in history.
`job history --id SPEC_UUID` returns the authored configuration and its revision
metadata; the UUID is the specification's `Key`.

`job runs` lists retained executions and distinguishes multiple incarnations by
UUID. `job runs --id RUN_UUID` includes the pinned specification, linked start
operation, and ordered steps. Configuration and execution inspection are
read-only and remain available after the live Job is removed.

Both Job commands share the history options used by Cargo and VM: a positional
key, `--id UUID` for detail, `--limit` and `--offset` for lists, and `--display`
for structured output. Cargo and VM keys are canonical `namespace.name` keys;
Job keys are global names.

Lists default to a table, with 50 results per page, a maximum limit of 100, and
offset zero. JSON and YAML lists are also supported. Detail defaults to the
configured inspection format, normally YAML. TOML requires `--id`, and `--id`
cannot be combined with pagination options. Results are ordered newest first.
Cargo and VM commands retrieve their existing revision-history response and
apply pagination locally; the Job API paginates both revisions and executions
on the server. VM history uses the existing API's live-VM and
current-incarnation scope.

The API exposes:

- `GET /jobs/{name}/histories`: configuration revisions, ordered by creation
  time descending, then specification UUID descending.
- `GET /jobs/{name}/histories/{id}`: one configuration by specification UUID.
- `GET /jobs/{name}/runs`: executions, ordered by acceptance time descending,
  then run UUID descending.
- `GET /jobs/{name}/runs/{run}`: one execution by run UUID, including its pinned
  specification, operation, and steps.

Both list endpoints accept `limit` (default 50, maximum 100) and nonnegative
`offset`. The client exposes `list_history_job` and `inspect_history_job` for
configurations, and `list_runs_job` and `inspect_run_job` for executions.

Execution history previously exposed by `nanocl job history` and the
`/histories` API now lives under `nanocl job runs` and `/runs`. Update scripts
and client calls that consume run IDs or outcomes to use the execution commands
and endpoints. Existing run IDs remain run IDs; `history --id` now expects a
specification UUID. This split uses the existing retained specifications and
run records without adding persistent state.

Current inspection/status uses the current run. Job wait pins that run, or the
next observed run of the selected incarnation when subscribed before its first
start. For Jobs, use the default wait condition or `-c not-running`; Docker's
`next-exit` and `removed` conditions are not supported for retained runs.
Both supported forms wait for the pinned run to settle. `Unknown` does not
prove that its containers stopped, so it does not complete the wait.

Execution records do not archive Docker logs. Logs depend on Docker retaining
runtime resources. This change adds no history-pruning framework.
If Docker auto-removes a container before a wait result can be observed, its exit
code remains unknown; retained runs cannot recover an exit code Docker no longer
has.

## Fresh-cluster requirement

Existing installations must be recreated from current Statefiles before using
this schema. There is no legacy adoption, backfill, dual storage, version fallback,
or rolling-upgrade path. Recreation is an operator action; development tests use
separate task-owned databases and never reset an existing installation.

## Maintainer reading guide

- `bin/nanocld/migrations/2023-11-06-175428_jobs` and
  `2026-10-27-000000_job_runs`: normalized definitions and retained execution schema.
- `bin/nanocld/src/repositories/job_operation.rs`: atomic admission, cancellation,
  shared operation completion, and retained-identity deletion.
- `bin/nanocld/src/repositories/job.rs`: retained configuration-history queries.
- `bin/nanocld/src/repositories/job_run.rs`: ordered results, terminal checks,
  execution queries, and live-status fencing.
- `bin/nanocld/src/tasks/job.rs` and `utils/container/job.rs`: ownership handoff,
  sequential execution, confirmed stop/delete, and TTL.
- `bin/nanocld/src/utils/container/job/recovery.rs`: interruption reconciliation.
- `bin/nanocld/src/services/job/{history,runs}.rs`,
  `crates/nanocld_client/src/job.rs`, and `bin/nanocl/src/commands/job.rs`:
  configuration/execution API, client, and CLI.
