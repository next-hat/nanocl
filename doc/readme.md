# Doc

This directory contains project documentation, generated man pages, and images
used by the documentation.

- [Multi-container Cargo RFC](multi-container-cargo-rfc.md)

## Lifecycle operation receipts

The following asynchronous API requests return the accepted operation UUID in
`X-Nanocl-Operation`. Paths below omit the API version prefix. An accepted
response does not mean the runtime operation has finished.

| Request | Terminal event action |
| --- | --- |
| `POST /processes/cargo/{key}/start` or `POST /processes/vm/{key}/start` | `start` |
| `POST /processes/cargo/{key}/stop` | `stop` |
| `POST /processes/cargo/{key}/restart` | `restart` |
| `PATCH` or `PUT /cargoes/{key}` | `update` |
| `PATCH /cargoes/{key}/histories/{history_key}/revert` | `update` |
| `PATCH /vms/{key}` | `update` |
| `DELETE /cargoes/{key}` or `DELETE /vms/{key}` | `destroy` |

The CLI subscribes before sending the request, then waits for the same actor
kind, canonical key, and `Metadata.Operation` UUID. It accepts a normal terminal
event for the requested action or reports an error from that exact operation.
Terminal events also include `Metadata.SpecKey` and are published only after the
operation is durably settled. If settlement cannot be confirmed, the daemon sends
a correlated warning with action and reason `operation_pending` instead. The CLI
returns an interrupted, outcome-unknown error for that warning; it does not claim
the operation has completed or retry it. Inspect runtime and pending operation
state before deciding whether to retry. API clients implementing the same wait
must handle these warnings and use an unconditioned event subscription, since
name-only server conditions can close the stream on an unrelated operation.

The daemon decides whether Cargo/VM start or stop is `Unchanged` while holding
lifecycle admission. It checks pending operations, persisted desired state, and
fresh runtime identity and readiness. The CLI does not skip admission from cached
status. Cargo instances assigned to other nodes use the existing coordinated
operation when a local check cannot establish their state.

Cargo/VM start and Cargo stop return HTTP 200 for a verified no-op, without an
operation UUID; accepted mutations return HTTP 202 with the operation receipt.
The CLI closes its pre-opened event subscription immediately for a no-op. The
synchronous VM stop endpoint returns HTTP 200 and adds `X-Nanocl-Unchanged: true`
only when it verified that no shutdown was needed. Jobs always use admission
because retained run status does not indicate whether to dispatch a new execution.

A successful healthchecked Cargo start emits one receipt-tagged terminal `start`,
then `healthy`. Both follow confirmed durable operation settlement and lease
release. Failed or unsettled starts do not publish a successful readiness event.

Job start and stop acknowledge dispatch and display `Accepted`. VM stop and
restart commands use the synchronous `/vms/{key}/stop` and
`/vms/{key}/restart` endpoints and await their HTTP result.

An accepted mutation response must include a valid receipt. Without it, the CLI
reports that the request was accepted but completion cannot be correlated;
it does not infer success from another operation's event. Do not retry blindly:
inspect workload status and recent events before deciding whether to retry.

## Resource revision completion

Resource revision completion suggests matching IDs among the newest 100 revisions.
The history command retains access to the complete revision history. The resource
history API accepts optional `limit` and `offset` fields in its JSON `filter` query
parameter; omitting them preserves the complete result. Filtering and custom ordering are not supported by that endpoint.
