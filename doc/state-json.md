# State command JSON output

Use `--json -y` with `nanocl state apply`, `remove`, `start`, or `stop` to
write newline-delimited JSON to stdout. Apply cannot combine this mode with
`--follow`. Human-readable progress is suppressed. Each record is flushed
individually; consumers should process records as a stream and ignore unknown
record types and fields.

Every record has `schema_version: 1`, `operation` and `type`. Records associated
with a rendered Statefile also have `statefile`. URL credentials, query strings
and fragments are omitted from this field. Optional fields are omitted when
unavailable.

| Type | Fields | Meaning |
| --- | --- | --- |
| `state` | `status`, `completed`, `total`, `elapsed_ms`, optional `success` | Start/completion of one rendered Statefile. Completed counts processed items, including a failed item. |
| `item` | `resource`, `status`, `completed`, `total`, `elapsed_ms`, optional `success`, `error` | One resource's phase or completed operation. |
| `image` | `resource`, `image`, `node`, optional `layer`, `current`, `total`, `error`, plus `status` | Image download progress. Byte counts are included only when reported. |
| `result` | `success`, optional `error` | Overall command result. |
| `summary` | `summary`, `reload`, `remove_orphans` | Apply preview emitted for each rendered Statefile before mutations. |
| `cargo_receipt` | `resource`, `status`, optional `receipt` | Best-effort Cargo snapshot after a successful item operation. |

An `item` record without `success` is progress, never a success signal.
In particular, `observed ...` statuses describe daemon events while the
operation remains pending. An observed healthy candidate does not establish
rollout completion. Wait for the existing item and command result records and
check the process exit code. Stream loss is not evidence of successful apply.
All phases of an item use the same elapsed-time clock. Consecutive identical
observations from the same actor and node are suppressed; health recovery and
observations from other candidates remain visible.

The `summary` object contains `source`, `namespace`, `group`, `explicit_group`,
`counts` and `cargoes`. Counts cover `cargoes`, `virtual_machines`, `jobs`,
`resources` and `secrets` for that rendered source. Each Cargo includes its
`name`, `replicas` and containers with `name`, `image`, `init`, `essential` and
`healthcheck` (`configured`, `disabled`, or `image_default`). Image-default
policy does not mean the image has no health check. This is a declaration
summary, not a diff: use `nanocl state diff` to preview actual changes.
It excludes raw Statefiles, environment values, secret data, resource payloads
and health-check commands.

Cargo receipt status is `observed` or `unavailable`. Observed receipts expose
the current revision and application-process observations; retained and pending
candidate processes, init containers and sandboxes are excluded. Image IDs
are Docker image IDs, not registry manifest digests. A receipt never carries
`success`: it is a snapshot, may reflect concurrent changes, and does not
override the operation result. The single inspection is limited to five
seconds; failure or timeout produces `unavailable`, without changing successful
apply into failure. Receipt output errors are also ignored so they cannot stop
later Statefile items; required progress and result records retain their output
error handling. No container environment, health command or health log is included.

```sh
nanocl state apply -s https://example.org/Statefile.yml -y --json
```

Summary and receipt types are additive to schema version 1; existing
`state`, `item`, `image` and `result` fields retain their meanings.
